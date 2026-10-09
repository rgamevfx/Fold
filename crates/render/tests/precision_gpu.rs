//! M4 scene storage boundary; spatial rendering qualification is separate.
#![cfg(feature = "gpu")]
use fold_render::{
    ImageOp, RenderGraph,
    frame::{Alpha, Component},
    gpu::{Host, Renderer},
};

fn graph(rgba: [f32; 4]) -> RenderGraph {
    RenderGraph {
        width: 19,
        height: 13,
        nodes: vec![ImageOp::Solid { rgba }],
        output: 0,
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn rgba16_preserves_scene_metadata_pixels_and_ownership() {
    let (host, info) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    eprintln!("M4 RGBA16F boundary: {info:?}");
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let time = fold_foundation::Time::new(7, 24).unwrap();
    let duration = fold_foundation::Time::new(1, 24).unwrap();
    for rgba in [[0.; 4], [0.1, 0.3, 0.6, 0.7], [0.00001, 0.01, 0.1, 1.]] {
        let frame = renderer
            .evaluate(graph(rgba), None, &mut decoder, &cancel)
            .unwrap()
            .with_timing(time, duration)
            .unwrap();
        let before = frame.descriptor().unwrap();
        assert_eq!(before.planes[0].component, Component::Float32);
        let mut half = renderer.rgba16(frame, &cancel).unwrap();
        let after = half.descriptor().unwrap();
        assert_eq!(after.planes[0].component, Component::Float16);
        assert_eq!(after.planes[0].row_stride, 19 * 8);
        assert_eq!(after.alpha, Alpha::Premultiplied);
        assert_eq!(after.color, before.color);
        assert_eq!(after.timing, before.timing);
        assert_ne!(after.storage, before.storage);
        assert_eq!(half.statistics.readback_bytes, 0);
        assert_eq!(half.statistics.cpu_adapter_nodes, 0);
        let passes = half.statistics.compute_passes;
        half = renderer.rgba16(half, &cancel).unwrap();
        assert_eq!(half.statistics.compute_passes, passes);
        assert_eq!(half.descriptor().unwrap().storage, after.storage);
        let actual = half.readback(&cancel).unwrap();
        let expected = rgba.map(|v| half::f16::from_f32(v).to_f32());
        for pixel in actual.pixels() {
            // Storage conversion may round toward zero; allow one half ULP
            // including the fixed spacing of subnormal values.
            for (a, b) in pixel.iter().zip(expected) {
                assert!(
                    (a - b).abs() <= (b.abs() / 1024.).max(6e-8),
                    "{pixel:?} != {expected:?}"
                );
            }
        }
        assert_eq!(half.statistics.readback_bytes, 256 * 13);
        let cpu_display = actual.over_black().to_display().unwrap();
        let gpu_display = renderer
            .output(&half, None, &cancel)
            .unwrap()
            .readback(&cancel)
            .unwrap();
        for (a, b) in cpu_display.rgba().iter().zip(gpu_display.rgba()) {
            assert!(a.abs_diff(*b) <= 1);
        }
    }
    let (other, _) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    let mut other = Renderer::new(other).unwrap();
    let frame = renderer
        .evaluate(graph([0.1; 4]), None, &mut decoder, &cancel)
        .unwrap();
    assert!(
        other
            .rgba16(frame, &cancel)
            .err()
            .unwrap()
            .message
            .contains("different host")
    );
    let frame = renderer
        .evaluate(graph([0.1; 4]), None, &mut decoder, &cancel)
        .unwrap();
    let cancelled = fold_media::Cancel::default();
    cancelled.cancel();
    assert!(renderer.rgba16(frame, &cancelled).is_err());

    // Dropping the renderer does not invalidate a frame still awaiting completion.
    let frame = renderer
        .evaluate(graph([0.25; 4]), None, &mut decoder, &cancel)
        .unwrap();
    let mut half = renderer.rgba16(frame, &cancel).unwrap();
    drop(renderer);
    assert_eq!(half.readback(&cancel).unwrap().pixels()[0], [0.25; 4]);
    host.device().destroy();
    let _ = host.poll();
    assert!(half.is_ready().is_err());
}
