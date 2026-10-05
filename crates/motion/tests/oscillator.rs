use fold_foundation::{ObjectId, Time};
use fold_motion::{
    Motion,
    authoring::{self, node, scene::assets},
    evaluation::{Evaluator, Value},
    fields::{Budget, Datum, Sample},
    graph::Link,
};
fn fixture() -> (Motion, ObjectId) {
    let mut m = Motion::new_scene();
    let target = authoring::add_node(&mut m, "fold.motion.value").unwrap();
    let driver = assets::attach_oscillator(&mut m, target, "value").unwrap();
    node(&mut m, driver)
        .unwrap()
        .set("minimum", Datum::Scalar(0.));
    node(&mut m, driver)
        .unwrap()
        .set("maximum", Datum::Scalar(120.));
    (m, driver)
}
fn sample(m: &Motion, driver: ObjectId, time: Time, s: Sample) -> Datum {
    let Value::Field(field) = Evaluator::new(m, time)
        .unwrap()
        .resolve(&Link {
            node: driver,
            socket: "value".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    field.sample(&s, &mut Budget::default()).unwrap()
}
#[test]
fn duration_bpm_reverse_time_and_stagger_share_random_access_semantics() {
    let (mut m, id) = fixture();
    for (n, expected) in [(1, 120.), (0, 60.), (3, 0.), (-1, 0.), (1, 120.)] {
        let v = sample(&m, id, Time::new(n, 2).unwrap(), Sample::default())
            .scalar()
            .unwrap();
        assert!((v - expected).abs() < 1e-9);
    }
    node(&mut m, id)
        .unwrap()
        .set("time_mode", Datum::Text("BPM".into()));
    node(&mut m, id).unwrap().set("bpm", Datum::Scalar(30.));
    assert_eq!(
        sample(&m, id, Time::new(1, 2).unwrap(), Sample::default()),
        Datum::Scalar(120.)
    );
    let delayed = Sample {
        time_offset: Some(Time::new(1, 2).unwrap()),
        ..Default::default()
    };
    assert_eq!(
        sample(&m, id, Time::new(1, 2).unwrap(), delayed),
        Datum::Scalar(60.)
    );
    node(&mut m, id)
        .unwrap()
        .set("time_scale", Datum::Scalar(-1.));
    assert_eq!(
        sample(&m, id, Time::new(1, 2).unwrap(), Sample::default()),
        Datum::Scalar(0.)
    );
}
#[test]
fn phase_spread_and_shapes_are_domain_aware() {
    let (mut m, id) = fixture();
    node(&mut m, id)
        .unwrap()
        .set("phase_spread", Datum::Scalar(0.25));
    assert_eq!(
        sample(
            &m,
            id,
            Time::ZERO,
            Sample {
                index: 3,
                count: 4,
                ..Default::default()
            }
        ),
        Datum::Scalar(120.)
    );
    node(&mut m, id)
        .unwrap()
        .set("phase_spread", Datum::Scalar(0.));
    for (shape, expected) in [
        ("Sine", 120.),
        ("Triangle", 60.),
        ("Square", 120.),
        ("Sawtooth", 30.),
        ("Custom", 120.),
    ] {
        node(&mut m, id)
            .unwrap()
            .set("shape", Datum::Text(shape.into()));
        assert_eq!(
            sample(&m, id, Time::new(1, 2).unwrap(), Sample::default()),
            Datum::Scalar(expected)
        );
    }
    let Value::Field(field) = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .resolve(&Link {
            node: id,
            socket: "value".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    assert!(field.uniform(&mut Budget::default()).is_ok());
}
#[test]
fn vector_channels_and_custom_curve_survive_serialization() {
    let mut m = Motion::new_scene();
    let target = authoring::add_node(&mut m, "fold.motion.separate_xy").unwrap();
    let id = assets::attach_oscillator(&mut m, target, "value").unwrap();
    node(&mut m, id)
        .unwrap()
        .set("minimum", Datum::Vector([-1., -1.]));
    node(&mut m, id)
        .unwrap()
        .set("maximum", Datum::Vector([1., 1.]));
    node(&mut m, id)
        .unwrap()
        .set("channel_phase", Datum::Scalar(0.25));
    assert_eq!(
        sample(&m, id, Time::ZERO, Sample::default()),
        Datum::Vector([0., 1.])
    );
    node(&mut m, id)
        .unwrap()
        .set("shape", Datum::Text("Custom".into()));
    let group = m
        .graph
        .node(id)
        .unwrap()
        .settings::<fold_motion::nodes::interface::GroupSettings>()
        .unwrap()
        .group
        .unwrap();
    let primitive = m
        .groups
        .get_mut(&group)
        .unwrap()
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.oscillator")
        .unwrap();
    primitive.settings["curve"] = serde_json::json!([[0., 0.], [1., 1.]]);
    primitive.settings["smooth"] = false.into();
    let m: Motion = serde_json::from_value(serde_json::to_value(&m).unwrap()).unwrap();
    assert_eq!(
        sample(&m, id, Time::ZERO, Sample::default()),
        Datum::Vector([-1., -0.5])
    );
}
#[test]
fn invalid_period_and_existing_animation_are_not_silently_replaced() {
    let (mut m, id) = fixture();
    node(&mut m, id).unwrap().set("duration", Datum::Scalar(0.));
    let Value::Field(f) = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .resolve(&Link {
            node: id,
            socket: "value".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    assert!(
        f.sample(&Sample::default(), &mut Budget::default())
            .is_err()
    );
    let target = authoring::add_node(&mut m, "fold.motion.value").unwrap();
    node(&mut m, target)
        .unwrap()
        .animation
        .entry("value.0".into())
        .or_default()
        .insert(Time::ZERO, 3.);
    let before = serde_json::to_value(&m).unwrap();
    assert!(assets::attach_oscillator(&mut m, target, "value").is_err());
    assert_eq!(serde_json::to_value(&m).unwrap(), before);
}
