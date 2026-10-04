use fold_render::channels::{self, Value};
use fold_render::{ImageOp as Op, RenderGraph, render};
fn fixture(amount: f32, invert: bool, mask: bool) -> RenderGraph {
    RenderGraph {
        width: 3,
        height: 2,
        output: 3,
        nodes: vec![
            Op::Solid {
                rgba: [0.1, 0.2, 0.3, 0.5],
            },
            Op::Solid {
                rgba: [0.6, 0.4, 0.2, 0.8],
            },
            Op::Solid {
                rgba: [0.25, 0.5, 0.75, 1.],
            },
            Op::Mix {
                original: 0,
                processed: 1,
                mask: mask.then_some(2),
                mask_channel: 0,
                invert,
                amount,
                channels: [true, false, true, false],
            },
        ],
    }
}
#[test]
fn effect_masks_preserve_alpha_and_unselected_channels() {
    for amount in [0., 0.3, 1.] {
        for invert in [false, true] {
            for mask in [false, true] {
                let weight = amount
                    * if mask {
                        if invert { 0.75 } else { 0.25 }
                    } else {
                        1.
                    };
                let expected = [
                    0.1 * (1. - weight) + 0.6 * weight,
                    0.2,
                    0.3 * (1. - weight) + 0.2 * weight,
                    0.5,
                ];
                for pixel in render(fixture(amount, invert, mask)).unwrap().pixels() {
                    for (a, b) in pixel.iter().zip(expected) {
                        assert!((a - b).abs() < 1e-6);
                    }
                }
            }
        }
    }
}
#[test]
fn channel_gather_handles_four_sources_constants_and_direct_reuse() {
    let mut graph = fixture(1., false, true);
    graph.nodes.push(Op::Solid {
        rgba: [0.9, 0.7, 0.4, 1.],
    });
    let c = |image, component| Value::Component { image, component };
    graph.output = channels::pack(&mut graph, [c(0, 1), c(1, 0), c(2, 2), c(4, 3)]).unwrap();
    assert_eq!(
        render(graph.clone()).unwrap().pixels(),
        &[[0.2, 0.6, 0.75, 1.]; 6]
    );
    let count = graph.nodes.len();
    assert_eq!(
        channels::pack(&mut graph, [c(0, 0), c(0, 1), c(0, 2), c(0, 3)]).unwrap(),
        0
    );
    assert_eq!(graph.nodes.len(), count);
    graph.output =
        channels::pack(&mut graph, [Value::One, Value::Zero, c(1, 1), Value::One]).unwrap();
    assert_eq!(render(graph).unwrap().pixels(), &[[1., 0., 0.4, 1.]; 6]);
}
#[test]
fn invalid_channel_mapping_is_rejected_before_evaluation() {
    let mut graph = fixture(1., false, true);
    graph.nodes.push(Op::Shuffle {
        first: 0,
        second: None,
        mapping: [4, 1, 2, 3],
    });
    // Even disconnected malformed operators must fail preflight.
    assert!(render(graph).unwrap_err().contains("connected B"));
    assert!(channels::ChannelName::try_from("depth..Z".to_owned()).is_err());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a native GPU"]
fn gpu_channel_gather_and_mask_match_cpu_with_shared_inputs() {
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(2 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    for mask in [false, true] {
        for invert in [false, true] {
            for amount in [0., 0.3, 1.] {
                let mut graph = fixture(amount, invert, mask);
                graph.nodes.push(Op::Shuffle {
                    first: 3,
                    second: Some(0),
                    mapping: [2, 4, 8, 9],
                });
                graph.output = 4;
                let expected = render(graph.clone()).unwrap();
                let mut actual = renderer
                    .evaluate(graph, None, &mut decoder, &cancel)
                    .unwrap();
                assert_eq!(actual.statistics.upload_bytes, 0);
                let result = actual.readback(&cancel).unwrap();
                for (a, b) in result
                    .pixels()
                    .iter()
                    .flatten()
                    .zip(expected.pixels().iter().flatten())
                {
                    assert!((a - b).abs() < 1e-6);
                }
            }
        }
    }
}

#[test]
fn grade_identity_transparency_and_signed_gamma_have_defined_results() {
    let settings = fold_render::operations::Grade {
        gamma: [2.; 3],
        offset: [-0.5, 0., 0.],
        ..Default::default()
    };
    let graph = RenderGraph {
        width: 1,
        height: 1,
        output: 1,
        nodes: vec![
            Op::Solid {
                rgba: [0.125, 0.125, 0.125, 0.5],
            },
            Op::ColorGrade { input: 0, settings },
        ],
    };
    assert_eq!(render(graph).unwrap().pixels(), &[[-0.25, 0.25, 0.25, 0.5]]);
    let settings = fold_render::operations::Grade {
        offset: [1.; 3],
        ..Default::default()
    };
    let graph = RenderGraph {
        width: 1,
        height: 1,
        output: 1,
        nodes: vec![
            Op::Solid { rgba: [0.; 4] },
            Op::ColorGrade { input: 0, settings },
        ],
    };
    assert_eq!(render(graph).unwrap().pixels(), &[[0.; 4]]);
}
#[test]
fn scalar_display_is_linear_grayscale_and_range_has_stable_identity() {
    let frame = render(RenderGraph {
        width: 1,
        height: 1,
        output: 0,
        nodes: vec![Op::Solid {
            rgba: [0.5, 0., 0., 1.],
        }],
    })
    .unwrap();
    assert_eq!(
        frame.to_data_display(Default::default()).unwrap().rgba(),
        &[128, 128, 128, 255]
    );
    assert_eq!(
        frame
            .to_data_display(fold_render::view::Range::new(0., 2.).unwrap())
            .unwrap()
            .rgba(),
        &[64, 64, 64, 255]
    );
    assert!(fold_render::view::Range::new(1., 1.).is_err());
    assert!(fold_render::view::Range::new(f32::NAN, 1.).is_err());
    assert_eq!(
        fold_render::view::Range::new(-0., 1.).unwrap(),
        fold_render::view::Range::default()
    );
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a native GPU"]
fn gpu_grade_merge_modes_and_data_visualization_match_cpu() {
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(2 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for mode in fold_render::operations::MergeMode::ALL {
        let settings = fold_render::operations::Grade {
            offset: [-0.5, 0.1, 0.],
            gamma: [2.; 3],
            ..Default::default()
        };
        let graph = RenderGraph {
            width: 3,
            height: 2,
            output: 3,
            nodes: vec![
                Op::Solid {
                    rgba: [0.2, 0.3, 0.1, 0.5],
                },
                Op::Solid {
                    rgba: [0.3, 0.4, 0.2, 0.75],
                },
                Op::ColorGrade { input: 0, settings },
                Op::Merge {
                    first: 2,
                    second: 1,
                    mode,
                },
            ],
        };
        let expected = render(graph.clone()).unwrap();
        let mut actual = renderer
            .evaluate(graph, None, &mut decoder, &cancel)
            .unwrap();
        let result = actual.readback(&cancel).unwrap();
        for (a, b) in result
            .pixels()
            .iter()
            .flatten()
            .zip(expected.pixels().iter().flatten())
        {
            assert!((a - b).abs() < 1e-5, "{mode:?}: {a} != {b}");
        }
        let range = fold_render::view::Range::new(-1., 2.).unwrap();
        let display = expected.to_data_display(range).unwrap();
        let mut gpu_display = renderer.output_data(&actual, range, &cancel).unwrap();
        assert_eq!(gpu_display.storage_bytes(), 3 * 2 * 4);
        let actual = gpu_display.readback(&cancel).unwrap();
        for (&a, &b) in actual.rgba().iter().zip(display.rgba()) {
            assert!(a.abs_diff(b) <= 1);
        }
    }
}

#[test]
fn fractional_transform_samples_between_pixels_and_gaussian_has_symmetric_support() {
    use fold_render::{
        Affine,
        operations::{Edges, Filter},
    };
    let base = vec![
        Op::Solid { rgba: [1.; 4] },
        Op::Crop {
            input: 0,
            rect: [2, 0, 3, 1],
        },
    ];
    let mut graph = RenderGraph {
        width: 5,
        height: 1,
        output: 2,
        nodes: base.clone(),
    };
    graph.nodes.push(Op::Resample {
        input: 1,
        transform: Affine {
            tx: 0.5,
            ..Affine::IDENTITY
        },
        filter: Filter::Linear,
    });
    let pixels = render(graph).unwrap();
    assert_eq!(pixels.pixels()[2], [0.5; 4]);
    assert_eq!(pixels.pixels()[3], [0.5; 4]);
    let mut graph = RenderGraph {
        width: 5,
        height: 1,
        output: 2,
        nodes: base,
    };
    graph.nodes.push(Op::Gaussian {
        input: 1,
        size: [0.7, 0.],
        edges: Edges::Transparent,
    });
    let pixels = render(graph).unwrap();
    assert_eq!(pixels.pixels()[0], pixels.pixels()[4]);
    assert_eq!(pixels.pixels()[1], pixels.pixels()[3]);
    assert!(pixels.pixels()[2][0] > pixels.pixels()[1][0]);
    assert!(pixels.pixels()[1][0] > pixels.pixels()[0][0]);
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native GPU"]
fn gpu_fractional_sampling_and_gaussian_match_reference() {
    use fold_render::{
        Affine,
        operations::{Edges, Filter},
    };
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(2 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    for filter in Filter::ALL {
        for edges in [Edges::Transparent, Edges::Clamp] {
            let graph = RenderGraph {
                width: 9,
                height: 7,
                output: 3,
                nodes: vec![
                    Op::Solid {
                        rgba: [0.7, 0.3, 0.1, 0.8],
                    },
                    Op::Crop {
                        input: 0,
                        rect: [2, 1, 7, 6],
                    },
                    Op::Resample {
                        input: 1,
                        transform: Affine {
                            a: 0.8,
                            d: 1.1,
                            tx: 0.3,
                            ty: -0.7,
                            ..Affine::IDENTITY
                        },
                        filter,
                    },
                    Op::Gaussian {
                        input: 2,
                        size: [0.7, 1.2],
                        edges,
                    },
                ],
            };
            let expected = render(graph.clone()).unwrap();
            let mut actual = renderer
                .evaluate(graph, None, &mut decoder, &cancel)
                .unwrap();
            let actual = actual.readback(&cancel).unwrap();
            for (a, b) in actual
                .pixels()
                .iter()
                .flatten()
                .zip(expected.pixels().iter().flatten())
            {
                assert!((a - b).abs() < 1e-5, "{filter:?} {edges:?}: {a} vs {b}");
            }
        }
    }
}

#[test]
fn explicit_alpha_operations_round_trip_and_define_zero_alpha() {
    use fold_render::operations::Unary;
    let graph = |rgba, operation| RenderGraph {
        width: 1,
        height: 1,
        output: 1,
        nodes: vec![
            Op::Solid { rgba },
            Op::Unary {
                input: 0,
                operation,
            },
        ],
    };
    let straight = render(graph([0.1, 0.2, 0.3, 0.5], Unary::Unpremult)).unwrap();
    assert_eq!(straight.pixels(), &[[0.2, 0.4, 0.6, 0.5]]);
    let mut round_trip = graph([0.1, 0.2, 0.3, 0.5], Unary::Unpremult);
    round_trip.nodes.push(Op::Unary {
        input: 1,
        operation: Unary::Premult,
    });
    round_trip.output = 2;
    assert_eq!(
        render(round_trip).unwrap().pixels(),
        &[[0.1, 0.2, 0.3, 0.5]]
    );
    assert_eq!(
        render(graph([0.; 4], Unary::Unpremult)).unwrap().pixels(),
        &[[0.; 4]]
    );
    assert_eq!(
        render(graph([0.1, 0.2, 0.3, 0.5], Unary::Exposure { stops: 2. }))
            .unwrap()
            .pixels(),
        &[[0.4, 0.8, 1.2, 0.5]]
    );
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native GPU"]
fn gpu_unary_operations_match_reference() {
    use fold_render::operations::Unary;
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(2 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    for operation in [
        Unary::Exposure { stops: -0.5 },
        Unary::Invert,
        Unary::Clamp {
            minimum: 0.15,
            maximum: 0.25,
        },
        Unary::Premult,
        Unary::Unpremult,
    ] {
        for rgba in [[0.1, 0.2, 0.3, 0.5], [0.; 4]] {
            let graph = RenderGraph {
                width: 1,
                height: 1,
                output: 1,
                nodes: vec![
                    Op::Solid { rgba },
                    Op::Unary {
                        input: 0,
                        operation: operation.clone(),
                    },
                ],
            };
            let expected = render(graph.clone()).unwrap();
            let mut actual = renderer
                .evaluate(graph, None, &mut decoder, &cancel)
                .unwrap();
            let actual = actual.readback(&cancel).unwrap();
            for (a, b) in actual.pixels()[0].into_iter().zip(expected.pixels()[0]) {
                assert!((a - b).abs() < 1e-6, "{operation:?}: {a} != {b}");
            }
        }
    }
}
