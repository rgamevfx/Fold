use fold_media::Cancel;
use fold_render::{
    ImageOp, RenderGraph,
    baked::{Batch, Format, Texture},
    textured::{Draw, Instance, Mesh, Vertex},
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
fn batch() -> Batch {
    let mesh = Arc::new(Mesh {
        vertices: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]
            .map(|uv| Vertex {
                position: [uv[0] * 8., uv[1] * 8.],
                uv,
                sample: uv,
                center: [4.; 2],
            })
            .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
    });
    let mut draw = Draw::new(mesh);
    draw.instances = (0..10)
        .map(|i| Instance {
            matrix: [1., 0., 0., 1.],
            offset: [(i % 5) as f32 * 10., (i / 5) as f32 * 10., 1., i as f32],
        })
        .collect();
    Batch {
        textures: (0..10)
            .map(|i| texture([i as f32 / 10., 0.25, 0.125, 1.], Format::Bc7))
            .collect(),
        draw,
    }
}
#[test]
fn batch_layers_and_graph_rebasing_are_bounded() {
    let b = batch();
    b.validate().unwrap();
    let mut bad = b.clone();
    bad.draw.instances[0].offset[3] = 20.;
    assert!(bad.validate().is_err());
    let g = RenderGraph {
        width: 50,
        height: 20,
        nodes: vec![ImageOp::BakedTextured {
            batch: Arc::new(b),
            driver: None,
        }],
        output: 0,
    };
    let f = fold_render::render(g.clone()).unwrap();
    assert_ne!(f.pixels()[0], f.pixels()[10]);
    let mut nested = g.clone();
    nested.output = nested.append(g).unwrap();
    assert!(fold_render::render(nested).is_ok());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native Vulkan"]
fn native_mipmapped_array_matches_cpu_and_reuses_resident_bytes() {
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(64 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = Cancel::default();
    let graph = RenderGraph {
        width: 50,
        height: 20,
        nodes: vec![ImageOp::BakedTextured {
            batch: Arc::new(batch()),
            driver: None,
        }],
        output: 0,
    };
    let cpu = fold_render::render(graph.clone()).unwrap();
    for warm in [false, true] {
        let mut gpu = renderer
            .evaluate(graph.clone(), None, &mut decoder, &cancel)
            .unwrap();
        assert_eq!(gpu.statistics.readback_bytes, 0);
        assert_eq!(gpu.statistics.compute_passes, 2);
        if warm {
            assert_eq!(gpu.statistics.upload_bytes, 96 + 10 * 32);
        }
        let actual = gpu.readback(&cancel).unwrap();
        let max = actual
            .pixels()
            .iter()
            .flatten()
            .zip(cpu.pixels().iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(max < 0.005, "{max}");
    }
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native Vulkan"]
fn worker_encoder_preserves_color_alpha_and_float_textures() {
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(64 * 1024 * 1024)).unwrap();
    let mut encoder = fold_render::gpu::BakeEncoder::new(host.clone()).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let cancel = Cancel::default();
    for (format, alpha) in [
        (Format::Bc7, 1.),
        (Format::Bc7, 0.5),
        (Format::Float16, 0.5),
    ] {
        let original = source([0.2, 0.1, 0.05, alpha]);
        let baked = Arc::new(encoder.encode(&original, format, &cancel).unwrap());
        if alpha == 1. {
            assert!(
                baked
                    .decode(0, &cancel)
                    .unwrap()
                    .pixels()
                    .iter()
                    .all(|p| p[3] == 1.)
            );
        }
        let graph = RenderGraph {
            width: 16,
            height: 12,
            nodes: vec![ImageOp::Baked(baked)],
            output: 0,
        };
        let cpu = fold_render::render(graph.clone()).unwrap();
        let mut gpu = renderer
            .evaluate(graph, None, &mut fold_media::Decoder::default(), &cancel)
            .unwrap();
        let actual = gpu.readback(&cancel).unwrap();
        for (a, b) in actual
            .pixels()
            .iter()
            .flatten()
            .zip(cpu.pixels().iter().flatten())
        {
            assert!((a - b).abs() < 0.005);
        }
        for (a, b) in cpu
            .pixels()
            .iter()
            .flatten()
            .zip(original.pixels().iter().flatten())
        {
            assert!((a - b).abs() < 0.02);
        }
    }
}
