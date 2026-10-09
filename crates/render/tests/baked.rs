use fold_media::Cancel;
use fold_render::{
    ImageOp, RenderGraph,
    baked::{Format, Texture},
};
use std::sync::Arc;
fn source(color: [f32; 4]) -> fold_render::Frame {
    fold_render::render(RenderGraph {
        width: 16,
        height: 12,
        nodes: vec![ImageOp::Solid { rgba: color }],
        output: 0,
    })
    .unwrap()
}
fn texture(color: [f32; 4], format: Format) -> Arc<Texture> {
    Arc::new(Texture::encode(&source(color), format, &Cancel::default()).unwrap())
}
#[test]
fn cache_roundtrip_mips_alpha_and_hdr_are_explicit() {
    let cancel = Cancel::default();
    let t = texture([0.2, 0.1, 0.05, 0.5], Format::Bc7);
    assert_eq!(t.levels.len(), 5);
    assert_eq!(t.format, Format::Bc7);
    let decoded = t.decode(4, &cancel).unwrap();
    for (a, b) in decoded.pixels()[0].iter().zip([0.2, 0.1, 0.05, 0.5]) {
        assert!((a - b).abs() < 0.02);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.foldtex");
    t.save(&path, &cancel).unwrap();
    let mut changed = t.as_ref().clone();
    changed.levels[0].bytes[0] ^= 1;
    assert!(
        changed
            .save(&dir.path().join("changed.foldtex"), &cancel)
            .is_err()
    );
    let a = Texture::load(&path, &t.identity, &cancel).unwrap();
    let b = Texture::load(&path, &t.identity, &cancel).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(
        a.decode(0, &cancel).unwrap().pixels(),
        t.decode(0, &cancel).unwrap().pixels()
    );
    std::fs::write(&path, b"broken").unwrap();
    assert!(Texture::load(&path, &t.identity, &cancel).is_err());
    let hdr = fold_render::render(RenderGraph {
        width: 16,
        height: 12,
        nodes: vec![
            ImageOp::Solid {
                rgba: [0., 0., 0., 0.5],
            },
            ImageOp::ColorGrade {
                input: 0,
                settings: fold_render::operations::Grade {
                    offset: [2., -0.2, 0.1],
                    unpremultiply: false,
                    ..Default::default()
                },
            },
        ],
        output: 1,
    })
    .unwrap();
    assert!(Texture::encode(&hdr, Format::Bc7, &cancel).is_err());
    let f = Texture::encode(&hdr, Format::Auto, &cancel).unwrap();
    assert_eq!(f.format, Format::Float16);
    assert!((f.decode(0, &cancel).unwrap().pixels()[0][0] - 2.).abs() < 0.001);
    let cancelled = Cancel::default();
    cancelled.cancel();
    assert!(Texture::encode(&hdr, Format::Auto, &cancelled).is_err());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires Vulkan, including software Vulkan"]
fn baked_image_sampling_matches_cpu_at_multiple_sizes_and_reuses_uploads() {
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(64 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let cancel = Cancel::default();
    let original = fold_render::render(RenderGraph {
        width: 16,
        height: 12,
        nodes: vec![
            ImageOp::Solid {
                rgba: [0.2, 0.1, 0.05, 0.5],
            },
            ImageOp::Crop {
                input: 0,
                rect: [0, 0, 8, 12],
            },
        ],
        output: 1,
    })
    .unwrap();
    for format in [Format::Bc7, Format::Float16] {
        let texture = Arc::new(Texture::encode(&original, format, &cancel).unwrap());
        for (index, [width, height]) in [[16, 12], [37, 29], [7, 5], [1, 1]].into_iter().enumerate()
        {
            let graph = RenderGraph {
                width,
                height,
                nodes: vec![ImageOp::Baked(texture.clone())],
                output: 0,
            };
            let expected = fold_render::render(graph.clone()).unwrap();
            if index == 0 {
                let decoded = texture.decode(0, &cancel).unwrap();
                assert_eq!(expected.pixels(), decoded.pixels());
            }
            let mut gpu = renderer
                .evaluate(graph, None, &mut fold_media::Decoder::default(), &cancel)
                .unwrap();
            assert_eq!(gpu.statistics.readback_bytes, 0);
            assert_eq!(gpu.statistics.compute_passes, 1);
            if index > 0 {
                assert_eq!(
                    gpu.statistics.upload_bytes, 16,
                    "cached image must not upload again"
                );
            }
            let actual = gpu.readback(&cancel).unwrap();
            for (a, b) in actual
                .pixels()
                .iter()
                .flatten()
                .zip(expected.pixels().iter().flatten())
            {
                assert!(
                    (a - b).abs() < 0.005,
                    "{format:?} {width}x{height}: {a} vs {b}"
                );
            }
        }
    }
}
