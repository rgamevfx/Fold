#![cfg(feature = "gpu")]
use fold_render::{
    Affine, ImageOp, RenderGraph,
    gpu::{Host, Renderer},
};

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
            .contains("intermediate")
    );
    // A per-frame validation error does not poison a healthy device.
    gpu.evaluate(fixture(true), Some(&config), &mut decoder, &cancel)
        .unwrap()
        .readback(&cancel)
        .unwrap();
}
