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

#[test]
fn evaluated_queries_report_nested_domains_and_reject_unavailable_fields() {
    let d = Motion::empty().document(Default::default()).unwrap();
    let text = ObjectId::new();
    let d = automation::author(
        &d,
        &[
            json!({"op":"object","id":text,"kind":"fold.motion.text","name":"Text"}),
            json!({"op":"set","node":text,"input":"text","value":{"Text":"abc"}}),
        ],
    )
    .unwrap();
    let q = json!({"mode":"evaluated","time":{"numerator":0,"denominator":1},"fields":["counts","elements"],"limit":1});
    let result = automation::discovery::query(&d, &q).unwrap();
    assert_eq!(result["counts"]["glyphs"], 3);
    assert_eq!(result["top_level_domains"]["glyphs"], 0);
    assert_eq!(result["diagnostics"][0]["kind"], "nested_glyphs");
    assert_eq!(result["elements"].as_array().unwrap().len(), 1);
    assert_eq!(result["truncated"], true);
    let mut unavailable = q.clone();
    unavailable["fields"] = json!(["ink_bounds"]);
    assert!(
        automation::discovery::query(&d, &unavailable)
            .unwrap_err()
            .contains("unsupported evaluated field")
    );
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    assert!(registry.query_cancellable(&d, &q, &cancel).is_err());
}

#[test]
fn typed_expression_drives_whole_phrase_and_sampled_verification_rejects_static_motion() {
    let d = Motion::empty().document(Default::default()).unwrap();
    let text = ObjectId::new();
    let driver = ObjectId::new();
    let expression = json!({"op":"vector","x":{"op":"binary","operation":"multiply","a":{"op":"time"},"b":{"op":"input","name":"speed"}},"y":{"op":"literal","value":{"Scalar":0}}});
    let d=automation::author(&d,&[
        json!({"op":"object","id":text,"kind":"fold.motion.text","name":"Phrase"}),
        json!({"op":"set","node":text,"input":"text","value":{"Text":"i love katie"}}),
        json!({"op":"create_node","id":driver,"kind":"fold.motion.expression","settings":{"expression":expression,"inputs":{"speed":{"Scalar":-50}}}}),
        json!({"op":"object_inputs","object":text,"role":"transform","inputs":{"translation":{"Link":{"node":driver,"socket":"value"}}}}),
    ]).unwrap();
    let q = json!({"mode":"verify","check":"displacement","times":[{"numerator":0,"denominator":1},{"numerator":1,"denominator":1}],"expected":[-50,0]});
    let result = automation::discovery::query(&d, &q).unwrap();
    assert_eq!(result["status"], "pass", "{result}");
    let mut direction = q.clone();
    direction["check"] = json!("direction");
    direction["expected"] = json!([-1, 0]);
    assert_eq!(
        automation::discovery::query(&d, &direction).unwrap()["status"],
        "pass"
    );
    let inputs = automation::discovery::query(
        &d,
        &json!({"mode":"parameters","node":driver,"time":{"numerator":1,"denominator":1}}),
    )
    .unwrap();
    assert_eq!(
        inputs["parameters"][0]["evaluated"]["value"],
        json!({"Scalar":-50.0})
    );
    let stationary = automation::author(
        &d,
        &[json!({"op":"set","node":driver,"input":"speed","value":{"Scalar":0}})],
    )
    .unwrap();
    let mut movement = q;
    movement["check"] = json!("movement");
    assert_eq!(
        automation::discovery::query(&stationary, &movement).unwrap()["status"],
        "fail"
    );
    let wrong = json!({"op":"create_node","id":ObjectId::new(),"kind":"fold.motion.expression","settings":{"expression":{"op":"binary","operation":"add","a":{"op":"literal","value":{"Scalar":1}},"b":{"op":"literal","value":{"Vector":[0,0]}}}}});
    assert!(automation::author(&d, &[wrong]).is_err());
}

#[test]
fn periodicity_does_not_accept_changed_paths_with_unchanged_transforms() {
    let d = Motion::empty().document(Default::default()).unwrap();
    let shape = ObjectId::new();
    let width = ObjectId::new();
    let d = automation::author(&d, &[
        json!({"op":"object","id":shape,"kind":"fold.motion.ellipse","name":"Changing shape"}),
        json!({"op":"create_node","id":width,"kind":"fold.motion.expression","settings":{"expression":{"op":"binary","operation":"add","a":{"op":"literal","value":{"Scalar":100}},"b":{"op":"time"}}}}),
        json!({"op":"connect","node":shape,"input":"width","source":{"node":width,"socket":"value"}}),
    ]).unwrap();
    let result = automation::discovery::query(&d, &json!({"mode":"verify","check":"periodicity","times":[{"numerator":0,"denominator":1},{"numerator":1,"denominator":2},{"numerator":1,"denominator":1}]})).unwrap();
    assert_eq!(result["status"], "fail", "{result}");
}

#[test]
fn editable_text_sequence_layout_and_shape_points_use_native_evaluation() {
    let d = Motion::empty().document(Default::default()).unwrap();
    let text = ObjectId::new();
    let selection = ObjectId::new();
    let time = ObjectId::new();
    let align = ObjectId::new();
    let points = ObjectId::new();
    let d = automation::author(&d, &[
        json!({"op":"object","id":text,"kind":"fold.motion.text","name":"Editable sequence"}),
        json!({"op":"create_node","id":time,"kind":"fold.motion.time"}),
        json!({"op":"create_node","id":selection,"kind":"fold.motion.text_select","inputs":{"text":{"Value":{"Text":"A \nWW"}},"index":{"Link":{"node":time,"socket":"value"}}}}),
        json!({"op":"connect","node":text,"input":"text","source":{"node":selection,"socket":"value"}}),
        json!({"op":"create_node","id":align,"kind":"fold.motion.align_content","inputs":{"content":{"Link":{"node":text,"socket":"content"}},"target":{"Value":{"Vector":[100,80]}}}}),
        json!({"op":"create_node","id":points,"kind":"fold.motion.shape_points","inputs":{"content":{"Link":{"node":align,"socket":"content"}},"spacing":{"Value":{"Scalar":8}}}}),
    ]).unwrap();
    let metrics = automation::discovery::query(
        &d,
        &json!({"mode":"text_metrics","node":text,"time":{"numerator":0,"denominator":1}}),
    )
    .unwrap();
    let width = |value: &serde_json::Value| value[2].as_f64().unwrap() - value[0].as_f64().unwrap();
    assert!(width(&metrics["layout_bounds"]) > width(&metrics["ink_bounds"]));
    assert_eq!(metrics["text"], "A ");
    for second in [0, 1, 2] {
        let aligned = automation::discovery::query(&d, &json!({"mode":"evaluated","node":align,"output":"content","time":{"numerator":second,"denominator":1},"fields":["bounds"]})).unwrap();
        let b = aligned["bounds"].as_array().unwrap();
        assert!(((b[0].as_f64().unwrap() + b[2].as_f64().unwrap()) / 2. - 100.).abs() < 1e-8);
        assert!(((b[1].as_f64().unwrap() + b[3].as_f64().unwrap()) / 2. - 80.).abs() < 1e-8);
    }
    let q = json!({"mode":"evaluated","node":points,"output":"points","time":{"numerator":0,"denominator":1},"fields":["counts","elements"],"limit":200});
    let initial = automation::discovery::query(&d, &q).unwrap();
    assert!(initial["counts"]["points"].as_u64().unwrap() > 5);
    assert_eq!(initial["counts"]["groups"], 0);
    let mut later = q.clone();
    later["time"] = json!({"numerator":1,"denominator":1});
    let changed = automation::discovery::query(&d, &later).unwrap();
    assert_ne!(initial["elements"], changed["elements"]);
    let mut wrapped = q;
    wrapped["time"] = json!({"numerator":2,"denominator":1});
    let repeated = automation::discovery::query(&d, &wrapped).unwrap();
    assert_eq!(initial["elements"], repeated["elements"]);
}

#[test]
fn relative_position_checks_bounds_and_center_offsets() {
    let mut m = Motion::empty();
    let a = Node::new("fold.motion.ellipse").unwrap();
    let mut b = Node::new("fold.motion.align_content").unwrap();
    b.connect("content", a.id, "content");
    b.set("target", Datum::Vector([250., 50.]));
    let (aid, bid) = (a.id, b.id);
    m.graph.nodes.extend([a, b]);
    let d = m.document(Default::default()).unwrap();
    let mut query = json!({"mode":"verify","check":"relative_position","target":{"node":aid,"output":"content"},"relative_to":{"node":bid,"output":"content"},"relation":"left","times":[{"numerator":0,"denominator":1},{"numerator":1,"denominator":1}]});
    assert_eq!(
        automation::discovery::query(&d, &query).unwrap()["status"],
        "pass"
    );
    query["relation"] = json!("right");
    assert_eq!(
        automation::discovery::query(&d, &query).unwrap()["status"],
        "fail"
    );
    query["relation"] = json!("offset");
    query["expected"] = json!([-200., 0.]);
    assert_eq!(
        automation::discovery::query(&d, &query).unwrap()["status"],
        "pass"
    );
}

#[test]
fn control_response_requires_a_connected_effect_and_preserves_the_document() {
    let d = Motion::empty().document(Default::default()).unwrap();
    let shape = ObjectId::new();
    let control = ObjectId::new();
    let control_id = ObjectId::new();
    let d = automation::author(&d, &[
        json!({"op":"object","id":shape,"kind":"fold.motion.ellipse","name":"Controlled width"}),
        json!({"op":"create_node","id":control,"kind":"fold.motion.published_control","settings":{"id":control_id,"name":"Width","value_type":"Scalar"},"inputs":{"default":{"Value":{"Scalar":100}}}}),
    ]).unwrap();
    let q = json!({"mode":"verify","check":"control_response","control":control,"values":[{"Scalar":80},{"Scalar":160}],"times":[{"numerator":0,"denominator":1},{"numerator":1,"denominator":1}]});
    assert_eq!(
        automation::discovery::query(&d, &q).unwrap()["status"],
        "fail"
    );
    let d = automation::author(&d,&[json!({"op":"connect","node":shape,"input":"width","source":{"node":control,"socket":"value"}})]).unwrap();
    let original = d.payload.clone();
    assert_eq!(
        automation::discovery::query(&d, &q).unwrap()["status"],
        "pass"
    );
    assert_eq!(d.payload, original);
    assert!(Motion::from_document(&d).unwrap().controls.is_empty());
}
