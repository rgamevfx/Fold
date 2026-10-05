#[cfg(feature = "gpu")]
static GPU_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
use fold_render::{
    Affine, ImageOp as Op, RenderGraph,
    operations::{Edges, Filter},
    region::{Plan, Region, crop},
};

fn fixture(filter: Filter, edges: Edges) -> RenderGraph {
    let dimensions = [160, 100];
    let rgb = (0..dimensions[0] * dimensions[1])
        .flat_map(|i| {
            [
                (i * 17 % 256) as u8,
                (i * 7 % 256) as u8,
                (i * 31 % 256) as u8,
            ]
        })
        .collect();
    let nodes = vec![
        Op::Media(fold_media::RgbImage::from_rgb(dimensions, rgb).unwrap()),
        Op::Gaussian {
            input: 0,
            size: [2., 3.],
            edges,
        },
        Op::Resample {
            input: 1,
            transform: Affine {
                a: 0.97,
                b: 0.02,
                c: -0.03,
                d: 1.02,
                tx: 7.25,
                ty: -3.5,
            },
            filter,
        },
        Op::Blur {
            input: 2,
            radius: 2,
        },
        Op::Solid {
            rgba: [0.2, 0.1, 0.3, 0.5],
        },
        Op::Crop {
            input: 4,
            rect: [3, 2, 138, 88],
        },
        Op::Mask { input: 3, mask: 5 },
        Op::Grade {
            input: 6,
            gain: [1.2, 0.8, 0.9],
        },
        Op::Opacity {
            input: 7,
            opacity: 0.6,
        },
        Op::Over {
            foreground: 8,
            background: 0,
        },
        Op::Shuffle {
            first: 9,
            second: Some(5),
            mapping: [2, 0, 1, 7],
        },
        Op::Mix {
            original: 9,
            processed: 10,
            mask: Some(5),
            mask_channel: 3,
            invert: true,
            amount: 0.75,
            channels: [true; 4],
        },
    ];
    RenderGraph {
        width: dimensions[0],
        height: dimensions[1],
        output: nodes.len() - 1,
        nodes,
    }
}
fn regions() -> [Region; 4] {
    [
        Region {
            x: 61,
            y: 35,
            width: 17,
            height: 13,
        },
        Region {
            x: 0,
            y: 0,
            width: 15,
            height: 12,
        },
        Region {
            x: 145,
            y: 88,
            width: 15,
            height: 12,
        },
        Region {
            x: 80,
            y: 50,
            width: 1,
            height: 1,
        },
    ]
}
fn assert_pixels(actual: &fold_render::Frame, expected: &fold_render::Frame, tolerance: f32) {
    assert_eq!(actual.dimensions(), expected.dimensions());
    let error = actual
        .pixels()
        .iter()
        .flatten()
        .zip(expected.pixels().iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0_f32, f32::max);
    assert!(error <= tolerance, "maximum error {error} > {tolerance}");
}
#[test]
fn regional_operators_match_full_frame_interior_and_edges() {
    for filter in [Filter::Nearest, Filter::Linear, Filter::Cubic] {
        for edges in [Edges::Transparent, Edges::Clamp] {
            let graph = fixture(filter, edges);
            for output in 0..graph.nodes.len() {
                let graph = RenderGraph {
                    output,
                    ..graph.clone()
                };
                for region in regions() {
                    let plan = Plan::new(graph.clone(), region, false).unwrap();
                    assert!(plan.graph.width * plan.graph.height < graph.width * graph.height);
                    let expected =
                        crop(fold_render::render(graph.clone()).unwrap(), region).unwrap();
                    let actual =
                        crop(fold_render::render(plan.graph).unwrap(), plan.output).unwrap();
                    assert_pixels(&actual, &expected, 0.00002);
                }
            }
        }
    }
}
#[test]
fn invalid_regions_and_cancellation_are_rejected() {
    let graph = fixture(Filter::Nearest, Edges::Transparent);
    for region in [
        Region {
            x: 160,
            y: 0,
            width: 1,
            height: 1,
        },
        Region {
            x: u32::MAX,
            y: 0,
            width: 2,
            height: 1,
        },
        Region {
            x: 0,
            y: 0,
            width: 0,
            height: 1,
        },
    ] {
        assert!(Plan::new(graph.clone(), region, false).is_err());
    }
    let region = regions()[0];
    let plan = Plan::new(graph, region, false).unwrap();
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(
        fold_render::render_with(plan.graph, &mut fold_media::Decoder::default(), &cancel).is_err()
    );
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a native Vulkan adapter"]
fn gpu_regions_match_full_frame_and_keep_display_cropped() {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap();
    let (host, info) =
        pollster::block_on(fold_render::gpu::Host::headless(64 * 1024 * 1024)).unwrap();
    eprintln!("ROI GPU: {info:?}");
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for filter in [Filter::Nearest, Filter::Linear, Filter::Cubic] {
        for edges in [Edges::Transparent, Edges::Clamp] {
            let graph = fixture(filter, edges);
            let full = renderer
                .evaluate(graph.clone(), None, &mut decoder, &cancel)
                .unwrap()
                .readback(&cancel)
                .unwrap();
            for region in regions() {
                let expected_pixels: Vec<_> = (region.y..region.y + region.height)
                    .flat_map(|y| {
                        full.pixels()[(y * graph.width + region.x) as usize
                            ..(y * graph.width + region.x + region.width) as usize]
                            .to_vec()
                    })
                    .collect();
                let mut result = renderer
                    .evaluate_region(graph.clone(), region, None, &mut decoder, &cancel, None)
                    .unwrap();
                assert_eq!(result.statistics.readback_bytes, 0);
                let actual = result.readback(&cancel).unwrap();
                let error = actual
                    .pixels()
                    .iter()
                    .flatten()
                    .zip(expected_pixels.iter().flatten())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0_f32, f32::max);
                assert!(error < 0.0001, "GPU region error {error}");
                let mut display = renderer.output(&result, None, &cancel).unwrap();
                assert_eq!(display.dimensions(), region.dimensions());
                assert_eq!(
                    display.readback(&cancel).unwrap().dimensions(),
                    region.dimensions()
                );
            }
        }
    }
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires Vulkan"]
fn gpu_region_reduces_working_memory_without_source_supersampling() {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap();
    let graph = RenderGraph {
        width: 640,
        height: 360,
        nodes: vec![
            Op::Solid {
                rgba: [0.1, 0.2, 0.3, 0.5],
            },
            Op::Gaussian {
                input: 0,
                size: [5.; 2],
                edges: Edges::Transparent,
            },
            Op::Grade {
                input: 1,
                gain: [1.2, 0.8, 1.],
            },
        ],
        output: 2,
    };
    let region = Region {
        x: 240,
        y: 140,
        width: 120,
        height: 80,
    };
    let mut peaks = vec![];
    for crop in [None, Some(region)] {
        let (host, _) =
            pollster::block_on(fold_render::gpu::Host::headless(32 * 1024 * 1024)).unwrap();
        let mut renderer = fold_render::gpu::Renderer::new(host.clone()).unwrap();
        let cancel = fold_media::Cancel::default();
        let mut decoder = fold_media::Decoder::default();
        let mut result = match crop {
            Some(region) => {
                renderer.evaluate_region(graph.clone(), region, None, &mut decoder, &cancel, None)
            }
            None => renderer.evaluate(graph.clone(), None, &mut decoder, &cancel),
        }
        .unwrap();
        // Capture allocation peaks before explicit test readback adds staging.
        peaks.push(host.memory().peak);
        let _ = result.readback(&cancel).unwrap();
    }
    eprintln!("full/ROI working allocation peaks: {peaks:?}");
    assert!(
        peaks[1] * 2 < peaks[0],
        "ROI must reduce actual allocations, not merely crop a full render"
    );
}

#[test]
fn translated_vector_regions_keep_fractional_coverage() {
    use fold_render::vector::{Drawing, Segment};
    let drawings = std::sync::Arc::new(vec![Drawing {
        path: std::sync::Arc::new(vec![
            Segment::Move([0., 0.]),
            Segment::Quad([45., -12.], [70., 25.]),
            Segment::Line([0., 45.]),
            Segment::Close,
        ]),
        transform: [1., 0., 0., 1., 45.25, 23.5],
        fill: Some([0.8, 0.2, 0.1, 0.7]),
        even_odd: false,
        stroke: None,
    }]);
    let graph = RenderGraph {
        width: 160,
        height: 100,
        nodes: vec![
            Op::Vector(drawings),
            Op::Gaussian {
                input: 0,
                size: [1.; 2],
                edges: Edges::Transparent,
            },
        ],
        output: 1,
    };
    let region = regions()[0];
    let expected = crop(fold_render::render(graph.clone()).unwrap(), region).unwrap();
    let plan = Plan::new(graph, region, false).unwrap();
    let actual = crop(fold_render::render(plan.graph).unwrap(), plan.output).unwrap();
    assert_pixels(&actual, &expected, 0.000001);
}
