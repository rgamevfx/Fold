use fold_foundation::{DocumentId, Time};
use fold_motion::{
    Motion,
    authoring::{node, scene::assets},
    evaluation::{Evaluator, Value},
    fields::Datum,
    geometry::{Content, Geometry},
    graph::Link,
};
fn fixture() -> (Motion, fold_foundation::ObjectId) {
    let mut m = Motion::new_scene();
    let source = assets::badge(&mut m).unwrap();
    let id = assets::duplicate(&mut m, source).unwrap();
    node(&mut m, id).unwrap().set("count", Datum::Scalar(4.));
    node(&mut m, id)
        .unwrap()
        .set("spacing", Datum::Vector([12., 3.]));
    (m, id)
}
fn copies(m: &Motion, id: fold_foundation::ObjectId, t: Time) -> Content {
    let Value::Content(c) = Evaluator::new(m, t)
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
fn position_wave_is_time_delayed_per_copy_and_preserves_spacing_and_identity() {
    let (mut m, id) = fixture();
    let wave = assets::attach_copy_wave(&mut m, id).unwrap();
    node(&mut m, wave)
        .unwrap()
        .set("amplitude", Datum::Scalar(20.));
    node(&mut m, id)
        .unwrap()
        .set("stagger", Datum::Scalar(0.125));
    for frequency in [0.5, 1., 2.] {
        node(&mut m, wave)
            .unwrap()
            .set("frequency", Datum::Scalar(frequency));
        for t in [
            Time::new(3, 4).unwrap(),
            Time::ZERO,
            Time::new(-1, 4).unwrap(),
            Time::new(3, 4).unwrap(),
        ] {
            let c = copies(&m, id, t);
            assert_eq!(c.len(), 4);
            for (i, e) in c.iter().enumerate() {
                let seconds = t.numerator() as f64 / t.denominator() as f64;
                let y = 3. * i as f64
                    + 20.
                        * ((seconds - i as f64 * 0.125) * frequency * std::f64::consts::TAU).sin();
                assert!((e.transform[5] - y).abs() < 1e-9);
                assert_eq!(e.transform[4], 12. * i as f64);
            }
        }
    }
    let before = copies(&m, id, Time::ZERO);
    node(&mut m, id).unwrap().set("count", Datum::Scalar(6.));
    let after = copies(&m, id, Time::ZERO);
    assert_eq!(
        before.iter().map(|e| e.id).collect::<Vec<_>>(),
        after[..4].iter().map(|e| e.id).collect::<Vec<_>>()
    );
    node(&mut m, id).unwrap().set("offset_y", Datum::Scalar(0.));
    assert_eq!(copies(&m, id, Time::new(9, 1).unwrap())[2].transform[5], 6.);
}
fn appearance(c: &Content, shapes: &mut Vec<[f64; 4]>, text: &mut Vec<[f64; 4]>) {
    for e in c.iter() {
        match &e.geometry {
            Geometry::Group(c) | Geometry::Instance(c) => appearance(c, shapes, text),
            Geometry::Path(_) => shapes.push(e.fill.unwrap()),
            Geometry::Glyph { .. } => text.push(e.fill.unwrap()),
            _ => {}
        }
    }
}
#[test]
fn palette_covers_copies_preserves_text_and_survives_roundtrip() {
    let (mut m, id) = fixture();
    let palette = assets::attach_copy_palette(&mut m, id).unwrap();
    let a = [0.1, 0.2, 0.3, 1.];
    let b = [0.7, 0.6, 0.5, 1.];
    node(&mut m, palette).unwrap().set("start", Datum::Color(a));
    node(&mut m, palette).unwrap().set("end", Datum::Color(b));
    let m = Motion::from_document(&m.document(DocumentId::new()).unwrap()).unwrap();
    let c = copies(&m, id, Time::ZERO);
    let (mut shapes, mut text) = (vec![], vec![]);
    appearance(&c, &mut shapes, &mut text);
    assert_eq!(shapes[0], a);
    assert_eq!(*shapes.last().unwrap(), b);
    assert!(text.iter().all(|c| *c == [0.9, 0.95, 1., 1.]));
    assert!((shapes[1][0] - 0.3).abs() < 1e-9);
}
#[test]
fn zero_and_one_copy_are_valid_and_source_changes_remain_live() {
    let (mut m, id) = fixture();
    node(&mut m, id).unwrap().set("count", Datum::Scalar(0.));
    assert!(copies(&m, id, Time::ZERO).is_empty());
    node(&mut m, id).unwrap().set("count", Datum::Scalar(1.));
    assets::attach_copy_wave(&mut m, id).unwrap();
    let _palette = assets::attach_copy_palette(&mut m, id).unwrap();
    assert_eq!(copies(&m, id, Time::ZERO).len(), 1);
    let source = assets::reference(&m, id, "source").unwrap();
    let text = m
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| {
            o.parent == Some(source) && m.graph.node(o.id).unwrap().kind == "fold.motion.text"
        })
        .unwrap()
        .id;
    node(&mut m, text)
        .unwrap()
        .set("text", Datum::Text("CHANGE".into()));
    let mut glyphs = vec![];
    appearance(&copies(&m, id, Time::ZERO), &mut vec![], &mut glyphs);
    assert_eq!(glyphs.len(), 6);
}

#[test]
fn stagger_accepts_keyframes_and_animated_parameters_through_groups() {
    use fold_motion::{
        animation::{Interpolation, Key, Track},
        graph::Node,
    };
    for authored_parameter in [false, true] {
        let (mut m, id) = fixture();
        node(&mut m, id)
            .unwrap()
            .set("stagger", Datum::Scalar(0.25));
        let mut driver = Node::new(if authored_parameter {
            "fold.motion.value"
        } else {
            "fold.motion.keyframes"
        })
        .unwrap();
        if authored_parameter {
            let curve = driver.animation.entry("value.0".into()).or_default();
            curve.insert(Time::ZERO, 0.);
            curve.insert(Time::new(1, 1).unwrap(), 100.);
        } else {
            driver.settings = serde_json::to_value(Track {
                channels: vec![],
                keys: vec![
                    Key {
                        time: Time::ZERO,
                        value: Datum::Scalar(0.),
                        interpolation: Interpolation::Linear,
                    },
                    Key {
                        time: Time::new(1, 1).unwrap(),
                        value: Datum::Scalar(100.),
                        interpolation: Interpolation::Linear,
                    },
                ],
            })
            .unwrap();
        }
        let driver_id = driver.id;
        m.graph.nodes.push(driver);
        fold_motion::authoring::group_node(&mut m, driver_id).unwrap();
        node(&mut m, id)
            .unwrap()
            .connect("offset_y", driver_id, "value");
        let c = copies(&m, id, Time::new(1, 1).unwrap());
        for (i, copy) in c.iter().enumerate() {
            assert!((copy.transform[5] - (100. - 25. * i as f64 + 3. * i as f64)).abs() < 1e-9);
        }
        assert!(!m.graph.node(id).unwrap().inputs.contains_key("amplitude"));
    }
}
#[test]
fn wave_and_palette_are_external_reusable_drivers() {
    let (mut m, id) = fixture();
    let wave = assets::attach_copy_wave(&mut m, id).unwrap();
    let palette = assets::attach_copy_palette(&mut m, id).unwrap();
    assert_eq!(assets::attach_copy_wave(&mut m, id).unwrap(), wave);
    assert_eq!(assets::attach_copy_palette(&mut m, id).unwrap(), palette);
    assert_eq!(assets::copy_driver(&m, id, "offset_y"), Some(wave));
    assert_eq!(assets::copy_driver(&m, id, "color"), Some(palette));
    node(&mut m, id).unwrap().connect("offset_x", wave, "value");
    let c = copies(&m, id, Time::new(1, 4).unwrap());
    for (i, copy) in c.iter().enumerate() {
        assert!(
            (copy.transform[4] - 12. * i as f64 - (copy.transform[5] - 3. * i as f64)).abs() < 1e-9
        );
    }
}

#[test]
fn count_interpolates_and_accepts_a_rounded_wave() {
    let (mut m, id) = fixture();
    let curve = node(&mut m, id)
        .unwrap()
        .animation
        .entry("count.0".into())
        .or_default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 120.);
    for (n, d, expected) in [(1, 2, 60), (0, 1, 0), (1, 1, 120), (1, 7, 17)] {
        assert_eq!(copies(&m, id, Time::new(n, d).unwrap()).len(), expected);
    }
    node(&mut m, id).unwrap().animation.clear();
    let wave = assets::attach_copy_wave(&mut m, id).unwrap();
    node(&mut m, wave)
        .unwrap()
        .set("amplitude", Datum::Scalar(60.));
    node(&mut m, wave)
        .unwrap()
        .set("offset", Datum::Scalar(60.));
    node(&mut m, wave)
        .unwrap()
        .set("frequency", Datum::Scalar(1.));
    let mut rounding = fold_motion::graph::Node::new("fold.motion.math").unwrap();
    rounding.settings = serde_json::json!({"operation":"Floor"});
    rounding.connect("a", wave, "value");
    node(&mut m, id)
        .unwrap()
        .connect("count", rounding.id, "value");
    m.graph.nodes.push(rounding);
    assert_eq!(copies(&m, id, Time::ZERO).len(), 60);
    assert_eq!(copies(&m, id, Time::new(1, 4).unwrap()).len(), 120);
    assert_eq!(copies(&m, id, Time::new(3, 4).unwrap()).len(), 0);
}

#[test]
fn rounding_math_handles_negative_values_and_vector_components() {
    use fold_motion::fields::{Budget, Sample};
    let (mut m, _) = fixture();
    let mut math = fold_motion::graph::Node::new("fold.motion.math").unwrap();
    let id = math.id;
    math.set("a", Datum::Vector([-1.6, 1.6]));
    math.set("b", Datum::Vector([0., 0.]));
    math.settings = serde_json::json!({"operation":"Round", "value_type":"Vector"});
    m.graph.nodes.push(math);
    for (mode, expected) in [
        ("Round", [-2., 2.]),
        ("Floor", [-2., 1.]),
        ("Ceil", [-1., 2.]),
        ("Truncate", [-1., 1.]),
    ] {
        node(&mut m, id).unwrap().settings["operation"] = serde_json::json!(mode);
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
        assert_eq!(
            field
                .sample(&Sample::default(), &mut Budget::default())
                .unwrap(),
            Datum::Vector(expected)
        );
    }
}

#[test]
fn dropping_a_new_path_into_badge_updates_every_copy() {
    let mut m = Motion::new_scene();
    let badge = assets::badge(&mut m).unwrap();
    let duplicate = assets::duplicate(&mut m, badge).unwrap();
    node(&mut m, duplicate)
        .unwrap()
        .set("count", Datum::Scalar(4.));
    let before = fold_motion::geometry::drawings(&copies(&m, duplicate, Time::ZERO), [1., 1.])
        .unwrap()
        .len();
    let path = fold_motion::authoring::scene::create_object(&mut m, "fold.motion.path").unwrap();
    fold_motion::authoring::scene::place_object(&mut m, path, Some(badge), None, Time::ZERO)
        .unwrap();
    let after = fold_motion::geometry::drawings(&copies(&m, duplicate, Time::ZERO), [1., 1.])
        .unwrap()
        .len();
    assert_eq!(after, before + 4);
}
