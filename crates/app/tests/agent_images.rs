//! The shipped demo runs through the real CLI transport and generated discovery.
#![cfg(target_os = "linux")]
use fold_app::session::Session;
use fold_platform::desktop::DesktopClient;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn public_image_sampling_demo_roundtrips_and_measures_clone_colors() {
    workflow(false, false);
}
#[test]
#[cfg(feature = "spatial")]
#[ignore = "requires Vulkan and the packaged OCIO runtime (FOLD_COLOR_ROOT)"]
fn public_image_sampling_native_frame() {
    workflow(true, true);
}
fn workflow(aces: bool, gpu: bool) {
    let directory = tempfile::tempdir().unwrap();
    let mut session = if aces {
        Session::default()
    } else {
        Session::new(fold_project::Project::new(16))
    };
    #[cfg(feature = "spatial")]
    if gpu {
        session.set_render_host(
            pollster::block_on(fold_render::gpu::Host::headless(256 * 1024 * 1024))
                .unwrap()
                .0,
        );
    }
    #[cfg(not(feature = "spatial"))]
    assert!(!gpu);
    let socket = session.enable_agent_transport(directory.path()).unwrap();
    let destination = directory.path().join("image-cloner.fold");
    let report = directory.path().join("report.json");
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/image-sampling.py");
    let child = Command::new("python3")
        .arg(script)
        .arg(env!("CARGO_BIN_EXE_fold-cli"))
        .arg(socket)
        .arg(&destination)
        .arg(&report)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let waiter = std::thread::spawn(move || child.wait_with_output().unwrap());
    let deadline = Instant::now() + Duration::from_secs(120);
    while !waiter.is_finished() {
        assert!(Instant::now() < deadline, "demo deadline");
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    let result = waiter.join().unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let project = fold_project::load(&destination, 16).unwrap();
    assert_eq!(project.snapshot().state().documents.len(), 2);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(
        report["observation"]["samples"].as_array().unwrap().len(),
        3
    );
    let registry = fold_app::packages::builtins();
    let id = serde_json::from_value(report["cloner_document"].clone()).unwrap();
    let output = registry.default_output(&project.snapshot(), id).unwrap();
    for dimensions in [[64, 32], [640, 320]] {
        let graph = registry
            .video_with(
                &project.snapshot(),
                &output,
                fold_foundation::Time::ZERO,
                dimensions,
                &Default::default(),
            )
            .unwrap();
        let colors: Vec<_> = graph
            .nodes
            .iter()
            .filter_map(|n| {
                if let fold_render::ImageOp::Spatial { scene, .. } = n {
                    Some(
                        scene
                            .materials
                            .values()
                            .map(|m| m.color)
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        for expected in [[1., 0., 0., 1.], [0., 1., 0., 1.]] {
            assert!(colors.iter().any(|color| {
                color
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (a - b).abs() < 1e-5)
            }));
        }
    }
    if let Ok(path) = std::env::var("FOLD_IMAGE_EVIDENCE") {
        let path = std::path::Path::new(&path);
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("public-workflow.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        std::fs::copy(destination, path.join("image-cloner.fold")).unwrap();
        if let Some(frame) = report["frame"]["file"].as_str() {
            std::fs::copy(frame, path.join("native-cloner.png")).unwrap();
        }
    }
}
