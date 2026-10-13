//! Focused native qualification: upstream artwork versus editable capture.
use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::{model::Canvas, scripts::Context};
use fold_motion_fframes::{
    Conversion, Renderer, Scripts, capture, document,
    fframes::{Svgr, svgr},
};

struct NoScripts;
impl Scripts for NoScripts {
    fn render<'a>(&mut self, _: ObjectId, _: Context<'a>) -> Result<Svgr<'a>, String> {
        Err("Captured artwork must contain no script objects".into())
    }
}
fn artwork() -> Svgr<'static> {
    artwork_named("Editable")
}
fn artwork_named(label: &'static str) -> Svgr<'static> {
    svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360" viewBox="0 0 640 360">
        <defs>
            <linearGradient id="linear" gradientUnits="userSpaceOnUse" x1="400" y1="120" x2="490" y2="160" gradientTransform="matrix(1 0.1 0.2 1 -20 -30)" spreadMethod="reflect">
                <stop offset="0" stop-color="#ff0044" stop-opacity="0.3"/><stop offset="0.4" stop-color="#44ff99"/><stop offset="1" stop-color="#2244ff" stop-opacity="0.8"/>
            </linearGradient>
            <linearGradient id="bounds"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient>
            <radialGradient id="radial" gradientUnits="userSpaceOnUse" cx="200" cy="300" r="35" fx="190" fy="290" gradientTransform="matrix(1 0.1 0.2 1 -50 -20)" spreadMethod="repeat">
                <stop offset="0" stop-color="white" stop-opacity="0.8"/><stop offset="0.4" stop-color="#9933ff"/><stop offset="1" stop-color="#2244ff" stop-opacity="0.3"/>
            </radialGradient>
            <radialGradient id="radialBounds" fx="0.3" fy="0.4"><stop offset="0" stop-color="white"/><stop offset="1" stop-color="#005577"/></radialGradient>
        </defs>
        <rect x="400" y="130" width="190" height="70" fill="url(#linear)" stroke="url(#linear)" stroke-width="5" stroke-opacity="0.5"/>
        <rect x="10" y="290" width="110" height="50" fill="url(#bounds)"/>
        <ellipse cx="200" cy="300" rx="50" ry="40" fill="url(#radial)" stroke="url(#radial)" stroke-width="4" stroke-opacity="0.7"/>
        <rect x="510" y="250" width="90" height="80" fill="url(#radialBounds)"/>
        <g transform="matrix(1 0.15 0.3 1 40 20)" opacity="0.5">
            <path d="M0 0 L100 0 L100 80 L0 80 Z" fill="#ff3300"/>
            <path d="M50 0 L150 0 L150 80 L50 80 Z" fill="#0066ff"/>
        </g>
        <g transform="translate(370 30) scale(-1 1.3)">
            <path d="M0 0 Q40 80 80 0 C100 20 100 80 0 0 Z" fill="none" stroke="#00cc77" stroke-width="3" stroke-linecap="round" stroke-linejoin="bevel"/>
        </g>
        <path d="M40 160 L160 160 L160 280 L40 280 Z M70 190 L130 190 L130 250 L70 250 Z" fill="#9933ff" fill-rule="evenodd"/>
        <text x="200" y="210" font-family="Noto Sans" font-size="40" fill="#ffffff">{label}</text>
        <ellipse cx="450" cy="260" rx="50" ry="30" fill="#ffaa33"/>
        <rect x="510" y="40" width="70" height="80" rx="12" fill="#33aaff"/>
        <path d="M280 330 L300 270 L310 330" fill="none" stroke="#ff5577" stroke-width="8" stroke-linejoin="miter" stroke-miterlimit="10"/>
        <path d="M350 330 L370 270 L380 330" fill="none" stroke="#55ff77" stroke-width="8" stroke-linejoin="miter" stroke-miterlimit="1"/>
    </svg>)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/fonts/NotoSans-Regular.ttf"
    ))?;
    let mut conversion = Conversion::new([font.clone()]);
    let start = std::time::Instant::now();
    let evaluated = conversion.evaluate(artwork())?;
    let cold_conversion_ms = start.elapsed().as_secs_f64() * 1000.;
    let mut warm_conversion_ms = vec![];
    for _ in 0..20 {
        let start = std::time::Instant::now();
        drop(conversion.evaluate(artwork())?);
        warm_conversion_ms.push(start.elapsed().as_secs_f64() * 1000.);
    }
    warm_conversion_ms.sort_by(f64::total_cmp);
    let captured = capture::tree(&evaluated, &Default::default())?;
    let canvas = Canvas {
        width: 640,
        height: 360,
        roots: captured.roots,
        objects: captured.objects,
        ..Default::default()
    };
    canvas.validate()?;
    assert!(canvas.objects.values().any(|object| matches!(
        object.paint.fill,
        Some(fold_motion_canvas::model::PaintSource::Radial { .. })
    )));
    assert!(canvas.objects.values().any(|object| matches!(
        object.paint.stroke,
        Some(fold_motion_canvas::model::PaintSource::Radial { .. })
    )));
    assert!(canvas.objects.values().any(|object| matches!(
        object.paint.fill,
        Some(fold_motion_canvas::model::PaintSource::Linear { .. })
    )));
    assert!(canvas.objects.values().any(|object| matches!(
        object.paint.stroke,
        Some(fold_motion_canvas::model::PaintSource::Linear { .. })
    )));
    assert!(
        canvas
            .objects
            .values()
            .any(|object| object.paint.miter_limit == 10.)
    );
    assert!(
        canvas
            .objects
            .values()
            .any(|object| object.paint.stroke.is_some() && object.paint.miter_limit == 1.)
    );
    let uuid = fold_gpu_sharing::Device::first_uuid()?;
    let mut renderer = Renderer::new(
        uuid,
        [640, 360],
        [font],
        256 * 1024 * 1024,
        64 * 1024 * 1024,
    )?;
    renderer.draw(artwork())?;
    let original = renderer.readback()?;
    renderer.draw(artwork())?;
    let warm_original = renderer.readback()?;
    assert_eq!(
        original, warm_original,
        "Retained text cache changed pixels"
    );
    renderer.draw(artwork_named("Changed"))?;
    assert_ne!(
        original,
        renderer.readback()?,
        "Text edit used stale outlines"
    );
    renderer.draw(artwork())?;
    assert_eq!(
        original,
        renderer.readback()?,
        "Restored text changed pixels"
    );
    renderer.draw(document(&canvas, Time::ZERO, &mut NoScripts)?)?;
    let editable = renderer.readback()?;
    let max_error = original
        .iter()
        .flatten()
        .zip(editable.iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    let differing_pixels = original
        .iter()
        .zip(&editable)
        .filter(|(a, b)| a != b)
        .count();
    let unsupported = conversion.evaluate(svgr!(<svg width="640" height="360">
        <defs><pattern id="pattern" patternUnits="userSpaceOnUse" width="10" height="10"><rect width="5" height="5" fill="red"/></pattern></defs>
        <rect width="100" height="100" fill="url(#pattern)"/>
    </svg>))?;
    assert!(capture::tree(&unsupported, &Default::default()).is_err());
    let report = serde_json::json!({"objects":canvas.objects.len(),"max_float_error":max_error,"differing_pixels":differing_pixels,"linear_gradients_captured":true,"radial_gradients_captured":true,"pattern_rejected":true,"qualification_readback":true,"device_uuid":uuid,"dimensions":[640,360],"schema":fold_motion_canvas::SCHEMA,
        "text_cache_capacity":fframes_skia_renderer::SkiaCacheConfig::default().text_capacity,
        "cold_conversion_ms":cold_conversion_ms,"warm_conversion_p50_ms":warm_conversion_ms[10],"warm_text_cache_pixels_identical":true,
        "text_edit_changes_pixels":true,"restored_text_pixels_identical":true});
    println!("{report}");
    if let Some(directory) = std::env::args().nth(1) {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(
            directory.join("capture-evidence.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        std::fs::write(
            directory.join("captured-canvas.json"),
            serde_json::to_vec_pretty(&canvas)?,
        )?;
        std::fs::write(
            directory.join("original.rgba32f"),
            bytemuck::cast_slice(&original),
        )?;
        std::fs::write(
            directory.join("editable.rgba32f"),
            bytemuck::cast_slice(&editable),
        )?;
    }
    if max_error > 0.002 {
        return Err("Editable capture changed rendered artwork beyond tolerance".into());
    }
    Ok(())
}
