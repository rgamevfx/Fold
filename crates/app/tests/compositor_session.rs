use fold_app::{media_workflow, packages, session::Session};
use fold_compositor::{Composite, Node, Parameters as P, package};
use fold_foundation::{DocumentId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand as C, ViewLocation},
    packages::CommandRequest,
};
use fold_project::{EditBatch, Mutation, Project};
fn fixture() -> (Session, DocumentId, Composite) {
    let source = Node::new(
        P::Solid {
            rgba: [1.0, 0.0, 0.0, 1.0],
        },
        vec![],
    );
    let output = Node::new(P::Output, vec![source.id]);
    let graph = Composite {
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 96,
        },
        output: output.id,
        nodes: vec![output, source],
        extensions: Default::default(),
    };
    let mut project = Project::new(32);
    let id = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(id).unwrap())],
        })
        .unwrap();
    (Session::new(project), id, graph)
}
fn request(session: &Session, id: DocumentId, graph: Composite) -> CommandRequest {
    CommandRequest {
        id: package::EDIT.into(),
        base: session.snapshot().unwrap().revision(),
        arguments: serde_json::to_vec(&package::EditArgs {
            document: id,
            composite: graph,
        })
        .unwrap(),
    }
}
#[test]
fn gesture_updates_preview_only_cancel_restores_and_release_is_one_undo_entry() {
    let (mut host, id, mut graph) = fixture();
    let original = host.snapshot().unwrap();
    let content = host.state().content.clone();
    for gain in [0.7, 0.5, 0.2] {
        graph.nodes[1].parameters = P::Solid {
            rgba: [gain, 0.0, 0.0, 1.0],
        };
        host.command(C::PreviewExtension(request(&host, id, graph.clone())));
        assert!(host.state().transient);
        assert_eq!(host.snapshot().unwrap().state(), original.state());
        assert_ne!(host.state().content, content);
    }
    // Persist/export callers receive committed roots, not the overlay.
    assert_eq!(
        media_workflow::content(&host.snapshot().unwrap()).unwrap(),
        content.clone().unwrap()
    );
    host.command(C::CancelPreviewEdit);
    assert!(!host.state().transient);
    assert_eq!(host.state().content, content);
    let edit = request(&host, id, graph.clone());
    host.command(C::PreviewExtension(edit.clone()));
    let key = host.state().preview_key(0, 1).unwrap();
    host.request_preview(key.clone());
    let start = std::time::Instant::now();
    let result = loop {
        if let Some(result) = host.take_preview() {
            break result;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert_eq!(result.key, key);
    let pixel = result.frame.unwrap();
    assert!(pixel.rgba()[0] < 200);
    host.command(C::Extension(edit));
    assert!(!host.state().transient);
    assert_eq!(
        Composite::from_document(&host.snapshot().unwrap().state().documents[&id]).unwrap(),
        graph
    );
    host.command(C::Undo);
    assert_eq!(host.state().content, content);
    host.command(C::Redo);
    assert_ne!(host.state().content, content);
}
#[test]
fn stale_preview_cannot_replace_newer_committed_state() {
    let (mut host, id, mut graph) = fixture();
    let stale = request(&host, id, graph.clone());
    graph.nodes[1].parameters = P::Solid {
        rgba: [0.0, 1.0, 0.0, 1.0],
    };
    host.command(C::Extension(request(&host, id, graph)));
    let content = host.state().content.clone();
    host.command(C::PreviewExtension(stale));
    assert!(!host.state().transient);
    assert_eq!(host.state().content, content);
    assert!(host.state().status.contains("stale"));
}
#[test]
fn navigation_preserves_exact_local_time_and_returns_to_parent_time() {
    let (mut host, id, graph) = fixture();
    let other = DocumentId::new();
    host.command(C::Extension(request(&host, other, graph)));
    host.command(C::Navigate(ViewLocation {
        document: id,
        time: Time::new(1, 1).unwrap(),
        label: "Parent".into(),
    }));
    let exact = Time::new(1001, 24000).unwrap();
    host.command(C::Navigate(ViewLocation {
        document: other,
        time: exact,
        label: "Source".into(),
    }));
    let key = host.state().preview_key(host.state().frame, 1).unwrap();
    assert_eq!(key.target, Some((other, exact)));
    assert_eq!(
        key.content,
        media_workflow::content_for(&host.snapshot().unwrap(), other).unwrap()
    );
    let frame = media_workflow::evaluate(
        &host.snapshot().unwrap(),
        &key,
        &mut Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(frame.pixels()[0], [1.0, 0.0, 0.0, 1.0]);
    host.command(C::NavigateBack);
    assert_eq!(
        host.state()
            .preview_key(host.state().frame, 1)
            .unwrap()
            .target,
        Some((id, Time::new(1, 1).unwrap()))
    );
    host.command(C::Seek(30));
    assert_eq!(
        host.state().preview_key(30, 1).unwrap().target,
        Some((id, Time::new(5, 4).unwrap()))
    );
}
#[test]
fn canvas_positions_and_storage_order_do_not_invalidate_render_content() {
    let (mut host, id, mut graph) = fixture();
    let before = host.state().content.clone();
    graph.auto_layout().unwrap();
    graph.nodes.reverse();
    host.command(C::Extension(request(&host, id, graph.clone())));
    assert_eq!(host.state().content, before);
    graph.nodes[0].position = Some([-120.0, 960.0]);
    host.command(C::Extension(request(&host, id, graph)));
    assert_eq!(host.state().content, before);
    host.command(C::Undo);
    assert_eq!(host.state().content, before);
}
#[test]
fn project_can_open_directly_into_its_compositor_workspace() {
    let (source, id, _) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.fold");
    fold_project::save(&source.snapshot().unwrap(), &path).unwrap();
    let mut host = Session::default();
    host.command(C::OpenInWorkspace {
        path,
        document_type: fold_compositor::COMPOSITE.into(),
    });
    let start = std::time::Instant::now();
    while host.state().busy {
        host.poll();
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(host.state().selection.document, Some(id));
    assert_eq!(host.state().navigation.last().unwrap().document, id);
}
#[test]
fn missing_input_is_a_render_diagnostic_not_a_silent_bypass() {
    let (mut host, id, mut graph) = fixture();
    graph.disconnect(graph.output, "image").unwrap();
    host.command(C::Extension(request(&host, id, graph)));
    let snapshot = host.snapshot().unwrap();
    let source = fold_project::DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    assert!(
        packages::builtins()
            .video(&snapshot, &source, Time::ZERO, 16, 16)
            .unwrap_err()
            .contains("connect 'image'")
    );
}
