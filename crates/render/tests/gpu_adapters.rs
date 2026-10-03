#![cfg(feature = "gpu")]
use fold_render::{
    ImageOp, RenderGraph,
    gpu::{Host, Renderer},
    vector::{Drawing, Segment},
};

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn native_nested_sources_vectors_and_scratch_upload_order() {
    let (host, _) = pollster::block_on(Host::headless(1024 * 1024)).unwrap();
    let mut gpu = Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let image = |rgb: [u8; 3]| {
        ImageOp::Media(fold_media::RgbImage::from_rgb([17, 11], rgb.repeat(17 * 11)).unwrap())
    };
    let drawings = std::sync::Arc::new(vec![Drawing {
        path: std::sync::Arc::new(vec![
            Segment::Move([1., 1.]),
            Segment::Line([16., 3.]),
            Segment::Line([2., 10.]),
            Segment::Close,
        ]),
        transform: [1., 0., 0., 1., 0.25, 0.75],
        fill: Some([0.7, 0.2, 0.1, 0.65]),
        even_odd: false,
        stroke: None,
    }]);
    let mut graph = RenderGraph {
        width: 17,
        height: 11,
        output: 2,
        nodes: vec![
            image([200, 20, 40]),
            ImageOp::Opacity {
                input: 0,
                opacity: 0.8,
            },
            ImageOp::Crop {
                input: 1,
                rect: [2, 1, 15, 10],
            },
        ],
    };
    // The first source's storage is now reusable; the encoded copy of this
    // second source must not run before the first source's opacity/crop reads.
    let nested = graph
        .append(RenderGraph {
            width: 17,
            height: 11,
            nodes: vec![
                image([10, 170, 30]),
                ImageOp::Vector(drawings),
                ImageOp::Over {
                    foreground: 1,
                    background: 0,
                },
            ],
            output: 2,
        })
        .unwrap();
    graph.nodes.push(ImageOp::Over {
        foreground: 2,
        background: nested,
    });
    graph.output = graph.nodes.len() - 1;
    for radius in [0, 1, 4, 64] {
        let mut plan = graph.clone();
        plan.nodes.push(ImageOp::Blur {
            input: plan.output,
            radius,
        });
        plan.output += 1;
        let expected = fold_render::render(plan.clone()).unwrap();
        let mut result = gpu.evaluate(plan, None, &mut decoder, &cancel).unwrap();
        assert_eq!(result.statistics.cpu_adapter_nodes, 3);
        assert_eq!(result.statistics.upload_bytes, 3 * 512 * 11);
        assert_eq!(result.statistics.readback_bytes, 0);
        assert!(result.statistics.status_readback_bytes >= 4);
        let actual = result.readback(&cancel).unwrap();
        for (a, b) in actual
            .pixels()
            .iter()
            .flatten()
            .zip(expected.pixels().iter().flatten())
        {
            assert!((a - b).abs() < 0.000003, "radius {radius}: {a} != {b}");
        }
    }
    for transform in [
        fold_render::Affine {
            a: 0.8,
            b: 0.3,
            c: -0.3,
            d: 0.8,
            tx: 0.125,
            ty: 0.625,
        },
        fold_render::Affine {
            a: -1.,
            d: 1.,
            tx: 17.,
            ..fold_render::Affine::IDENTITY
        },
    ] {
        let mut plan = graph.clone();
        plan.nodes.push(ImageOp::Transform {
            input: plan.output,
            transform,
        });
        plan.output += 1;
        let expected = fold_render::render(plan.clone()).unwrap();
        let actual = gpu
            .evaluate(plan, None, &mut decoder, &cancel)
            .unwrap()
            .readback(&cancel)
            .unwrap();
        for (a, b) in actual
            .pixels()
            .iter()
            .flatten()
            .zip(expected.pixels().iter().flatten())
        {
            assert!((a - b).abs() < 0.000003, "transform: {a} != {b}");
        }
    }
    // Aggregate backpressure also covers held outputs across worker instances.
    let (small, _) = pollster::block_on(Host::headless(20_000)).unwrap();
    let mut first = Renderer::new(small.clone()).unwrap();
    let mut second = Renderer::new(small.clone()).unwrap();
    let plan = RenderGraph {
        width: 17,
        height: 11,
        output: 0,
        nodes: vec![ImageOp::Solid { rgba: [0.5; 4] }],
    };
    let mut held = Vec::new();
    loop {
        match first.evaluate(plan.clone(), None, &mut decoder, &cancel) {
            Ok(frame) => {
                let start = std::time::Instant::now();
                while !frame.is_ready().unwrap() {
                    small.poll().unwrap();
                    assert!(start.elapsed().as_secs() < 10);
                }
                held.push(frame);
            }
            Err(error) => {
                assert!(error.contains("budget"));
                break;
            }
        }
        assert!(held.len() < 10);
    }
    assert!(
        second
            .evaluate(plan.clone(), None, &mut decoder, &cancel)
            .is_err()
    );
    held.clear();
    small.poll().unwrap();
    let mut recovered = second.evaluate(plan, None, &mut decoder, &cancel).unwrap();
    recovered.readback(&cancel).unwrap();
    assert!(small.memory().peak <= small.memory().budget);
}
