//! Write exports its selected branch without changing Fold's internal Output.
use fold_compositor::{Composite, Node, OperationConfig as P};
use fold_foundation::DocumentId;
use fold_media::Cancel;
use fold_platform::delivery::{Precision, Settings};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
#[test]
fn write_exports_own_branch_with_step_precision_alpha_and_no_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let settings = Settings {
        destination: directory
            .path()
            .join("renders/branch.####.exr")
            .to_string_lossy()
            .into(),
        first: 0,
        last: 2,
        step: 2,
        premultiplied: false,
        precision: Precision::Float,
        color_space: fold_color::LINEAR_SRGB.into(),
        ..Default::default()
    };
    let red = Node::new(
        P::Solid {
            rgba: [0.25, 0.125, 0., 0.5],
        },
        vec![],
    );
    let blue = Node::new(
        P::Solid {
            rgba: [0., 0., 1., 1.],
        },
        vec![],
    );
    let write = Node::new(
        P::Write {
            settings: settings.clone(),
        },
        vec![red.id],
    );
    let output = Node::new(P::Output, vec![blue.id]);
    let reference = DocumentRef {
        document: DocumentId::new(),
        output: format!("write/{}", write.id),
        extensions: Default::default(),
    };
    let graph = Composite {
        info: fold_media::VideoInfo {
            width: 2,
            height: 2,
            frames: 3,
            rate: [24, 1],
        },
        output: output.id,
        nodes: vec![red, blue, write, output],
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                graph.document(reference.document).unwrap(),
            )],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let cancel = Cancel::default();
    fold_app::output::export_frames(
        &snapshot,
        &reference,
        &settings,
        &cancel,
        #[cfg(feature = "gpu")]
        None,
    )
    .unwrap();
    for frame in [0, 2] {
        let path = settings.path(frame).unwrap();
        let source = fold_media::exr::Source {
            info: fold_media::exr::inspect(&path, &cancel).unwrap(),
            fingerprint: fold_media::fingerprint(&path, &cancel).unwrap(),
            path,
        };
        let decoded = fold_media::exr::decode(
            &source,
            [Some("R"), Some("G"), Some("B"), Some("A")],
            [2, 2],
            &cancel,
        )
        .unwrap();
        assert_eq!(decoded.pixels, vec![[0.5, 0.25, 0., 0.5]; 4]);
    }
    let alpha_settings = Settings {
        destination: directory.path().join("alpha.exr").to_string_lossy().into(),
        channels: fold_platform::delivery::Channels::Alpha,
        first: 0,
        last: 0,
        ..settings.clone()
    };
    fold_app::output::export_frames(
        &snapshot,
        &reference,
        &alpha_settings,
        &cancel,
        #[cfg(feature = "gpu")]
        None,
    )
    .unwrap();
    let path = alpha_settings.path(0).unwrap();
    let source = fold_media::exr::Source {
        info: fold_media::exr::inspect(&path, &cancel).unwrap(),
        fingerprint: fold_media::fingerprint(&path, &cancel).unwrap(),
        path,
    };
    assert_eq!(source.info.channels.len(), 1);
    let alpha =
        fold_media::exr::decode(&source, [None, None, None, Some("A")], [2, 2], &cancel).unwrap();
    assert!(alpha.pixels.iter().all(|pixel| pixel[3] == 0.5));
    assert!(!settings.path(1).unwrap().exists());
    let before = std::fs::read(settings.path(0).unwrap()).unwrap();
    assert!(
        fold_app::output::export_frames(
            &snapshot,
            &reference,
            &settings,
            &cancel,
            #[cfg(feature = "gpu")]
            None
        )
        .unwrap_err()
        .contains("already exists")
    );
    assert_eq!(before, std::fs::read(settings.path(0).unwrap()).unwrap());
    assert_eq!(project.snapshot().revision(), snapshot.revision());
    let registry = fold_app::packages::builtins();
    let internal = registry
        .video(
            &snapshot,
            &DocumentRef {
                output: "video".into(),
                ..reference
            },
            fold_foundation::Time::ZERO,
            2,
            2,
        )
        .unwrap();
    assert_eq!(
        fold_render::render(internal).unwrap().pixels(),
        &[[0., 0., 1., 1.]; 4]
    );
}

fn movie_fixture(settings: &Settings) -> (Project, DocumentRef, fold_foundation::ObjectId) {
    let red = Node::new(
        P::Solid {
            rgba: [0.25, 0., 0., 0.5],
        },
        vec![],
    );
    let blue = Node::new(
        P::Solid {
            rgba: [0., 0., 1., 1.],
        },
        vec![],
    );
    let write = Node::new(
        P::Write {
            settings: settings.clone(),
        },
        vec![red.id],
    );
    let write_id = write.id;
    let output = Node::new(P::Output, vec![blue.id]);
    let reference = DocumentRef {
        document: DocumentId::new(),
        output: format!("write/{write_id}"),
        extensions: Default::default(),
    };
    let graph = Composite {
        info: fold_media::VideoInfo {
            width: 32,
            height: 16,
            frames: 4,
            rate: [24000, 1001],
        },
        output: output.id,
        nodes: vec![red, blue, write, output],
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                graph.document(reference.document).unwrap(),
            )],
        })
        .unwrap();
    project
        .commit(fold_app::output::select_output(&project.snapshot(), reference.document).unwrap())
        .unwrap();
    (project, reference, write_id)
}

#[test]
fn movie_write_exports_own_branch_with_timing_alpha_and_atomic_destinations() {
    use fold_platform::delivery::FileType;
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let mut settings = Settings {
        destination: directory
            .path()
            .join("branch.####.exr")
            .to_string_lossy()
            .into(),
        first: 1,
        last: 3,
        step: 2,
        ..Default::default()
    };
    let (project, reference, _) = movie_fixture(&settings);
    let snapshot = project.snapshot();
    for format in [FileType::QuickTime, FileType::Mp4] {
        settings.set_file_type(format);
        fold_app::output::export_frames(
            &snapshot,
            &reference,
            &settings,
            &Cancel::default(),
            #[cfg(feature = "gpu")]
            None,
        )
        .unwrap();
        let path = settings.path(1).unwrap();
        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-show_streams", "-of", "json"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(probe.status.success());
        let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let stream = &probe["streams"][0];
        assert_eq!(
            stream["nb_frames"], "3",
            "movie ignores the retained EXR frame step"
        );
        assert_eq!(stream["r_frame_rate"], "24000/1001");
        assert_eq!(
            stream["codec_name"],
            if format == FileType::QuickTime {
                "prores"
            } else {
                "h264"
            }
        );
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-threads", "1", "-i"])
            .arg(&path)
            .args([
                "-threads", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        assert_eq!(decoded.stdout.len(), 3 * 32 * 16 * 4);
        for p in decoded.stdout.chunks_exact(4) {
            assert!(
                p[0] > 100 && p[1] < 10 && p[2] < 10,
                "Write must export red, not the blue Output: {p:?}"
            );
            if format == FileType::QuickTime {
                assert!((i32::from(p[3]) - 128).abs() <= 2);
            } else {
                assert_eq!(p[3], 255);
            }
        }
        let original = std::fs::read(&path).unwrap();
        assert!(
            fold_app::output::export_frames(
                &snapshot,
                &reference,
                &settings,
                &Cancel::default(),
                #[cfg(feature = "gpu")]
                None
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        settings.destination = directory
            .path()
            .join(format!("cancelled.{}", format.extension()))
            .to_string_lossy()
            .into();
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(
            fold_app::output::export_frames(
                &snapshot,
                &reference,
                &settings,
                &cancel,
                #[cfg(feature = "gpu")]
                None
            )
            .is_err()
        );
        assert!(!settings.path(1).unwrap().exists());
        settings.destination = path.to_string_lossy().into();
    }
    assert_eq!(project.snapshot().revision(), snapshot.revision());
    assert_eq!(
        fold_app::output::output(&snapshot).unwrap().0.output,
        "video"
    );
}

#[test]
fn write_format_changes_commit_undo_and_reopen_through_session() {
    use fold_platform::{
        delivery::FileType,
        desktop::{DesktopClient, DesktopCommand},
        packages::CommandRequest,
    };
    let original = Settings {
        destination: "/tmp/sequence.####.exr".into(),
        last: 3,
        ..Default::default()
    };
    let (project, reference, write) = movie_fixture(&original);
    let mut session = fold_app::session::Session::new(project);
    let snapshot = session.snapshot().unwrap();
    let mut graph =
        Composite::from_document(&snapshot.state().documents[&reference.document]).unwrap();
    let mut edited = original.clone();
    edited.set_file_type(FileType::QuickTime);
    edited.prores.profile = fold_media::encoding::ProResProfile::Hq;
    graph
        .node_mut(write)
        .unwrap()
        .set_configuration(P::Write {
            settings: edited.clone(),
        })
        .unwrap();
    session.command(DesktopCommand::Extension(CommandRequest {
        id: fold_compositor::package::EDIT.into(),
        base: snapshot.revision(),
        arguments: serde_json::to_vec(&fold_compositor::package::EditArgs {
            document: reference.document,
            composite: graph,
        })
        .unwrap(),
    }));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while session.state().busy {
        assert!(std::time::Instant::now() < deadline);
        session.poll();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let read = |snapshot: &fold_project::Snapshot| {
        let graph =
            Composite::from_document(&snapshot.state().documents[&reference.document]).unwrap();
        match graph.node(write).unwrap().prepare().unwrap().config() {
            P::Write { settings } => settings.clone(),
            _ => unreachable!(),
        }
    };
    assert_eq!(
        read(&session.snapshot().unwrap()),
        edited,
        "{}",
        session.state().status
    );
    session.command(DesktopCommand::Undo);
    assert_eq!(read(&session.snapshot().unwrap()), original);
    session.command(DesktopCommand::Redo);
    assert_eq!(read(&session.snapshot().unwrap()), edited);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("write.fold");
    fold_project::save(&session.snapshot().unwrap(), &path).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    assert_eq!(read(&reopened.snapshot()), edited);
}
