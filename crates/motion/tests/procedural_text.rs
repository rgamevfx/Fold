use fold_foundation::{ObjectId, Time};
use fold_motion::{
    Motion,
    evaluation::{Evaluator, Value},
    fields::{Budget, Datum, Sample},
    graph::{Link, Node},
};

fn resolve(motion: &Motion, id: ObjectId, socket: &str, time: i64) -> Result<Value, String> {
    Evaluator::new(motion, Time::new(time, 1).unwrap())?.resolve(&Link {
        node: id,
        socket: socket.into(),
    })
}

#[test]
fn formats_varying_fields_and_rejects_invalid_templates_and_precision() {
    let mut m = Motion::empty();
    let index = Node::new("fold.motion.index").unwrap();
    let mut format = Node::new("fold.motion.format_text").unwrap();
    format.connect("value", index.id, "value");
    format.set(
        "template",
        Datum::Text("{{copy}} {value}: x={x}, y={y}".into()),
    );
    format.set("x", Datum::Scalar(-0.0001));
    format.set("y", Datum::Scalar(12.375));
    format.set("decimals", Datum::Scalar(2.));
    let id = format.id;
    m.graph.nodes.extend([index, format]);
    let Value::Field(field) = resolve(&m, id, "value", 0).unwrap() else {
        panic!()
    };
    assert!(field.uniform(&mut Budget::default()).is_err());
    assert_eq!(
        field
            .sample(
                &Sample {
                    index: 3,
                    ..Default::default()
                },
                &mut Budget::default()
            )
            .unwrap(),
        Datum::Text("{copy} 3.00: x=0.00, y=12.38".into())
    );
    for template in ["{missing}", "{x", "y}"] {
        m.graph
            .nodes
            .last_mut()
            .unwrap()
            .set("template", Datum::Text(template.into()));
        assert!(resolve(&m, id, "value", 0).is_err());
    }
    m.graph
        .nodes
        .last_mut()
        .unwrap()
        .set("template", Datum::Text("{value}".into()));
    for precision in [-1., 0.5, 7.] {
        m.graph
            .nodes
            .last_mut()
            .unwrap()
            .set("decimals", Datum::Scalar(precision));
        assert!(
            resolve(&m, id, "value", 0)
                .unwrap_err()
                .contains("decimal places")
        );
    }
}

fn labels_fixture() -> (Motion, ObjectId, ObjectId) {
    let mut m = Motion::empty();
    let font_id = ObjectId::new();
    m.fonts
        .insert(font_id, fold_motion::document::Font::builtin());
    let mut points = Node::new("fold.motion.linear_points").unwrap();
    points.set("count", Datum::Scalar(3.));
    points.set("spacing", Datum::Vector([30., 10.]));
    let position = Node::new("fold.motion.position").unwrap();
    let mut separate = Node::new("fold.motion.separate_xy").unwrap();
    separate.connect("value", position.id, "value");
    let time = Node::new("fold.motion.time").unwrap();
    let mut format = Node::new("fold.motion.format_text").unwrap();
    format.set("template", Datum::Text("x: {x} y: {y} t: {value}".into()));
    format.connect("x", separate.id, "x");
    format.connect("y", separate.id, "y");
    format.connect("value", time.id, "value");
    let mut labels = Node::new("fold.motion.text_on_points").unwrap();
    labels.settings = serde_json::json!({"font":font_id});
    labels.connect("points", points.id, "points");
    labels.connect("text", format.id, "value");
    let ids = (points.id, labels.id);
    m.graph
        .nodes
        .extend([points, position, separate, time, format, labels]);
    (m, ids.0, ids.1)
}

#[test]
fn labels_follow_points_time_edits_and_serialization_with_stable_identity() {
    let (mut m, points, labels) = labels_fixture();
    let Value::Content(first) = resolve(&m, labels, "content", 0).unwrap() else {
        panic!()
    };
    assert_eq!(first.len(), 3);
    assert_eq!(
        first[2].attributes["text"],
        Datum::Text("x: 60 y: 20 t: 0".into())
    );
    assert_eq!(&first[2].transform[4..], &[60., 4.]);
    m.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == points)
        .unwrap()
        .set("spacing", Datum::Vector([50., 20.]));
    let reopened: Motion = serde_json::from_slice(&serde_json::to_vec(&m).unwrap()).unwrap();
    let Value::Content(changed) = resolve(&reopened, labels, "content", 2).unwrap() else {
        panic!()
    };
    assert_eq!(
        changed[2].attributes["text"],
        Datum::Text("x: 100 y: 40 t: 2".into())
    );
    assert_eq!(first[2].id, changed[2].id);
    assert_eq!(&changed[2].transform[4..], &[100., 24.]);
    let Value::Content(back) = resolve(&m, labels, "content", 0).unwrap() else {
        panic!()
    };
    assert_eq!(
        back[2].attributes["text"],
        Datum::Text("x: 100 y: 40 t: 0".into())
    );
    m.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == labels)
        .unwrap()
        .set("selection", Datum::Bool(false));
    let Value::Content(hidden) = resolve(&m, labels, "content", 0).unwrap() else {
        panic!()
    };
    assert!(hidden.is_empty());
}

#[test]
fn labels_require_embedded_fonts_and_bound_total_shaping_work() {
    let (mut m, _, labels) = labels_fixture();
    m.fonts.clear();
    assert!(
        resolve(&m, labels, "content", 0)
            .unwrap_err()
            .contains("missing embedded font")
    );
    let (mut m, points, labels) = labels_fixture();
    m.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == points)
        .unwrap()
        .set("count", Datum::Scalar(1000.));
    m.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == labels)
        .unwrap()
        .set("text", Datum::Text("A".repeat(100)));
    assert!(
        resolve(&m, labels, "content", 0)
            .unwrap_err()
            .contains("budget")
    );
}
