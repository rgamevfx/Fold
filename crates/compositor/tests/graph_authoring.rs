use fold_compositor::{Composite, Node, Parameters as P};
use fold_foundation::DocumentId;
fn fixture() -> Composite {
    let source = Node::new(
        P::Solid {
            rgba: [1.0, 0.0, 0.0, 1.0],
        },
        vec![],
    );
    let blur = Node::new(P::Blur { radius: 2 }, vec![source.id]);
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
    assert_eq!(graph.node(blur).unwrap().inputs, vec![None]);
    assert!(graph.render_order().is_err());
    let old = graph.clone();
    assert!(graph.remove(&[graph.output]).is_err());
    assert_eq!(graph, old);
}
#[test]
fn incomplete_spare_nodes_are_preserved_but_not_silently_rendered() {
    let mut graph = fixture();
    let unused = Node::disconnected(P::Grade { gain: [1.0; 3] });
    let id = unused.id;
    graph.nodes.push(unused);
    graph.validate().unwrap();
    assert!(!graph.render_order().unwrap().contains(&id));
    graph.connect(id, graph.output, "image").unwrap();
    assert!(graph.render_order().unwrap_err().contains("Grade"));
}
#[test]
fn schema_one_roundtrips_without_reordering_or_dropping_extension_data() {
    let mut graph = fixture();
    graph
        .extensions
        .insert("future".into(), serde_json::json!({"a":[1,2]}));
    let mut doc = graph.document(DocumentId::new()).unwrap();
    doc.schema_version = 1;
    // Old payloads have connected UUID values and no position field.
    let mut payload: serde_json::Value = serde_json::from_slice(&doc.payload).unwrap();
    for node in payload["nodes"].as_array_mut().unwrap() {
        node.as_object_mut().unwrap().remove("position");
    }
    doc.payload = serde_json::to_vec(&payload).unwrap();
    assert_eq!(Composite::from_document(&doc).unwrap(), graph);
}
