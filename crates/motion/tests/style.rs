use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{FailureKind, Limits, Prepared, Value},
    model::{Construction, Node, Parameter, Reference},
    registry, source,
    types::{Alpha, Encoding, Literal, Unit},
};
use std::sync::Arc;
fn contours(
    geometry: &fold_motion::evaluation::Geometry,
) -> &Arc<Vec<fold_motion::evaluation::Contour>> {
    let fold_motion::evaluation::GeometryData::Contours(contours) = &geometry.data else {
        panic!("expected contours")
    };
    contours
}
fn link(node: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node }.into(),
    }
}
fn literal(value: Literal) -> Binding {
    Binding::Literal { value }
}
fn color(value: [f64; 4]) -> Literal {
    Literal::Color {
        value,
        encoding: Encoding::LinearRec709,
        alpha: Alpha::Straight,
    }
}
fn node(key: &str, mode: &str, name: &str, bindings: Vec<(&str, Binding)>) -> Node {
    registry::resolve(&format!("fold.motion.{key}"), 1, mode)
        .unwrap()
        .node(
            name,
            bindings.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        )
        .unwrap()
}
fn fixture() -> Construction {
    let mut c =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    let definition = &mut c.definitions[0];
    let shape = definition
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.shape")
        .unwrap()
        .id;
    // The style oracle compares an explicitly unpainted branch against styled
    // branches; interactive starters now include their own visible fill.
    definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .get_mut("geometry")
        .unwrap()
        .value = serde_json::to_value(link(shape)).unwrap();
    let tween = node(
        "tween",
        "linear",
        "Palette",
        vec![
            ("from", literal(color([2., 0.125, -0.2, 0.5]))),
            ("to", literal(color([4., 0.25, 0., 0.75]))),
            (
                "duration",
                literal(Literal::Duration {
                    value: Time::new(1, 1).unwrap(),
                }),
            ),
        ],
    );
    let fill = node(
        "fill",
        "solid",
        "Fill",
        vec![
            ("geometry", link(shape)),
            ("color", link(tween.id)),
            ("even_odd", literal(Literal::Bool { value: true })),
        ],
    );
    let stroke = node(
        "stroke",
        "solid_centered_butt_miter",
        "Stroke",
        vec![("geometry", link(fill.id))],
    );
    let other = node(
        "fill",
        "solid",
        "Alternate fill",
        vec![
            ("geometry", link(stroke.id)),
            ("color", literal(color([0., 0., 1., 1.]))),
        ],
    );
    let mut placements = vec![];
    for (name, id) in [
        ("Filled", fill.id),
        ("Stroked", stroke.id),
        ("Alternate", other.id),
    ] {
        placements.push(node("place", "screen", name, vec![("geometry", link(id))]));
    }
    let scene = definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.extend(placements.iter().map(|p| link(p.id)));
    scene.parameters.get_mut("children").unwrap().value =
        serde_json::to_value(Binding::List { values }).unwrap();
    definition.nodes.extend([tween, fill, stroke, other]);
    definition.nodes.extend(placements);
    c
}
fn evaluate(c: &Construction, time: Time) -> Result<Value, fold_motion::evaluation::Failure> {
    Prepared::new(c.clone()).unwrap().evaluate_detailed(
        c.definitions[0].outputs[0].id,
        time,
        Limits::default(),
        &|| Ok(()),
    )
}
#[test]
fn native_styles_round_trip_share_contours_and_preserve_independent_appearance() {
    let c = fixture();
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let mut ids = None;
    for (time, expected) in [
        (Time::ZERO, [2., 0.125, -0.2, 0.5]),
        (Time::new(1, 1).unwrap(), [4., 0.25, 0., 0.75]),
        (Time::new(1, 2).unwrap(), [3., 0.1875, -0.1, 0.625]),
        (Time::ZERO, [2., 0.125, -0.2, 0.5]),
    ] {
        let Value::Scene(scene) = evaluate(&c, time).unwrap() else {
            panic!()
        };
        let original = &scene.entries[0].placement.geometry;
        let filled = &scene.entries[1].placement.geometry;
        let stroked = &scene.entries[2].placement.geometry;
        let other = &scene.entries[3].placement.geometry;
        for styled in [filled, stroked, other] {
            assert_eq!(styled.id, original.id);
            assert!(Arc::ptr_eq(contours(styled), contours(original)));
        }
        assert!(original.appearance.fill.is_none() && original.appearance.stroke.is_none());
        assert!(filled.appearance.stroke.is_none());
        let fill = filled.appearance.fill.as_ref().unwrap();
        assert_eq!(fill.color.value, expected);
        assert!(fill.even_odd);
        assert_eq!(stroked.appearance.fill.as_ref().unwrap(), fill);
        let stroke = stroked.appearance.stroke.as_ref().unwrap();
        assert_eq!(stroke.width, 2.);
        assert_eq!(stroke.unit, Unit::Canvas);
        assert_eq!(stroke.color.value, [1.; 4]);
        assert_eq!(other.appearance.stroke.as_ref().unwrap(), stroke);
        assert_eq!(
            other.appearance.fill.as_ref().unwrap().color.value,
            [0., 0., 1., 1.]
        );
        let current: Vec<_> = contours(original)
            .iter()
            .flat_map(|c| c.points.iter().map(|p| p.id))
            .collect();
        if let Some(ids) = &ids {
            assert_eq!(&current, ids)
        } else {
            ids = Some(current)
        }
    }
}
#[test]
fn invalid_stroke_width_reports_native_operation_and_wrong_units_fail_types() {
    let c = fixture();
    for width in [-1., 10001.] {
        let mut invalid = c.clone();
        let stroke = invalid.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.operation == "fold.motion.stroke")
            .unwrap();
        let id = stroke.id;
        stroke.parameters.insert(
            "width".into(),
            Parameter::new(literal(Literal::Distance {
                value: width,
                unit: Unit::Canvas,
            }))
            .unwrap(),
        );
        let failure = evaluate(&invalid, Time::ZERO).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Invalid);
        assert_eq!(failure.context.unwrap().operation, id);
    }
    let mut invalid = c.clone();
    let stroke = invalid.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.stroke")
        .unwrap();
    stroke.parameters.insert(
        "width".into(),
        Parameter::new(literal(Literal::Distance {
            value: 2.,
            unit: Unit::Meters,
        }))
        .unwrap(),
    );
    assert!(Prepared::new(invalid).is_err());
}

#[test]
fn stroke_choices_survive_source_and_reach_vector_preparation() {
    use fold_motion::evaluation::style::StrokeMode;
    use fold_render::vector::{Cap, Join};
    for (mode, retained, cap, join) in [
        (
            "solid_centered_butt_miter",
            StrokeMode::SolidCenteredButtMiter,
            Cap::Butt,
            Join::Miter,
        ),
        (
            "solid_centered_butt_round",
            StrokeMode::SolidCenteredButtRound,
            Cap::Butt,
            Join::Round,
        ),
        (
            "solid_centered_round_miter",
            StrokeMode::SolidCenteredRoundMiter,
            Cap::Round,
            Join::Miter,
        ),
        (
            "solid_centered_round_round",
            StrokeMode::SolidCenteredRoundRound,
            Cap::Round,
            Join::Round,
        ),
    ] {
        let mut c = fixture();
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.operation == "fold.motion.stroke")
            .unwrap()
            .mode = mode.into();
        let restored = source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction;
        assert_eq!(restored, c);
        let Value::Scene(scene) = evaluate(&restored, Time::ZERO).unwrap() else {
            panic!()
        };
        let stroke = scene.entries[2]
            .placement
            .geometry
            .appearance
            .stroke
            .as_ref()
            .unwrap();
        assert_eq!(stroke.mode, retained);
        assert_eq!(stroke.mode.cap(), cap);
        assert_eq!(stroke.mode.join(), join);
    }
}

#[test]
fn native_cap_choice_changes_open_path_endpoint_coverage() {
    use fold_render::vector::{Drawing, Segment, Stroke};
    let mut alpha = vec![];
    for mode in ["solid_centered_butt_miter", "solid_centered_round_miter"] {
        let mut c = fixture();
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.operation == "fold.motion.stroke")
            .unwrap()
            .mode = mode.into();
        let Value::Scene(scene) = evaluate(&c, Time::ZERO).unwrap() else {
            panic!()
        };
        let s = scene.entries[2]
            .placement
            .geometry
            .appearance
            .stroke
            .as_ref()
            .unwrap();
        let drawing = Drawing {
            feather: None,
            path: Arc::new(vec![Segment::Move([20., 20.]), Segment::Line([40., 20.])]),
            transform: [1., 0., 0., 1., 0., 0.],
            fill: None,
            stroke: Some(Stroke {
                width: 10.,
                color: s.color.value,
                cap: s.mode.cap(),
                join: s.mode.join(),
            }),
            even_odd: false,
        };
        let image = fold_render::render(fold_render::RenderGraph {
            width: 64,
            height: 40,
            nodes: vec![fold_render::ImageOp::Vector(Arc::new(vec![drawing]))],
            output: 0,
        })
        .unwrap();
        alpha.push(image.pixels()[20 * 64 + 17][3]);
    }
    assert_eq!(alpha, [0., 1.]);
}
