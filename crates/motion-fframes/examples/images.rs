//! Upstream image-path qualification; this is not a document import API.
use fold_motion_fframes::{
    Conversion, Images, Renderer, capture,
    fframes::{Svgr, svgr},
};
fn images() -> Images {
    let image = std::sync::Arc::new(
        fold_motion_fframes::fframes::usvgr::PreloadedImageData::new(
            "fold-image-fixture".into(),
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 255, 0,
            ],
        ),
    );
    Images::from([("fixture".into(), image)])
}
fn artwork() -> Svgr<'static> {
    svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
        <image width="32" height="32" href="fixture" image-rendering="pixelated"/>
    </svg>)
}
struct Program;
impl fold_motion_fframes::Scripts for Program {
    fn render<'a>(
        &mut self,
        _: fold_foundation::ObjectId,
        input: fold_motion_fframes::Context<'a>,
    ) -> Result<Svgr<'a>, String> {
        let href = input.image("Photo")?;
        Ok(svgr!(<image href={href} width="32" height="32" preserveAspectRatio="xMidYMid meet"/>))
    }
}
fn worker_check(script: bool) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    use fold_motion_canvas::protocol::{ImageSnapshot, Operation, Raster, Request, Response};
    use std::io::{BufRead, Read, Write};
    let id = fold_foundation::ObjectId::new();
    let asset = fold_foundation::AssetId::new();
    let mut canvas = fold_motion_canvas::model::Canvas {
        roots: vec![id],
        objects: [(id, fold_motion_canvas::tools::image(asset, [32, 32])?)].into(),
        width: 32,
        height: 32,
        ..Default::default()
    };
    if script {
        use fold_motion_canvas::model::{Content, Control, ControlValue};
        canvas.objects.get_mut(&id).unwrap().content = Content::Script {
            source: "worker image fixture".into(),
            controls: [(
                fold_foundation::ObjectId::new(),
                Control {
                    name: "Photo".into(),
                    value: ControlValue::Image(asset),
                    protected: false,
                    extensions: Default::default(),
                },
            )]
            .into(),
            extensions: Default::default(),
        };
    }
    let mut observations = Vec::new();
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("--worker")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().ok_or("Missing input")?;
    let mut output = std::io::BufReader::new(child.stdout.take().ok_or("Missing output")?);
    let mut previous = Vec::new();
    for sequence in 1..=7 {
        let snapshot = match sequence {
            1 | 3 | 4 => {
                let data = if sequence == 1 {
                    images().remove("fixture").unwrap().data.to_vec()
                } else if sequence == 3 {
                    vec![255; 16]
                } else {
                    vec![255; 4]
                };
                Some(ImageSnapshot {
                    byte_limit: 16,
                    images: [(
                        asset.to_string(),
                        Raster {
                            identity: "fold-image-fixture".into(),
                            dimensions: [2, 2],
                            rgba8_premultiplied_srgb: data,
                        },
                    )]
                    .into(),
                })
            }
            _ => None,
        };
        let request = Request {
            id: sequence,
            operation: Operation::Render,
            canvas: &canvas,
            time: fold_foundation::Time::ZERO,
            dimensions: [32, 32],
            allocation_limit: 4 * 1024 * 1024,
            cache_limit: if sequence == 6 { 1 } else { 64 * 1024 * 1024 },
            fonts: if sequence == 1 {
                Some(Vec::<Vec<u8>>::new())
            } else {
                None
            },
            images: snapshot,
        };
        serde_json::to_writer(&mut input, &request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut header = String::new();
        output.read_line(&mut header)?;
        let header: Response = serde_json::from_str(&header)?;
        if header.id != sequence || header.bytes > 32 * 32 * 16 {
            return Err("Invalid worker framing".into());
        }
        let mut pixels = vec![0; header.bytes];
        output.read_exact(&mut pixels)?;
        if sequence == 4 {
            if header.error.is_none() || !pixels.is_empty() {
                return Err("Malformed worker resource was accepted".into());
            }
        } else if sequence == 6 {
            if !header
                .error
                .is_some_and(|error| error.contains("cache exceeds"))
                || !pixels.is_empty()
            {
                return Err("Worker accepted an exhausted GPU cache allowance".into());
            }
        } else {
            if let Some(error) = header.error {
                return Err(error.into());
            }
            if pixels.len() != 32 * 32 * 16 {
                return Err("Missing worker pixels".into());
            }
            if sequence == 1 {
                let offset = (4 * 32 + 4) * 16;
                let red = f32::from_le_bytes(pixels[offset..offset + 4].try_into()?);
                if red < 0.99 {
                    return Err("Native image object did not render the resolved red sample".into());
                }
            }
            if matches!(sequence, 2 | 5 | 7) && pixels != previous {
                return Err("Worker did not retain image snapshot".into());
            }
            if sequence == 3 && pixels == previous {
                return Err("Worker image replacement used stale pixels".into());
            }
            observations.push(pixels.clone());
            previous = pixels;
        }
    }
    if script {
        let request = Request {
            id: 8,
            operation: Operation::Capture { object: id },
            canvas: &canvas,
            time: fold_foundation::Time::ZERO,
            dimensions: [32, 32],
            allocation_limit: 4 * 1024 * 1024,
            cache_limit: 64 * 1024 * 1024,
            fonts: None::<Vec<Vec<u8>>>,
            images: None,
        };
        serde_json::to_writer(&mut input, &request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut header = String::new();
        output.read_line(&mut header)?;
        let header: fold_motion_canvas::protocol::CaptureResponse = serde_json::from_str(&header)?;
        if header.id != 8 || header.bytes > fold_motion_canvas::protocol::MAX_REQUEST_BYTES {
            return Err("Invalid capture response".into());
        }
        if let Some(error) = header.error {
            return Err(error.into());
        }
        let mut body = vec![0; header.bytes];
        output.read_exact(&mut body)?;
        let captured = serde_json::from_slice(&body)?;
        let after = fold_motion_canvas::editing::apply(
            &canvas,
            &[fold_motion_canvas::editing::Edit::Capture {
                id,
                expected: Box::new(canvas.objects[&id].clone()),
                value: Box::new(captured),
                time: fold_foundation::Time::ZERO,
            }],
        )?;
        if after.assets() != vec![asset] {
            return Err("Worker capture lost the project image".into());
        }
        let request = Request {
            id: 9,
            operation: Operation::Render,
            canvas: &after,
            time: fold_foundation::Time::ZERO,
            dimensions: [32, 32],
            allocation_limit: 4 * 1024 * 1024,
            cache_limit: 64 * 1024 * 1024,
            fonts: None::<Vec<Vec<u8>>>,
            images: None,
        };
        serde_json::to_writer(&mut input, &request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut header = String::new();
        output.read_line(&mut header)?;
        let header: Response = serde_json::from_str(&header)?;
        if header.id != 9 || header.bytes != previous.len() {
            return Err("Invalid captured frame".into());
        }
        if let Some(error) = header.error {
            return Err(error.into());
        }
        let mut pixels = vec![0; header.bytes];
        output.read_exact(&mut pixels)?;
        if pixels != previous {
            return Err("Worker capture changed image pixels".into());
        }
        let request = Request {
            id: 10,
            operation: Operation::Capture { object: id },
            canvas: &canvas,
            time: fold_foundation::Time::ZERO,
            dimensions: [32, 32],
            allocation_limit: 4 * 1024 * 1024,
            cache_limit: 64 * 1024 * 1024,
            fonts: None::<Vec<Vec<u8>>>,
            images: Some(ImageSnapshot {
                byte_limit: 0,
                images: Default::default(),
            }),
        };
        serde_json::to_writer(&mut input, &request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut header = String::new();
        output.read_line(&mut header)?;
        let header: fold_motion_canvas::protocol::CaptureResponse = serde_json::from_str(&header)?;
        if header.id != 10
            || header.bytes != 0
            || !header.error.is_some_and(|e| e.contains("Missing resolved"))
        {
            return Err("Capture silently dropped a missing image".into());
        }
    }
    drop(input);
    if !child.wait()?.success() {
        return Err("Worker failed".into());
    }
    Ok(observations)
}
fn main() -> Result<(), String> {
    if std::env::args().nth(1).as_deref() == Some("--worker") {
        return fold_motion_fframes::worker::run(Program);
    }
    let native = worker_check(false).map_err(|error| error.to_string())?;
    let script = worker_check(true).map_err(|error| error.to_string())?;
    if native != script {
        return Err("Native and script image pixels differ".into());
    }
    let mut conversion = Conversion::new([]);
    let retained = images();
    conversion.set_images(retained.clone(), 16)?;
    if conversion.set_images(retained.clone(), 16)? {
        return Err("Retained resource handles invalidated conversion".into());
    }
    if conversion.set_images(retained, 15).is_ok() {
        return Err("Retained resource handles bypassed a reduced allowance".into());
    }
    let tree = conversion.evaluate(artwork())?;
    let capture_error = capture::tree(&tree, &Images::new())
        .err()
        .ok_or("Image capture must reject undeclared resources")?;
    if !capture_error.contains("resource") {
        return Err(capture_error);
    }
    let device = fold_gpu_sharing::Device::first_uuid()?;
    let mut renderer = Renderer::new(device, [32, 32], [], 4 * 1024 * 1024, 64 * 1024 * 1024)?;
    renderer.set_images(images(), 16)?;
    renderer.draw(artwork())?;
    let first = renderer.readback()?;
    renderer.draw(artwork())?;
    let second = renderer.readback()?;
    if first != second {
        return Err("Retained image rendering changed pixels".into());
    }
    let samples = [
        first[4 * 32 + 4],
        first[4 * 32 + 28],
        first[28 * 32 + 4],
        first[28 * 32 + 28],
    ];
    if samples[0][0] < 0.99 || samples[1][1] < 0.49 || samples[2][2] < 0.99 || samples[3][3] != 0. {
        return Err(format!("Image color/alpha samples failed: {samples:?}"));
    }
    if renderer.set_images(images(), 15).is_ok() {
        return Err("Image byte allowance was ignored".into());
    }
    let malformed = std::sync::Arc::new(fold_motion_fframes::fframes::usvgr::PreloadedImageData {
        id: "malformed".into(),
        width: 2,
        height: 2,
        data: std::borrow::Cow::Owned(vec![255; 4]),
    });
    if renderer
        .set_images(Images::from([("fixture".into(), malformed)]), 16)
        .is_ok()
    {
        return Err("Malformed image storage was accepted".into());
    }
    renderer.draw(artwork())?;
    if renderer.readback()? != first {
        return Err("Rejected snapshot changed resources".into());
    }
    let replacement = std::sync::Arc::new(
        fold_motion_fframes::fframes::usvgr::PreloadedImageData::new(
            "fold-image-fixture".into(),
            2,
            2,
            &[255; 16],
        ),
    );
    let weak = std::sync::Arc::downgrade(&replacement);
    renderer.set_images(Images::from([("fixture".into(), replacement)]), 16)?;
    renderer.draw(artwork())?;
    if renderer.readback()? == first {
        return Err("Image replacement used stale pixels".into());
    }
    // Native pattern paint uses the same upstream image and SVG pattern path.
    let asset = fold_foundation::AssetId::new();
    let id = fold_foundation::ObjectId::new();
    let mut object = fold_motion_canvas::tools::new_object("rectangle")?;
    if let fold_motion_canvas::model::Content::Rectangle { width, height, .. } = &mut object.content
    {
        width.value = 32.;
        height.value = 32.;
    }
    object.paint.fill = Some(fold_motion_canvas::model::PaintSource::ImagePattern {
        asset,
        origin: [0.; 2],
        size: [16.; 2],
        transform: [1., 0., 0., 1., 0., 0.],
        extensions: Default::default(),
    });
    let mut canvas = fold_motion_canvas::model::Canvas {
        width: 32,
        height: 32,
        roots: vec![id],
        objects: [(id, object)].into(),
        ..Default::default()
    };
    let resources = Images::from([(asset.to_string(), images().remove("fixture").unwrap())]);
    renderer.set_images(resources.clone(), 16)?;
    renderer.draw(fold_motion_fframes::document(
        &canvas,
        fold_foundation::Time::ZERO,
        &mut Program,
    )?)?;
    let pattern = renderer.readback()?;
    for point in [[2, 2], [18, 2], [2, 18], [18, 18]] {
        if pattern[point[1] * 32 + point[0]][0] < 0.99 {
            return Err(format!(
                "Pattern tile repetition failed at {point:?}: {:?}",
                pattern[point[1] * 32 + point[0]]
            ));
        }
    }
    if pattern[14 * 32 + 14][3] != 0. {
        return Err("Pattern alpha failed".into());
    }
    if let Some(fold_motion_canvas::model::PaintSource::ImagePattern { origin, .. }) =
        &mut canvas.objects.get_mut(&id).unwrap().paint.fill
    {
        *origin = [8., 0.];
    }
    renderer.draw(fold_motion_fframes::document(
        &canvas,
        fold_foundation::Time::ZERO,
        &mut Program,
    )?)?;
    let shifted = renderer.readback()?;
    if shifted == pattern || shifted[2 * 32 + 10][0] < 0.99 {
        return Err("Pattern origin failed".into());
    }
    capture_roundtrip(&canvas, &resources, &mut renderer)?;
    for fit in [
        fold_motion_canvas::model::ImageFit::Contain,
        fold_motion_canvas::model::ImageFit::Cover,
        fold_motion_canvas::model::ImageFit::Stretch,
    ] {
        let mut object = fold_motion_canvas::tools::image(asset, [24, 16])?;
        if let fold_motion_canvas::model::Content::Image { fit: value, .. } = &mut object.content {
            *value = fit;
        }
        object.transform.x.value = 3.;
        object.transform.y.value = 5.;
        object.transform.rotation.value = 0.31;
        object.transform.scale_x.value = 0.8;
        object.transform.opacity.value = 0.7;
        canvas.objects.insert(id, object);
        capture_roundtrip(&canvas, &resources, &mut renderer)?;
    }
    renderer.set_images(Images::new(), 0)?;
    if weak.upgrade().is_some() {
        return Err("Removed image data is still retained".into());
    }
    println!(
        "{}",
        serde_json::json!({"dimensions":[32,32],"device_uuid":device,"samples":samples,"warm_pixels_identical":true,"undeclared_image_capture_rejected":true,"worker_image_capture_parity":true,"missing_capture_resource_rejected":true,"declared_image_capture_parity_all_fits":true,"image_pattern_capture_parity":true,"named_resources":true,"over_budget_snapshot_preserves_pixels":true,"replacement_changes_pixels":true,"removed_data_released":true,"worker_retention_replacement_and_error_recovery":true,"worker_cache_pressure_recovery":true,"script_image_control_matches_native_pixels":true,"retained_handle_allowance_checked":true,"native_pattern_repeat_origin_alpha":true})
    );
    Ok(())
}

fn capture_roundtrip(
    canvas: &fold_motion_canvas::model::Canvas,
    images: &Images,
    renderer: &mut Renderer,
) -> Result<(), String> {
    let mut conversion = Conversion::new([]);
    conversion.set_images(images.clone(), 16)?;
    let tree = conversion.evaluate(fold_motion_fframes::document(
        canvas,
        fold_foundation::Time::ZERO,
        &mut Program,
    )?)?;
    if capture::tree(&tree, &Images::new()).is_ok() {
        return Err("Image capture accepted an undeclared resource".into());
    }
    let captured = capture::tree(&tree, images)?;
    let mut after = canvas.clone();
    after.roots = captured.roots;
    after.objects = captured.objects;
    after.validate()?;
    if after.assets() != canvas.assets() {
        return Err("Captured resource identity changed".into());
    }
    renderer.draw(fold_motion_fframes::document(
        canvas,
        fold_foundation::Time::ZERO,
        &mut Program,
    )?)?;
    let before = renderer.readback()?;
    renderer.draw(fold_motion_fframes::document(
        &after,
        fold_foundation::Time::ZERO,
        &mut Program,
    )?)?;
    if renderer.readback()? != before {
        return Err("Captured image/pattern changed pixels".into());
    }
    Ok(())
}
