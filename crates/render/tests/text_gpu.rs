//! M4 vector candidate qualification, including explicit RGBA16F output storage.
//! Spatial integration and mixed-scene rendering remain separate qualifications.
#![cfg(feature = "gpu")]
use fold_render::{
    ImageOp, RenderGraph,
    gpu::{Host, Renderer},
    text::{Service, outlines},
    vector::{Cap, Drawing, Join, Segment, Stroke},
};
use std::sync::Arc;

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn retained_text_outlines_match_cpu_across_zoom_alpha_and_paint_order() {
    let (host, info) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    eprintln!("M4 text vector adapter: {info:?}; intermediate=RGBA32F; wgpu=27; no history");
    let mut renderer = Renderer::new(host).unwrap();
    let mut text = Service::new(
        include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
        "Noto Sans",
    )
    .unwrap();
    let layout = text.layout("O B 8", 48., None).unwrap();
    eprintln!(
        "font_sha256={:02x?}; layout={:02x?}",
        layout.font_hash, layout.identity
    );
    let cancel = fold_media::Cancel::default();
    let outlines = outlines::prepare(layout, &cancel).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let marker_path = Arc::new(vec![
        Segment::Move([520., 80.]),
        Segment::Line([600., 80.]),
        Segment::Line([600., 140.]),
        Segment::Line([520., 140.]),
        Segment::Close,
    ]);
    for scale in [0.25, 1., 4.] {
        let mut drawings = outlines.drawings(
            Some([0.8, 0.2, 0.1, 0.6]),
            Some(Stroke {
                color: [1., 1., 1., 0.75],
                width: 0.5,
                cap: Cap::Round,
                join: Join::Round,
            }),
        );
        for drawing in &mut drawings {
            for value in &mut drawing.transform {
                *value *= scale;
            }
        }
        for fill in [[0.1, 0.6, 0.2, 0.25], [0.8, 0.2, 0.1, 0.5]] {
            drawings.push(Drawing {
                path: marker_path.clone(),
                transform: [1., 0., 0., 1., 0., 0.],
                fill: Some(fill),
                even_odd: false,
                stroke: None,
            });
        }
        let plan = RenderGraph {
            width: 640,
            height: 220,
            nodes: vec![ImageOp::Vector(Arc::new(drawings))],
            output: 0,
        };
        let cpu = fold_render::render(plan.clone()).unwrap();
        let mut gpu = renderer
            .evaluate(plan, None, &mut decoder, &cancel)
            .unwrap();
        assert_eq!(gpu.statistics.cpu_adapter_nodes, 0);
        assert_eq!(gpu.statistics.readback_bytes, 0);
        assert!(gpu.statistics.vector_instances > 0);
        let actual = gpu.readback(&cancel).unwrap();
        let max = actual
            .pixels()
            .iter()
            .flatten()
            .zip(cpu.pixels().iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(max <= 2e-3, "scale={scale}: linear error={max}");
        assert_eq!(actual.pixels()[0], [0.; 4]);
        // Independent interior reference: straight fills premultiply once, then Over.
        let pixel = actual.pixels()[100 * 640 + 550];
        for (actual, expected) in pixel.into_iter().zip([
            0.8 * 0.5 + 0.1 * 0.25 * 0.5,
            0.2 * 0.5 + 0.6 * 0.25 * 0.5,
            0.1 * 0.5 + 0.2 * 0.25 * 0.5,
            0.5 + 0.25 * 0.5,
        ]) {
            assert!(
                (actual - expected).abs() < 2e-3,
                "paint order/premultiplication {actual} != {expected}"
            );
        }
        let mut gpu = renderer.rgba16(gpu, &cancel).unwrap();
        let half = gpu.readback(&cancel).unwrap();
        let half_error = half
            .pixels()
            .iter()
            .flatten()
            .zip(cpu.pixels().iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(
            half_error <= 2e-3,
            "scale={scale}: RGBA16F error={half_error}"
        );
        eprintln!("scale={scale}; RGBA16F linear error={half_error}");
        let expected_display = cpu.over_black().to_display().unwrap();
        let display = renderer
            .output(&gpu, None, &cancel)
            .unwrap()
            .readback(&cancel)
            .unwrap();
        let max_display = display
            .rgba()
            .iter()
            .zip(expected_display.rgba())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(
            max_display <= 1,
            "scale={scale}: display error={max_display}"
        );
        eprintln!(
            "scale={scale}; max_linear_error={max}; max_display_byte_error={max_display}; vector_instances={}",
            gpu.statistics.vector_instances
        );
        if let Some(directory) = std::env::var_os("FOLD_TEXT_GPU_CAPTURE_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let mut ppm = b"P6\n640 220\n255\n".to_vec();
            for pixel in display.rgba().chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(directory.join(format!("text-{scale}.ppm")), ppm).unwrap();
        }
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn vector_fill_rules_strokes_and_rectangular_clip_match_reference() {
    let (host, info) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    eprintln!("M4 vector fixtures: {info:?}; intermediate=RGBA32F");
    let mut renderer = Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let outer = vec![
        Segment::Move([8., 8.]),
        Segment::Line([56., 8.]),
        Segment::Line([56., 56.]),
        Segment::Line([8., 56.]),
        Segment::Close,
    ];
    let inner = vec![
        Segment::Move([24., 24.]),
        Segment::Line([40., 24.]),
        Segment::Line([40., 40.]),
        Segment::Line([24., 40.]),
        Segment::Close,
    ];
    let reverse = vec![
        Segment::Move([24., 24.]),
        Segment::Line([24., 40.]),
        Segment::Line([40., 40.]),
        Segment::Line([40., 24.]),
        Segment::Close,
    ];
    let filled = |path, even_odd| Drawing {
        path: Arc::new(path),
        transform: [1., 0., 0., 1., 0., 0.],
        fill: Some([0.2, 0.4, 0.8, 1.]),
        even_odd,
        stroke: None,
    };
    let stroked = |path, width, cap, join| Drawing {
        path: Arc::new(path),
        transform: [1., 0., 0., 1., 0., 0.],
        fill: None,
        even_odd: false,
        stroke: Some(Stroke {
            color: [0.2, 0.4, 0.8, 1.],
            width,
            cap,
            join,
        }),
    };
    let mut same = outer.clone();
    same.extend(inner);
    let mut opposite = outer.clone();
    opposite.extend(reverse);
    let line = vec![Segment::Move([20., 30.]), Segment::Line([40., 30.])];
    let corner = vec![
        Segment::Move([20., 40.]),
        Segment::Line([20., 20.]),
        Segment::Line([40., 20.]),
    ];
    let cases = [
        (
            "nonzero_same",
            filled(same.clone(), false),
            false,
            [32, 32],
            1.,
        ),
        ("even_odd", filled(same, true), false, [32, 32], 0.),
        (
            "nonzero_opposite",
            filled(opposite, false),
            false,
            [32, 32],
            0.,
        ),
        (
            "self_overlap",
            filled(
                vec![
                    Segment::Move([8., 8.]),
                    Segment::Line([56., 56.]),
                    Segment::Line([8., 56.]),
                    Segment::Line([56., 8.]),
                    Segment::Close,
                ],
                false,
            ),
            false,
            [32, 16],
            1.,
        ),
        (
            "butt",
            stroked(line.clone(), 10., Cap::Butt, Join::Miter),
            false,
            [16, 29],
            0.,
        ),
        (
            "round",
            stroked(line, 10., Cap::Round, Join::Round),
            false,
            [16, 29],
            1.,
        ),
        (
            "miter",
            stroked(corner.clone(), 10., Cap::Butt, Join::Miter),
            false,
            [15, 15],
            1.,
        ),
        (
            "round_join",
            stroked(corner, 10., Cap::Butt, Join::Round),
            false,
            [15, 15],
            0.,
        ),
        ("clip", filled(outer, false), true, [10, 32], 0.),
        (
            "thin",
            stroked(
                vec![Segment::Move([20., 20.5]), Segment::Line([40., 20.5])],
                0.25,
                Cap::Butt,
                Join::Miter,
            ),
            false,
            [30, 20],
            0.25,
        ),
    ];
    for (name, drawing, clip, probe, alpha) in cases {
        for scale in [0.25, 1., 4.] {
            let mut drawing = drawing.clone();
            for value in &mut drawing.transform {
                *value *= scale;
            }
            let extent = (64. * scale) as u32;
            let mut plan = RenderGraph {
                width: extent,
                height: extent,
                nodes: vec![ImageOp::Vector(Arc::new(vec![drawing]))],
                output: 0,
            };
            if clip {
                plan.nodes.push(ImageOp::Crop {
                    input: 0,
                    rect: [
                        (16. * scale) as u32,
                        (16. * scale) as u32,
                        (48. * scale) as u32,
                        (48. * scale) as u32,
                    ],
                });
                plan.output = 1;
            }
            let cpu = fold_render::render(plan.clone()).unwrap();
            let mut result = renderer
                .evaluate(plan, None, &mut decoder, &cancel)
                .unwrap();
            assert_eq!(result.statistics.cpu_adapter_nodes, 0);
            let gpu = result.readback(&cancel).unwrap();
            let max = gpu
                .pixels()
                .iter()
                .flatten()
                .zip(cpu.pixels().iter().flatten())
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            assert!(max <= 2e-3, "{name} scale {scale}: {max}");
            if scale == 1. {
                assert!(
                    (gpu.pixels()[probe[1] * 64 + probe[0]][3] - alpha).abs() <= 2e-3,
                    "independent {name} probe"
                );
            }
            if name == "thin" {
                let column = (30. * scale) as usize;
                let coverage: f32 = (0..extent as usize)
                    .map(|y| gpu.pixels()[y * extent as usize + column][3])
                    .sum();
                assert!(
                    (coverage - 0.25 * scale as f32).abs() <= 2e-3,
                    "thin coverage {coverage}"
                );
            }
            if clip {
                assert_eq!(
                    gpu.pixels()[(32. * scale) as usize * extent as usize + (32. * scale) as usize]
                        [3],
                    1.
                );
            }
            eprintln!("fixture={name}; scale={scale}; max_linear_error={max}");
        }
    }
}
