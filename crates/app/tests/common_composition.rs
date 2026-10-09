use fold_app::{composition_layers as workflow, packages};
use fold_composition::{Composition, Layer};
use fold_foundation::{DocumentId, Time};
use fold_platform::{image_source::Source, packages::CommandRequest};
use fold_project::{Document, DocumentRef, EditBatch, Mutation, Project};
fn reference(document: DocumentId) -> DocumentRef {
    DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    }
}
fn put(project: &mut Project, documents: Vec<Document>) {
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: documents.into_iter().map(Mutation::PutDocument).collect(),
        })
        .unwrap();
}
fn stage(project: &Project, id: &str, args: serde_json::Value) -> Result<EditBatch, String> {
    packages::builtins().stage(
        &project.snapshot(),
        &CommandRequest {
            id: id.into(),
            base: project.snapshot().revision(),
            arguments: serde_json::to_vec(&args).unwrap(),
        },
        &Default::default(),
    )
}
fn apply(project: &mut Project, id: &str, args: serde_json::Value) {
    let batch = stage(project, id, args).unwrap();
    project.commit(batch).unwrap();
}
fn pixels(project: &Project, id: DocumentId, time: Time) -> Vec<[f32; 4]> {
    fold_render::render(
        packages::builtins()
            .video(&project.snapshot(), &reference(id), time, 32, 32)
            .unwrap(),
    )
    .unwrap()
    .pixels()
    .to_vec()
}
fn close(a: &[[f32; 4]], b: &[[f32; 4]]) {
    assert_eq!(a.len(), b.len());
    let error = a
        .iter()
        .zip(b)
        .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
        .fold(0_f32, f32::max);
    assert!(error < 0.005, "pixel error {error}");
}
fn fixture() -> (Project, DocumentId, fold_compositor::Composite) {
    use fold_compositor::{Composite, Node, Parameters, Shape};
    let profile = Node::new(
        Parameters::Shape {
            shape: Shape::Rectangle,
            bounds: [10., 11., 22., 21.],
        },
        vec![],
    );
    let output = Node::new(Parameters::Output, vec![profile.id]);
    let composite = Composite {
        info: fold_media::VideoInfo {
            width: 32,
            height: 32,
            rate: [24, 1],
            frames: 48,
        },
        output: output.id,
        nodes: vec![profile, output],
        extensions: [(
            "unknown.future".into(),
            serde_json::json!({"retained":true}),
        )]
        .into(),
    };
    let document = DocumentId::new();
    let mut project = Project::new(32);
    put(&mut project, vec![composite.document(document).unwrap()]);
    (project, document, composite)
}

#[test]
fn mixed_layers_processing_connections_visibility_animation_and_cycle_rejection() {
    let (mut project, image_id, image) = fixture();
    let id = DocumentId::new();
    let layer = Layer::new(
        "Foreground",
        Some(Source::Output(reference(image_id))),
        image.info.clone(),
    )
    .unwrap();
    let foreground = layer.id;
    let c = Composition {
        info: image.info.clone(),
        layers: vec![layer],
        extensions: Default::default(),
    };
    put(&mut project, vec![c.document(id).unwrap()]);
    let expected = pixels(&project, id, Time::ZERO);
    apply(
        &mut project,
        workflow::ADD,
        serde_json::json!({"document":id,"kind":"process","layer":foreground}),
    );
    close(&expected, &pixels(&project, id, Time::ZERO));
    apply(
        &mut project,
        workflow::ADD,
        serde_json::json!({"document":id,"kind":"image"}),
    );
    let snapshot = project.snapshot();
    let mut c = Composition::from_document(&snapshot.state().documents[&id]).unwrap();
    let target = c.layers[0].id;
    apply(
        &mut project,
        workflow::CONNECT,
        serde_json::json!({"document":id,"source":foreground,"target":target}),
    );
    let candidate = stage(
        &project,
        workflow::CONNECT,
        serde_json::json!({"document":id,"source":target,"target":foreground}),
    );
    let before = project.snapshot();
    match candidate {
        Err(_) => {}
        Ok(batch) => assert!(project.commit(batch).is_err()),
    };
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
    c.layers[0].visible = false;
    c.layers[1].animation.insert(
        "scene.opacity.0".into(),
        fold_animation::Curve {
            keys: vec![
                fold_animation::Key::new(Time::ZERO, 0.),
                fold_animation::Key::new(Time::new(1, 1).unwrap(), 1.),
            ],
        },
    );
    put(&mut project, vec![c.document(id).unwrap()]);
    assert!(pixels(&project, id, Time::ZERO).iter().all(|p| p[3] == 0.));
    assert!(
        pixels(&project, id, Time::new(1, 1).unwrap())
            .iter()
            .any(|p| p[3] > 0.9)
    );
    let mut c = Composition::from_document(&project.snapshot().state().documents[&id]).unwrap();
    c.layer_mut(foreground).unwrap().locked = true;
    put(&mut project, vec![c.document(id).unwrap()]);
    assert!(
        stage(
            &project,
            workflow::ADD,
            serde_json::json!({"document":id,"kind":"process","layer":foreground})
        )
        .is_err()
    );
}
#[test]
fn nesting_preserves_mixed_pixels_and_exact_trim_origin() {
    let (mut project, source, image) = fixture();
    let id = DocumentId::new();
    let mut layer = Layer::new(
        "Source",
        Some(Source::Output(reference(source))),
        image.info.clone(),
    )
    .unwrap();
    let selected = layer.id;
    let t = Time::new(1, 3).unwrap();
    layer
        .set_range(t, layer.end.checked_add(t).unwrap())
        .unwrap();
    assert_eq!(layer.local_time(t).unwrap(), Time::ZERO);
    let end = layer.end;
    layer.set_range(Time::new(1, 2).unwrap(), end).unwrap();
    assert_eq!(
        layer.local_time(Time::new(1, 2).unwrap()).unwrap(),
        Time::new(1, 6).unwrap()
    );
    let c = Composition {
        info: image.info.clone(),
        layers: vec![layer],
        extensions: Default::default(),
    };
    put(&mut project, vec![c.document(id).unwrap()]);
    let before = pixels(&project, id, Time::new(1, 1).unwrap());
    apply(
        &mut project,
        fold_composition::package::NEST,
        serde_json::json!({"document":id,"child":DocumentId::new(),"name":"Nested","layers":[selected]}),
    );
    close(&before, &pixels(&project, id, Time::new(1, 1).unwrap()));
    project.undo().unwrap();
    close(&before, &pixels(&project, id, Time::new(1, 1).unwrap()));
}

#[test]
fn named_image_inputs_bind_independently_without_changing_other_sources() {
    use fold_compositor::{Composite, Node, Parameters as P, Source as S};
    let (mut project, source, image) = fixture();
    let target = DocumentId::new();
    let id = DocumentId::new();
    let read = || {
        Node::new(
            P::Read {
                source: S::Unassigned {
                    info: image.info.clone(),
                },
                start: Time::ZERO,
                source_start: Time::ZERO,
                duration: image.info.time(image.info.frames).unwrap(),
            },
            vec![],
        )
    };
    let a = read();
    let b = read();
    let merge = Node::new(P::Merge, vec![a.id, b.id]);
    let output = Node::new(P::Output, vec![merge.id]);
    let graph = Composite {
        info: image.info.clone(),
        output: output.id,
        nodes: vec![a, b, merge, output],
        extensions: Default::default(),
    };
    let from = Layer::new(
        "Image",
        Some(Source::Output(reference(source))),
        image.info.clone(),
    )
    .unwrap();
    let to = Layer::new(
        "Merge graph",
        Some(Source::Output(reference(target))),
        image.info.clone(),
    )
    .unwrap();
    let (from_id, to_id) = (from.id, to.id);
    let composition = Composition {
        info: image.info.clone(),
        layers: vec![to, from],
        extensions: Default::default(),
    };
    put(
        &mut project,
        vec![
            graph.document(target).unwrap(),
            composition.document(id).unwrap(),
        ],
    );
    let ports = packages::builtins()
        .image_inputs(&project.snapshot(), target)
        .unwrap();
    assert_eq!(ports.len(), 2);
    assert!(
        stage(
            &project,
            workflow::CONNECT,
            serde_json::json!({"document":id,"source":from_id,"target":to_id})
        )
        .is_err()
    );
    apply(
        &mut project,
        workflow::CONNECT,
        serde_json::json!({"document":id,"source":from_id,"target":to_id,"port":ports[1].id}),
    );
    let after = packages::builtins()
        .image_inputs(&project.snapshot(), target)
        .unwrap();
    assert!(after[0].source.is_none());
    assert_eq!(after[1].source, Some(Source::Output(reference(source))));
}

#[test]
fn deleting_the_only_graph_layer_is_undoable() {
    let mut project = Project::new(8);
    let document = DocumentId::new();
    put(
        &mut project,
        vec![Composition::default().document(document).unwrap()],
    );
    apply(
        &mut project,
        workflow::ADD,
        serde_json::json!({"document":document,"kind":"image"}),
    );
    let before = project.snapshot();
    let mut composition = Composition::from_document(&before.state().documents[&document]).unwrap();
    assert_eq!(composition.layers.len(), 1);
    composition.layers.clear();
    apply(
        &mut project,
        fold_composition::package::EDIT,
        serde_json::json!({"document":document,"composition":composition}),
    );
    assert!(
        Composition::from_document(&project.snapshot().state().documents[&document])
            .unwrap()
            .layers
            .is_empty()
    );
    project.undo().unwrap();
    assert!(before.same_content(&project.snapshot()));
}

#[test]
fn composition_exposes_compositor_keys_without_network_editor() {
    let mut project = Project::new(16);
    let id = DocumentId::new();
    put(
        &mut project,
        vec![Composition::default().document(id).unwrap()],
    );
    apply(
        &mut project,
        workflow::ADD,
        serde_json::json!({"document":id,"kind":"image"}),
    );
    let composition =
        Composition::from_document(&project.snapshot().state().documents[&id]).unwrap();
    let layer = &composition.layers[0];
    let Some(Source::Output(source)) = &layer.source else {
        panic!()
    };
    let mut graph = fold_compositor::Composite::from_document(
        &project.snapshot().state().documents[&source.document],
    )
    .unwrap();
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.2);
    curve.insert(Time::new(1, 1).unwrap(), 0.8);
    graph.nodes[0].animation.insert("color.3".into(), curve);
    put(&mut project, vec![graph.document(source.document).unwrap()]);
    let mut channels = packages::builtins()
        .animation_channels(
            &project.snapshot().state().documents[&source.document],
            None,
        )
        .unwrap();
    assert_eq!(channels.len(), 1);
    channels[0].curve.keys[0].value = 0.4;
    apply(
        &mut project,
        fold_composition::package::EDIT,
        serde_json::json!({"document":id,"composition":composition,"graph_animation":[{"layer":layer.id,"channels":channels,"time":Time::ZERO}]}),
    );
    let edited = fold_compositor::Composite::from_document(
        &project.snapshot().state().documents[&source.document],
    )
    .unwrap();
    assert_eq!(edited.nodes[0].animation["color.3"].keys[0].value, 0.4);
}

#[test]
fn composition_auto_key_is_shared_with_graph_inspector_contexts() {
    use fold_platform::{
        desktop::{DesktopClient, DesktopCommand, Selection, ViewLocation},
        panel_context::PanelContext,
        workspace::{EditorInstance, LinkGroup},
    };
    let (project, id, _) = fixture();
    let mut host = fold_app::session::Session::new(project);
    let mut editor = EditorInstance {
        contribution: fold_composition::package::PANEL.into(),
        document_type: fold_compositor::COMPOSITE.into(),
        group: LinkGroup::A,
        navigation: vec![ViewLocation {
            document: id,
            time: Time::ZERO,
            label: "Layer".into(),
        }],
        selection: Selection {
            document: Some(id),
            objects: vec![],
        },
        mapped_navigation: false,
    };
    let mut scoped = PanelContext::new(&mut host, &mut editor);
    scoped.command(DesktopCommand::AutoKey(true));
    assert!(scoped.state().auto_key);
    drop(scoped);
    let graph = PanelContext::new(&mut host, &mut editor);
    assert!(graph.state().auto_key);
}
