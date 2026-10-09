use fold_animation::Curve;
use fold_compositor::{Composite, Node, OperationConfig as P};
use fold_foundation::{DocumentId, Time};
use fold_platform::packages::{CommandRequest, PackageRegistry};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
fn fixture() -> Composite {
    let source = Node::new(
        P::Solid {
            rgba: [1., 0., 0., 1.],
        },
        vec![],
    );
    let transform = Node::new(
        P::ColorGrade {
            settings: Default::default(),
        },
        vec![source.id],
    );
    let output = Node::new(P::Output, vec![transform.id]);
    Composite {
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 96,
        },
        output: output.id,
        nodes: vec![source, transform, output],
        extensions: Default::default(),
    }
}
#[test]
fn animation_compiles_from_snapshots_roundtrips_extensions_and_undoes_atomically() {
    let id = DocumentId::new();
    let mut graph = fixture();
    graph.nodes[1]
        .extensions
        .insert("future".into(), serde_json::json!({"data":[1,2,3]}));
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(id).unwrap())],
        })
        .unwrap();
    let before = project.snapshot();
    let mut curve = Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(2, 1).unwrap(), 1.);
    graph.nodes[1]
        .animation
        .insert("gain.0".into(), curve)
        .unwrap();
    let mut registry = PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    let request = CommandRequest {
        id: fold_compositor::package::EDIT.into(),
        base: before.revision(),
        arguments: serde_json::to_vec(&fold_compositor::package::EditArgs {
            document: id,
            composite: graph.clone(),
        })
        .unwrap(),
    };
    project
        .commit(
            registry
                .stage(&before, &request, &Default::default())
                .unwrap(),
        )
        .unwrap();
    let edited = project.snapshot();
    let reference = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    for (n, expected) in [(2, 1.), (0, 0.), (1, 0.5), (1, 0.5)] {
        let frame = fold_render::render(
            registry
                .video(&edited, &reference, Time::new(n, 1).unwrap(), 16, 16)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(frame.pixels()[0], [expected, 0., 0., 1.]);
    }
    // Old immutable snapshots are unaffected by subsequent animation edits.
    let frame = fold_render::render(
        registry
            .video(&before, &reference, Time::ZERO, 16, 16)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(frame.pixels()[0][3], 1.);
    let bytes = serde_json::to_vec(&graph.document(id).unwrap()).unwrap();
    let reopened = Composite::from_document(&serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(reopened, graph);
    project.undo().unwrap();
    assert!(
        Composite::from_document(&project.snapshot().state().documents[&id])
            .unwrap()
            .nodes[1]
            .animation
            .is_empty()
    );
    project.redo().unwrap();
    assert_eq!(
        Composite::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        graph
    );
}
#[test]
fn retired_schema_is_rejected_and_component_curves_do_not_change_other_values() {
    let mut graph = fixture();
    graph.nodes[1]
        .set_configuration(P::TransformImage {
            settings: Default::default(),
        })
        .unwrap();
    let id = DocumentId::new();
    let mut legacy = graph.document(id).unwrap();
    legacy.schema_version = 2;
    assert!(Composite::from_document(&legacy).is_err());
    let mut curve = Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 100.);
    graph.nodes[1]
        .animation
        .insert("position.0".into(), curve.clone())
        .unwrap();
    let P::TransformImage { settings } = graph.nodes[1]
        .prepare()
        .unwrap()
        .evaluated_parameters(Time::new(1, 2).unwrap())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(settings.translate, [50., 0.]);
    assert_eq!(settings.scale, [1., 1.]);
    graph.nodes[1]
        .animation
        .insert("unavailable.0".into(), curve)
        .unwrap();
    assert!(graph.validate().is_err());
}

#[test]
fn removing_the_last_channel_holds_its_current_value_and_static_parameters_are_exact() {
    let mut node = Node::disconnected(P::Crop {
        rect: [0, 0, 100_000, 100_000],
    });
    assert_eq!(
        node.prepare()
            .unwrap()
            .evaluated_parameters(Time::ZERO)
            .unwrap(),
        node.prepare().unwrap().config().clone()
    );
    let mut curve = Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 100.);
    node.animation.insert("bounds.0".into(), curve).unwrap();
    let time = Time::new(1, 2).unwrap();
    let value = node.prepare().unwrap().evaluated_parameters(time).unwrap();
    assert_eq!(
        value,
        P::Crop {
            rect: [50, 0, 100_000, 100_000]
        }
    );
    node.remove_animation_channel("bounds.0", time).unwrap();
    assert!(node.animation.is_empty());
    assert_eq!(node.prepare().unwrap().config().clone(), value);
}

#[test]
fn native_tracks_preserve_metadata_and_use_shared_combination_and_bezier_sampling() {
    use fold_graph::binding::{Binding, Combination};
    use fold_graph::types::Literal;
    let mut graph = fixture();
    let mut curve = Curve::default();
    let first = curve.insert(Time::ZERO, 0.2);
    curve.insert(Time::new(2, 1).unwrap(), 0.6);
    curve.keys[0].interpolation = fold_animation::Interpolation::Bezier;
    curve.keys[0].outgoing.value = 0.1;
    graph.nodes[1]
        .animation
        .insert("gain.0".into(), curve.clone())
        .unwrap();
    let parameter = graph.nodes[1].animation.bindings()["gain.0"].clone();
    let Binding::Track { mut track } = parameter.binding().unwrap() else {
        panic!()
    };
    let track_id = track.id;
    track.combination = Combination::Add;
    track.base = Some(Literal::Scalar { value: 0.1 });
    track
        .extensions
        .insert("future.track".into(), serde_json::json!([1, 2]));
    track.keys[0]
        .extensions
        .insert("future.key".into(), serde_json::json!({"x":true}));
    let mut native = serde_json::to_value(&graph.nodes[1].animation).unwrap();
    native["gain.0"]["future.parameter"] = serde_json::json!({"v":3});
    native["gain.0"]["value"] = serde_json::to_value(Binding::Track { track }).unwrap();
    graph.nodes[1].animation = serde_json::from_value(native).unwrap();
    let time = Time::new(1, 1).unwrap();
    let P::ColorGrade { settings } = graph.nodes[1]
        .prepare()
        .unwrap()
        .evaluated_parameters(time)
        .unwrap()
    else {
        panic!()
    };
    assert!((f64::from(settings.gain[0]) - (0.1 + curve.sample(time).unwrap())).abs() < 1e-7);
    curve.keys[0].value = 0.3;
    graph.nodes[1]
        .animation
        .insert("gain.0".into(), curve)
        .unwrap();
    let after = &graph.nodes[1].animation.bindings()["gain.0"];
    assert_eq!(after.id, parameter.id);
    assert_eq!(
        after.extensions["future.parameter"],
        serde_json::json!({"v":3})
    );
    let Binding::Track { track } = after.binding().unwrap() else {
        panic!()
    };
    assert_eq!(track.id, track_id);
    assert_eq!(track.keys[0].id, first);
    assert_eq!(
        track.keys[0].extensions["future.key"],
        serde_json::json!({"x":true})
    );
    assert_eq!(track.extensions["future.track"], serde_json::json!([1, 2]));
    assert_eq!(track.combination, Combination::Add);
    let id = DocumentId::new();
    assert_eq!(
        Composite::from_document(&graph.document(id).unwrap()).unwrap(),
        graph
    );
}

#[test]
fn protected_tracks_reject_editor_removal_and_whole_document_replacement() {
    let mut graph = fixture();
    let mut curve = Curve::default();
    curve.insert(Time::ZERO, 0.4);
    graph.nodes[1]
        .animation
        .insert("gain.0".into(), curve.clone())
        .unwrap();
    let mut native = serde_json::to_value(&graph.nodes[1].animation).unwrap();
    native["gain.0"]["protected"] = serde_json::json!(true);
    graph.nodes[1].animation = serde_json::from_value(native).unwrap();
    let unchanged = graph.clone();
    curve.keys[0].value = 0.6;
    assert!(
        graph.nodes[1]
            .animation
            .insert("gain.0".into(), curve)
            .is_err()
    );
    assert!(
        graph.nodes[1]
            .remove_animation_channel("gain.0", Time::ZERO)
            .is_err()
    );
    assert_eq!(graph, unchanged);
    let id = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(id).unwrap())],
        })
        .unwrap();
    graph.nodes[1].animation = Default::default();
    let mut registry = PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    let snapshot = project.snapshot();
    let result = registry.stage(
        &snapshot,
        &CommandRequest {
            id: fold_compositor::package::EDIT.into(),
            base: snapshot.revision(),
            arguments: serde_json::to_vec(&fold_compositor::package::EditArgs {
                document: id,
                composite: graph,
            })
            .unwrap(),
        },
        &Default::default(),
    );
    assert!(result.unwrap_err().contains("protected"));
    assert_eq!(
        Composite::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        unchanged
    );
}

#[test]
fn retired_compositor_payload_is_preserved_opaquely_by_project_save_and_reopen() {
    let graph = fixture();
    let id = DocumentId::new();
    let mut retired = graph.document(id).unwrap();
    retired.schema_version = 6;
    let mut payload = serde_json::to_value(&graph).unwrap();
    for (node, native) in payload["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(&graph.nodes)
    {
        node["inputs"] = serde_json::to_value(native.input_ids()).unwrap();
    }
    payload["extensions"]["future.payload"] = serde_json::json!({"preserve":[1,2,3]});
    retired.payload = serde_json::to_vec(&payload).unwrap();
    let bytes = retired.payload.clone();
    let mut registry = PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    assert!(!registry.supports(&retired));
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(retired)],
        })
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "fold-retired-compositor-{:?}.fold",
        fold_foundation::ObjectId::new()
    ));
    fold_project::save_new(&project.snapshot(), &path, &|| Ok(())).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    std::fs::remove_file(path).unwrap();
    let reopened = reopened.snapshot();
    let retained = &reopened.state().documents[&id];
    assert_eq!(retained.payload, bytes);
    assert_eq!(retained.schema_version, 6);
    assert!(!registry.supports(retained));
    assert!(Composite::from_document(retained).is_err());
}
