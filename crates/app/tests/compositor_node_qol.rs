//! Bypass and extraction use the real guarded Session preview/commit lifecycle.
use fold_compositor::{Composite, Node, OperationConfig as P};
use fold_foundation::{DocumentId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand},
    packages::CommandRequest,
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
fn request(
    session: &fold_app::session::Session,
    document: DocumentId,
    graph: &Composite,
) -> CommandRequest {
    CommandRequest {
        id: fold_compositor::package::EDIT.into(),
        base: session.snapshot().unwrap().revision(),
        arguments: serde_json::to_vec(&fold_compositor::package::EditArgs {
            document,
            composite: graph.clone(),
        })
        .unwrap(),
    }
}
fn current(session: &fold_app::session::Session, document: DocumentId) -> Composite {
    Composite::from_document(&session.snapshot().unwrap().state().documents[&document]).unwrap()
}
#[test]
fn bypass_retime_and_extract_preview_cancel_commit_undo() {
    let source = Node::new(
        P::Solid {
            rgba: [0.2, 0.4, 0.8, 1.],
        },
        vec![],
    );
    let source_id = source.id;
    let mut retime = Node::new(
        P::Retime {
            settings: fold_compositor::retime::Retime {
                interpolation: fold_compositor::retime::Interpolation::Rife,
                offset: 0.5,
                motion_blur: true,
                ..Default::default()
            },
        },
        vec![source_id],
    );
    let retime_id = retime.id;
    retime.position = Some([0., 100.]);
    let output = Node::new(P::Output, vec![retime_id]);
    let mut graph = Composite {
        info: fold_media::VideoInfo {
            width: 2,
            height: 2,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![source, retime, output],
        extensions: Default::default(),
    };
    let viewer = graph.view_node(retime_id).unwrap();
    let document = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
        })
        .unwrap();
    let mut session = fold_app::session::Session::new(project);
    let original = graph.clone();
    graph.node_mut(retime_id).unwrap().disabled = true;
    session.command(DesktopCommand::Extension(request(
        &session, document, &graph,
    )));
    assert_eq!(
        current(&session, document),
        graph,
        "{}",
        session.state().status
    );
    let registry = fold_app::packages::builtins();
    for port in ["video", "viewer"] {
        let rendered = registry
            .video(
                &session.snapshot().unwrap(),
                &DocumentRef {
                    document,
                    output: port.into(),
                    extensions: Default::default(),
                },
                Time::new(1, 24).unwrap(),
                2,
                2,
            )
            .unwrap();
        assert_eq!(
            rendered.nodes.len(),
            1,
            "Disabled Retime must produce neither RIFE nor shutter samples"
        );
        assert!(
            matches!(rendered.nodes[0], fold_render::ImageOp::Solid { rgba } if rgba == [0.2,0.4,0.8,1.])
        );
    }
    session.command(DesktopCommand::Undo);
    assert_eq!(current(&session, document), original);
    session.command(DesktopCommand::Redo);
    let disabled = graph.clone();
    graph.detach(&[retime_id]).unwrap();
    graph.node_mut(retime_id).unwrap().position = Some([200., 100.]);
    session.command(DesktopCommand::PreviewExtension(request(
        &session, document, &graph,
    )));
    session.command(DesktopCommand::CancelPreviewEdit);
    assert_eq!(current(&session, document), disabled);
    session.command(DesktopCommand::PreviewExtension(request(
        &session, document, &graph,
    )));
    session.command(DesktopCommand::Extension(request(
        &session, document, &graph,
    )));
    assert_eq!(current(&session, document), graph);
    assert_eq!(
        graph.node(viewer).unwrap().input_ids(),
        vec![Some(source_id)]
    );
    session.command(DesktopCommand::Undo);
    assert_eq!(current(&session, document), disabled);
    session.command(DesktopCommand::Redo);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qol.fold");
    fold_project::save(&session.snapshot().unwrap(), &path).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    assert_eq!(
        Composite::from_document(&reopened.snapshot().state().documents[&document]).unwrap(),
        graph
    );
}

#[test]
fn add_after_selected_preserves_branches_viewer_and_one_undo() {
    let read = Node::new(P::Solid { rgba: [1.; 4] }, vec![]);
    let source = read.id;
    let output = Node::new(P::Output, vec![source]);
    let mut graph = Composite {
        info: fold_media::VideoInfo {
            width: 2,
            height: 2,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![read, output],
        extensions: Default::default(),
    };
    let viewer = graph.view_node(source).unwrap();
    let document = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
        })
        .unwrap();
    let mut session = fold_app::session::Session::new(project);
    let original = graph.clone();
    for config in [
        P::ColorGrade {
            settings: Default::default(),
        },
        P::Composite {
            mode: Default::default(),
            channels: Default::default(),
        },
    ] {
        graph = original.clone();
        let slot = config.primary_slot().unwrap();
        let added = Node::disconnected(config);
        let id = added.id;
        graph.nodes.push(added);
        graph.insert_after(source, id).unwrap();
        assert_eq!(graph.node(id).unwrap().input_ids()[slot], Some(source));
        assert_eq!(graph.node(viewer).unwrap().input_ids()[0], Some(id));
        assert_eq!(graph.node(graph.output).unwrap().input_ids()[0], Some(id));
        session.command(DesktopCommand::Extension(request(
            &session, document, &graph,
        )));
        assert_eq!(current(&session, document), graph);
        session.command(DesktopCommand::Undo);
        assert_eq!(current(&session, document), original);
        session.command(DesktopCommand::Redo);
        assert_eq!(current(&session, document), graph);
        session.command(DesktopCommand::Undo);
    }
}
