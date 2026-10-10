use fold_compositor::{Composite, Node, OperationConfig as P};
use fold_foundation::DocumentId;
fn fixture() -> Composite {
    let source = Node::new(
        P::Solid {
            rgba: [1.0, 0.0, 0.0, 1.0],
        },
        vec![],
    );
    let blur = Node::new(
        P::Blur {
            filter: Default::default(),
            size: [2.; 2],
            edges: Default::default(),
        },
        vec![source.id],
    );
    let output = Node::new(P::Output, vec![blur.id]);
    Composite {
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![output, source, blur],
        extensions: Default::default(),
    }
}
#[test]
fn storage_order_and_layout_are_not_execution_order() {
    let mut graph = fixture();
    graph.validate().unwrap();
    let expected = graph.render_order().unwrap();
    assert_eq!(*expected.last().unwrap(), graph.output);
    graph.nodes.reverse();
    graph.auto_layout().unwrap();
    assert_eq!(graph.render_order().unwrap(), expected);
    let doc = graph.document(DocumentId::new()).unwrap();
    assert_eq!(Composite::from_document(&doc).unwrap(), graph);
    graph.nodes[0].position = Some([1234.0, -32.0]);
    assert_eq!(graph.render_order().unwrap(), expected);
}
#[test]
fn named_sockets_rewire_disconnect_delete_and_reject_cycles_atomically() {
    let mut graph = fixture();
    let source = graph.nodes[1].id;
    let blur = graph.nodes[2].id;
    let old = graph.clone();
    assert!(
        graph
            .connect(blur, blur, "image")
            .unwrap_err()
            .contains("cycle")
    );
    assert_eq!(graph, old);
    let mask = Node::new(P::Mask { rect: [0, 0, 8, 8] }, vec![]);
    let mask_id = mask.id;
    graph.nodes.push(mask);
    assert!(graph.connect(mask_id, blur, "image").is_err());
    graph.disconnect(blur, "image").unwrap();
    graph.validate().unwrap();
    assert!(
        graph
            .render_order()
            .unwrap_err()
            .contains("connect 'image'")
    );
    graph.connect(source, blur, "image").unwrap();
    graph.remove(&[source]).unwrap();
    graph.validate().unwrap();
    assert_eq!(graph.node(blur).unwrap().input_ids(), vec![None]);
    assert!(graph.render_order().is_err());
    let old = graph.clone();
    assert!(graph.remove(&[graph.output]).is_err());
    assert_eq!(graph, old);
}
#[test]
fn incomplete_spare_nodes_are_preserved_but_not_silently_rendered() {
    let mut graph = fixture();
    let unused = Node::disconnected(P::ColorGrade {
        settings: fold_render::operations::Grade {
            gain: [1.0; 3],
            ..Default::default()
        },
    });
    let id = unused.id;
    graph.nodes.push(unused);
    graph.validate().unwrap();
    assert!(!graph.render_order().unwrap().contains(&id));
    graph.connect(id, graph.output, "image").unwrap();
    assert!(graph.render_order().unwrap_err().contains("Grade"));
}
#[test]
fn native_schema_roundtrips_without_reordering_or_dropping_extension_data() {
    let mut graph = fixture();
    graph
        .extensions
        .insert("future".into(), serde_json::json!({"a":[1,2]}));
    let mut doc = graph.document(DocumentId::new()).unwrap();

    // Optional presentation fields do not affect native graph identity.
    let mut payload: serde_json::Value = serde_json::from_slice(&doc.payload).unwrap();
    for node in payload["nodes"].as_array_mut().unwrap() {
        node.as_object_mut().unwrap().remove("position");
    }
    doc.payload = serde_json::to_vec(&payload).unwrap();
    assert_eq!(Composite::from_document(&doc).unwrap(), graph);
}

#[test]
fn rewiring_retains_connection_identity_and_unknown_data_and_rejects_aliases() {
    let mut graph = fixture();
    let output = graph.output;
    let source = graph.nodes[1].id;
    let edge = graph.node_mut(output).unwrap().inputs[0].as_mut().unwrap();
    edge.extensions
        .insert("future.route".into(), serde_json::json!({"v":1}));
    let id = edge.id;
    graph.connect(source, output, "image").unwrap();
    let edge = graph.node(output).unwrap().inputs[0].as_ref().unwrap();
    assert_eq!(edge.id, id);
    assert_eq!(edge.extensions["future.route"], serde_json::json!({"v":1}));
    assert_eq!(graph.node(output).unwrap().input_ids(), vec![Some(source)]);
    let document = graph.document(DocumentId::new()).unwrap();
    assert_eq!(Composite::from_document(&document).unwrap(), graph);
    graph.nodes[2].inputs[0].as_mut().unwrap().id = id;
    assert!(
        graph
            .validate()
            .unwrap_err()
            .contains("connection identities")
    );
}

#[test]
fn duplicate_allocates_new_native_connection_parameter_track_and_key_identities() {
    use fold_foundation::{ObjectId, Time};
    let mut graph = fixture();
    let node = graph.nodes[2].id;
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 2.);
    graph.nodes[2]
        .animation
        .insert("size.0".into(), curve)
        .unwrap();
    let copy = ObjectId::new();
    let document = graph.document(DocumentId::new()).unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    let (copied, _) = registry
        .author_cancellable(
            &document,
            &[serde_json::json!({"op":"duplicate","node":node,"id":copy})],
            &Default::default(),
        )
        .unwrap();
    let copied = Composite::from_document(&copied).unwrap();
    let original = copied.node(node).unwrap();
    let duplicate = copied.node(copy).unwrap();
    assert_eq!(original.input_ids(), duplicate.input_ids());
    assert_ne!(
        original.inputs[0].as_ref().unwrap().id,
        duplicate.inputs[0].as_ref().unwrap().id
    );
    let a = &original.animation.bindings()["size.0"];
    let b = &duplicate.animation.bindings()["size.0"];
    assert_ne!(a.id, b.id);
    let fold_graph::binding::Binding::Track { track: a } = a.binding().unwrap() else {
        panic!()
    };
    let fold_graph::binding::Binding::Track { track: b } = b.binding().unwrap() else {
        panic!()
    };
    assert_ne!(a.id, b.id);
    assert_ne!(a.keys[0].id, b.keys[0].id);
    assert_eq!(a.sample(Time::ZERO).unwrap(), b.sample(Time::ZERO).unwrap());
}
