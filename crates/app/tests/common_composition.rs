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
fn fixture() -> (Project, DocumentId, fold_motion::Motion) {
    let mut motion = fold_motion::Motion::new_scene();
    motion.info = fold_media::VideoInfo {
        width: 32,
        height: 32,
        rate: [24, 1],
        frames: 48,
    };
    let id =
        fold_motion::authoring::scene::create_object(&mut motion, "fold.motion.rectangle").unwrap();
    let object = motion.scene.as_ref().unwrap().objects[0].clone();
    let node = fold_motion::authoring::node(&mut motion, id).unwrap();
    node.set("width", fold_motion::fields::Datum::Scalar(12.));
    node.set("height", fold_motion::fields::Datum::Scalar(10.));
    fold_motion::authoring::node(&mut motion, object.transform.unwrap())
        .unwrap()
        .set(
            "translation",
            fold_motion::fields::Datum::Vector([16., 16.]),
        );
    motion.extensions.insert(
        "unknown.future".into(),
        serde_json::json!({"retained":true}),
    );
    let document = DocumentId::new();
    let mut project = Project::new(32);
    put(&mut project, vec![motion.document(document).unwrap()]);
    (project, document, motion)
}

#[test]
fn mixed_layers_processing_connections_visibility_animation_and_cycle_rejection() {
    let (mut project, motion_id, motion) = fixture();
    let id = DocumentId::new();
    let layer = Layer::new(
        "Foreground",
        Some(Source::Output(reference(motion_id))),
        motion.info.clone(),
    )
    .unwrap();
    let foreground = layer.id;
    let c = Composition {
        info: motion.info.clone(),
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
    let (mut project, source, motion) = fixture();
    let id = DocumentId::new();
    let mut layer = Layer::new(
        "Source",
        Some(Source::Output(reference(source))),
        motion.info.clone(),
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
        info: motion.info.clone(),
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
fn new_motion_and_image_graphs_share_one_composition_and_layer_controls() {
    let id = DocumentId::new();
    let mut project = Project::new(16);
    let mut composition = Composition::default();
    composition.info.width = 32;
    composition.info.height = 32;
    put(&mut project, vec![composition.document(id).unwrap()]);
    for kind in ["motion", "image"] {
        apply(
            &mut project,
            workflow::ADD,
            serde_json::json!({"document":id,"kind":kind}),
        );
    }
    let mut composition =
        Composition::from_document(&project.snapshot().state().documents[&id]).unwrap();
    assert_eq!(composition.layers.len(), 2);
    let top = composition.layers[0].id;
    assert!(pixels(&project, id, Time::ZERO).iter().all(|p| p[3] > 0.99));
    composition.layer_mut(top).unwrap().visible = false;
    put(&mut project, vec![composition.document(id).unwrap()]);
    assert!(pixels(&project, id, Time::ZERO).iter().any(|p| p[3] < 0.01));
}

#[test]
fn provider_image_inputs_connect_into_motion_and_disconnect_without_losing_nodes() {
    let (mut project, source, mut motion) = fixture();
    // Build an image-driven motion construction. Its provider exposes the source
    // socket, while Composition never interprets the Motion settings.
    motion.scene = Some(Default::default());
    let image = fold_motion::authoring::add_node(&mut motion, "fold.motion.image").unwrap();
    let card = fold_motion::authoring::scene::create_object(&mut motion, "fold.motion.image_card")
        .unwrap();
    fold_motion::authoring::node(&mut motion, card)
        .unwrap()
        .connect("image", image, "image");
    let target = DocumentId::new();
    let id = DocumentId::new();
    let a = Layer::new(
        "Source",
        Some(Source::Output(reference(source))),
        motion.info.clone(),
    )
    .unwrap();
    let b = Layer::new(
        "Image-driven motion",
        Some(Source::Output(reference(target))),
        motion.info.clone(),
    )
    .unwrap();
    let (from, to) = (a.id, b.id);
    let composition = Composition {
        info: motion.info.clone(),
        layers: vec![b, a],
        extensions: Default::default(),
    };
    put(
        &mut project,
        vec![
            motion.document(target).unwrap(),
            composition.document(id).unwrap(),
        ],
    );
    let inputs = packages::builtins()
        .image_inputs(&project.snapshot(), target)
        .unwrap();
    assert_eq!(inputs.len(), 1);
    let port = inputs[0].id.clone();
    let before = project.snapshot();
    apply(
        &mut project,
        workflow::CONNECT,
        serde_json::json!({"document":id,"source":from,"target":to,"port":port}),
    );
    let bound = packages::builtins()
        .image_inputs(&project.snapshot(), target)
        .unwrap();
    assert_eq!(bound[0].source, Some(Source::Output(reference(source))));
    let graph =
        fold_motion::Motion::from_document(&project.snapshot().state().documents[&target]).unwrap();
    assert_eq!(
        graph.graph.nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
        motion.graph.nodes.iter().map(|n| n.id).collect::<Vec<_>>()
    );
    assert!(pixels(&project, id, Time::ZERO).iter().any(|p| p[3] > 0.));
    apply(
        &mut project,
        workflow::CONNECT,
        serde_json::json!({"document":id,"source":null,"target":to,"port":port}),
    );
    assert!(
        packages::builtins()
            .image_inputs(&project.snapshot(), target)
            .unwrap()[0]
            .source
            .is_none()
    );
    project.undo().unwrap();
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn named_image_inputs_bind_independently_without_changing_other_sources() {
    use fold_compositor::{Composite, Node, Parameters as P, Source as S};
    let (mut project, source, motion) = fixture();
    let target = DocumentId::new();
    let id = DocumentId::new();
    let read = || {
        Node::new(
            P::Read {
                source: S::Unassigned {
                    info: motion.info.clone(),
                },
                start: Time::ZERO,
                source_start: Time::ZERO,
                duration: motion.info.time(motion.info.frames).unwrap(),
            },
            vec![],
        )
    };
    let a = read();
    let b = read();
    let merge = Node::new(P::Merge, vec![a.id, b.id]);
    let output = Node::new(P::Output, vec![merge.id]);
    let graph = Composite {
        info: motion.info.clone(),
        output: output.id,
        nodes: vec![a, b, merge, output],
        extensions: Default::default(),
    };
    let from = Layer::new(
        "Motion",
        Some(Source::Output(reference(source))),
        motion.info.clone(),
    )
    .unwrap();
    let to = Layer::new(
        "Merge graph",
        Some(Source::Output(reference(target))),
        motion.info.clone(),
    )
    .unwrap();
    let (from_id, to_id) = (from.id, to.id);
    let composition = Composition {
        info: motion.info.clone(),
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
fn badge_wave_projects_open_as_compositions_without_rewriting_content() {
    use fold_app::session::Session;
    use fold_platform::desktop::{DesktopClient, DesktopCommand};
    for name in ["fold-badge-live.fold", "fold-badge-color-ramp.fold"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../demos")
            .join(name);
        let project = fold_project::load(&path, 32).unwrap();
        let snapshot = project.snapshot();
        let compositions: Vec<_> = snapshot
            .state()
            .documents
            .values()
            .filter(|d| d.type_id == fold_composition::COMPOSITION)
            .collect();
        assert_eq!(compositions.len(), 2, "{name}");
        let stack = compositions
            .iter()
            .find_map(|d| {
                let comp = Composition::from_document(d).unwrap();
                (comp.layers.len() == 5).then_some(comp)
            })
            .expect("Badge Wave exposes Badge children in the Composition hierarchy");
        assert_eq!(
            stack
                .layers
                .iter()
                .map(|l| l.name.to_lowercase())
                .collect::<Vec<_>>(),
            ["duplicator", "badge", "text", "background", "background"]
        );
        assert_eq!(
            stack.layers.iter().map(|l| l.visible).collect::<Vec<_>>(),
            [true, false, true, true, true]
        );
        for layer in &stack.layers {
            let Some(Source::Output(source)) = &layer.source else {
                panic!("Missing editable graph")
            };
            let graph = &snapshot.state().documents[&source.document];
            assert_eq!(graph.type_id, fold_motion::MOTION);
            assert_eq!(
                graph.extensions.get(fold_composition::LAYER_GRAPH),
                Some(&serde_json::json!(true))
            );
            assert!(
                source
                    .extensions
                    .contains_key(fold_composition::graph::OBJECT)
            );
        }
        for document in &compositions {
            let frame = pixels(&project, document.id, Time::new(5, 4).unwrap());
            assert!(frame.iter().any(|p| p[3] > 0.9));
        }
        let mut session = Session::default();
        session.command(DesktopCommand::Open(path));
        let start = std::time::Instant::now();
        while session.state().busy {
            session.poll();
            assert!(start.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            snapshot.same_content(&session.snapshot().unwrap()),
            "Opening must not convert {name}"
        );
        assert!(!session.state().dirty);
    }
}

#[test]
fn badge_children_control_referenced_copies_with_undo_and_graph_edits() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/fold-badge-live.fold");
    let mut project = fold_project::load(path, 32).unwrap();
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| {
            d.type_id == fold_composition::COMPOSITION
                && Composition::from_document(d).unwrap().layers.len() == 5
        })
        .unwrap();
    let id = document.id;
    let original = Composition::from_document(document).unwrap();
    let badge = original
        .layers
        .iter()
        .find(|l| l.name.to_lowercase() == "badge")
        .unwrap();
    assert!(badge.graph_children);
    let children: Vec<_> = original
        .layers
        .iter()
        .filter(|l| l.parent == Some(badge.id))
        .collect();
    assert_eq!(
        children.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
        ["Text", "Background"]
    );
    assert!(!badge.visible, "Badge is a hidden reference source");
    let child = children[1].id;
    let time = Time::new(13, 4).unwrap();
    let baseline = pixels(&project, id, time);
    for change in ["visible", "offset", "range", "opacity"] {
        let mut updated = original.clone();
        let layer = updated.layer_mut(child).unwrap();
        match change {
            "visible" => layer.visible = false,
            "offset" => layer.offset[4] += 180.,
            "range" => layer.end = Time::new(1, 1).unwrap(),
            "opacity" => layer.opacity = 0.1,
            _ => unreachable!(),
        }
        apply(
            &mut project,
            fold_composition::package::EDIT,
            serde_json::json!({"document":id,"composition":updated}),
        );
        assert_ne!(
            pixels(&project, id, time),
            baseline,
            "Child {change} must affect the Duplicator's source"
        );
        project.undo().unwrap();
        close(&pixels(&project, id, time), &baseline);
    }
    let Some(Source::Output(source)) = &children[0].source else {
        unreachable!()
    };
    let mut motion =
        fold_motion::Motion::from_document(&project.snapshot().state().documents[&source.document])
            .unwrap();
    let text = children[0].graph_object().unwrap();
    fold_motion::authoring::node(&mut motion, text)
        .unwrap()
        .set("text", fold_motion::fields::Datum::Text("Edited".into()));
    let batch = stage(
        &project,
        fold_motion::package::EDIT,
        serde_json::json!({"document":source.document,"motion":motion}),
    )
    .unwrap();
    project.commit(batch).unwrap();
    assert_ne!(
        pixels(&project, id, time),
        baseline,
        "The original text graph remains editable through Badge references"
    );
    project.undo().unwrap();
    close(&pixels(&project, id, time), &baseline);
}

#[test]
fn native_badge_authoring_adds_children_to_the_same_composition_transaction() {
    let mut project = Project::new(32);
    let id = DocumentId::new();
    put(
        &mut project,
        vec![Composition::default().document(id).unwrap()],
    );
    apply(
        &mut project,
        workflow::ADD,
        serde_json::json!({"document":id,"kind":"motion"}),
    );
    let composition =
        Composition::from_document(&project.snapshot().state().documents[&id]).unwrap();
    let Some(Source::Output(source)) = &composition.layers[0].source else {
        unreachable!()
    };
    let before = project.snapshot();
    let old =
        fold_motion::Motion::from_document(&before.state().documents[&source.document]).unwrap();
    let mut motion = old.clone();
    let badge = fold_motion::authoring::scene::assets::badge(&mut motion).unwrap();
    let copies = fold_motion::authoring::scene::assets::duplicate(&mut motion, badge).unwrap();
    let changes = fold_motion::edits::Changes::between(source.document, &old, &motion).unwrap();
    apply(
        &mut project,
        fold_motion::package::CHANGE,
        serde_json::to_value(changes).unwrap(),
    );
    let composition =
        Composition::from_document(&project.snapshot().state().documents[&id]).unwrap();
    let badge_layer = composition
        .layers
        .iter()
        .find(|l| l.graph_object() == Some(badge))
        .unwrap();
    assert!(badge_layer.graph_children);
    assert!(!badge_layer.visible);
    assert_eq!(
        composition
            .layers
            .iter()
            .filter(|l| l.parent == Some(badge_layer.id))
            .count(),
        2
    );
    assert!(
        composition
            .layers
            .iter()
            .any(|l| l.graph_object() == Some(copies))
    );
    assert_eq!(project.snapshot().revision().0, before.revision().0 + 1);
    project.undo().unwrap();
    assert!(before.same_content(&project.snapshot()));
    project.redo().unwrap();
    assert!(pixels(&project, id, Time::ZERO).iter().any(|p| p[3] > 0.));
}

#[test]
fn precompose_retains_badge_hierarchy_and_rejects_splitting_shared_graph_layers() {
    let mut project = fold_project::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/fold-badge-live.fold"),
        32,
    )
    .unwrap();
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| {
            d.type_id == fold_composition::COMPOSITION
                && Composition::from_document(d).unwrap().layers.len() == 5
        })
        .unwrap();
    let id = document.id;
    let composition = Composition::from_document(document).unwrap();
    let badge = composition
        .layers
        .iter()
        .find(|l| l.graph_children)
        .unwrap();
    assert!(stage(&project, fold_composition::package::NEST, serde_json::json!({"document":id,"child":DocumentId::new(),"name":"Badge","layers":[badge.id]})).is_err());
    assert!(snapshot.same_content(&project.snapshot()));
    let time = Time::new(13, 4).unwrap();
    let before = pixels(&project, id, time);
    let roots: Vec<_> = composition
        .layers
        .iter()
        .filter(|l| l.parent.is_none())
        .map(|l| l.id)
        .collect();
    let child = DocumentId::new();
    apply(
        &mut project,
        fold_composition::package::NEST,
        serde_json::json!({"document":id,"child":child,"name":"Badge Wave","layers":roots}),
    );
    assert_eq!(
        Composition::from_document(&project.snapshot().state().documents[&child])
            .unwrap()
            .layers
            .len(),
        5
    );
    close(&before, &pixels(&project, id, time));
    project.undo().unwrap();
    assert!(snapshot.same_content(&project.snapshot()));
}

#[test]
fn deleting_the_only_graph_layer_is_undoable_for_both_providers() {
    for kind in ["motion", "image"] {
        let mut project = Project::new(8);
        let document = DocumentId::new();
        put(
            &mut project,
            vec![Composition::default().document(document).unwrap()],
        );
        apply(
            &mut project,
            workflow::ADD,
            serde_json::json!({"document":document,"kind":kind}),
        );
        let before = project.snapshot();
        let mut composition =
            Composition::from_document(&before.state().documents[&document]).unwrap();
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
}
