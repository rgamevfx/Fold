use fold_foundation::ObjectId;
use fold_motion::{
    Motion,
    fields::Datum,
    graph::{Input, Link, Node},
};
use serde_json::json;
#[test]
fn configured_nodes_specialize_defaults_and_return_roles() {
    let d = Motion::empty().document(Default::default()).unwrap();
    let id = ObjectId::new();
    let next=automation::author(&d,&[json!({"op":"create_node","id":id,"kind":"fold.motion.math","settings":{"value_type":"Vector","operation":"Multiply"},"inputs":{"a":{"Value":{"Vector":[3,4]}}}})]).unwrap();
    let m = Motion::from_document(&next).unwrap();
    assert!(matches!(
        m.graph.node(id).unwrap().inputs["b"],
        Input::Value(Datum::Vector(_))
    ));
    let r = automation::discovery::query(&next, &json!({"node":id})).unwrap();
    assert_eq!(r["nodes"][0]["resolved"]["outputs"][0][1], "Vector");
}
#[test]
fn grouping_promoting_and_independent_copy_preserve_structure() {
    let mut m = Motion::empty();
    let mut a = Node::new("fold.motion.math").unwrap();
    a.set("a", Datum::Scalar(3.));
    let mut b = Node::new("fold.motion.math").unwrap();
    b.connect("a", a.id, "value");
    let mut c = Node::new("fold.motion.math").unwrap();
    c.connect("a", b.id, "value");
    let (aid, bid, cid) = (a.id, b.id, c.id);
    m.graph.nodes.extend([a, b, c]);
    let d = m.document(Default::default()).unwrap();
    let instance = ObjectId::new();
    let group = ObjectId::new();
    let next=automation::author(&d,&[json!({"op":"group_nodes","nodes":[bid],"id":instance,"definition":group,"name":"Tool"}),json!({"op":"publish","group":group,"node":bid,"input":"b","port":"gain","name":"Gain"})]).unwrap();
    let m = Motion::from_document(&next).unwrap();
    assert_eq!(m.groups[&group].inputs.len(), 2);
    assert!(matches!(&m.graph.node(cid).unwrap().inputs["a"],Input::Link(l)if l.node==instance));
    assert!(
        matches!(&m.graph.node(instance).unwrap().inputs["input_0"],Input::Link(l)if l.node==aid)
    );
    let copy = ObjectId::new();
    let next = automation::author(
        &next,
        &[json!({"op":"duplicate","node":instance,"id":copy,"independent":true})],
    )
    .unwrap();
    let m = Motion::from_document(&next).unwrap();
    assert_ne!(m.graph.node(copy).unwrap().settings["group"], json!(group));
}
#[test]
fn insertion_is_targeted_and_invalid_replacement_is_atomic() {
    let mut m = Motion::empty();
    let a = Node::new("fold.motion.math").unwrap();
    let mut b = Node::new("fold.motion.math").unwrap();
    b.connect("a", a.id, "value");
    let c = Node::new("fold.motion.math").unwrap();
    let (aid, bid, cid) = (a.id, b.id, c.id);
    m.graph.nodes.extend([a, b, c]);
    let d = m.document(Default::default()).unwrap();
    let next=automation::author(&d,&[json!({"op":"insert","node":bid,"input":"a","modifier":cid,"modifier_input":"a","output":"value"})]).unwrap();
    let m = Motion::from_document(&next).unwrap();
    assert_eq!(
        m.graph.node(cid).unwrap().inputs["a"],
        Input::Link(Link {
            node: aid,
            socket: "value".into()
        })
    );
    assert!(
        automation::author(
            &next,
            &[json!({"op":"replace","node":cid,"kind":"fold.motion.rectangle"})]
        )
        .is_err()
    );
    assert_eq!(Motion::from_document(&next).unwrap(), m);
}

mod automation {
    use fold_project::Document;
    use serde_json::Value;
    fn registry() -> fold_platform::packages::PackageRegistry {
        let mut r = fold_platform::packages::PackageRegistry::default();
        fold_motion::package::register(&mut r).unwrap();
        r
    }
    pub fn author(d: &Document, edits: &[Value]) -> Result<Document, String> {
        registry().author(d, edits)
    }
    pub mod discovery {
        use super::*;
        pub fn query(d: &Document, q: &Value) -> Result<Value, String> {
            registry().query(d, q)
        }
    }
}

#[test]
fn publishing_animation_moves_curves_to_instances() {
    let mut m = Motion::empty();
    let n = Node::new("fold.motion.math").unwrap();
    let id = n.id;
    m.graph.nodes.push(n);
    fold_motion::authoring::group_node(&mut m, id).unwrap();
    let group: ObjectId =
        serde_json::from_value(m.graph.node(id).unwrap().settings["group"].clone()).unwrap();
    let inner = m
        .groups
        .get_mut(&group)
        .unwrap()
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap();
    inner.set("a", Datum::Scalar(2.));
    let mut curve = fold_animation::Curve::default();
    curve.insert(fold_foundation::Time::ZERO, 2.);
    curve.insert(fold_foundation::Time::new(1, 1).unwrap(), 8.);
    inner.animation.insert("a.0".into(), curve.clone());
    let d = m.document(Default::default()).unwrap();
    let next = automation::author(
        &d,
        &[json!({"op":"publish","group":group,"node":id,"input":"a","port":"gain","name":"Gain"})],
    )
    .unwrap();
    let m = Motion::from_document(&next).unwrap();
    assert_eq!(m.graph.node(id).unwrap().animation["gain.0"], curve);
    assert!(
        m.groups[&group]
            .graph
            .node(id)
            .unwrap()
            .animation
            .is_empty()
    );
}

#[test]
fn independent_copy_remaps_control_overrides_and_reports_interface_changes() {
    let mut m = Motion::empty();
    let mut first = Node::new("fold.motion.published_control").unwrap();
    let control = ObjectId::new();
    first.settings["id"] = json!(control);
    let second = Node::new("fold.motion.published_control").unwrap();
    m.controls.insert(control, Datum::Scalar(5.));
    let (a, b) = (first.id, second.id);
    m.graph.nodes.extend([first, second]);
    let document = m.document(Default::default()).unwrap();
    let (group, instance, copy) = (ObjectId::new(), ObjectId::new(), ObjectId::new());
    let grouped = automation::author(&document, &[
        json!({"op":"group_nodes","nodes":[a,b],"id":instance,"definition":group,"name":"Shared control"}),
        json!({"op":"duplicate","node":instance,"id":copy,"independent":true})
    ]).unwrap();
    let m = Motion::from_document(&grouped).unwrap();
    let copied_group: ObjectId =
        serde_json::from_value(m.graph.node(copy).unwrap().settings["group"].clone()).unwrap();
    let nodes = &m.groups[&copied_group].graph;
    assert_ne!(
        nodes.node(a).unwrap().settings["id"],
        nodes.node(b).unwrap().settings["id"]
    );
    let copied_control: ObjectId =
        serde_json::from_value(nodes.node(a).unwrap().settings["id"].clone()).unwrap();
    assert_eq!(m.controls[&copied_control], Datum::Scalar(5.));
    assert_ne!(nodes.node(a).unwrap().settings["id"], json!(control));
    let published = automation::author(&grouped, &[json!({"op":"publish","group":group,"node":a,"input":"default","port":"value","name":"Value"})]).unwrap();
    let renamed = automation::author(
        &published,
        &[json!({"op":"port","group":group,"port":"value","name":"Strength"})],
    )
    .unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    let receipt = registry.authoring_receipt(&published, &renamed).unwrap();
    assert_eq!(receipt["groups"][0]["inputs"][0]["name"], "Strength");
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(
        registry
            .query_cancellable(&renamed, &json!({"mode":"validate","edits":[]}), &cancel)
            .is_err()
    );
}
