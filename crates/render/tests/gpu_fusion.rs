#![cfg(feature = "gpu")]
use fold_render::{
    ImageOp, RenderGraph,
    gpu::{Host, Renderer},
};

#[test]
#[ignore = "requires native Vulkan and pinned OCIO runtime"]
fn opacity_over_preserves_pixels_branches_and_validation() {
    let root = std::path::PathBuf::from(std::env::var_os("FOLD_TEST_COLOR_ROOT").unwrap());
    let runtime = fold_color::Runtime::at(&root).unwrap();
    let config = runtime.bundled().unwrap();
    let (host, _) = pollster::block_on(Host::headless(2 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for config in [None, Some(&config)] {
        for opacity in [0., 0.17, 0.5, 1.] {
            for branched in [false, true] {
                let mut graph = RenderGraph {
                    width: 19,
                    height: 13,
                    nodes: vec![
                        ImageOp::Solid {
                            rgba: if config.is_some() {
                                [-0.2, 2.5, 0.1, 0.7]
                            } else {
                                [0.2, 0.5, 0.1, 0.7]
                            },
                        },
                        ImageOp::Crop {
                            input: 0,
                            rect: [2, 1, 17, 11],
                        },
                        ImageOp::Opacity { input: 1, opacity },
                        // Intervening work must not recycle the deferred input.
                        ImageOp::Solid {
                            rgba: [0.1, 0.2, 0.3, 0.5],
                        },
                        ImageOp::Grade {
                            input: 3,
                            gain: [1.5, 0.7, 0.9],
                        },
                        ImageOp::Over {
                            foreground: 2,
                            background: 4,
                        },
                    ],
                    output: 5,
                };
                if branched {
                    graph.nodes.push(ImageOp::Over {
                        foreground: 5,
                        background: 2,
                    });
                    graph.output = 6;
                }
                let expected = match config {
                    Some(c) => {
                        fold_render::render_aces_with(graph.clone(), c, &mut decoder, &cancel)
                            .unwrap()
                    }
                    None => fold_render::render(graph.clone()).unwrap(),
                };
                let mut actual = renderer
                    .evaluate(graph, config, &mut decoder, &cancel)
                    .unwrap();
                assert_eq!(actual.statistics.fused_opacity_passes, u32::from(!branched));
                let actual = actual.readback(&cancel).unwrap();
                for (a, b) in actual
                    .pixels()
                    .iter()
                    .flatten()
                    .zip(expected.pixels().iter().flatten())
                {
                    assert!((a - b).abs() < 0.000003, "{a} != {b}");
                }
            }
        }
    }
    // Overflow before a zero opacity must still reject the whole evaluation.
    let graph = RenderGraph {
        width: 19,
        height: 13,
        nodes: vec![
            ImageOp::Solid {
                rgba: [f32::MAX, 0., 0., 1.],
            },
            ImageOp::Grade {
                input: 0,
                gain: [16., 1., 1.],
            },
            ImageOp::Opacity {
                input: 1,
                opacity: 0.,
            },
            ImageOp::Solid {
                rgba: [0., 0., 0., 1.],
            },
            ImageOp::Over {
                foreground: 2,
                background: 3,
            },
        ],
        output: 4,
    };
    let mut frame = renderer
        .evaluate(graph.clone(), Some(&config), &mut decoder, &cancel)
        .unwrap();
    assert_eq!(frame.statistics.fused_opacity_passes, 1);
    assert!(frame.readback(&cancel).is_err());
    let mut invalid = graph;
    invalid.nodes.push(ImageOp::Opacity {
        input: 0,
        opacity: f32::NAN,
    });
    assert!(
        renderer
            .evaluate(invalid, Some(&config), &mut decoder, &cancel)
            .is_err()
    );
    assert!(host.memory().peak <= host.memory().budget);
}
