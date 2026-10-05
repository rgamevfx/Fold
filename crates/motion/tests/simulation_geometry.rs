use fold_foundation::Time;
use fold_motion::{
    Motion, authoring as a,
    evaluation::{Evaluator, Value},
    fields::Datum,
    geometry::Geometry,
    graph::Link,
};
fn evaluate(m: &Motion, id: fold_foundation::ObjectId) -> fold_motion::geometry::Content {
    let Value::Content(c) = Evaluator::new(m, Time::ZERO)
        .unwrap()
        .resolve(&Link {
            node: id,
            socket: "content".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    c
}
#[test]
fn grid_preserves_holes_and_can_return_original_curves() {
    let mut m = Motion::empty();
    let path = a::add_node(&mut m, "fold.motion.path").unwrap();
    a::node(&mut m, path).unwrap().settings = serde_json::json!({"segments":[
        {"Move":[0.,0.]},{"Line":[20.,0.]},{"Line":[20.,20.]},{"Line":[0.,20.]},"Close",
        {"Move":[5.,5.]},{"Line":[5.,15.]},{"Line":[15.,15.]},{"Line":[15.,5.]},"Close"
    ],"attributes":{}});
    let grid = a::add_node(&mut m, "fold.motion.shape_grid").unwrap();
    a::node(&mut m, grid)
        .unwrap()
        .connect("content", path, "content");
    a::node(&mut m, grid)
        .unwrap()
        .set("cell_size", Datum::Scalar(5.));
    let c = evaluate(&m, grid);
    let Geometry::Path(p) = &c[0].geometry else {
        panic!()
    };
    assert_eq!(
        p.iter()
            .filter(|s| matches!(s, fold_render::vector::Segment::Close))
            .count(),
        12
    );
    a::node(&mut m, grid)
        .unwrap()
        .set("cell_size", Datum::Scalar(0.));
    assert_eq!(
        fold_motion::geometry::drawings(&evaluate(&m, grid), [1., 1.]).unwrap(),
        fold_motion::geometry::drawings(&evaluate(&m, path), [1., 1.]).unwrap()
    );
}
#[test]
fn diagonal_convex_sweep_has_six_corners_and_retains_opacity() {
    let mut m = Motion::empty();
    let rect = a::add_node(&mut m, "fold.motion.rectangle").unwrap();
    a::node(&mut m, rect)
        .unwrap()
        .set("width", Datum::Scalar(10.));
    a::node(&mut m, rect)
        .unwrap()
        .set("height", Datum::Scalar(10.));
    let alpha = a::add_node(&mut m, "fold.motion.set_opacity").unwrap();
    a::node(&mut m, alpha).unwrap().settings =
        serde_json::json!({"domain":"Children","combine":"Replace"});
    a::node(&mut m, alpha)
        .unwrap()
        .connect("content", rect, "content");
    a::node(&mut m, alpha)
        .unwrap()
        .set("value", Datum::Scalar(0.3));
    let sweep = a::add_node(&mut m, "fold.motion.convex_sweep").unwrap();
    a::node(&mut m, sweep)
        .unwrap()
        .connect("content", alpha, "content");
    a::node(&mut m, sweep)
        .unwrap()
        .set("offset", Datum::Vector([10., 10.]));
    let c = evaluate(&m, sweep);
    let Geometry::Path(p) = &c[0].geometry else {
        panic!()
    };
    assert_eq!(p.len(), 7);
    let d = fold_motion::geometry::drawings(&c, [1., 1.]).unwrap();
    assert!((d[0].fill.unwrap()[3] - 0.3).abs() < 1e-12);
}
#[test]
fn geometry_limits_fail_without_unbounded_allocation() {
    let mut m = Motion::empty();
    let rect = a::add_node(&mut m, "fold.motion.rectangle").unwrap();
    a::node(&mut m, rect)
        .unwrap()
        .set("width", Datum::Scalar(10000.));
    a::node(&mut m, rect)
        .unwrap()
        .set("height", Datum::Scalar(10000.));
    let grid = a::add_node(&mut m, "fold.motion.shape_grid").unwrap();
    a::node(&mut m, grid)
        .unwrap()
        .connect("content", rect, "content");
    a::node(&mut m, grid)
        .unwrap()
        .set("cell_size", Datum::Scalar(2.));
    let error = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .resolve(&Link {
            node: grid,
            socket: "content".into(),
        })
        .unwrap_err();
    assert!(error.contains("candidate cells"));
}
