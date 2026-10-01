use fold_render::{Affine, ImageOp, RenderGraph, render};

fn solid(rgba: [f32; 4]) -> ImageOp {
    ImageOp::Solid { rgba }
}
fn graph(nodes: Vec<ImageOp>) -> RenderGraph {
    RenderGraph {
        width: 3,
        height: 2,
        output: nodes.len() - 1,
        nodes,
    }
}
fn transform(input: usize, transform: Affine) -> ImageOp {
    ImageOp::Transform { input, transform }
}
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const CLEAR: [f32; 4] = [0.0; 4];

#[test]
fn transforms_use_pixel_centers_and_transparent_borders() {
    let translated = graph(vec![
        solid(RED),
        transform(
            0,
            Affine {
                tx: 1.0,
                ty: 1.0,
                ..Affine::IDENTITY
            },
        ),
    ]);
    assert_eq!(
        render(translated).unwrap().pixels(),
        &[CLEAR, CLEAR, CLEAR, CLEAR, RED, RED]
    );
    let scaled = graph(vec![
        solid(RED),
        transform(
            0,
            Affine {
                a: 0.5,
                d: 0.5,
                ..Affine::IDENTITY
            },
        ),
    ]);
    assert_eq!(
        render(scaled).unwrap().pixels(),
        &[RED, CLEAR, CLEAR, CLEAR, CLEAR, CLEAR]
    );
    // 90 degrees clockwise about the origin, then translate into the raster.
    let rotated = graph(vec![
        solid(RED),
        transform(
            0,
            Affine {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                tx: 2.0,
                ty: 0.0,
            },
        ),
    ]);
    assert_eq!(
        render(rotated).unwrap().pixels(),
        &[RED, RED, CLEAR, RED, RED, CLEAR]
    );
    let identity = graph(vec![solid(RED), transform(0, Affine::IDENTITY)]);
    assert_eq!(render(identity).unwrap().pixels(), &[RED; 6]);
    let offscreen = graph(vec![
        solid(RED),
        transform(
            0,
            Affine {
                tx: -3.0,
                ..Affine::IDENTITY
            },
        ),
    ]);
    assert_eq!(render(offscreen).unwrap().pixels(), &[CLEAR; 6]);
}

#[test]
fn premultiplied_opacity_and_ordered_over_match_delivery() {
    let plan = graph(vec![
        solid(BLUE),
        solid(RED),
        ImageOp::Opacity {
            input: 1,
            opacity: 0.5,
        },
        transform(
            2,
            Affine {
                tx: 1.0,
                ..Affine::IDENTITY
            },
        ),
        ImageOp::Over {
            foreground: 3,
            background: 0,
        },
    ]);
    let frame = render(plan.clone()).unwrap();
    let purple = [0.5, 0.0, 0.5, 1.0];
    assert_eq!(
        frame.pixels(),
        &[BLUE, purple, purple, BLUE, purple, purple]
    );
    let display = frame.to_display().unwrap();
    let mut ppm = Vec::new();
    render(plan.clone()).unwrap().write_ppm(&mut ppm).unwrap();
    let rgb: Vec<_> = display
        .rgba()
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter().copied())
        .collect();
    assert_eq!(&ppm[b"P6\n3 2\n255\n".len()..], rgb);
    assert_eq!(&display.rgba()[4..8], &[188, 0, 188, 255]);
    let mut reverse = plan;
    reverse.nodes[4] = ImageOp::Over {
        foreground: 0,
        background: 3,
    };
    assert_eq!(render(reverse).unwrap().pixels(), &[BLUE; 6]);
    let transparent = graph(vec![
        solid([0.5, 0.0, 0.0, 0.5]),
        ImageOp::Opacity {
            input: 0,
            opacity: 0.5,
        },
        ImageOp::Over {
            foreground: 1,
            background: 0,
        },
    ]);
    let frame = render(transparent).unwrap();
    assert_eq!(frame.pixels(), &[[0.625, 0.0, 0.0, 0.625]; 6]);
    assert!(frame.to_display().is_err());
    assert!(frame.write_ppm(Vec::new()).is_err());
    for opacity in [0.0, 1.0] {
        let frame = render(graph(vec![
            solid(RED),
            ImageOp::Opacity { input: 0, opacity },
        ]))
        .unwrap();
        assert_eq!(frame.pixels(), &[RED.map(|v| v * opacity); 6]);
    }
}

#[test]
fn shared_inputs_and_duplicate_edges_are_released_correctly() {
    let plan = graph(vec![
        solid([0.5, 0.0, 0.0, 0.5]),
        ImageOp::Over {
            foreground: 0,
            background: 0,
        },
        ImageOp::Opacity {
            input: 0,
            opacity: 0.5,
        },
        ImageOp::Over {
            foreground: 1,
            background: 2,
        },
    ]);
    assert_eq!(
        render(plan).unwrap().pixels(),
        &[[0.8125, 0.0, 0.0, 0.8125]; 6]
    );
}

#[test]
fn validation_rejects_bad_edges_parameters_and_dimensions() {
    for input in [1, 2, usize::MAX] {
        assert!(
            render(graph(vec![
                solid(RED),
                ImageOp::Opacity {
                    input,
                    opacity: 1.0
                }
            ]))
            .is_err()
        );
    }
    // A two-node cycle also violates topological ordering.
    assert!(
        render(graph(vec![
            ImageOp::Opacity {
                input: 1,
                opacity: 1.0
            },
            ImageOp::Opacity {
                input: 0,
                opacity: 1.0
            }
        ]))
        .is_err()
    );
    for opacity in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(
            render(graph(vec![
                solid(RED),
                ImageOp::Opacity { input: 0, opacity }
            ]))
            .is_err()
        );
    }
    for m in [
        Affine {
            a: 0.0,
            ..Affine::IDENTITY
        },
        Affine {
            tx: f64::NAN,
            ..Affine::IDENTITY
        },
        Affine {
            a: f64::INFINITY,
            ..Affine::IDENTITY
        },
        Affine {
            a: f64::MIN_POSITIVE / 16.0,
            ..Affine::IDENTITY
        },
    ] {
        assert!(render(graph(vec![solid(RED), transform(0, m)])).is_err());
    }
    for rgba in [[1.0, 0.0, 0.0, 0.5], [0.0, 0.0, 0.0, -1.0], [f32::NAN; 4]] {
        assert!(render(graph(vec![solid(rgba)])).is_err());
    }
    let mut plan = graph(vec![solid(RED)]);
    plan.output = 1;
    assert!(render(plan.clone()).is_err());
    plan.nodes.clear();
    assert!(render(plan).is_err());
    for (width, height) in [(0, 1), (1, 0), (u32::MAX, u32::MAX)] {
        let mut plan = graph(vec![solid(RED)]);
        plan.width = width;
        plan.height = height;
        assert!(render(plan).is_err());
    }
    assert!(render(graph(vec![solid(RED); 4097])).is_err());
    let mut dead_invalid = graph(vec![
        solid(RED),
        ImageOp::Opacity {
            input: 1,
            opacity: 1.0,
        },
    ]);
    dead_invalid.output = 0;
    assert!(render(dead_invalid).is_err());
}

#[test]
fn live_pixel_budget_releases_intermediates_and_skips_dead_work() {
    // Each frame is 16 MiB. A long chain fits because only two are live.
    let mut plan = RenderGraph {
        width: 1024,
        height: 1024,
        nodes: vec![solid(RED)],
        output: 8,
    };
    for input in 0..8 {
        plan.nodes.push(ImageOp::Opacity {
            input,
            opacity: 1.0,
        });
    }
    assert_eq!(render(plan).unwrap().pixels()[0], RED);
    // Four sources are still needed when the first merge allocates a fifth.
    let mut plan = RenderGraph {
        width: 1024,
        height: 1024,
        nodes: vec![solid(RED); 4],
        output: 6,
    };
    plan.nodes.extend([
        ImageOp::Over {
            foreground: 0,
            background: 1,
        },
        ImageOp::Over {
            foreground: 2,
            background: 3,
        },
        ImageOp::Over {
            foreground: 4,
            background: 5,
        },
    ]);
    assert_eq!(
        render(plan.clone()).unwrap_err(),
        "render graph exceeds 64 MiB live pixel budget"
    );
    // Selecting an earlier output makes all the over-budget branch dead.
    plan.output = 0;
    assert_eq!(render(plan).unwrap().pixels()[0], RED);
}
