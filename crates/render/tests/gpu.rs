#![cfg(feature = "gpu")]
use fold_render::{
    Affine, ImageOp, RenderGraph,
    gpu::{Host, Renderer},
};

#[test]
#[ignore = "requires native Vulkan"]
fn pointwise_fusion_preserves_pixels_and_intermediate_failures() {
    let (host, _) = pollster::block_on(Host::headless(16 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let graph = RenderGraph {
        width: 17,
        height: 9,
        nodes: vec![
            ImageOp::Solid {
                rgba: [0.2, 0.1, 0.05, 0.5],
            },
            ImageOp::Grade {
                input: 0,
                gain: [0.5, 1.2, 0.7],
            },
            ImageOp::Opacity {
                input: 1,
                opacity: 0.7,
            },
            ImageOp::Unary {
                input: 2,
                operation: fold_render::operations::Unary::Exposure { stops: 0.5 },
            },
        ],
        output: 3,
    };
    let expected = fold_render::render_with(graph.clone(), &mut decoder, &cancel).unwrap();
    let mut result = renderer
        .evaluate(graph, None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(result.statistics.fused_pointwise_passes, 2);
    assert_eq!(result.statistics.compute_passes, 2);
    let actual = result.readback(&cancel).unwrap();
    assert!(
        actual
            .pixels()
            .iter()
            .zip(expected.pixels())
            .all(|(a, b)| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-6))
    );
    let graph = RenderGraph {
        width: 1,
        height: 1,
        nodes: vec![
            ImageOp::Solid {
                rgba: [1., 1., 1., 1.],
            },
            ImageOp::Unary {
                input: 0,
                operation: fold_render::operations::Unary::Exposure { stops: 32. },
            },
            ImageOp::Unary {
                input: 1,
                operation: fold_render::operations::Unary::Exposure { stops: 32. },
            },
            ImageOp::Unary {
                input: 2,
                operation: fold_render::operations::Unary::Exposure { stops: 32. },
            },
            ImageOp::Unary {
                input: 3,
                operation: fold_render::operations::Unary::Exposure { stops: 32. },
            },
            ImageOp::Unary {
                input: 4,
                operation: fold_render::operations::Unary::Exposure { stops: 32. },
            },
            ImageOp::Unary {
                input: 5,
                operation: fold_render::operations::Unary::Clamp {
                    minimum: 0.,
                    maximum: 1.,
                },
            },
        ],
        output: 6,
    };
    let mut result = renderer
        .evaluate(graph, None, &mut decoder, &cancel)
        .unwrap();
    assert!(
        result.readback(&cancel).is_err(),
        "final clamp must not hide overflow"
    );
}

#[test]
#[ignore = "requires native Vulkan"]
fn packed_alpha_delivery_matches_cpu_without_scene_readback() {
    let (host, _) = pollster::block_on(Host::headless(16 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    for alpha in [0., 0.0001, 0.25, 0.5, 1.] {
        let graph = RenderGraph {
            width: 17,
            height: 9,
            nodes: vec![ImageOp::Solid {
                rgba: [0.2 * alpha, 0.01 * alpha, 0.7 * alpha, alpha],
            }],
            output: 0,
        };
        let expected = fold_render::render_with(graph.clone(), &mut decoder, &cancel)
            .unwrap()
            .into_rgba16(None)
            .unwrap();
        let scene = renderer
            .evaluate(graph, None, &mut decoder, &cancel)
            .unwrap();
        let actual = renderer.output_rgba16(&scene, None, &cancel).unwrap();
        assert_eq!(actual.bytes().len(), 17 * 9 * 8);
        for (a, b) in actual
            .bytes()
            .chunks_exact(2)
            .zip(expected.bytes().chunks_exact(2))
        {
            assert!(
                u16::from_le_bytes(a.try_into().unwrap())
                    .abs_diff(u16::from_le_bytes(b.try_into().unwrap()))
                    <= 1
            );
        }
    }
}

#[test]
#[ignore = "requires native Vulkan"]
fn all_shader_inputs_release_after_their_last_consumer() {
    let size = 128;
    let image_bytes = u64::from(size * size) * 16;
    let (host, _) = pollster::block_on(Host::headless(6 * image_bytes + 65536)).unwrap();
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let program = std::sync::Arc::new(fold_render::shader::Program {
        source: "fn effect(uv:vec2<f32>)->vec4<f32>{return (a(uv)+b(uv)+c(uv)+d(uv))*0.25;}".into(),
        images: vec!["a".into(), "b".into(), "c".into(), "d".into()],
        values: vec![],
        time: 0.,
        progress: 0.,
        logical_size: [size as f32; 2],
    });
    let mut graph = RenderGraph {
        width: size,
        height: size,
        nodes: vec![],
        output: 0,
    };
    for group in 0..5 {
        let start = graph.nodes.len();
        for _ in 0..4 {
            graph.nodes.push(ImageOp::Solid {
                rgba: [0.25, 0.125, 0., 1.],
            });
        }
        let shader = graph.nodes.len();
        graph.nodes.push(ImageOp::Shader {
            inputs: (start..start + 4).collect(),
            program: program.clone(),
        });
        if group == 0 {
            graph.output = shader;
        } else {
            let background = graph.output;
            graph.output = graph.nodes.len();
            graph.nodes.push(ImageOp::Over {
                foreground: shader,
                background,
            });
        }
    }
    let cancel = fold_media::Cancel::default();
    let mut frame = renderer
        .evaluate(graph, None, &mut fold_media::Decoder::default(), &cancel)
        .unwrap();
    let result = frame.readback(&cancel).unwrap();
    assert!(result.pixels().iter().all(|p| *p == [0.25, 0.125, 0., 1.]));
    assert!(host.memory().peak <= 6 * image_bytes + 65536);
}

fn fixture(aces: bool) -> RenderGraph {
    let pixels = (0..19 * 13)
        .flat_map(|i| {
            [
                (i * 17 % 256) as u8,
                (i * 31 % 256) as u8,
                (i * 7 % 256) as u8,
            ]
        })
        .collect();
    RenderGraph {
        width: 19,
        height: 13,
        nodes: vec![
            ImageOp::Media(fold_media::RgbImage::from_rgb([19, 13], pixels).unwrap()),
            ImageOp::Solid {
                rgba: if aces {
                    [-0.2, 2.5, 0.1, 0.7]
                } else {
                    [0.1, 0.5, 0.2, 0.7]
                },
            },
            ImageOp::Crop {
                input: 1,
                rect: [2, 1, 17, 11],
            },
            ImageOp::Transform {
                input: 2,
                transform: Affine {
                    a: 0.,
                    b: 1.,
                    c: -1.,
                    d: 0.,
                    tx: 16.,
                    ty: -2.,
                },
            },
            ImageOp::Blur {
                input: 3,
                radius: 2,
            },
            ImageOp::Grade {
                input: 4,
                gain: [2., 0.5, 1.3],
            },
            ImageOp::Opacity {
                input: 5,
                opacity: 0.65,
            },
            ImageOp::Mask { input: 6, mask: 4 },
            ImageOp::Over {
                foreground: 7,
                background: 0,
            },
        ],
        output: 8,
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn native_operators_lifetimes_and_budgets() {
    let (host, info) = pollster::block_on(Host::headless(2 * 1024 * 1024)).unwrap();
    eprintln!("GPU adapter: {info:?}");
    let mut gpu = Renderer::new(host.clone()).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let graph = fixture(false);
    for output in 0..graph.nodes.len() {
        let plan = RenderGraph {
            output,
            ..graph.clone()
        };
        let expected = fold_render::render(plan.clone()).unwrap();
        let mut actual = gpu.evaluate(plan, None, &mut decoder, &cancel).unwrap();
        assert_eq!(actual.statistics.readback_bytes, 0);
        let result = actual.readback(&cancel).unwrap();
        assert!(actual.is_ready().unwrap());
        let max = result
            .pixels()
            .iter()
            .flatten()
            .zip(expected.pixels().iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(max < 0.000003, "operator {output}: max error {max}");
    }
    // A held output remains immutable while another request reuses scratch.
    let mut held = gpu
        .evaluate(graph.clone(), None, &mut decoder, &cancel)
        .unwrap();
    let original = held.readback(&cancel).unwrap();
    for _ in 0..5 {
        let mut frame = gpu
            .evaluate(
                RenderGraph {
                    nodes: vec![ImageOp::Solid {
                        rgba: [1., 0., 0., 1.],
                    }],
                    output: 0,
                    ..graph.clone()
                },
                None,
                &mut decoder,
                &cancel,
            )
            .unwrap();
        frame.readback(&cancel).unwrap();
    }
    assert_eq!(held.readback(&cancel).unwrap().pixels(), original.pixels());
    assert!(host.memory().peak <= host.memory().budget);
    let memory = host.memory();
    assert_eq!(
        memory.allocated,
        memory.working + memory.presentation + memory.decoded + memory.geometry + memory.scratch
    );
    // A mapped timestamp remains exact across the explicit readback boundary.
    let timestamp = fold_foundation::Time::new(1001, 24000).unwrap();
    let mut timed = gpu
        .evaluate(fixture(false), None, &mut decoder, &cancel)
        .unwrap()
        .with_timing(timestamp, timestamp)
        .unwrap();
    assert_eq!(
        timed.descriptor().unwrap().timing.unwrap().timestamp,
        timestamp
    );
    assert_eq!(
        timed.readback(&cancel).unwrap().descriptor().timing,
        timed.descriptor().unwrap().timing
    );
    drop(timed);
    let cancelled = fold_media::Cancel::default();
    let mut submitted = gpu
        .evaluate(graph.clone(), None, &mut decoder, &cancelled)
        .unwrap();
    cancelled.cancel();
    assert!(
        submitted.readback(&cancelled).is_err(),
        "cancellation cannot publish submitted work"
    );
    drop(submitted);
    host.poll().unwrap();
    assert!(
        gpu.evaluate(graph.clone(), None, &mut decoder, &cancelled)
            .is_err()
    );
    let (tiny, _) = pollster::block_on(Host::headless(32)).unwrap();
    let output = gpu.output(&held, None, &cancel).unwrap();
    assert_eq!(output.owner(), host.id());
    assert_ne!(
        output.owner(),
        tiny.id(),
        "same adapter is not the same device"
    );
    drop(output);
    assert!(
        Renderer::new(tiny)
            .unwrap()
            .evaluate(graph.clone(), None, &mut decoder, &cancel)
            .is_err()
    );
    let mut invalid = graph;
    invalid.nodes.push(ImageOp::Opacity {
        input: 0,
        opacity: f32::NAN,
    });
    assert!(
        gpu.evaluate(invalid, None, &mut decoder, &cancel).is_err(),
        "unused nodes are validated"
    );
    // Device loss has a stopped-render outcome, not valid-looking stale pixels.
    host.device().destroy();
    let _ = host.poll();
    assert!(held.is_ready().is_err());
    assert!(
        gpu.evaluate(fixture(false), None, &mut decoder, &cancel)
            .is_err()
    );
}

#[test]
#[ignore = "requires native Vulkan and FOLD_TEST_COLOR_ROOT"]
fn native_aces_signed_hdr_and_alpha() {
    let runtime = fold_color::Runtime::at(&std::path::PathBuf::from(
        std::env::var_os("FOLD_TEST_COLOR_ROOT").expect("set FOLD_TEST_COLOR_ROOT"),
    ))
    .unwrap();
    let config = runtime.bundled().unwrap();
    let (host, _) = pollster::block_on(Host::headless(2 * 1024 * 1024)).unwrap();
    let mut gpu = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let graph = fixture(true);
    for output in 0..graph.nodes.len() {
        let plan = RenderGraph {
            output,
            ..graph.clone()
        };
        let expected =
            fold_render::render_aces_with(plan.clone(), &config, &mut decoder, &cancel).unwrap();
        let mut result = gpu
            .evaluate(plan, Some(&config), &mut decoder, &cancel)
            .unwrap();
        let actual = result.readback(&cancel).unwrap();
        assert_eq!(actual.config_identity(), expected.config_identity());
        for (a, b) in actual
            .pixels()
            .iter()
            .flatten()
            .zip(expected.pixels().iter().flatten())
        {
            assert!((a - b).abs() < 0.0002, "operator {output}: {a} != {b}");
        }
    }
    // Later transparent crops cannot conceal an invalid intermediate.
    let overflow = RenderGraph {
        width: 1,
        height: 1,
        nodes: vec![
            ImageOp::Solid {
                rgba: [f32::MAX, 0., 0., 1.],
            },
            ImageOp::Grade {
                input: 0,
                gain: [16.; 3],
            },
            ImageOp::Crop {
                input: 1,
                rect: [0; 4],
            },
        ],
        output: 2,
    };
    assert!(
        fold_render::render_aces_with(overflow.clone(), &config, &mut decoder, &cancel).is_err()
    );
    let mut invalid = gpu
        .evaluate(overflow, Some(&config), &mut decoder, &cancel)
        .unwrap();
    assert!(
        invalid
            .readback(&cancel)
            .unwrap_err()
            .message
            .contains("intermediate")
    );
    // A per-frame validation error does not poison a healthy device.
    gpu.evaluate(fixture(true), Some(&config), &mut decoder, &cancel)
        .unwrap()
        .readback(&cancel)
        .unwrap();
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn agent_presentation_capture_matches_rgba_and_decodes_compressed_cache() {
    let (host, _) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    let mut gpu = Renderer::new(host.clone()).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let frame = gpu
        .evaluate(fixture(false), None, &mut decoder, &cancel)
        .unwrap();
    let mut display = gpu.output(&frame, None, &cancel).unwrap();
    let expected = display.readback(&cancel).unwrap();
    let captured = host
        .capture_presentation(
            display.texture(),
            display.dimensions(),
            display.transform_identity().into(),
            &cancel,
        )
        .unwrap();
    assert_eq!(captured.rgba(), expected.rgba());
    if host
        .device()
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    {
        // The patterned fixture can correctly fail the compression quality gate.
        let simple = gpu
            .evaluate(
                RenderGraph {
                    width: 19,
                    height: 13,
                    nodes: vec![ImageOp::Solid {
                        rgba: [0.2, 0.4, 0.1, 1.],
                    }],
                    output: 0,
                },
                None,
                &mut decoder,
                &cancel,
            )
            .unwrap();
        let mut display = gpu.output(&simple, None, &cancel).unwrap();
        let expected = display.readback(&cancel).unwrap();
        let mut compressor = fold_render::gpu::PresentationCompressor::new(host.clone()).unwrap();
        let compressed = compressor.compress(&display).unwrap();
        while !compressed.is_ready().unwrap() {
            host.poll().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let captured = host
            .capture_presentation(
                compressed.texture(),
                compressed.dimensions(),
                compressed.transform_identity().into(),
                &cancel,
            )
            .unwrap();
        assert_eq!(captured.dimensions(), expected.dimensions());
        let max = captured
            .rgba()
            .iter()
            .zip(expected.rgba())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 32, "compressed capture max error {max}");
    }
    cancel.cancel();
    assert!(
        host.capture_presentation(
            display.texture(),
            display.dimensions(),
            String::new(),
            &cancel
        )
        .is_err()
    );
    assert!(host.memory().peak <= host.memory().budget);
}
