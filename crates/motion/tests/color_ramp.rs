use fold_foundation::{ObjectId, Time};
use fold_motion::{
    Motion,
    authoring::{self, node, scene::assets},
    evaluation::{Evaluator, Value},
    fields::{Budget, Datum, Sample},
    graph::{Input, Link},
    nodes::color_ramp::{Interpolation, Settings, sample, stop},
};
fn field(m: &Motion, id: ObjectId, time: Time, s: Sample) -> Datum {
    let Value::Field(f) = Evaluator::new(m, time)
        .unwrap()
        .resolve(&Link {
            node: id,
            socket: "value".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    f.sample(&s, &mut Budget::default()).unwrap()
}
#[test]
fn ramp_maps_multiple_stops_and_opacity_with_explicit_boundary_rules() {
    let s = vec![
        stop(0., [2., 0., 0., 0.]),
        stop(0.5, [0., 1., 0., 0.5]),
        stop(1., [0., 0., 1., 1.]),
    ];
    Settings {
        stops: s.clone(),
        interpolation: Interpolation::Linear,
    }
    .validate()
    .unwrap();
    assert_eq!(
        sample(&s, Interpolation::Linear, 0.25).unwrap(),
        Datum::Color([1., 0.5, 0., 0.25])
    );
    assert_eq!(
        sample(&s, Interpolation::Stepped, 0.49).unwrap(),
        s[0].value
    );
    assert_eq!(sample(&s, Interpolation::Stepped, 0.5).unwrap(), s[1].value);
    assert_eq!(
        sample(&s, Interpolation::Smooth, 0.125).unwrap(),
        Datum::Color([1.6875, 0.15625, 0., 0.078125])
    );
    assert_eq!(sample(&s, Interpolation::Linear, -20.).unwrap(), s[0].value);
    assert_eq!(sample(&s, Interpolation::Linear, 20.).unwrap(), s[2].value);
    assert!(sample(&s, Interpolation::Linear, f64::NAN).is_err());
    assert!(
        Settings {
            stops: vec![stop(0., [0.; 4]), stop(0., [1.; 4])],
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        Settings {
            stops: vec![stop(0., [0., 0., 0., 2.])],
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}
#[test]
fn copy_progress_is_a_separate_field_and_ramp_accepts_an_oscillator() {
    let mut m = Motion::new_scene();
    let target = authoring::add_node(&mut m, "fold.motion.fill").unwrap();
    // Supply content so validation tests the full graph, not an incomplete node.
    let shape = authoring::add_node(&mut m, "fold.motion.rectangle").unwrap();
    node(&mut m, target)
        .unwrap()
        .connect("content", shape, "content");
    let ramp = assets::attach_color_ramp(&mut m, target, "color").unwrap();
    assets::set_color_ramp(
        &mut m,
        ramp,
        Settings {
            stops: vec![stop(0., [1., 0., 0., 1.]), stop(1., [0., 0., 1., 0.])],
            ..Default::default()
        },
    )
    .unwrap();
    let progress = authoring::add_node(&mut m, "fold.motion.copy_progress").unwrap();
    node(&mut m, ramp)
        .unwrap()
        .connect("value", progress, "value");
    assert_eq!(
        field(
            &m,
            ramp,
            Time::ZERO,
            Sample {
                index: 1,
                count: 3,
                ..Default::default()
            }
        ),
        Datum::Color([0.5, 0., 0.5, 0.5])
    );
    assert_eq!(
        field(
            &m,
            progress,
            Time::ZERO,
            Sample {
                count: 1,
                ..Default::default()
            }
        ),
        Datum::Scalar(0.)
    );
    node(&mut m, ramp).unwrap().set("value", Datum::Scalar(0.));
    let oscillator = assets::attach_oscillator(&mut m, ramp, "value").unwrap();
    assert_eq!(
        m.graph.node(oscillator).unwrap().inputs["minimum"],
        Input::Value(Datum::Scalar(0.))
    );
    assert_eq!(
        m.graph.node(oscillator).unwrap().inputs["maximum"],
        Input::Value(Datum::Scalar(1.))
    );
    assert_eq!(
        field(&m, ramp, Time::new(1, 2).unwrap(), Sample::default()),
        Datum::Color([0., 0., 1., 0.])
    );
    let m: Motion = serde_json::from_value(serde_json::to_value(&m).unwrap()).unwrap();
    assert_eq!(
        field(&m, ramp, Time::new(3, 2).unwrap(), Sample::default()),
        Datum::Color([1., 0., 0., 1.])
    );
}
#[test]
fn duplicator_convenience_wires_progress_and_preserves_existing_connections() {
    let mut m = Motion::new_scene();
    let source = assets::badge(&mut m).unwrap();
    let copies = assets::duplicate(&mut m, source).unwrap();
    let ramp = assets::attach_color_ramp(&mut m, copies, "color").unwrap();
    let Input::Link(progress) = &m.graph.node(ramp).unwrap().inputs["value"] else {
        panic!()
    };
    assert_eq!(
        m.graph.node(progress.node).unwrap().kind,
        "fold.motion.copy_progress"
    );
    assert_eq!(
        m.graph.node(copies).unwrap().inputs["color_enabled"],
        Input::Value(Datum::Bool(true))
    );
    let before = serde_json::to_value(&m).unwrap();
    assert_eq!(
        assets::attach_color_ramp(&mut m, copies, "color").unwrap(),
        ramp
    );
    assert_eq!(serde_json::to_value(&m).unwrap(), before);
}
