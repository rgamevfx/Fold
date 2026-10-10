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
