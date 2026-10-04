use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    Motion, authoring as a,
    evaluation::{Evaluator, Value, compile},
    fields::{Datum, Kind},
    geometry::Domain,
    graph::{Input, Link, Node},
};
fn link(id: ObjectId, socket: &str) -> Link {
    Link {
        node: id,
        socket: socket.into(),
    }
}
fn scalar(m: &Motion, id: ObjectId, time: Time) -> f64 {
    let Value::Field(f) = Evaluator::new(m, time)
        .unwrap()
        .resolve(&link(id, "value"))
        .unwrap()
    else {
        panic!("field")
    };
    f.uniform(&mut Default::default())
        .unwrap()
        .scalar()
        .unwrap()
}
fn small() -> Motion {
    let mut m = Motion::empty();
    m.info.width = 128;
    m.info.height = 96;
    m
}
#[test]
fn catalog_has_38_nodes_and_group_instances_with_unique_stable_sockets() {
    let catalog = fold_motion::graph::registry::definitions();
    assert_eq!(catalog.len(), 39);
    let mut ids = std::collections::BTreeSet::new();
    for d in catalog {
        assert!(ids.insert(d.id));
        let n = Node::new(d.id).unwrap();
        (d.validate)(&n).unwrap();
        let mut sockets = std::collections::BTreeSet::new();
        for s in &d.inputs {
            assert!(sockets.insert(&s.id));
            if let Some(default) = &s.default {
                default.validate().unwrap();
            }
        }
    }
}
#[test]
fn native_shapes_render_alpha_at_arbitrary_resolution() {
    let mut m = small();
    let source = a::add_content(&mut m, "fold.motion.rectangle").unwrap();
    a::node(&mut m, source)
        .unwrap()
        .set("width", Datum::Scalar(20.));
    a::node(&mut m, source)
        .unwrap()
        .set("height", Datum::Scalar(20.));
    let result = fold_render::render(compile(&m, Time::ZERO, 128, 96).unwrap()).unwrap();
    assert_eq!(result.pixels()[0], [0.; 4]);
    assert_eq!(result.pixels()[45 * 128 + 40], [1.; 4]);
    let large = fold_render::render(compile(&m, Time::ZERO, 256, 192).unwrap()).unwrap();
    assert_eq!(large.pixels()[90 * 256 + 80], [1.; 4]);
}
#[test]
fn cancellation_interrupts_fields_and_rejects_motion_compilation() {
    let cancel = fold_media::Cancel::default();
    let mut budget = fold_motion::fields::Budget::cancellable(&cancel);
    let field = fold_motion::fields::Field::constant(Datum::Scalar(1.));
    field.uniform(&mut budget).unwrap();
    cancel.cancel();
    assert!(
        field
            .uniform(&mut budget)
            .unwrap_err()
            .contains("cancelled")
    );
    assert!(
        fold_motion::evaluation::compile_with(&small(), Time::ZERO, 128, 96, &cancel)
            .unwrap_err()
            .contains("cancelled")
    );
    assert!(
        fold_motion::geometry::drawings_with(&std::sync::Arc::new(vec![]), [1., 1.], &cancel)
            .unwrap_err()
            .contains("cancelled")
    );
}

#[test]
fn procedural_documents_allow_non_codec_dimensions_and_long_duration() {
    let mut motion = small();
    motion.info.width = 127;
    motion.info.height = 95;
    motion.info.frames = 20_000;
    a::add_content(&mut motion, "fold.motion.rectangle").unwrap();
    motion.validate().unwrap();
    let plan = compile(&motion, motion.info.time(19_999).unwrap(), 127, 95).unwrap();
    let frame = fold_render::render(plan).unwrap();
    assert_eq!(frame.pixels().len(), 127 * 95);
}

#[test]
fn direct_title_and_radial_tools_create_actual_editable_graphs() {
    let mut m = small();
    let text = a::add_content(&mut m, "fold.motion.text").unwrap();
    a::node(&mut m, text)
        .unwrap()
        .set("size", Datum::Scalar(20.));
    let response = a::wave_position(&mut m, text, Domain::Glyphs).unwrap();
    a::radial_influence(&mut m, response).unwrap();
    let times = [0, 5, 19, 42, 80];
    let sequential: Vec<_> = times
        .iter()
        .map(|&t| {
            fold_render::render(compile(&m, Time::new(t, 24).unwrap(), 128, 96).unwrap())
                .unwrap()
                .pixels()
                .to_vec()
        })
        .collect();
    for index in [4, 0, 3, 1, 2, 4] {
        let frame = fold_render::render(
            compile(&m, Time::new(times[index], 24).unwrap(), 128, 96).unwrap(),
        )
        .unwrap();
        assert_eq!(frame.pixels(), sequential[index]);
    }
    assert_ne!(sequential[0], sequential[1]);
    let mut radial = small();
    let shape = a::add_content(&mut radial, "fold.motion.ellipse").unwrap();
    a::node(&mut radial, shape)
        .unwrap()
        .set("width", Datum::Scalar(8.));
    a::node(&mut radial, shape)
        .unwrap()
        .set("height", Datum::Scalar(8.));
    let instances = a::repeat(&mut radial, shape, "fold.motion.radial_points").unwrap();
    let points = radial
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.radial_points")
        .unwrap();
    points.set("radius", Datum::Scalar(20.));
    a::wave_position(&mut radial, instances, Domain::Instances).unwrap();
    assert!(compile(&radial, Time::ZERO, 128, 96).is_ok());
}
#[test]
fn socket_type_errors_and_cycles_reject_atomically() {
    let mut m = small();
    let value = a::add_node(&mut m, "fold.motion.value").unwrap();
    let out = m.graph.output.node;
    let original = m.clone();
    assert!(
        a::connect(&mut m, out, "content", link(value, "value"))
            .unwrap_err()
            .contains("expected Content")
    );
    assert_eq!(m, original);
    let math = a::add_node(&mut m, "fold.motion.math").unwrap();
    assert!(
        a::connect(&mut m, math, "a", link(math, "value"))
            .unwrap_err()
            .contains("cyclic")
    );
}
#[test]
fn unavailable_nodes_survive_roundtrip_and_fail_if_demanded() {
    let mut m = small();
    let mut n = Node::new("fold.motion.value").unwrap();
    n.kind = "other.future.operation".into();
    n.version = 42;
    n.settings = serde_json::json!({"unknown":[1,2,3]});
    n.extensions
        .insert("future".into(), serde_json::json!({"keep":true}));
    m.graph.output = link(n.id, "content");
    m.graph.nodes.push(n);
    let doc = m.document(DocumentId::new()).unwrap();
    let reopened = Motion::from_document(&doc).unwrap();
    assert_eq!(m, reopened);
    assert!(
        compile(&reopened, Time::ZERO, 128, 96)
            .unwrap_err()
            .contains("unavailable")
    );
}
#[test]
fn rational_keys_hold_linear_eased_and_reverse_requests() {
    use fold_motion::animation::{Interpolation, Key, Track};
    let mut m = small();
    let keys = a::add_node(&mut m, "fold.motion.keyframes").unwrap();
    let track = Track {
        channels: vec![],
        keys: vec![
            Key {
                time: Time::new(1001, 24000).unwrap(),
                value: Datum::Scalar(0.),
                interpolation: Interpolation::Linear,
            },
            Key {
                time: Time::new(3003, 24000).unwrap(),
                value: Datum::Scalar(10.),
                interpolation: Interpolation::Hold,
            },
        ],
    };
    a::node(&mut m, keys).unwrap().settings = serde_json::to_value(track).unwrap();
    assert_eq!(scalar(&m, keys, Time::new(3003, 24000).unwrap()), 10.);
    assert!((scalar(&m, keys, Time::new(2002, 24000).unwrap()) - 5.).abs() < 1e-12);
    assert_eq!(scalar(&m, keys, Time::ZERO), 0.);
}
#[test]
fn text_keeps_shaped_clusters_and_missing_fonts_fail_explicitly() {
    let font = fold_motion::document::Font::builtin();
    let content = fold_motion::geometry::text::shape("office", &font, 24., 1.2, 0., 123).unwrap();
    assert!(
        content.len() < 6,
        "font ligature shaping must not be per-character drawing"
    );
    assert!(
        content
            .iter()
            .all(|e| matches!(e.geometry, fold_motion::geometry::Geometry::Glyph { .. }))
    );
    assert!(
        fold_motion::geometry::text::shape("\u{10ffff}", &font, 24., 1.2, 0., 123)
            .unwrap_err()
            .contains("no glyph")
    );
    let mut m = small();
    a::add_content(&mut m, "fold.motion.text").unwrap();
    m.fonts.clear();
    assert!(
        compile(&m, Time::ZERO, 128, 96)
            .unwrap_err()
            .contains("missing embedded font")
    );
}
#[test]
fn numeric_fields_support_vector_math_and_explicit_domain_consumption() {
    let mut m = small();
    let math = a::add_node(&mut m, "fold.motion.math").unwrap();
    let n = a::node(&mut m, math).unwrap();
    n.settings = serde_json::json!({"operation":"Multiply","value_type":"Vector"});
    n.set("a", Datum::Vector([2., 3.]));
    n.set("b", Datum::Vector([4., 5.]));
    let Value::Field(value) = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .resolve(&link(math, "value"))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        value.uniform(&mut Default::default()).unwrap(),
        Datum::Vector([8., 15.])
    );
    let index = a::add_node(&mut m, "fold.motion.index").unwrap();
    let rect = a::add_node(&mut m, "fold.motion.rectangle").unwrap();
    a::connect(&mut m, rect, "width", link(index, "value")).unwrap();
    assert!(
        Evaluator::new(&m, Time::ZERO)
            .unwrap()
            .resolve(&link(rect, "content"))
            .unwrap_err()
            .contains("explicit consuming domain")
    );
}
#[test]
fn excessive_counts_and_nonfinite_fields_fail() {
    let mut m = small();
    let points = a::add_node(&mut m, "fold.motion.line_points").unwrap();
    a::node(&mut m, points)
        .unwrap()
        .set("count", Datum::Scalar(1e9));
    assert!(
        Evaluator::new(&m, Time::ZERO)
            .unwrap()
            .resolve(&link(points, "points"))
            .is_err()
    );
    let math = a::add_node(&mut m, "fold.motion.math").unwrap();
    a::node(&mut m, math).unwrap().settings = serde_json::json!({"operation":"Divide"});
    let Value::Field(f) = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .resolve(&link(math, "value"))
        .unwrap()
    else {
        panic!()
    };
    assert!(f.uniform(&mut Default::default()).is_err());
}
#[test]
fn reusable_groups_bind_values_without_copying_their_construction() {
    use fold_motion::nodes::interface::{GroupDefinition, Port};
    let mut m = small();
    let mut input = Node::new("fold.motion.group_input").unwrap();
    input.settings = serde_json::json!({"key":"amount","value_type":"Scalar"});
    let mut output = Node::new("fold.motion.group_output").unwrap();
    output.connect("value", input.id, "value");
    let out = link(output.id, "value");
    let group = ObjectId::new();
    m.groups.insert(
        group,
        GroupDefinition {
            name: "Identity".into(),
            version: 1,
            inputs: vec![Port {
                id: "amount".into(),
                name: "Amount".into(),
                kind: Kind::Scalar,
                default: Some(Datum::Scalar(0.)),
            }],
            outputs: std::collections::BTreeMap::from([("value".into(), out.clone())]),
            graph: fold_motion::graph::Graph {
                nodes: vec![input, output],
                output: out,
            },
        },
    );
    let instance = a::add_node(&mut m, "fold.motion.group_instance").unwrap();
    let n = a::node(&mut m, instance).unwrap();
    n.settings = serde_json::json!({"group":group});
    n.set("amount", Datum::Scalar(7.));
    assert_eq!(scalar(&m, instance, Time::ZERO), 7.);
    let other = a::add_node(&mut m, "fold.motion.group_instance").unwrap();
    let n = a::node(&mut m, other).unwrap();
    n.settings = serde_json::json!({"group":group});
    n.set("amount", Datum::Scalar(3.));
    assert_eq!(scalar(&m, other, Time::ZERO), 3.);
    assert_eq!(scalar(&m, instance, Time::ZERO), 7.);
    assert!(matches!(
        m.graph.node(instance).unwrap().inputs["amount"],
        Input::Value(_)
    ));
}

#[test]
fn input_channel_animation_is_independent_and_does_not_replace_graph_drivers() {
    let mut m = small();
    let value = a::add_node(&mut m, "fold.motion.value").unwrap();
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 10.);
    a::node(&mut m, value)
        .unwrap()
        .animation
        .insert("value.0".into(), curve);
    for n in [4, 1, 3, 0, 2] {
        assert!((scalar(&m, value, Time::new(n, 4).unwrap()) - n as f64 * 2.5).abs() < 1e-10);
    }
    let reopened = Motion::from_document(&m.document(DocumentId::new()).unwrap()).unwrap();
    assert_eq!(m, reopened);
    let driver = a::add_node(&mut m, "fold.motion.value").unwrap();
    let before = m.clone();
    assert!(a::connect(&mut m, value, "value", link(driver, "value")).is_err());
    assert_eq!(m, before);
}

#[test]
fn legacy_ease_converts_to_equivalent_editable_bezier_channels() {
    use fold_motion::animation::{Interpolation, Key, Track};
    let mut track = Track {
        channels: vec![],
        keys: vec![
            Key {
                time: Time::ZERO,
                value: Datum::Vector([0., 5.]),
                interpolation: Interpolation::Ease,
            },
            Key {
                time: Time::new(1, 1).unwrap(),
                value: Datum::Vector([10., 15.]),
                interpolation: Interpolation::Linear,
            },
        ],
    };
    let legacy = track.clone();
    track.channels = track.scalar_curves();
    for n in 0..=10 {
        let time = Time::new(n, 10).unwrap();
        let a = legacy.sample(time).unwrap().vector().unwrap();
        let b = track.sample(time).unwrap().vector().unwrap();
        assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    }
    track.channels[0].insert(Time::new(1, 2).unwrap(), 100.);
    assert!(
        (track
            .sample(Time::new(1, 2).unwrap())
            .unwrap()
            .vector()
            .unwrap()[1]
            - 10.)
            .abs()
            < 1e-9
    );
}

#[test]
fn cleared_driver_components_hold_their_authored_value_without_recreating_keys() {
    use fold_motion::animation::{Interpolation, Key, Track};
    let track = Track {
        keys: vec![Key {
            time: Time::ZERO,
            value: Datum::Vector([3., 7.]),
            interpolation: Interpolation::Linear,
        }],
        channels: vec![
            fold_animation::Curve::default(),
            fold_animation::Curve {
                keys: vec![fold_animation::Key::new(Time::ZERO, 10.)],
            },
        ],
    };
    track.validate().unwrap();
    assert_eq!(
        track.sample(Time::new(1, 2).unwrap()).unwrap(),
        Datum::Vector([3., 10.])
    );
}

#[test]
fn motion_animation_migration_preserves_unowned_extension_fields() {
    let mut motion = small();
    let node = &mut motion.graph.nodes[0];
    node.extensions.insert(
        "animation".into(),
        serde_json::json!({"unowned":"preserve"}),
    );
    let mut document = motion.document(DocumentId::new()).unwrap();
    document.schema_version = 1;
    assert_eq!(Motion::from_document(&document).unwrap(), motion);
}

#[test]
fn legacy_negative_keys_remain_editable_without_changing_sampling() {
    use fold_motion::animation::{Interpolation, Key, Track};
    let mut track = Track {
        keys: vec![
            Key {
                time: Time::new(-1, 1).unwrap(),
                value: Datum::Scalar(0.),
                interpolation: Interpolation::Linear,
            },
            Key {
                time: Time::new(1, 1).unwrap(),
                value: Datum::Scalar(10.),
                interpolation: Interpolation::Linear,
            },
        ],
        channels: vec![],
    };
    track.channels = track.scalar_curves();
    track.validate().unwrap();
    assert_eq!(track.sample(Time::ZERO).unwrap(), Datum::Scalar(5.));
}
