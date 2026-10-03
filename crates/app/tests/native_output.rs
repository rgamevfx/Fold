#![cfg(feature = "native-video")]
use fold_media::Cancel;
use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "requires Vulkan/NVDEC, packaged OCIO/helper and FOLD_NATIVE_FIXTURE"]
fn still_cli_honors_native_backend_and_matches_software_delivery() {
    let root = tempfile::tempdir().unwrap();
    let mut project = fold_app::color::new_project(8);
    let batch = fold_timeline::import_media(
        &project.snapshot(),
        &fold_timeline::ImportArgs {
            document: None,
            paths: vec![PathBuf::from(
                std::env::var_os("FOLD_NATIVE_FIXTURE").unwrap(),
            )],
            at: None,
            video_track: None,
            audio_track: None,
            audio_only: false,
        },
        &Cancel::default(),
    )
    .unwrap();
    project.commit(batch).unwrap();
    let (source, _) = fold_timeline::active(&project.snapshot()).unwrap();
    project
        .commit(
            fold_app::media_workflow::select_output(&project.snapshot(), source.document).unwrap(),
        )
        .unwrap();
    project
        .commit(
            fold_app::color::set_output(
                &project.snapshot(),
                source.document,
                fold_color::settings::OutputTransform {
                    display: "sRGB - Display".into(),
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
    let path = root.path().join("project.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let run = |backend: &str, decoder: &str, name: &str| {
        Command::new(env!("CARGO_BIN_EXE_fold-cli"))
            .env("FOLD_RENDER_BACKEND", backend)
            .env("FOLD_VIDEO_DECODER", decoder)
            .arg("render")
            .arg(&path)
            .arg(root.path().join(name))
            .args(["31", "17", "0", "1"])
            .output()
            .unwrap()
    };
    let bad = run("invalid", "software", "invalid.ppm");
    assert!(!bad.status.success());
    assert!(!root.path().join("invalid.ppm").exists());
    for (backend, decoder, name) in [
        ("cpu", "software", "cpu.ppm"),
        ("gpu", "cuda-native", "gpu.ppm"),
    ] {
        let result = run(backend, decoder, name);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let a = std::fs::read(root.path().join("cpu.ppm")).unwrap();
    let b = std::fs::read(root.path().join("gpu.ppm")).unwrap();
    assert_eq!(a.len(), b.len());
    assert!(a.iter().zip(&b).all(|(a, b)| a.abs_diff(*b) <= 1));
}
