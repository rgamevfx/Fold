#![cfg(feature = "gpu")]
use fold_render::{
    ImageOp, RenderGraph,
    gpu::{Host, Renderer},
    operations::Unary,
};

#[test]
#[ignore = "requires native Vulkan"]
fn repeated_filter_subgraphs_execute_once() {
    let (host, _) = pollster::block_on(Host::headless(16 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let solid = ImageOp::Solid {
        rgba: [0.2, 0.1, 0.05, 0.5],
    };
    let graph = RenderGraph {
        width: 17,
        height: 9,
        nodes: vec![
            solid.clone(),
            solid,
            ImageOp::Blur {
                input: 0,
                radius: 3,
            },
            ImageOp::Blur {
                input: 1,
                radius: 3,
            },
            ImageOp::Over {
                foreground: 2,
                background: 3,
            },
        ],
        output: 4,
    };
    let expected = fold_render::render_with(graph.clone(), &mut decoder, &cancel).unwrap();
    let mut frame = renderer
        .evaluate(graph, None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(frame.statistics.compute_passes, 4);
    let actual = frame.readback(&cancel).unwrap();
    for (a, b) in actual.pixels().iter().zip(expected.pixels()) {
        assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-6));
    }
}

#[test]
#[ignore = "requires native Vulkan"]
fn masked_pointwise_and_shuffle_preserve_pixels_and_hidden_errors() {
    let (host, _) = pollster::block_on(Host::headless(16 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for amount in [0., 0.25, 1.] {
        for invert in [false, true] {
            for mask in [None, Some(0), Some(1)] {
                let graph = RenderGraph {
                    width: 17,
                    height: 9,
                    nodes: vec![
                        ImageOp::Solid {
                            rgba: [0.2, 0.1, 0.05, 0.5],
                        },
                        ImageOp::Solid {
                            rgba: [0.25, 0., 0., 1.],
                        },
                        ImageOp::Unary {
                            input: 0,
                            operation: Unary::Exposure { stops: 1. },
                        },
                        ImageOp::Shuffle {
                            first: 2,
                            second: None,
                            mapping: [2, 1, 0, 3],
                        },
                        ImageOp::Mix {
                            original: 0,
                            processed: 3,
                            mask,
                            mask_channel: 0,
                            invert,
                            amount,
                            channels: [true, false, true, false],
                        },
                    ],
                    output: 4,
                };
                let expected =
                    fold_render::render_with(graph.clone(), &mut decoder, &cancel).unwrap();
                let mut frame = renderer
                    .evaluate(graph, None, &mut decoder, &cancel)
                    .unwrap();
                assert_eq!(frame.statistics.fused_pointwise_passes, 2);
                assert_eq!(
                    frame.statistics.compute_passes,
                    if mask == Some(1) { 3 } else { 2 }
                );
                let actual = frame.readback(&cancel).unwrap();
                for (a, b) in actual.pixels().iter().zip(expected.pixels()) {
                    assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-6));
                }
            }
        }
    }
    let mut nodes = vec![ImageOp::Solid { rgba: [1.; 4] }];
    for input in 0..5 {
        nodes.push(ImageOp::Unary {
            input,
            operation: Unary::Exposure { stops: 32. },
        });
    }
    nodes.push(ImageOp::Mix {
        original: 0,
        processed: 5,
        mask: None,
        mask_channel: 0,
        invert: false,
        amount: 0.,
        channels: [true; 4],
    });
    let mut frame = renderer
        .evaluate(
            RenderGraph {
                width: 1,
                height: 1,
                nodes,
                output: 6,
            },
            None,
            &mut decoder,
            &cancel,
        )
        .unwrap();
    assert!(
        frame.readback(&cancel).is_err(),
        "zero mix must not hide overflow"
    );
}
