//! Absolute color qualification through the actual upstream SVG/Skia path.
use fframes_skia_renderer::{render, skia_safe as skia};
use fold_motion_fframes::{
    Conversion, Renderer,
    fframes::{Svgr, svgr},
};

fn artwork() -> Svgr<'static> {
    svgr!(<svg width="96" height="32" viewBox="0 0 96 32">
        <rect x="0" y="0" width="32" height="32" fill="#3399ff"/>
        <ellipse cx="48" cy="16" rx="15" ry="15" fill="#3399ff"/>
        <path d="M64 0H96V32H64Z" fill="#3399ff"/>
    </svg>)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tree = Conversion::new([]).evaluate(artwork())?;
    let info = skia::ImageInfo::new(
        (96, 32),
        skia::ColorType::RGBAF16,
        skia::AlphaType::Premul,
        skia::ColorSpace::new_srgb_linear(),
    );
    let mut surface = skia::surfaces::raster(&info, None, None).ok_or("CPU probe surface")?;
    render::render_tree(&tree, surface.canvas(), &mut render::RenderCache::new());
    let info = skia::ImageInfo::new(
        (96, 32),
        skia::ColorType::RGBAF32,
        skia::AlphaType::Premul,
        skia::ColorSpace::new_srgb_linear(),
    );
    let mut cpu = vec![[0_f32; 4]; 96 * 32];
    assert!(surface.read_pixels(&info, bytemuck::cast_slice_mut(&mut cpu), 96 * 16, (0, 0)));
    let mut renderer = Renderer::new(
        fold_gpu_sharing::Device::first_uuid()?,
        [96, 32],
        [],
        1024 * 1024,
        64 * 1024 * 1024,
    )?;
    renderer.draw(artwork())?;
    let gpu = renderer.readback()?;
    let expected = [0.033104767_f32, 0.31854678, 1., 1.];
    let samples =
        [16, 48, 80].map(|x| serde_json::json!({"x":x,"cpu":cpu[16*96+x],"gpu":gpu[16*96+x]}));
    println!(
        "{}",
        serde_json::json!({"expected":expected,"samples":samples})
    );
    for x in [16, 48, 80] {
        for (actual, expected) in gpu[16 * 96 + x].iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.00025,
                "x={x}: {actual} != {expected}"
            );
        }
    }
    // Shader output is already in the destination's linear working space.
    // Signed and over-range premultiplied values must survive the float target.
    let shader = fold_motion_fframes::fframes::Shader::sksl(
        "half4 main(float2 p) { return half4(-0.25, 2.0, 0.125, 0.5); }",
    );
    render::compile_shader(&shader)?;
    let layer = shader.draw(
        &fold_motion_fframes::fframes::Frame::new(0, 0, 30),
        Default::default(),
    );
    let href = layer.href();
    renderer.draw(svgr!(<svg width="96" height="32" viewBox="0 0 96 32">
        <image href={href} width="96" height="32"/>
    </svg>))?;
    let hdr = renderer.readback()?[16 * 96 + 48];
    println!("{}", serde_json::json!({"signed_hdr_premultiplied":hdr}));
    assert_eq!(hdr, [-0.25, 2., 0.125, 0.5]);
    Ok(())
}
