use fold_foundation::Time;
use fold_motion::{
    Motion, authoring as a,
    evaluation::{Evaluator, Value},
    fields::{Datum, Kind},
    graph::{Link, Node},
    nodes::interface::Port,
    simulation::Runtime,
};
use std::sync::Arc;
fn fixture() -> (Motion, fold_foundation::ObjectId) {
    let mut m = Motion::empty();
    let source = a::add_node(&mut m, "fold.motion.rectangle").unwrap();
    let solver = a::solver::create(&mut m, Kind::Content).unwrap();
    a::node(&mut m, solver)
        .unwrap()
        .connect("initial", source, "content");
    let group = a::node(&mut m, solver)
        .unwrap()
        .settings::<fold_motion::nodes::solver::Settings>()
        .unwrap()
        .group
        .unwrap();
    let g = m.groups.get_mut(&group).unwrap();
    g.inputs.push(Port {
        id: "speed".into(),
        name: "Speed".into(),
        kind: Kind::Vector,
        default: Some(Datum::Vector([25., 0.])),
        unit: "px".into(),
        section: String::new(),
        advanced: false,
    });
    let prev = g.graph.nodes[0].id;
    let mut input = Node::new("fold.motion.group_input").unwrap();
    input.settings = serde_json::json!({"key":"speed","value_type":"Vector"});
    let mut dt = Node::new("fold.motion.combine_xy").unwrap();
    dt.connect("x", prev, "delta_time");
    dt.connect("y", prev, "delta_time");
    let mut multiply = Node::new("fold.motion.math").unwrap();
    multiply.settings = serde_json::json!({"operation":"Multiply","value_type":"Vector"});
    multiply.connect("a", input.id, "value");
    multiply.connect("b", dt.id, "value");
    let mut move_node = Node::new("fold.motion.set_position").unwrap();
    move_node.settings = serde_json::json!({"domain":"Children","combine":"Add"});
    move_node.connect("content", prev, "state");
    move_node.connect("value", multiply.id, "value");
    g.graph.nodes[1].connect("value", move_node.id, "content");
    g.graph.nodes.extend([input, dt, multiply, move_node]);
    (m, solver)
}
fn content(
    m: &Motion,
    node: fold_foundation::ObjectId,
    time: Time,
    runtime: Arc<Runtime>,
) -> fold_motion::geometry::Content {
    let mut e = Evaluator::with_runtime(m, time, runtime).unwrap();
    let Value::Content(c) = e
        .resolve(&Link {
            node,
            socket: "content".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    c
}
#[test]
fn seeks_eviction_and_animated_live_inputs_are_deterministic() {
    let (mut m, solver) = fixture();
    let runtime = Arc::new(Runtime::default());
    for frame in [50, 0, 12, 49, 5, 50, 26] {
        let t = Time::new(frame, 25).unwrap();
        let warm = content(&m, solver, t, runtime.clone());
        let cold = content(&m, solver, t, Arc::new(Runtime::new(0)));
        assert_eq!(warm[0].transform, cold[0].transform);
        assert!((warm[0].transform[4] - frame as f64).abs() < 1e-9);
    }
    assert!(runtime.retained_bytes() > 0);
    runtime.clear();
    assert_eq!(runtime.retained_bytes(), 0);
    assert_eq!(
        content(&m, solver, Time::new(-1, 1).unwrap(), runtime.clone())[0].transform[4],
        0.
    );
    assert_eq!(
        content(&m, solver, Time::new(3, 100).unwrap(), runtime.clone())[0].transform[4],
        0.
    );
    a::node(&mut m, solver)
        .unwrap()
        .set("speed", Datum::Vector([50., 0.]));
    assert!(
        (content(&m, solver, Time::new(1, 1).unwrap(), runtime.clone())[0].transform[4] - 50.)
            .abs()
            < 1e-9
    );
    let time = a::add_node(&mut m, "fold.motion.time").unwrap();
    let vector = a::add_node(&mut m, "fold.motion.combine_xy").unwrap();
    a::node(&mut m, vector).unwrap().connect("x", time, "value");
    a::node(&mut m, solver)
        .unwrap()
        .connect("speed", vector, "value");
    // Right-endpoint integration at .04, .08, ... 1.0 seconds.
    assert!(
        (content(&m, solver, Time::new(1, 1).unwrap(), runtime)[0].transform[4] - 0.52).abs()
            < 1e-9
    );
}
#[test]
fn shared_update_groups_have_independent_state_and_roundtrip() {
    let (mut m, first) = fixture();
    let mut second = m.graph.node(first).unwrap().clone();
    second.id = Default::default();
    second.set("speed", Datum::Vector([-25., 0.]));
    let second_id = second.id;
    m.graph.nodes.push(second);
    let m = Motion::from_document(&m.document(Default::default()).unwrap()).unwrap();
    let runtime = Arc::new(Runtime::new(20000));
    let t = Time::new(1, 1).unwrap();
    assert!((content(&m, first, t, runtime.clone())[0].transform[4] - 25.).abs() < 1e-9);
    assert!((content(&m, second_id, t, runtime.clone())[0].transform[4] + 25.).abs() < 1e-9);
    assert!(runtime.retained_bytes() <= 20000);
}
#[test]
fn history_samples_solver_without_advancing_its_clock() {
    let (mut m, solver) = fixture();
    let history = a::add_node(&mut m, "fold.motion.history").unwrap();
    let n = a::node(&mut m, history).unwrap();
    n.connect("content", solver, "content");
    n.set("samples", Datum::Scalar(3.));
    let result = content(
        &m,
        history,
        Time::new(2, 25).unwrap(),
        Arc::new(Runtime::default()),
    );
    assert_eq!(result.len(), 3);
    for (i, item) in result.iter().enumerate() {
        let fold_motion::geometry::Geometry::Group(c) = &item.geometry else {
            panic!()
        };
        assert!((c[0].transform[4] - i as f64).abs() < 1e-9);
    }
}
#[test]
fn invalid_step_and_cancelled_replay_fail_explicitly() {
    let (mut m, solver) = fixture();
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    let mut e = Evaluator::new(&m, Time::new(2, 1).unwrap()).unwrap();
    e.budget = fold_motion::fields::Budget::cancellable(&cancel);
    assert!(
        e.resolve(&Link {
            node: solver,
            socket: "content".into()
        })
        .unwrap_err()
        .contains("cancelled")
    );
    a::node(&mut m, solver).unwrap().settings["step"] = serde_json::to_value(Time::ZERO).unwrap();
    assert!(m.validate().unwrap_err().contains("step"));
}

#[test]
fn solver_inside_editable_group_rebinds_live_animation_at_each_step() {
    let (mut m, solver) = fixture();
    let time = a::add_node(&mut m, "fold.motion.time").unwrap();
    let vector = a::add_node(&mut m, "fold.motion.combine_xy").unwrap();
    a::node(&mut m, vector).unwrap().connect("x", time, "value");
    a::node(&mut m, solver)
        .unwrap()
        .connect("speed", vector, "value");
    a::group_node(&mut m, solver).unwrap();
    let runtime = Arc::new(Runtime::default());
    for frame in [25, 5, 50, 25] {
        let expected = (1..=frame).map(|i| i as f64 / 625.).sum::<f64>();
        assert!(
            (content(&m, solver, Time::new(frame, 25).unwrap(), runtime.clone())[0].transform[4]
                - expected)
                .abs()
                < 1e-9
        );
    }
}

#[test]
fn same_solver_supports_persistent_decaying_attributes_without_motion() {
    let (mut m, solver) = fixture();
    let group = m
        .graph
        .node(solver)
        .unwrap()
        .settings::<fold_motion::nodes::solver::Settings>()
        .unwrap()
        .group
        .unwrap();
    let g = m.groups.get_mut(&group).unwrap();
    let previous = g.graph.nodes[0].id;
    g.graph.nodes.truncate(2);
    g.inputs.clear();
    g.inputs.push(Port {
        id: "influence".into(),
        name: "Influence".into(),
        kind: Kind::Scalar,
        default: Some(Datum::Scalar(1.)),
        unit: String::new(),
        section: String::new(),
        advanced: false,
    });
    let mut read = Node::new("fold.motion.read_attribute").unwrap();
    read.connect("content", previous, "state");
    read.set("name", Datum::Text("heat".into()));
    let mut decay = Node::new("fold.motion.math").unwrap();
    decay.settings = serde_json::json!({"operation":"Multiply"});
    decay.connect("a", read.id, "value");
    decay.set("b", Datum::Scalar(0.5));
    let mut input = Node::new("fold.motion.group_input").unwrap();
    input.settings = serde_json::json!({"key":"influence","value_type":"Scalar"});
    let mut add = Node::new("fold.motion.math").unwrap();
    add.connect("a", decay.id, "value");
    add.connect("b", input.id, "value");
    let mut store = Node::new("fold.motion.store_attribute").unwrap();
    store.connect("content", previous, "state");
    store.connect("value", add.id, "value");
    store.set("name", Datum::Text("heat".into()));
    g.graph.nodes[1].connect("value", store.id, "content");
    g.graph.nodes.extend([read, decay, input, add, store]);
    let runtime = Arc::new(Runtime::default());
    let state = content(&m, solver, Time::new(3, 25).unwrap(), runtime.clone());
    assert_eq!(state[0].attributes["heat"], Datum::Scalar(1.75));
    assert_eq!(state[0].transform[4], 0.);
    let track = a::add_node(&mut m, "fold.motion.keyframes").unwrap();
    a::node(&mut m, track).unwrap().settings = serde_json::json!({"channels":[],"keys":[
        {"time":Time::ZERO,"value":{"Scalar":1.},"interpolation":"Hold"},
        {"time":Time::new(2,25).unwrap(),"value":{"Scalar":0.},"interpolation":"Hold"}
    ]});
    a::node(&mut m, solver)
        .unwrap()
        .connect("influence", track, "value");
    assert_eq!(
        content(&m, solver, Time::new(3, 25).unwrap(), runtime)[0].attributes["heat"],
        Datum::Scalar(0.25)
    );
}

#[test]
fn drift_preset_is_editable_bounded_and_works_in_arbitrary_frame_order() {
    let mut m = Motion::empty();
    let source = a::add_node(&mut m, "fold.motion.rectangle").unwrap();
    a::node(&mut m, source)
        .unwrap()
        .set("width", Datum::Scalar(8.));
    a::node(&mut m, source)
        .unwrap()
        .set("height", Datum::Scalar(8.));
    let drift = a::drift::create(&mut m).unwrap();
    a::node(&mut m, drift)
        .unwrap()
        .connect("source", source, "content");
    a::node(&mut m, drift)
        .unwrap()
        .set("count", Datum::Scalar(4.));
    let runtime = Arc::new(Runtime::default());
    for frame in [75, 2, 25, 0] {
        let time = Time::new(frame, 25).unwrap();
        let warm = content(&m, drift, time, runtime.clone());
        let cold = content(&m, drift, time, Arc::new(Runtime::new(0)));
        assert_eq!(
            fold_motion::geometry::drawings(&warm, [1., 1.]).unwrap(),
            fold_motion::geometry::drawings(&cold, [1., 1.]).unwrap()
        );
    }
    let project = Motion::from_document(&m.document(Default::default()).unwrap()).unwrap();
    assert!(
        project.groups.values().any(|g| g
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == "fold.motion.solver"))
    );
}

#[test]
fn point_state_fractional_steps_and_retyping_are_explicit() {
    let mut m = Motion::empty();
    let solver = a::add_node(&mut m, "fold.motion.solver").unwrap();
    a::solver::set_state_kind(&mut m, solver, Kind::Points).unwrap();
    let points = a::add_node(&mut m, "fold.motion.linear_points").unwrap();
    a::node(&mut m, solver)
        .unwrap()
        .connect("initial", points, "points");
    a::node(&mut m, solver).unwrap().settings["step"] =
        serde_json::to_value(Time::new(1001, 24000).unwrap()).unwrap();
    let runtime = Arc::new(Runtime::default());
    for t in [
        Time::ZERO,
        Time::new(1001, 24000).unwrap(),
        Time::new(1001, 12000).unwrap(),
    ] {
        let mut e = Evaluator::with_runtime(&m, t, runtime.clone()).unwrap();
        let Value::Points(c) = e
            .resolve(&Link {
                node: solver,
                socket: "content".into(),
            })
            .unwrap()
        else {
            panic!()
        };
        assert!(
            c.iter()
                .all(|p| matches!(p.geometry, fold_motion::geometry::Geometry::Point))
        );
    }
    let before = m.clone();
    assert!(a::solver::set_state_kind(&mut m, solver, Kind::Content).is_err());
    assert_eq!(m, before);
}

#[test]
fn concurrent_viewers_share_checkpoints_without_sharing_time_or_mutating_snapshots() {
    let (m, solver) = fixture();
    let runtime = Arc::new(Runtime::default());
    let unchanged = m.clone();
    std::thread::scope(|scope| {
        for frames in [[50, 10, 30], [5, 40, 0]] {
            let motion = &m;
            let runtime = runtime.clone();
            scope.spawn(move || {
                for frame in frames {
                    let state = content(
                        motion,
                        solver,
                        Time::new(frame, 25).unwrap(),
                        runtime.clone(),
                    );
                    assert!((state[0].transform[4] - frame as f64).abs() < 1e-9);
                }
            });
        }
    });
    assert_eq!(m, unchanged);
}
