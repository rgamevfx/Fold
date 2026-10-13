//! Working viewer routing must not change published pixels or escape Session Undo.
use fold_compositor::{Composite, Node, OperationConfig as P};
use fold_foundation::{DocumentId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand},
    packages::CommandRequest,
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
#[test]
fn working_viewer_preserves_output_and_roundtrips_session_undo() {
    let red = Node::new(
        P::Solid {
            rgba: [1., 0., 0., 1.],
        },
        vec![],
    );
    let red_id = red.id;
    let blue = Node::new(
        P::Solid {
            rgba: [0., 0., 1., 1.],
        },
        vec![],
    );
    let output = Node::new(P::Output, vec![blue.id]);
    let mut graph = Composite {
        info: fold_media::VideoInfo {
            width: 2,
            height: 2,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![red, blue, output],
        extensions: Default::default(),
    };
    let document = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
        })
        .unwrap();
    let mut session = fold_app::session::Session::new(project);
    assert_eq!(session.working_output(document).unwrap().output, "video");
    let viewer = graph.view_node(red_id).unwrap();
    assert_eq!(graph.view_node(red_id).unwrap(), viewer);
    assert!(graph.node(viewer).unwrap().operation.parameters.is_empty());
    assert!(graph.connect(viewer, graph.output, "image").is_err());
    session.command(DesktopCommand::Extension(CommandRequest {
        id: fold_compositor::package::EDIT.into(),
        base: session.snapshot().unwrap().revision(),
        arguments: serde_json::to_vec(&fold_compositor::package::EditArgs {
            document,
            composite: graph.clone(),
        })
        .unwrap(),
    }));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while session.state().busy {
        assert!(std::time::Instant::now() < deadline);
        session.poll();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        session.working_output(document).unwrap().output,
        "viewer",
        "{}",
        session.state().status
    );
    let mut published_preview = graph.clone();
    published_preview.view_node(graph.output).unwrap();
    published_preview.validate().unwrap();
    let registry = fold_app::packages::builtins();
    for (port, expected, excluded) in [
        ("viewer", [1., 0., 0., 1.], [0., 0., 1., 1.]),
        ("video", [0., 0., 1., 1.], [1., 0., 0., 1.]),
    ] {
        let compiled = registry
            .video(
                &session.snapshot().unwrap(),
                &DocumentRef {
                    document,
                    output: port.into(),
                    extensions: Default::default(),
                },
                Time::ZERO,
                2,
                2,
            )
            .unwrap();
        assert!(
            compiled
                .nodes
                .iter()
                .any(|n| matches!(n,fold_render::ImageOp::Solid{rgba} if *rgba==expected))
        );
        assert!(
            !compiled
                .nodes
                .iter()
                .any(|n| matches!(n,fold_render::ImageOp::Solid{rgba} if *rgba==excluded))
        );
    }
    session.command(DesktopCommand::Undo);
    assert_eq!(session.working_output(document).unwrap().output, "video");
    session.command(DesktopCommand::Redo);
    assert_eq!(session.working_output(document).unwrap().output, "viewer");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("viewer.fold");
    fold_project::save(&session.snapshot().unwrap(), &path).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    let actual =
        Composite::from_document(&reopened.snapshot().state().documents[&document]).unwrap();
    assert_eq!(actual, graph);
    assert_eq!(
        actual.node(viewer).unwrap().extensions["fold.viewer.docked"],
        true
    );
}
