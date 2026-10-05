use fold_foundation::{ObjectId, Time};
use fold_motion::{
    Motion, authoring as a,
    evaluation::{Evaluator, Value},
    fields::Datum,
    geometry::Content,
    graph::{Input, Link},
    simulation::Runtime,
};
use std::sync::Arc;
fn fixture() -> (Motion, ObjectId, ObjectId) {
    let mut m = Motion::empty();
    let shape = a::scene::create_object(&mut m, "fold.motion.rectangle").unwrap();
    let tool = a::particles::from_object(&mut m, shape).unwrap();
    a::node(&mut m, tool)
        .unwrap()
        .set("variation", Datum::Scalar(0.));
    let Input::Link(points) = &m.graph.node(tool).unwrap().inputs["points"] else {
        panic!()
    };
    let points = points.node;
    a::node(&mut m, points)
        .unwrap()
        .set("count", Datum::Scalar(40.));
    (m, tool, shape)
}
fn result(m: &Motion, tool: ObjectId, frame: i64, runtime: Arc<Runtime>) -> Content {
    let mut e = Evaluator::with_runtime(m, Time::new(frame, 25).unwrap(), runtime).unwrap();
    let Value::Content(c) = e
        .resolve(&Link {
            node: tool,
            socket: "content".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    c
}
#[test]
fn particles_seek_roundtrip_edit_and_settle() {
    let (mut m, tool, shape) = fixture();
    let runtime = Arc::new(Runtime::default());
    for frame in [200, 0, 25, 125, 10, 200] {
        let warm = result(&m, tool, frame, runtime.clone());
        let cold = result(&m, tool, frame, Arc::new(Runtime::new(0)));
        assert_eq!(
            fold_motion::geometry::drawings(&warm, [1., 1.]).unwrap(),
            fold_motion::geometry::drawings(&cold, [1., 1.]).unwrap()
        );
    }
    let before = result(&m, tool, 200, runtime.clone());
    let initial = result(&m, tool, 0, runtime.clone());
    for (a, b) in initial.iter().zip(before.iter()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.fill, b.fill);
        let target = b.attributes["particle.target"].vector().unwrap();
        assert!((target[0] - b.transform[4]).hypot(target[1] - b.transform[5]) < 0.01);
        assert!(
            (b.transform[4] - a.transform[4]).abs() + (b.transform[5] - a.transform[5]).abs() > 1.
        );
    }
    a::node(&mut m, shape)
        .unwrap()
        .set("width", Datum::Scalar(300.));
    let edited = result(&m, tool, 200, runtime.clone());
    assert_ne!(before[0].transform, edited[0].transform);
    let saved = m.document(Default::default()).unwrap();
    let loaded = Motion::from_document(&saved).unwrap();
    assert_eq!(
        fold_motion::geometry::drawings(&edited, [1., 1.]).unwrap(),
        fold_motion::geometry::drawings(&result(&loaded, tool, 200, runtime.clone()), [1., 1.])
            .unwrap()
    );
    a::node(&mut m, tool)
        .unwrap()
        .animation
        .get_mut("attraction.0")
        .unwrap()
        .keys
        .iter_mut()
        .for_each(|k| k.value = 0.);
    assert_eq!(
        result(&m, tool, 200, runtime)[0].transform,
        initial[0].transform
    );
}
#[test]
fn alternate_distribution_palette_and_empty_points() {
    let (mut m, tool, _) = fixture();
    let grid = a::add_node(&mut m, "fold.motion.grid_points").unwrap();
    a::node(&mut m, tool)
        .unwrap()
        .connect("points", grid, "points");
    a::node(&mut m, tool)
        .unwrap()
        .set("variation", Datum::Scalar(0.75));
    let first = result(&m, tool, 80, Arc::new(Runtime::default()));
    assert_eq!(first.len(), 16);
    let Input::Link(palette) = &m.graph.node(tool).unwrap().inputs["color"] else {
        panic!()
    };
    let palette = palette.node;
    a::node(&mut m, palette)
        .unwrap()
        .set("seed", Datum::Scalar(42.));
    let second = result(&m, tool, 80, Arc::new(Runtime::default()));
    let first_colors = fold_motion::geometry::drawings(&first, [1., 1.]).unwrap();
    let second_colors = fold_motion::geometry::drawings(&second, [1., 1.]).unwrap();
    assert!(
        first_colors
            .iter()
            .zip(&second_colors)
            .any(|(a, b)| a.fill != b.fill)
    );
    assert!(
        first
            .iter()
            .zip(&*second)
            .all(|(a, b)| a.transform == b.transform && a.id == b.id)
    );
    a::node(&mut m, grid)
        .unwrap()
        .set("columns", Datum::Scalar(0.));
    assert!(result(&m, tool, 80, Arc::new(Runtime::default())).is_empty());
}
#[test]
fn target_holes_transform_and_identity_are_preserved() {
    let mut m = Motion::empty();
    let path = a::add_node(&mut m, "fold.motion.path").unwrap();
    a::node(&mut m, path).unwrap().settings = serde_json::json!({"segments":[
        {"Move":[0,0]},{"Line":[100,0]},{"Line":[100,100]},{"Line":[0,100]},"Close",
        {"Move":[25,25]},{"Line":[25,75]},{"Line":[75,75]},{"Line":[75,25]},"Close"]});
    let transform = a::add_node(&mut m, "fold.motion.transform").unwrap();
    a::node(&mut m, transform)
        .unwrap()
        .connect("content", path, "content");
    a::node(&mut m, transform)
        .unwrap()
        .set("translation", Datum::Vector([300., 200.]));
    let points = a::add_node(&mut m, "fold.motion.scatter_points").unwrap();
    a::node(&mut m, points)
        .unwrap()
        .set("count", Datum::Scalar(200.));
    let targets = a::add_node(&mut m, "fold.motion.particle_targets").unwrap();
    a::node(&mut m, targets)
        .unwrap()
        .connect("points", points, "points");
    a::node(&mut m, targets)
        .unwrap()
        .connect("attractor", transform, "content");
    let mut e = Evaluator::new(&m, Time::ZERO).unwrap();
    let Value::Points(c) = e
        .resolve(&Link {
            node: targets,
            socket: "points".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    for p in c.iter() {
        let t = p.attributes["particle.target"].vector().unwrap();
        assert!((300. ..=400.).contains(&t[0]) && (200. ..=300.).contains(&t[1]));
        assert!(!((325. ..375.).contains(&t[0]) && (225. ..275.).contains(&t[1])));
    }
}
#[test]
fn cancellation_and_invalid_inputs_are_explicit() {
    let (mut m, tool, shape) = fixture();
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    let mut e = Evaluator::new(&m, Time::new(2, 1).unwrap()).unwrap();
    e.budget = fold_motion::fields::Budget::cancellable(&cancel);
    assert!(
        e.resolve(&Link {
            node: tool,
            socket: "content".into()
        })
        .unwrap_err()
        .contains("cancelled")
    );
    a::node(&mut m, shape)
        .unwrap()
        .set("width", Datum::Scalar(0.));
    let mut e = Evaluator::new(&m, Time::ZERO).unwrap();
    assert!(
        e.resolve(&Link {
            node: tool,
            socket: "content".into()
        })
        .is_err()
    );
    let (mut m, tool, _) = fixture();
    a::node(&mut m, tool)
        .unwrap()
        .set("frequency", Datum::Scalar(-1.));
    let mut e = Evaluator::new(&m, Time::new(2, 1).unwrap()).unwrap();
    assert!(
        e.resolve(&Link {
            node: tool,
            socket: "content".into()
        })
        .unwrap_err()
        .contains("frequency")
    );
}

#[test]
fn scatter_limits_and_strong_springs_stay_bounded() {
    let (mut m, tool, _) = fixture();
    a::node(&mut m, tool)
        .unwrap()
        .set("frequency", Datum::Scalar(100.));
    a::node(&mut m, tool)
        .unwrap()
        .set("damping", Datum::Scalar(0.));
    let c = result(&m, tool, 225, Arc::new(Runtime::new(0)));
    for p in c.iter() {
        let target = p.attributes["particle.target"].vector().unwrap();
        assert!((p.transform[4] - target[0]).hypot(p.transform[5] - target[1]) < 0.01);
    }
    let Input::Link(points) = &m.graph.node(tool).unwrap().inputs["points"] else {
        panic!()
    };
    let points = points.node;
    a::node(&mut m, points)
        .unwrap()
        .set("count", Datum::Scalar(16385.));
    let mut e = Evaluator::new(&m, Time::ZERO).unwrap();
    assert!(
        e.resolve(&Link {
            node: tool,
            socket: "content".into()
        })
        .unwrap_err()
        .contains("count")
    );
}

#[test]
fn particle_scatter_jitter_retains_identity_and_respects_bounds() {
    let mut m = Motion::empty();
    let points = a::add_node(&mut m, "fold.motion.scatter_points").unwrap();
    a::node(&mut m, points)
        .unwrap()
        .set("perimeter", Datum::Bool(true));
    let read = |m: &Motion| {
        let mut e = Evaluator::new(m, Time::ZERO).unwrap();
        let Value::Points(c) = e
            .resolve(&Link {
                node: points,
                socket: "points".into(),
            })
            .unwrap()
        else {
            panic!()
        };
        c
    };
    let plain = read(&m);
    a::node(&mut m, points)
        .unwrap()
        .set("jitter", Datum::Vector([20., 35.]));
    let scattered = read(&m);
    for (a, b) in plain.iter().zip(scattered.iter()) {
        assert_eq!(a.id, b.id);
        assert!((a.transform[4] - b.transform[4]).abs() <= 20.);
        assert!((a.transform[5] - b.transform[5]).abs() <= 35.);
    }
    assert!(
        plain
            .iter()
            .zip(scattered.iter())
            .any(|(a, b)| a.transform != b.transform)
    );
}
