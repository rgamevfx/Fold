//! M1 must preserve unsupported documents without registering a legacy reader.
use fold_foundation::{DocumentId, Time};
use fold_project::{Document, DocumentRef, EditBatch, Mutation, Project, Revision};

#[test]
fn removed_agent_cli_is_not_dispatched() {
    for args in [vec!["agent", "mcp"], vec!["agent", "catalog"]] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_fold-cli"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage: fold-cli"));
    }
}

#[test]
fn old_motion_is_opaque_and_cannot_render_after_save_reopen() {
    let id = DocumentId::new();
    let payload =
        br#"{"old_engine":"must not execute","extension":{"nested":[1,null,3]}}"#.to_vec();
    let document = Document {
        id,
        type_id: "fold.motion.document".into(),
        package_id: "fold.motion".into(),
        schema_version: 1,
        revision: Revision(0),
        dependencies: vec![],
        assets: vec![],
        payload: payload.clone(),
        extensions: [("future".into(), serde_json::json!({"value":17}))].into(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opaque.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    let snapshot = reopened.snapshot();
    let record = &snapshot.state().documents[&id];
    assert_eq!(record.payload, payload);
    assert_eq!(record.extensions["future"], serde_json::json!({"value":17}));
    let registry = fold_app::packages::builtins();
    assert!(!registry.supports(record));
    assert!(registry.output(&snapshot, id).is_err());
    assert!(
        registry
            .document_kinds()
            .iter()
            .all(|kind| kind.type_id != "fold.motion.document")
    );
    let source = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let error = registry
        .video(&snapshot, &source, Time::ZERO, 16, 16)
        .unwrap_err();
    assert!(error.message.contains("provider"), "{error}");
}

#[test]
fn compositor_remains_renderable_with_transaction_history() {
    use fold_compositor::{Composite, Node, OperationConfig};
    let id = DocumentId::new();
    let mut project = Project::new(8);
    let make = |rgba| {
        let solid = Node::new(OperationConfig::Solid { rgba }, vec![]);
        let output = Node::new(OperationConfig::Output, vec![solid.id]);
        Composite {
            info: fold_media::VideoInfo {
                width: 16,
                height: 16,
                rate: [24, 1],
                frames: 24,
            },
            output: output.id,
            nodes: vec![solid, output],
            extensions: Default::default(),
        }
        .document(id)
        .unwrap()
    };
    for rgba in [[0.25, 0.5, 0.75, 1.], [1., 0., 0., 1.]] {
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(make(rgba))],
            })
            .unwrap();
    }
    let source = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let render = |snapshot: &fold_project::Snapshot| {
        let graph = fold_app::packages::builtins()
            .video(snapshot, &source, Time::ZERO, 16, 16)
            .unwrap();
        fold_render::render(graph).unwrap().pixels().to_vec()
    };
    assert!(
        render(&project.snapshot())
            .iter()
            .all(|p| *p == [1., 0., 0., 1.])
    );
    project.undo().unwrap();
    assert!(
        render(&project.snapshot())
            .iter()
            .all(|p| *p == [0.25, 0.5, 0.75, 1.])
    );
    project.redo().unwrap();
    assert!(
        render(&project.snapshot())
            .iter()
            .all(|p| *p == [1., 0., 0., 1.])
    );
}

#[test]
fn native_motion_opens_for_editing_with_its_named_video_output() {
    use fold_platform::desktop::{DesktopClient, DesktopCommand, ViewLocation};
    // Existing native graph files remain editable; the removed Motion creation
    // affordance is not used to seed this document-provider regression.
    let mut project = Project::new(16);
    let graph = fold_motion::package::create(DocumentId::new()).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph)],
        })
        .unwrap();
    let mut session = fold_app::session::Session::new(project);
    let snapshot = session.snapshot().unwrap();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_motion::CONSTRUCTION)
        .expect("native document creation");
    let id = document.id;
    let registry = fold_app::packages::builtins();
    assert!(registry.supports(document));
    assert!(registry.has_video(document));
    let output = session.default_output(id).unwrap();
    let construction = fold_motion::model::Construction::from_document(document).unwrap();
    assert_eq!(output.document, id);
    assert_eq!(
        output.output,
        construction.definitions[0].outputs[0].id.to_string()
    );
    session.command(DesktopCommand::Navigate(ViewLocation {
        document: id,
        time: Time::ZERO,
        label: "Motion".into(),
    }));
    assert_eq!(session.state().selection.document, Some(id));
    assert_eq!(session.state().navigation.last().unwrap().document, id);
    assert_eq!(session.state().viewer_document, Some(id));
    assert_eq!(
        session.state().viewer_output.as_deref(),
        Some(output.output.as_str())
    );
    let key = session.state().preview_key(0, 1).unwrap();
    assert_eq!(key.output, output.output);
    let mut construction = construction;
    construction.definitions[0].inputs[0].default = fold_motion::types::Literal::Distance {
        value: 320.,
        unit: fold_motion::types::Unit::Canvas,
    };
    session.command(DesktopCommand::Extension(
        fold_platform::packages::CommandRequest {
            id: fold_motion::package::EDIT.into(),
            base: snapshot.revision(),
            arguments: serde_json::to_vec(&fold_motion::package::EditArgs {
                document: id,
                construction: construction.clone(),
            })
            .unwrap(),
        },
    ));
    assert_eq!(
        fold_motion::model::Construction::from_document(
            &session.snapshot().unwrap().state().documents[&id]
        )
        .unwrap(),
        construction
    );
    session.command(DesktopCommand::Undo);
    assert_eq!(
        fold_motion::model::Construction::from_document(
            &session.snapshot().unwrap().state().documents[&id]
        )
        .unwrap(),
        fold_motion::model::Construction::from_document(document).unwrap()
    );
}

#[test]
fn artist_unlock_uses_local_desktop_route_and_one_undo_entry() {
    use fold_platform::desktop::{DesktopClient, DesktopCommand};
    // Existing native graph files remain editable; the removed Motion creation
    // affordance is not used to seed this document-provider regression.
    let mut project = Project::new(16);
    let graph = fold_motion::package::create(DocumentId::new()).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph)],
        })
        .unwrap();
    let mut session = fold_app::session::Session::new(project);
    let snapshot = session.snapshot().unwrap();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_motion::CONSTRUCTION)
        .unwrap();
    let id = document.id;
    let mut locked = fold_motion::model::Construction::from_document(document).unwrap();
    locked.definitions[0].inputs[0].protected = true;
    let control = locked.definitions[0].inputs[0].id;
    session.command(DesktopCommand::Extension(
        fold_platform::packages::CommandRequest {
            id: fold_motion::package::EDIT.into(),
            base: snapshot.revision(),
            arguments: serde_json::to_vec(&fold_motion::package::EditArgs {
                document: id,
                construction: locked.clone(),
            })
            .unwrap(),
        },
    ));
    let base = session.snapshot().unwrap().revision();
    session.command(DesktopCommand::UnlockControls {
        object: None,
        document: id,
        targets: vec![control],
        base,
    });
    let mut expected = locked.clone();
    expected.definitions[0].inputs[0].protected = false;
    let actual = || {
        fold_motion::model::Construction::from_document(
            &session.snapshot().unwrap().state().documents[&id],
        )
        .unwrap()
    };
    assert_eq!(actual(), expected);
    session.command(DesktopCommand::Undo);
    assert_eq!(
        fold_motion::model::Construction::from_document(
            &session.snapshot().unwrap().state().documents[&id]
        )
        .unwrap(),
        locked
    );
    let current = session.snapshot().unwrap().revision();
    session.command(DesktopCommand::UnlockControls {
        object: None,
        document: id,
        targets: vec![control],
        base,
    });
    assert_eq!(session.snapshot().unwrap().revision(), current);
    assert!(session.state().status.contains("stale"));
}
