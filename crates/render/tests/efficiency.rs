//! Reproducible release microprofiles; native desktop acceptance is separate.
#![cfg(feature = "gpu")]
use fold_render::{
    ImageOp, RenderGraph,
    gpu::{Host, Renderer},
    vector::{Drawing, Segment},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn fixture(kind: &str, frame: u32, sources: &[fold_media::VideoSource]) -> RenderGraph {
    let mut nodes = vec![ImageOp::Solid {
        rgba: [0.08, 0.12, 0.2, 1.],
    }];
    if kind == "video" {
        nodes[0] = ImageOp::Video {
            source: sources[0].clone(),
            time: sources[0].info.time(frame).unwrap(),
        };
        nodes.push(ImageOp::Video {
            source: sources[1].clone(),
            time: sources[1].info.time(frame).unwrap(),
        });
    } else if kind == "motion" {
        let drawings = (0..64)
            .map(|i| Drawing {
                path: Arc::new(vec![
                    Segment::Move([0., 0.]),
                    Segment::Line([80., 0.]),
                    Segment::Line([80., 50.]),
                    Segment::Line([0., 50.]),
                    Segment::Close,
                ]),
                transform: [
                    1.,
                    0.,
                    0.,
                    1.,
                    f64::from(i % 8 * 220 + frame % 60),
                    f64::from(i / 8 * 120),
                ],
                fill: Some([0.8, 0.2, 0.1, 0.7]),
                even_odd: false,
                stroke: None,
            })
            .collect();
        nodes.push(ImageOp::Vector(Arc::new(drawings)));
    } else {
        nodes.push(ImageOp::Solid {
            rgba: [0.4, 0.15, 0.1, 0.6],
        });
        nodes.push(ImageOp::Crop {
            input: 1,
            rect: [64, 32, 1800, 1000],
        });
    }
    let input = nodes.len() - 1;
    nodes.push(ImageOp::Opacity {
        input,
        opacity: 0.5 + (frame % 30) as f32 / 100.,
    });
    nodes.push(ImageOp::Over {
        foreground: nodes.len() - 1,
        background: 0,
    });
    if kind == "compositor" {
        for _ in 0..4 {
            nodes.push(ImageOp::Opacity {
                input: nodes.len() - 1,
                opacity: 0.9,
            });
            nodes.push(ImageOp::Over {
                foreground: nodes.len() - 1,
                background: 0,
            });
        }
    }
    RenderGraph {
        width: 1920,
        height: 1080,
        output: nodes.len() - 1,
        nodes,
    }
}

#[test]
#[ignore = "release native GTX 1070 profile, pinned OCIO and two video fixtures"]
fn render_efficiency_profile() {
    let root = std::path::PathBuf::from(std::env::var_os("FOLD_TEST_COLOR_ROOT").unwrap());
    let runtime = fold_color::Runtime::at(&root).unwrap();
    let config = runtime.bundled().unwrap();
    let cancel = fold_media::Cancel::default();
    let sources: Vec<_> = ["FOLD_NATIVE_FIXTURE", "FOLD_NATIVE_SECOND_FIXTURE"]
        .iter()
        .map(|key| {
            fold_media::inspect(std::path::Path::new(&std::env::var(key).unwrap()), &cancel)
                .unwrap()
        })
        .collect();
    let count: u32 = std::env::var("FOLD_PROFILE_FRAMES")
        .unwrap_or("180".into())
        .parse()
        .unwrap();
    assert!(count > 30 && count <= sources.iter().map(|s| s.info.frames).min().unwrap());
    for kind in ["compositor", "motion", "video"] {
        let (host, info) = pollster::block_on(Host::headless(Host::DEFAULT_BUDGET)).unwrap();
        eprintln!("adapter={info:?}, workload={kind}");
        let mut renderer = Renderer::new(host.clone()).unwrap();
        let mut decoder = fold_media::Decoder::from_environment().unwrap();
        for cached in [false, true] {
            for frame in 0..count {
                let begin = Instant::now();
                let graph = fixture(kind, if cached { 0 } else { frame }, &sources);
                let compile_ns = begin.elapsed().as_nanos();
                let scene = renderer
                    .evaluate(graph, Some(&config), &mut decoder, &cancel)
                    .unwrap();
                while !scene.is_ready().unwrap() {
                    host.poll().unwrap();
                    assert!(begin.elapsed() < Duration::from_secs(30));
                    std::thread::sleep(Duration::from_micros(100));
                }
                println!(
                    "PROFILE {kind} {cached} {frame} wall_ns={} compile_ns={compile_ns} gpu_ns={} stats={:?} memory={:?}",
                    begin.elapsed().as_nanos(),
                    scene.gpu_nanoseconds().unwrap().unwrap(),
                    scene.statistics,
                    host.memory()
                );
            }
        }
    }
}
