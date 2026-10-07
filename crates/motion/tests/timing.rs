use fold_animation::timing::Range;
use fold_foundation::{ObjectId, Time};
use fold_motion::{
    Motion, authoring,
    evaluation::{Evaluator, Value},
    fields::{Budget, Sample},
    graph::Link,
};
fn t(n: i64, d: u32) -> Time {
    Time::new(n, d).unwrap()
}
fn sample(
    m: &Motion,
    node: ObjectId,
    time: Time,
    range: Option<Range>,
    socket: &str,
    delay: Time,
) -> f64 {
    let mut e = Evaluator::new(m, time).unwrap();
    e.instance_timing = range;
    let Value::Field(field) = e
        .resolve(&Link {
            node,
            socket: socket.into(),
        })
        .unwrap()
    else {
        panic!()
    };
    field
        .sample(
            &Sample {
                time_offset: Some(delay),
                ..Default::default()
            },
            &mut Budget::default(),
        )
        .unwrap()
        .scalar()
        .unwrap()
}
#[test]
fn instances_have_independent_ranges_and_curves_do_not_change_raw_progress() {
    let mut m = Motion::new_scene();
    let node = authoring::add_node(&mut m, "fold.motion.timing").unwrap();
    authoring::node(&mut m, node).unwrap().settings["smooth"] = true.into();
    let a = Some(Range {
        start: t(10, 1),
        duration: t(4, 1),
    });
    let b = Some(Range {
        start: t(10, 1),
        duration: t(2, 1),
    });
    assert_eq!(sample(&m, node, t(11, 1), a, "progress", Time::ZERO), 0.25);
    assert_eq!(sample(&m, node, t(11, 1), a, "value", Time::ZERO), 0.15625);
    assert_eq!(sample(&m, node, t(11, 1), b, "value", Time::ZERO), 0.5);
    assert_eq!(sample(&m, node, t(14, 1), a, "value", Time::ZERO), 1.);
    assert_eq!(sample(&m, node, t(11, 1), a, "elapsed", Time::ZERO), 1.);
    assert_eq!(sample(&m, node, t(11, 1), a, "duration", Time::ZERO), 4.);
    assert_eq!(sample(&m, node, t(12, 1), a, "value", t(1, 1)), 0.15625);
    let copy =
        Motion::from_document(&m.document(fold_foundation::DocumentId::new()).unwrap()).unwrap();
    assert_eq!(
        sample(&copy, node, t(11, 1), a, "value", Time::ZERO),
        0.15625
    );
}
#[test]
fn explicit_exact_range_overrides_document_and_rejects_zero_duration() {
    let mut m = Motion::new_scene();
    let node = authoring::add_node(&mut m, "fold.motion.timing").unwrap();
    let range = Range {
        start: t(1001, 24000),
        duration: t(1001, 12000),
    };
    authoring::node(&mut m, node).unwrap().settings["range"] = serde_json::to_value(range).unwrap();
    assert_eq!(
        sample(&m, node, t(1001, 12000), None, "progress", Time::ZERO),
        0.5
    );
    authoring::node(&mut m, node).unwrap().settings["range"] = serde_json::to_value(Range {
        duration: Time::ZERO,
        ..range
    })
    .unwrap();
    assert!(m.validate().is_err());
}
