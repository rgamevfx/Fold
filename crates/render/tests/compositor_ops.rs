use fold_render::{ImageOp as Op, RenderGraph, render};
fn graph(nodes: Vec<Op>) -> RenderGraph {
    RenderGraph {
        width: 3,
        height: 3,
        output: nodes.len() - 1,
        nodes,
    }
}
fn red() -> Op {
    Op::Solid {
        rgba: [0.5, 0.0, 0.0, 0.5],
    }
}
#[test]
fn crop_blur_grade_and_mask_have_explicit_premultiplied_semantics() {
    let cropped = vec![
        red(),
        Op::Crop {
            input: 0,
            rect: [1, 1, 2, 2],
        },
    ];
    let frame = render(graph(cropped.clone())).unwrap();
    assert_eq!(frame.pixels()[4], [0.5, 0.0, 0.0, 0.5]);
    assert_eq!(frame.pixels().iter().filter(|p| p[3] > 0.0).count(), 1);
    let mut blurred = cropped;
    blurred.push(Op::Blur {
        input: 1,
        radius: 1,
    });
    for p in render(graph(blurred)).unwrap().pixels() {
        assert!((p[0] - 0.5 / 9.0).abs() < 1e-7);
        assert_eq!(p[0], p[3]);
    }
    let frame = render(graph(vec![
        red(),
        Op::Grade {
            input: 0,
            gain: [4.0, 1.0, 1.0],
        },
        Op::Solid {
            rgba: [0.0, 0.0, 0.0, 0.25],
        },
        Op::Mask { input: 1, mask: 2 },
    ]))
    .unwrap();
    assert_eq!(frame.pixels(), &[[0.125, 0.0, 0.0, 0.125]; 9]);
}
#[test]
fn blur_matches_naive_transparent_border_kernel_for_small_and_large_radii() {
    for radius in [0, 1, 2, 4, 64] {
        let base = vec![
            red(),
            Op::Crop {
                input: 0,
                rect: [0, 1, 2, 3],
            },
        ];
        let original = render(graph(base.clone())).unwrap();
        let mut ops = base;
        ops.push(Op::Blur { input: 1, radius });
        let actual = render(graph(ops)).unwrap();
        let r = radius as i32;
        for y in 0..3 {
            for x in 0..3 {
                let mut expected = [0.0; 4];
                for sy in y - r..=y + r {
                    for sx in x - r..=x + r {
                        if (0..3).contains(&sx) && (0..3).contains(&sy) {
                            for (c, v) in expected.iter_mut().enumerate() {
                                *v += original.pixels()[(sy * 3 + sx) as usize][c]
                                    / ((2 * r + 1) * (2 * r + 1)) as f32;
                            }
                        }
                    }
                }
                for (a, e) in actual.pixels()[(y * 3 + x) as usize].iter().zip(expected) {
                    assert!((a - e).abs() < 1e-6);
                }
            }
        }
    }
}
#[test]
fn invalid_effects_fail_and_fragments_remap_every_input() {
    for op in [
        Op::Crop {
            input: 0,
            rect: [2, 0, 1, 2],
        },
        Op::Blur {
            input: 0,
            radius: 65,
        },
        Op::Grade {
            input: 0,
            gain: [f32::NAN, 1.0, 1.0],
        },
        Op::Mask { input: 0, mask: 2 },
    ] {
        assert!(render(graph(vec![red(), op])).is_err());
    }
    let fragment = graph(vec![
        red(),
        Op::Crop {
            input: 0,
            rect: [0, 0, 2, 2],
        },
        Op::Blur {
            input: 1,
            radius: 1,
        },
        Op::Grade {
            input: 2,
            gain: [0.5; 3],
        },
        Op::Mask { input: 3, mask: 1 },
    ]);
    let expected = render(fragment.clone()).unwrap();
    let mut parent = graph(vec![Op::Solid { rgba: [1.0; 4] }]);
    parent.output = parent.append(fragment).unwrap();
    assert_eq!(render(parent).unwrap().pixels(), expected.pixels());
}
