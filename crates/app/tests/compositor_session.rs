use fold_app::{output, packages, session::Session};
use fold_compositor::{Composite, Node, OperationConfig as P, package};
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
        graph.nodes[1]
            .set_configuration(P::Solid {
                rgba: [gain, 0.0, 0.0, 1.0],
            })
            .unwrap();
        host.command(C::PreviewExtension(request(&host, id, graph.clone())));
        assert!(host.state().transient);
        assert_eq!(host.snapshot().unwrap().state(), original.state());
        assert_ne!(host.state().content, content);
    }
    // Persist/export callers receive committed roots, not the overlay.
    assert_eq!(
        output::content(&host.snapshot().unwrap()).unwrap(),
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
    graph.nodes[1]
        .set_configuration(P::Solid {
            rgba: [0.0, 1.0, 0.0, 1.0],
        })
        .unwrap();
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
    // Establish the parent while it is the sole output. With two independent
    // composites, UUID ordering can otherwise make `other` the implicit root;
    // navigating to it would correctly return to an ancestor, not open a child.
    host.command(C::Navigate(ViewLocation {
        document: id,
        time: Time::new(1, 1).unwrap(),
        label: "Parent".into(),
    }));
    let other = DocumentId::new();
    host.command(C::Extension(request(&host, other, graph)));
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
        output::content_for(&host.snapshot().unwrap(), other).unwrap()
    );
    let frame = output::evaluate(
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
fn returning_to_timeline_restores_transport_and_evaluates_nested_frames() {
    use fold_compositor::Source;
    use fold_foundation::{AssetId, ObjectId};
    use fold_project::DocumentRef;
    use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
    let (_, source, solid) = fixture();
    let composite = DocumentId::new();
    let timeline = DocumentId::new();
    let reference = |document| DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    // A real temporal change in a nested graph: black until 0.5s, then red.
    let read = Node::new(
        P::Read {
            source: Source::Document {
                source: reference(source),
                info: solid.info.clone(),
            },
            start: Time::new(1, 2).unwrap(),
            source_start: Time::ZERO,
            duration: Time::new(7, 2).unwrap(),
        },
        vec![],
    );
    let output = Node::new(P::Output, vec![read.id]);
    let graph = Composite {
        info: solid.info.clone(),
        output: output.id,
        nodes: vec![read, output],
        extensions: Default::default(),
    };
    let track = Track::new(TrackKind::Video, "V1");
    let sequence = Sequence {
        transitions: vec![],
        dimensions: [16, 16],
        rate: [24, 1],
        clips: vec![Clip {
            effects: vec![],
            id: ObjectId::new(),
            track: track.id,
            link: None,
            asset: AssetId::new(),
            info: SourceMedia::Document {
                source: reference(composite),
                info: solid.info.clone(),
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(4, 1).unwrap(),
            level: 1.0,
            extensions: Default::default(),
        }],
        tracks: vec![track],
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(solid.document(source).unwrap()),
                Mutation::PutDocument(graph.document(composite).unwrap()),
                Mutation::PutDocument(sequence.document(timeline).unwrap()),
            ],
        })
        .unwrap();
    let mut host = Session::new(project);
    host.command(C::Seek(6));
    let parent_content = host.state().content.clone();
    host.command(C::Navigate(ViewLocation {
        document: composite,
        time: Time::new(1, 1).unwrap(),
        label: "Composite".into(),
    }));
    assert_eq!(host.state().navigation.len(), 2);
    host.command(C::ActivateWorkspace(fold_timeline::SEQUENCE.into()));
    assert_eq!(host.state().navigation.len(), 1);
    assert_eq!(host.state().selection.document, Some(timeline));
    assert_eq!(
        host.state().frame,
        6,
        "restore parent time, not the source playhead"
    );
    assert_eq!(host.state().content, parent_content);
    let key = host.state().preview_key(6, 1).unwrap();
    assert_eq!(key.target, Some((timeline, Time::new(1, 4).unwrap())));
    let before = output::evaluate(
        &host.snapshot().unwrap(),
        &key,
        &mut Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(before.pixels()[0], [0.0, 0.0, 0.0, 1.0]);

    #[cfg(feature = "desktop")]
    {
        host.command(C::Play);
        assert!(
            host.state().playing,
            "timeline Play must not be blocked by the old source view"
        );
        // Repeated workspace focus notifications must not stop playback.
        host.command(C::ActivateWorkspace(fold_timeline::SEQUENCE.into()));
        assert!(host.state().playing);
        let started = std::time::Instant::now();
        while host.state().frame < 13 {
            host.poll();
            assert!(
                host.state().playing,
                "transport stopped: {}",
                host.state().status
            );
            assert!(
                started.elapsed() < std::time::Duration::from_secs(5),
                "timeline did not advance"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let key = host.state().preview_key(host.state().frame, 1).unwrap();
        assert_eq!(key.target.unwrap().0, timeline);
        host.request_preview(key.clone());
        let result = loop {
            if let Some(result) = host.take_preview() {
                break result;
            }
            assert!(started.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(result.key, key);
        assert_eq!(&result.frame.unwrap().rgba()[..4], &[255, 0, 0, 255]);
        host.command(C::Pause);
        assert!(!host.state().playing);
    }
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
            .message
            .contains("connect 'image'")
    );
}

#[test]
fn shader_publication_validates_on_worker_and_cancel_never_publishes() {
    let (mut session, id, mut graph) = fixture();
    let source = graph
        .nodes
        .iter()
        .find(|n| matches!(n.prepare().unwrap().config().clone(), P::Solid { .. }))
        .unwrap()
        .id;
    let shader = Node::new(
        P::Shader {
            settings: Default::default(),
        },
        vec![source],
    );
    graph.nodes.push(shader.clone());
    let make_request = |session: &Session, graph: &Composite| CommandRequest {
        id: package::SHADER_EDIT.into(),
        base: session.snapshot().unwrap().revision(),
        arguments: serde_json::to_vec(&package::EditArgs {
            document: id,
            composite: graph.clone(),
        })
        .unwrap(),
    };
    let drain = |session: &mut Session| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while session.state().busy {
            session.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    };
    let before = session.snapshot().unwrap();
    let request = make_request(&session, &graph);
    session.command(C::Extension(request.clone()));
    assert!(session.extension_pending(&request));
    {
        let mut panel = fold_platform::panel_context::PanelContext::for_panel(
            &mut session,
            fold_platform::workspace::PanelInstanceId(1),
        );
        assert!(panel.extension_pending(&request));
        panel.command(C::CancelExtension(request));
    }
    drain(&mut session);
    assert_eq!(session.snapshot().unwrap().revision(), before.revision());
    session.command(C::Extension(make_request(&session, &graph)));
    drain(&mut session);
    assert_ne!(session.snapshot().unwrap().revision(), before.revision());
    let good = session.snapshot().unwrap();
    graph
        .node_mut(shader.id)
        .unwrap()
        .edit_configuration(|config| {
            if let P::Shader { settings } = config {
                settings.source = "invalid WGSL".into();
            }
        })
        .unwrap();
    session.command(C::Extension(make_request(&session, &graph)));
    drain(&mut session);
    assert_eq!(session.snapshot().unwrap().revision(), good.revision());
    assert!(!session.state().status.is_empty());
}

#[test]
fn scoped_inspector_can_discover_published_effect_interfaces() {
    let id = DocumentId::new();
    let mut project = Project::new(8);
    let document =
        fold_compositor::presets::create(fold_compositor::presets::Preset::Glow, id).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let mut session = Session::new(project);
    let expected = session.effect_interface(id).unwrap();
    let panel = fold_platform::panel_context::PanelContext::for_panel(
        &mut session,
        fold_platform::workspace::PanelInstanceId(1),
    );
    assert_eq!(panel.effect_interface(id).unwrap(), expected);
}

#[test]
fn opening_during_a_transient_edit_preserves_the_gesture() {
    let (mut host, id, mut graph) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("original.fold");
    fold_project::save(&host.snapshot().unwrap(), &path).unwrap();
    host.command(C::Open(path));
    graph.info.width += 1;
    host.command(C::PreviewExtension(request(&host, id, graph)));
    let transient = host.state().content.clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while host.state().busy {
        assert!(std::time::Instant::now() < deadline);
        host.poll();
        std::thread::yield_now();
    }
    assert!(host.state().transient);
    assert_eq!(host.state().content, transient);
    assert!(host.state().project_path.is_none());
    assert!(host.state().project_error.is_some());
}
