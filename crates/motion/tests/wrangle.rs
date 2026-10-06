use fold_foundation::Time;
use fold_motion::{
    Motion,
    evaluation::{Evaluator, Value},
    fields::Datum,
    graph::Node,
    wrangle,
};
fn scene(source: &str) -> (Motion, fold_foundation::ObjectId) {
    let mut m = Motion::empty();
    let mut points = Node::new("fold.motion.linear_points").unwrap();
    points.set("count", Datum::Scalar(3.));
    points.set("spacing", Datum::Vector([10., 0.]));
    let mut code = Node::new("fold.motion.wrangle").unwrap();
    code.settings["source"] = source.into();
    wrangle::sync_controls(&mut code).unwrap();
    code.connect("content", points.id, "points");
    let id = code.id;
    m.graph.nodes.extend([points, code]);
    (m, id)
}
fn positions(m: &Motion, id: fold_foundation::ObjectId, time: Time) -> Vec<[f64; 2]> {
    let mut e = Evaluator::new(m, time).unwrap();
    let v = e
        .resolve(&fold_motion::graph::Link {
            node: id,
            socket: "content".into(),
        })
        .unwrap();
    let Value::Points(p) = v else { panic!() };
    p.iter().map(|v| [v.transform[4], v.transform[5]]).collect()
}
#[test]
fn real_rust_hot_compile_channels_time_and_reload() {
    let (mut m, id) = scene("position[1] = chf(\"height\") + index as f64 + time.seconds;");
    m.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .set("height", Datum::Scalar(20.));
    let a = positions(&m, id, Time::new(1, 2).unwrap());
    assert_eq!(a[2][1], 22.5);
    let saved = m.document(Default::default()).unwrap();
    let m2 = Motion::from_document(&saved).unwrap();
    assert_eq!(a, positions(&m2, id, Time::new(1, 2).unwrap()));
    let n = m.graph.nodes.iter_mut().find(|n| n.id == id).unwrap();
    n.settings["source"] = "position[1] = chf(\"height\") * 2.0;".into();
    wrangle::sync_controls(n).unwrap();
    assert_eq!(positions(&m, id, Time::ZERO)[0][1], 40.);
}
#[test]
fn discovery_preserves_channels_and_rejects_compiler_host_access() {
    let mut n = Node::new("fold.motion.wrangle").unwrap();
    n.set("amplitude", Datum::Scalar(9.));
    n.settings["source"] = "position[0] += chf(\"other\");".into();
    wrangle::sync_controls(&mut n).unwrap();
    assert_eq!(
        n.inputs["amplitude"],
        fold_motion::graph::Input::Value(Datum::Scalar(9.))
    );
    for source in [
        "let x = include_str!(\"/etc/passwd\");",
        "mod steal;",
        "#[path=\"x\"] mod steal;",
        "let name=\"dynamic\"; chf(name);",
        "chf(\"x\"); chv(\"x\");",
    ] {
        assert!(wrangle::source::discover(source).is_err(), "{source}");
    }
    assert!(
        wrangle::source::discover(&"(".repeat(129))
            .unwrap_err()
            .contains("budget")
    );
}
#[test]
fn infinite_loop_is_stopped_by_fuel() {
    let (m, id) = scene("loop {position[0] += 1.0;}");
    let mut e = Evaluator::new(&m, Time::ZERO).unwrap();
    let err = e
        .resolve(&fold_motion::graph::Link {
            node: id,
            socket: "content".into(),
        })
        .unwrap_err();
    assert!(err.contains("fuel"), "{err}");
}

#[test]
fn attributes_survive_sequential_wrangles_and_exact_ids() {
    let (mut m, id) =
        scene("set_attrv(\"velocity\", [3.0,4.0]); position[1] = (id == 0) as u8 as f64;");
    let mut next = Node::new("fold.motion.wrangle").unwrap();
    next.settings["source"] = "position[0] = attrv(\"velocity\")[0];".into();
    wrangle::sync_controls(&mut next).unwrap();
    next.connect("content", id, "content");
    let target = next.id;
    m.graph.nodes.push(next);
    assert_eq!(positions(&m, target, Time::ZERO)[0][0], 3.);
}
#[test]
fn compile_errors_are_structured_and_cancelled_work_does_not_run() {
    let err = wrangle::compiler::compile("position[0] = unknown_value;", &[], &Default::default())
        .err()
        .unwrap();
    let error: serde_json::Value = serde_json::from_str(&err).unwrap();
    assert_eq!(error["kind"], "rust_compile");
    assert_eq!(error["diagnostics"][0]["spans"][0]["line"], 1);
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(wrangle::compiler::compile("position[0] = 99.;", &[], &cancel).is_err());
}

#[test]
fn animated_channels_and_out_of_order_requests_use_exact_time() {
    let (mut m, id) =
        scene("position[1] = chf(\"height\") + time.numerator as f64 / time.denominator as f64;");
    let node = m.graph.nodes.iter_mut().find(|n| n.id == id).unwrap();
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 10.);
    node.animation.insert("height.0".into(), curve);
    for (n, d, want) in [(1, 1, 11.), (0, 1, 0.), (1, 2, 5.5), (1, 1, 11.)] {
        assert!((positions(&m, id, Time::new(n, d).unwrap())[0][1] - want).abs() < 1e-9);
    }
}

#[test]
fn wrangle_and_native_geometry_produce_identical_render_plans() {
    let mut base = Motion::empty();
    base.info.width = 64;
    base.info.height = 64;
    let shape = fold_motion::authoring::add_content(&mut base, "fold.motion.rectangle").unwrap();
    let mut scripted = base.clone();
    let mut node = Node::new("fold.motion.wrangle").unwrap();
    node.settings["geometry_kind"] = "Content".into();
    node.settings["domain"] = "Children".into();
    node.settings["source"] = "position[0] += 5.0;".into();
    let id = node.id;
    scripted.graph.nodes.push(node);
    fold_motion::authoring::insert_after(&mut scripted, shape, id, "content").unwrap();
    let transform = fold_motion::authoring::add_node(&mut base, "fold.motion.transform").unwrap();
    fold_motion::authoring::node(&mut base, transform)
        .unwrap()
        .set("translation", Datum::Vector([5., 0.]));
    fold_motion::authoring::insert_after(&mut base, shape, transform, "content").unwrap();
    let expected =
        fold_render::render(fold_motion::evaluation::compile(&base, Time::ZERO, 64, 64).unwrap())
            .unwrap();
    let actual = fold_render::render(
        fold_motion::evaluation::compile(&scripted, Time::ZERO, 64, 64).unwrap(),
    )
    .unwrap();
    assert_eq!(actual.pixels(), expected.pixels());
}

#[test]
fn channel_fields_preserve_delayed_sampling() {
    let (mut m, id) = scene("position[1] = chf(\"height\");");
    let mut value = Node::new("fold.motion.value").unwrap();
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 10.);
    value.animation.insert("value.0".into(), curve);
    let mut stagger = Node::new("fold.motion.stagger").unwrap();
    stagger.set("delay", Datum::Scalar(0.25));
    stagger.connect("value", value.id, "value");
    m.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .connect("height", stagger.id, "value");
    m.graph.nodes.extend([value, stagger]);
    let result = positions(&m, id, Time::new(1, 1).unwrap());
    assert_eq!(
        result.iter().map(|p| p[1]).collect::<Vec<_>>(),
        [10., 7.5, 5.]
    );
}

#[test]
#[ignore = "manual cold/warm runtime comparison; run release with --nocapture"]
fn compare_native_and_wrangle_translation() {
    let (mut scripted, code) = scene("position[1] += 7.25;");
    let points = scripted
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.linear_points")
        .unwrap();
    points.set("count", Datum::Scalar(1000.));
    let points_id = points.id;
    let mut native = scripted.clone();
    native.graph.nodes.retain(|n| n.id != code);
    let mut node = Node::new("fold.motion.set_position").unwrap();
    node.settings["domain"] = "Points".into();
    node.set("value", Datum::Vector([0., 7.25]));
    node.connect("content", points_id, "points");
    let native_id = node.id;
    native.graph.nodes.push(node);
    let start = std::time::Instant::now();
    let actual = positions(&scripted, code, Time::ZERO);
    let cold = start.elapsed();
    assert_eq!(actual, positions(&native, native_id, Time::ZERO));
    let run = |m: &Motion, id| {
        let start = std::time::Instant::now();
        for frame in 0..30 {
            std::hint::black_box(positions(m, id, Time::new(frame, 24).unwrap()));
        }
        start.elapsed().as_secs_f64() * 1000. / 30.
    };
    let warm = run(&scripted, code);
    println!(
        "1000 points: cold Wrangle {:.2} ms; warm Wrangle {:.2} ms/frame; native {:.2} ms/frame",
        cold.as_secs_f64() * 1000.,
        warm,
        run(&native, native_id)
    );
    if let Ok(limit) = std::env::var("FOLD_WRANGLE_MAX_MS") {
        assert!(
            warm <= limit.parse::<f64>().unwrap(),
            "warm Wrangle exceeded budget: {warm:.3} ms"
        );
    }
}

#[test]
fn guest_memory_and_heap_are_fresh_for_every_element_and_request() {
    let (m, id) = scene(
        "position[0] = core::arch::wasm32::memory_size::<0>() as f64; core::arch::wasm32::memory_grow::<0>(1); let values = Vec::<f64>::with_capacity(128); position[1] = values.as_ptr() as usize as f64; std::mem::forget(values);",
    );
    let first = positions(&m, id, Time::ZERO);
    assert!(first.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(first, positions(&m, id, Time::new(1, 2).unwrap()));
    assert_eq!(first, positions(&m, id, Time::ZERO));
}

#[test]
fn reused_numeric_guest_resets_attribute_flags_and_values() {
    let (m, id) = scene(
        "if index == 0 { set_attrf(\"only_first\", 12.); } position[1] = attrf(\"only_first\");",
    );
    let mut evaluator = Evaluator::new(&m, Time::ZERO).unwrap();
    let Value::Points(points) = evaluator
        .resolve(&fold_motion::graph::Link {
            node: id,
            socket: "content".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(points[0].attributes["only_first"], Datum::Scalar(12.));
    assert_eq!(points[0].transform[5], 12.);
    for point in &points[1..] {
        assert!(!point.attributes.contains_key("only_first"));
        assert_eq!(point.transform[5], 0.);
    }
}
