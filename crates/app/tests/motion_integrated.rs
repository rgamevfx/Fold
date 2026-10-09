#![cfg(feature = "spatial")]
//! Render the project authored by the public CLI workflow, without rebuilding its nodes here.
use fold_app::output::{SceneRequest, evaluate_scene_gpu};
use fold_foundation::{ObjectId, Time};
use fold_render::gpu::{Host, Renderer};

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn saved_integrated_motion_renders_required_times_and_retains_frames() {
    let project = fold_project::load(
        std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../demos/m5b-integrated.fold"
        )),
        8,
    )
    .unwrap();
    let snapshot = project.snapshot();
    let document = *snapshot.state().documents.keys().next().unwrap();
    let source = fold_app::packages::builtins()
        .default_output(&snapshot, document)
        .unwrap();
    let output: ObjectId = serde_json::from_value(serde_json::json!(source.output)).unwrap();
    let (host, info) = pollster::block_on(Host::headless(512 * 1024 * 1024)).unwrap();
    eprintln!("Integrated Motion GPU: {info:?}");
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let mut held = None;
    let mut settled = None;
    for (numerator, denominator) in [
        (0, 1),
        (1, 4),
        (1, 2),
        (3, 4),
        (1, 1),
        (2, 1),
        (4, 1),
        (0, 1),
    ] {
        let time = Time::new(numerator, denominator).unwrap();
        let request = SceneRequest {
            source: source.clone(),
            time,
            dimensions: [1920, 1080],
            region: None,
            preview: None,
        };
        let mut frame = if time == Time::new(4, 1).unwrap() {
            // The native construction has a well-defined endpoint. The VideoProvider
            // currently has a half-open duration, so record that boundary explicitly.
            assert!(
                evaluate_scene_gpu(&snapshot, &request, &mut renderer, &mut decoder, &cancel)
                    .is_err()
            );
            let graph = fold_motion::rendering::image(
                &snapshot,
                document,
                output,
                time,
                request.dimensions,
                Default::default(),
                &|| Ok(cancel.check()?),
            )
            .unwrap();
            renderer
                .evaluate(graph, None, &mut decoder, &cancel)
                .unwrap()
        } else {
            let frame =
                evaluate_scene_gpu(&snapshot, &request, &mut renderer, &mut decoder, &cancel)
                    .unwrap();
            assert_eq!(frame.descriptor().unwrap().timing.unwrap().timestamp, time);
            frame
        };
        assert_eq!(frame.statistics.cpu_adapter_nodes, 0);
        assert_eq!(frame.statistics.readback_bytes, 0);
        let pixels = frame.readback(&cancel).unwrap();
        let alpha_error = pixels
            .pixels()
            .iter()
            .map(|p| (p[3] - 1.).abs())
            .fold(0_f32, f32::max);
        assert!(
            pixels
                .pixels()
                .iter()
                .all(|p| p.iter().all(|v| v.is_finite()))
        );
        // Frozen acceptance allows 0.002 absolute/channel, including alpha.
        assert!(alpha_error <= 0.002, "alpha error {alpha_error}");
        for (actual, expected) in pixels.pixels()[0].iter().zip([0.015, 0.02, 0.035, 1.]) {
            assert!((actual - expected).abs() < 0.002);
        }
        assert!(pixels.pixels().iter().any(|p| p[0] > 0.9 && p[1] < 0.2));
        if time == Time::new(2, 1).unwrap() {
            settled = Some(pixels.pixels().to_vec());
        }
        if time == Time::new(4, 1).unwrap() {
            assert_eq!(pixels.pixels(), settled.as_ref().unwrap());
        }
        if let Some((old, original)) = &mut held {
            let old: &mut fold_render::gpu::GpuFrame = old;
            assert_eq!(old.readback(&cancel).unwrap().pixels(), original);
            if time == Time::ZERO {
                assert_eq!(pixels.pixels(), original);
            }
        }
        if let Some(directory) = std::env::var_os("FOLD_M5_RENDER_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let display = renderer
                .output(&frame, None, &cancel)
                .unwrap()
                .readback(&cancel)
                .unwrap();
            let mut ppm = b"P6\n1920 1080\n255\n".to_vec();
            for pixel in display.rgba().chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(
                directory.join(format!("integrated-{numerator}-{denominator}.ppm")),
                ppm,
            )
            .unwrap();
        }
        if held.is_none() {
            held = Some((frame, pixels.pixels().to_vec()));
        }
        eprintln!(
            "Integrated time={numerator}/{denominator}: finite output, alpha error={alpha_error}, scene colors, exact registered timing except native endpoint, zero CPU image transfer, retained-frame/reseek checks passed"
        );
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn saved_control_variation_changes_rendered_text_and_palette_without_mutating_held_frames() {
    let (host, info) = pollster::block_on(Host::headless(512 * 1024 * 1024)).unwrap();
    eprintln!("Integrated control variation GPU: {info:?}");
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let mut held = None;
    let mut baseline_pixels: Option<Vec<[f32; 4]>> = None;
    for name in ["baseline", "variation", "baseline"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../docs/rewrite/evidence/m5b-controls-{name}.fold"
        ));
        let project = fold_project::load(&path, 8).unwrap();
        let snapshot = project.snapshot();
        let document = *snapshot.state().documents.keys().next().unwrap();
        let source = fold_app::packages::builtins()
            .default_output(&snapshot, document)
            .unwrap();
        let time = Time::new(1, 1).unwrap();
        let mut frame = evaluate_scene_gpu(
            &snapshot,
            &SceneRequest {
                source,
                time,
                dimensions: [1920, 1080],
                region: None,
                preview: None,
            },
            &mut renderer,
            &mut decoder,
            &cancel,
        )
        .unwrap();
        assert_eq!(frame.descriptor().unwrap().timing.unwrap().timestamp, time);
        assert_eq!(frame.statistics.cpu_adapter_nodes, 0);
        assert_eq!(frame.statistics.readback_bytes, 0);
        let pixels = frame.readback(&cancel).unwrap();
        assert!(
            pixels
                .pixels()
                .iter()
                .all(|p| p.iter().all(|v| v.is_finite()) && (p[3] - 1.).abs() <= 0.002)
        );
        if let Some(original) = &baseline_pixels {
            if name == "variation" {
                // Screen wording lives in the upper title region; clone palette
                // and count live below it. The native public test verifies exact
                // counts and formulas; these checks establish visible propagation.
                let changed = |rows: std::ops::Range<usize>| {
                    rows.flat_map(|y| y * 1920..(y + 1) * 1920)
                        .filter(|&i| pixels.pixels()[i] != original[i])
                        .count()
                };
                let title_changes = changed(100..300);
                let clone_changes = changed(580..940);
                assert!(title_changes > 1000, "Wording did not visibly change");
                assert!(clone_changes > 1000, "Cloner did not visibly change");
                let green = pixels.pixels()[580 * 1920..940 * 1920]
                    .iter()
                    .filter(|p| p[1] > 0.5 && p[0] < 0.1)
                    .count();
                assert!(green > 100, "New palette did not reach rendered copies");
                eprintln!(
                    "Control variation: {title_changes} changed title pixels, {clone_changes} changed clone-region pixels, {green} green pixels"
                );
            } else {
                assert_eq!(pixels.pixels(), original);
            }
        } else {
            baseline_pixels = Some(pixels.pixels().to_vec());
        }
        if let Some(old) = &mut held {
            let old: &mut fold_render::gpu::GpuFrame = old;
            assert_eq!(
                old.readback(&cancel).unwrap().pixels(),
                baseline_pixels.as_ref().unwrap()
            );
        }
        if let Some(directory) = std::env::var_os("FOLD_M5_RENDER_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let display = renderer
                .output(&frame, None, &cancel)
                .unwrap()
                .readback(&cancel)
                .unwrap();
            let mut ppm = b"P6\n1920 1080\n255\n".to_vec();
            for pixel in display.rgba().chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(directory.join(format!("controls-{name}.ppm")), ppm).unwrap();
        }
        if held.is_none() {
            held = Some(frame);
        }
    }
}
