//! Compare animated native paths with independently specified static poses.
use fold_foundation::{ObjectId, Time};
use fold_graph::{
    binding::{Interpolation, Key, Track},
    types::{Literal, Space, Unit},
};
use fold_motion_canvas::model::{Canvas, Content, Contour, Object, Point};
use fold_motion_fframes::{Context, Renderer, Scripts, document, fframes::Svgr};
struct NoScripts;
impl Scripts for NoScripts {
    fn render<'a>(&mut self, _: ObjectId, _: Context<'a>) -> Result<Svgr<'a>, String> {
        Err("Native anchor fixture must not evaluate scripts".into())
    }
}
// Diagnostic only: isolates GPU coverage from the shared upstream path conversion.
fn cpu_probe(canvas: &Canvas, scale: usize) -> Result<Vec<[f32; 4]>, String> {
    use fframes_skia_renderer::{render, skia_safe as skia};
    let tree = fold_motion_fframes::Conversion::new([]).evaluate(document(
        canvas,
        Time::ZERO,
        &mut NoScripts,
    )?)?;
    let info = skia::ImageInfo::new(
        ((96 * scale) as i32, (64 * scale) as i32),
        skia::ColorType::RGBAF16,
        skia::AlphaType::Premul,
        skia::ColorSpace::new_srgb_linear(),
    );
    let mut surface =
        skia::surfaces::raster(&info, None, None).ok_or("CPU probe surface failed")?;
    surface.canvas().clear(skia::Color::TRANSPARENT);
    surface.canvas().scale((scale as f32, scale as f32));
    render::render_tree(&tree, surface.canvas(), &mut render::RenderCache::new());
    let info = skia::ImageInfo::new(
        ((96 * scale) as i32, (64 * scale) as i32),
        skia::ColorType::RGBAF32,
        skia::AlphaType::Premul,
        skia::ColorSpace::new_srgb_linear(),
    );
    let mut pixels = vec![[0_f32; 4]; 96 * 64 * scale * scale];
    if !surface.read_pixels(
        &info,
        bytemuck::cast_slice_mut(&mut pixels),
        96 * scale * 16,
        (0, 0),
    ) {
        return Err("CPU probe readback failed".into());
    }
    if scale == 1 {
        return Ok(pixels);
    }
    let mut reduced = vec![[0_f32; 4]; 96 * 64];
    for y in 0..64 {
        for x in 0..96 {
            for axis in 0..4 {
                let mut sum = 0_f64;
                for sy in 0..scale {
                    for sx in 0..scale {
                        sum += f64::from(
                            pixels[((y * scale + sy) * 96 * scale) + x * scale + sx][axis],
                        );
                    }
                }
                reduced[y * 96 + x][axis] = (sum / (scale * scale) as f64) as f32;
            }
        }
    }
    Ok(reduced)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args()
        .nth(1)
        .ok_or("Provide an evidence directory")?;
    let destination = std::path::PathBuf::from(destination);
    let closed = std::env::args().nth(2).as_deref() == Some("--closed");
    std::fs::create_dir_all(&destination)?;
    let vector = |p: [f64; 2]| Literal::Vector {
        value: p.to_vec(),
        unit: Unit::Canvas,
        space: Space::Screen,
    };
    let mut track = Track::constant(vector([10., 10.]), Time::ZERO)?;
    track.keys.push(Key {
        id: ObjectId::new(),
        time: Time::new(1, 1)?,
        value: vector([30., 20.]),
        interpolation: Interpolation::Linear,
        handles: vec![],
        extensions: Default::default(),
    });
    let mut outgoing = Track::constant(vector([15., -7.]), Time::ZERO)?;
    outgoing.keys.push(Key {
        id: ObjectId::new(),
        time: Time::new(3, 2)?,
        value: vector([-3., 29.]),
        interpolation: Interpolation::Linear,
        handles: vec![],
        extensions: Default::default(),
    });
    let mut incoming = Track::constant(vector([-5., 8.]), Time::new(1, 2)?)?;
    incoming.keys.push(Key {
        id: ObjectId::new(),
        time: Time::new(1, 1)?,
        value: vector([-20., -12.]),
        interpolation: Interpolation::Linear,
        handles: vec![],
        extensions: Default::default(),
    });
    let point = |position| Point {
        id: ObjectId::new(),
        position,
        position_track: None,
        incoming_track: None,
        outgoing_track: None,
        incoming: [-5., 8.],
        outgoing: [15., -7.],
        extensions: Default::default(),
    };
    let mut a = point([0., 0.]);
    a.position_track = Some(track);
    a.outgoing_track = Some(outgoing);
    let mut b = point([65., 35.]);
    b.incoming_track = Some(incoming);
    let mut object = Object::new(
        "Animated cubic",
        Content::Path {
            contours: vec![Contour {
                id: ObjectId::new(),
                points: vec![a, b],
                closed,
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        },
    );
    object.paint.fill = None;
    object.paint.stroke = Some([0.2, 0.9, 0.5, 0.7].into());
    object.paint.stroke_width.value = 5.;
    object.transform.shear.value = 0.25;
    object.transform.x.value = 3.;
    let id = ObjectId::new();
    let canvas = Canvas {
        width: 96,
        height: 64,
        roots: vec![id],
        objects: [(id, object)].into(),
        ..Default::default()
    };
    let Content::Path { contours, .. } = &canvas.objects[&id].content else {
        unreachable!()
    };
    let command = fold_motion_canvas::editing::Edit::SplitSegment {
        object: id,
        expected: Box::new(canvas.objects[&id].clone()),
        contour: contours[0].id,
        start: contours[0].points[0].id,
        fraction: 0.35,
        time: Time::ZERO,
    };
    let subdivided = fold_motion_canvas::editing::apply(&canvas, &[command])?;
    let cpu_original = cpu_probe(&canvas, 1)?;
    let cpu_split = cpu_probe(&subdivided, 1)?;
    let cpu_changed = cpu_original
        .iter()
        .zip(&cpu_split)
        .filter(|(a, b)| a != b)
        .count();
    let cpu_max = cpu_original
        .iter()
        .zip(&cpu_split)
        .flat_map(|(a, b)| a.iter().zip(b))
        .map(|(a, b)| (a - b).abs())
        .fold(0_f32, f32::max);
    let cpu_evidence = serde_json::json!({"time":Time::ZERO,"changed_pixels":cpu_changed,"max_component_error":cpu_max,"scope":"diagnostic Skia CPU raster of the same upstream trees"});
    println!(
        "CPU subdivision: {cpu_changed} changed pixels, maximum Float component error {cpu_max}"
    );
    let reference8 = cpu_probe(&canvas, 8)?;
    let reference16 = cpu_probe(&canvas, 16)?;
    let split_reference16 = cpu_probe(&subdivided, 16)?;
    let errors = |a: &[[f32; 4]], b: &[[f32; 4]]| {
        let differences = a
            .iter()
            .zip(b)
            .flat_map(|(a, b)| a.iter().zip(b))
            .map(|(a, b)| f64::from((a - b).abs()))
            .collect::<Vec<_>>();
        serde_json::json!({"max_component_error":differences.iter().copied().fold(0_f64, f64::max),
            "sum_absolute_component_error":differences.iter().sum::<f64>()})
    };
    let mut quality = serde_json::json!({"scope":"first-pose linear premultiplied Float RGBA; box-reduced Skia CPU reference, not analytic ground truth",
        "reference8_vs_reference16":errors(&reference8, &reference16),
        "reference16_vs_split_reference16":errors(&reference16, &split_reference16),
        "cpu_vs_reference16":errors(&cpu_original, &reference16),
        "cpu_split_vs_reference16":errors(&cpu_split, &reference16)});
    std::fs::write(
        destination.join("reference16.rgba32f"),
        reference16
            .iter()
            .flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes()))
            .collect::<Vec<_>>(),
    )?;
    let uuid = fold_gpu_sharing::Device::first_uuid()?;
    let mut renderer = Renderer::new(uuid, [96, 64], [], 32 * 1024 * 1024, 64 * 1024 * 1024)?;
    let mut frames = vec![];
    let mut first = None;
    let mut previous = None;
    for (index, (time, expected, outgoing, incoming)) in [
        (Time::ZERO, [10., 10.], [15., -7.], [-5., 8.]),
        (Time::new(1, 2)?, [20., 15.], [9., 5.], [-5., 8.]),
        (Time::new(1, 1)?, [30., 20.], [3., 17.], [-20., -12.]),
        (Time::new(2, 1)?, [30., 20.], [-3., 29.], [-20., -12.]),
        (Time::ZERO, [10., 10.], [15., -7.], [-5., 8.]),
    ]
    .into_iter()
    .enumerate()
    {
        renderer.draw(document(&canvas, time, &mut NoScripts)?)?;
        let animated = renderer.readback()?;
        let mut posed = canvas.clone();
        let Content::Path { contours, .. } = &mut posed.objects.get_mut(&id).unwrap().content
        else {
            unreachable!()
        };
        contours[0].points[0].position = expected;
        contours[0].points[0].position_track = None;
        contours[0].points[0].outgoing = outgoing;
        contours[0].points[0].outgoing_track = None;
        contours[0].points[1].incoming = incoming;
        contours[0].points[1].incoming_track = None;
        renderer.draw(document(&posed, time, &mut NoScripts)?)?;
        let static_pose = renderer.readback()?;
        assert_eq!(
            animated, static_pose,
            "Native animated pixels differ from specified static pose at {time:?}"
        );
        renderer.draw(document(&canvas, time, &mut NoScripts)?)?;
        assert_eq!(animated, renderer.readback()?, "Warm output changed");
        assert!(animated.iter().any(|p| p[3] > 0.));
        if index == 0 {
            first = Some(animated.clone());
        }
        if (1..=3).contains(&index) {
            assert_ne!(previous.as_ref().unwrap(), &animated);
        }
        if index == 4 {
            assert_eq!(first.as_ref().unwrap(), &animated);
        }
        let bytes = animated
            .iter()
            .flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes()))
            .collect::<Vec<_>>();
        std::fs::write(destination.join(format!("frame-{index}.rgba32f")), bytes)?;
        renderer.draw(document(&subdivided, time, &mut NoScripts)?)?;
        let split = renderer.readback()?;
        if index == 0 {
            quality["gpu_vs_reference16"] = errors(&animated, &reference16);
            quality["gpu_split_vs_reference16"] = errors(&split, &reference16);
            let mut fresh = Renderer::new(uuid, [96, 64], [], 32 * 1024 * 1024, 64 * 1024 * 1024)?;
            fresh.draw(document(&subdivided, time, &mut NoScripts)?)?;
            assert_eq!(
                split,
                fresh.readback()?,
                "Subdivision depends on retained renderer state"
            );
        }
        renderer.draw(document(&subdivided, time, &mut NoScripts)?)?;
        assert_eq!(
            split,
            renderer.readback()?,
            "Subdivided warm output changed"
        );
        let changed_pixels = animated.iter().zip(&split).filter(|(a, b)| a != b).count();
        let max_error = animated
            .iter()
            .zip(&split)
            .flat_map(|(a, b)| a.iter().zip(b))
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        let absolute_error: f64 = animated
            .iter()
            .zip(&split)
            .flat_map(|(a, b)| a.iter().zip(b))
            .map(|(a, b)| f64::from((a - b).abs()))
            .sum();
        let bytes = split
            .iter()
            .flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes()))
            .collect::<Vec<_>>();
        std::fs::write(destination.join(format!("split-{index}.rgba32f")), bytes)?;
        println!(
            "Subdivision {time:?}: {changed_pixels} changed pixels, maximum Float component error {max_error}"
        );
        frames.push(serde_json::json!({"time":time,"expected_anchor":expected,"expected_outgoing":outgoing,"expected_incoming":incoming,"exact_static_pose_parity":true,"exact_warm_parity":true,
            "subdivision":{"changed_pixels":changed_pixels,"max_component_error":max_error,"sum_absolute_component_error":absolute_error,"exact_warm_parity":true}}));
        previous = Some(animated);
    }
    std::fs::write(
        destination.join("evidence.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"dimensions":[96,64],"fixture":"native animated anchor and both handle offsets; distinct key schedules; sheared transform; translucent stroke","device_uuid":uuid,"cpu_probe":cpu_evidence,"quality_probe":quality,"frames":frames}),
        )?,
    )?;
    println!("Five native anchor and handle poses passed exact Float RGBA and warm-cache parity");
    Ok(())
}
