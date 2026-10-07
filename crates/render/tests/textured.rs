#[cfg(feature = "gpu")]
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
use fold_render::{
    ImageOp, RenderGraph,
    textured::{Draw, Mesh, Vertex},
};
use std::sync::Arc;
fn graph() -> RenderGraph {
    let mesh = Mesh {
        vertices: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]
            .map(|uv| Vertex {
                position: [uv[0] * 16., uv[1] * 16.],
                uv,
                sample: uv,
                center: [8.; 2],
            })
            .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
    };
    let mut draw = Draw::new(Arc::new(mesh));
    draw.opacity = 0.5;
    RenderGraph {
        width: 16,
        height: 16,
        nodes: vec![
            ImageOp::Solid {
                rgba: [0.5, 0.25, 0.125, 0.5],
            },
            ImageOp::Textured {
                input: 0,
                driver: 0,
                draw: Arc::new(draw),
            },
        ],
        output: 1,
    }
}
#[test]
fn quad_uv_coverage_and_premultiplication() {
    let result = fold_render::render(graph()).unwrap();
    for pixel in result.pixels() {
        assert_eq!(*pixel, [0.25, 0.125, 0.0625, 0.25]);
    }
    let mut invalid = graph();
    if let ImageOp::Textured { draw, .. } = &mut invalid.nodes[1] {
        Arc::make_mut(&mut Arc::make_mut(draw).mesh).indices[0] = 999;
    }
    assert!(fold_render::render(invalid).is_err());
}
#[test]
fn append_offsets_texture_and_driver_dependencies() {
    let mut a = graph();
    let id = a.append(graph()).unwrap();
    a.output = id;
    assert_eq!(id, 3);
    assert!(matches!(
        a.nodes[3],
        ImageOp::Textured {
            input: 2,
            driver: 2,
            ..
        }
    ));
    assert!(fold_render::render(a).is_ok());
}
#[test]
fn sampling_unpremultiplies_luminance_but_preserves_alpha() {
    use fold_render::textured::{Channel, scalar};
    assert!((scalar([0.5, 0.5, 0.5, 0.5], Channel::Luminance, true) - 1.).abs() < 1e-6);
    assert_eq!(scalar([0.5, 0.5, 0.5, 0.5], Channel::Alpha, true), 0.5);
    assert_eq!(scalar([0.; 4], Channel::Luminance, true), 0.);
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native Vulkan"]
fn native_textures_match_cpu_and_retain_meshes() {
    let _guard = GPU_LOCK.lock().unwrap();
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(32 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host.clone()).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let graph = graph();
    let cpu = fold_render::render(graph.clone()).unwrap();
    for i in 0..2 {
        let mut frame = renderer
            .evaluate(graph.clone(), None, &mut decoder, &cancel)
            .unwrap();
        if i == 1 {
            assert_eq!(frame.statistics.upload_bytes, 128);
        }
        assert_eq!(frame.statistics.readback_bytes, 0);
        let actual = frame.readback(&cancel).unwrap();
        for (a, b) in actual
            .pixels()
            .iter()
            .flatten()
            .zip(cpu.pixels().iter().flatten())
        {
            assert!((a - b).abs() < 0.001, "{a} != {b}");
        }
    }
    let mut oversized = graph.clone();
    oversized.width = 2048;
    oversized.height = 2048;
    assert!(
        renderer
            .evaluate(oversized, None, &mut decoder, &cancel)
            .is_err()
    );
    let mut recovered = renderer
        .evaluate(graph.clone(), None, &mut decoder, &cancel)
        .unwrap();
    recovered.readback(&cancel).unwrap();
    let mut invalid = graph.clone();
    if let ImageOp::Textured { draw, .. } = &mut invalid.nodes[1] {
        let draw = Arc::make_mut(draw);
        draw.remap = [1e8, 0.];
        draw.displacement = [1e8, 0.];
    }
    assert!(fold_render::render(invalid.clone()).is_err());
    let mut bad = renderer
        .evaluate(invalid, None, &mut decoder, &cancel)
        .unwrap();
    assert!(bad.readback(&cancel).is_err());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native Vulkan"]
fn native_image_response_uv_alpha_instances_and_crop_match_reference() {
    let _guard = GPU_LOCK.lock().unwrap();
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(32 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for variant in 0..5 {
        let mut g = graph();
        let mut ppm = b"P6\n16 16\n255\n".to_vec();
        for y in 0..16 {
            for x in 0..16 {
                ppm.extend_from_slice(&[(x * 16) as u8, (y * 16) as u8, 128]);
            }
        }
        g.nodes[0] = ImageOp::Media(fold_media::RgbImage::decode_ppm(&ppm).unwrap());
        let ImageOp::Textured { draw, .. } = &mut g.nodes[1] else {
            panic!()
        };
        let d = Arc::make_mut(draw);
        if variant == 1 {
            d.displacement = [2., -1.];
            d.remap = [0.8, 0.1];
            d.color = 0.7;
            d.alpha = 0.5;
        }
        if variant == 2 {
            d.instances.push(fold_render::textured::Instance {
                matrix: [-0.5, 0., 0., 0.5],
                offset: [12., 4., 0.8, 0.],
            });
        }
        if variant == 3 {
            d.scale = 0.5;
            d.rotation = 17.;
            d.remap = [0., 0.5];
            d.transform = [0.7, 0.1, -0.1, 0.7, 2.1, 0.3];
        }
        if variant == 4 {
            d.atlas = [4., 0.25, 1.];
            for v in &mut Arc::make_mut(&mut d.mesh).vertices {
                v.sample = [0.75; 2];
            }
        }
        let cpu = fold_render::render(g.clone()).unwrap();
        let mut gpu = renderer
            .evaluate(g.clone(), None, &mut decoder, &cancel)
            .unwrap();
        assert_eq!(gpu.statistics.readback_bytes, 0);
        let actual = gpu.readback(&cancel).unwrap();
        let max = actual
            .pixels()
            .iter()
            .flatten()
            .zip(cpu.pixels().iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(max < 0.002, "variant {variant}: {max}");
        let mut repeated = renderer
            .evaluate(g.clone(), None, &mut decoder, &cancel)
            .unwrap();
        assert_eq!(repeated.statistics.resident_still_nodes, 1);
        assert_eq!(
            repeated.statistics.upload_bytes,
            96 + 32 * draw_instance_count(&g)
        );
        repeated.readback(&cancel).unwrap();
        let crop = fold_render::region::Region {
            x: 2,
            y: 3,
            width: 7,
            height: 8,
        };
        let plan = fold_render::region::Plan::new(g, crop, false).unwrap();
        assert_eq!(plan.working, fold_render::region::Region::full([16, 16]));
    }
}

#[cfg(feature = "gpu")]
fn draw_instance_count(g: &RenderGraph) -> u64 {
    if let ImageOp::Textured { draw, .. } = &g.nodes[1] {
        draw.instances.len() as u64
    } else {
        0
    }
}
