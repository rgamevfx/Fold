#![cfg(target_os = "linux")]
//! Integrated artwork authoring through the public CLI/socket, without native-model access in the authoring script.
use fold_app::session::Session;
use fold_foundation::DocumentId;
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_project::{EditBatch, Mutation, Project};
use std::{
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn integrated_motion_public_authoring() {
    author_integrated("cli", false);
}

#[test]
fn integrated_motion_mcp_authoring() {
    author_integrated("mcp", false);
}

#[cfg(feature = "spatial")]
#[test]
#[ignore = "requires a native Vulkan adapter and private local sockets"]
fn integrated_motion_public_gpu_authoring() {
    author_integrated("cli", true);
}

#[cfg(feature = "spatial")]
#[test]
#[ignore = "requires a native Vulkan adapter and private local sockets"]
fn integrated_motion_mcp_gpu_authoring() {
    author_integrated("mcp", true);
}

fn author_integrated(transport: &str, gpu: bool) {
    let mut project = Project::new(32);
    let document = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                fold_motion::package::create(document).unwrap(),
            )],
        })
        .unwrap();
    let mut session = Session::new(project);
    #[cfg(feature = "spatial")]
    if gpu {
        let (host, info) =
            pollster::block_on(fold_render::gpu::Host::headless(512 * 1024 * 1024)).unwrap();
        eprintln!("Integrated public frames ({transport}): {info:?}");
        session.set_render_host(host);
    }
    let directory = tempfile::tempdir().unwrap();
    let socket = session.enable_agent_transport(directory.path()).unwrap();
    let report = directory.path().join("report.json");
    let mut child = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../demos/m5-integrated.py"
        ))
        .arg(env!("CARGO_BIN_EXE_fold-cli"))
        .arg(socket)
        .arg(document.to_string())
        .arg(&report)
        .arg(transport)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(if gpu { 300 } else { 90 });
    loop {
        session.poll();
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Public authoring exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(report["frames"]["available"], gpu);
    let modes = report["native_modes"].as_array().unwrap();
    assert_eq!(modes.len(), 8);
    for mode in modes {
        assert_eq!(mode["observation"]["samples"].as_array().unwrap().len(), 1);
        assert_eq!(!mode["frame"].is_null(), gpu);
    }

    if gpu {
        assert_eq!(report["frames"]["baseline"].as_array().unwrap().len(), 7);
        assert_eq!(report["frames"]["variation"].as_array().unwrap().len(), 1);
        assert_eq!(report["frames"]["restored"].as_array().unwrap().len(), 1);
    }
    let snapshot = session.snapshot().unwrap();
    let saved = std::path::PathBuf::from(report["saved_project"]["path"].as_str().unwrap());
    assert_eq!(saved, directory.path().join("report.json.fold"));
    assert_eq!(
        report["saved_project"]["revision"],
        serde_json::json!(snapshot.revision())
    );
    assert_eq!(report["saved_project"]["durable"], true);
    let reopened = fold_project::load(&saved, 32).unwrap();
    assert_eq!(snapshot.state(), reopened.snapshot().state());
    session.command(DesktopCommand::Undo);
    let variation = session.snapshot().unwrap();
    let construction =
        fold_motion::model::Construction::from_document(&variation.state().documents[&document])
            .unwrap();
    assert_eq!(
        fold_motion::source::canonical(&construction).unwrap(),
        report["variation"]["source"].as_str().unwrap()
    );
    let variation_path = directory.path().join("variation.fold");
    fold_project::save(&variation, &variation_path).unwrap();
    assert_eq!(
        fold_project::load(&variation_path, 32)
            .unwrap()
            .snapshot()
            .state()
            .documents,
        variation.state().documents
    );
    session.command(DesktopCommand::Redo);
    assert_eq!(
        session.snapshot().unwrap().state().documents,
        snapshot.state().documents
    );
    if let Some(destination) = std::env::var_os("FOLD_M5_EVIDENCE_DIR") {
        let destination = std::path::PathBuf::from(destination).join(document.to_string());
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(
            destination.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        std::fs::write(
            destination.join("construction.foldsource"),
            report["source"].as_str().unwrap(),
        )
        .unwrap();
        std::fs::copy(saved, destination.join("integrated.fold")).unwrap();
        std::fs::copy(variation_path, destination.join("variation.fold")).unwrap();
        eprintln!("Integrated evidence: {}", destination.display());
    }
    assert_eq!(report["restored"], true);
    assert_eq!(
        report["variation"]["observations"]["samples"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(snapshot.revision().0, report["revision"].as_u64().unwrap());
    assert_eq!(report["transport"], transport);
    assert_eq!(report["nodes"], 35);
    assert_eq!(
        report["observations"]["samples"].as_array().unwrap().len(),
        7
    );
}
