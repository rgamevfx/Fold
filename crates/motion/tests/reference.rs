//! Real native graph samples against the frozen M2 semantic fixture.
use fold_foundation::{ObjectId, Time};
use fold_motion::{
    binding::{Binding, Combination, Interpolation, Key, Track},
    evaluation::{Limits, Prepared, Scene, Value},
    expression::{Binary, Expr},
    model::{
        Canvas, Construction, Definition, Input, Instance, Output, Parameter, Presentation,
        Reference,
    },
    registry,
    types::{Alpha, Clock, Encoding, Literal, Ratio, Space, Type, Unit},
};
use fold_project::Metadata;
use std::{collections::BTreeMap, sync::Arc};
fn contours(geometry: &fold_motion::evaluation::Geometry) -> &[fold_motion::evaluation::Contour] {
    let fold_motion::evaluation::GeometryData::Contours(contours) = &geometry.data else {
        panic!("expected contours")
    };
    contours
}
fn literal(value: Literal) -> Binding {
    Binding::Literal { value }
}
fn link(node: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node }.into(),
    }
}
fn vector(y: f64) -> Literal {
    Literal::Vector {
        value: vec![0., y],
        unit: Unit::Canvas,
        space: Space::Screen,
    }
}
fn input(name: &str, default: Literal) -> Input {
    Input {
        id: ObjectId::new(),
        name: name.into(),
        ty: default.ty(),
        default,
        protected: false,
        extensions: Metadata::new(),
    }
}
fn node(
    key: &str,
    mode: &str,
    name: &str,
    parameters: Vec<(&str, Binding)>,
) -> fold_motion::model::Node {
    registry::resolve(&format!("fold.motion.{key}"), 1, mode)
        .unwrap()
        .node(
            name,
            parameters
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        )
        .unwrap()
}
fn fixture() -> Construction {
    let size = input(
        "size",
        Literal::Distance {
            value: 100.0,
            unit: Unit::Canvas,
        },
    );
    let accent = input(
        "accent",
        Literal::Color {
            value: [1., 0., 0., 1.],
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        },
    );
    let mut count = input("count", Literal::Integer { value: 1 });
    count.protected = true;
    let width = node(
        "expression",
        "value",
        "width",
        vec![(
            "value",
            Binding::Expression {
                expression: Expr::Binary {
                    op: Binary::Multiply,
                    left: Box::new(Expr::Reference {
                        reference: Reference::Input { input: size.id }.into(),
                    }),
                    right: Box::new(Expr::Literal {
                        value: Literal::Scalar { value: 2.0 },
                    }),
                },
            },
        )],
    );
    let shape = node(
        "shape",
        "rectangle",
        "profile",
        vec![
            ("width", link(width.id)),
            (
                "height",
                Binding::Link {
                    reference: Reference::Input { input: size.id }.into(),
                },
            ),
        ],
    );
    let entrance = node(
        "tween",
        "linear",
        "entrance",
        vec![
            ("from", literal(vector(80.))),
            ("to", literal(vector(0.))),
            (
                "duration",
                literal(Literal::Duration {
                    value: Time::new(3, 5).unwrap(),
                }),
            ),
        ],
    );
    let opacity = node(
        "keyframes",
        "value",
        "opacity",
        vec![(
            "track",
            Binding::Track {
                track: Track {
                    id: ObjectId::new(),
                    ty: Type::Scalar,
                    combination: Combination::Replace,
                    base: None,
                    extensions: Metadata::new(),
                    keys: [(0, 0.), (1, 1.), (2, 0.5)]
                        .into_iter()
                        .map(|(n, value)| Key {
                            id: ObjectId::new(),
                            time: Time::new(n, 2).unwrap(),
                            value: Literal::Scalar { value },
                            interpolation: Interpolation::Linear,
                            handles: Default::default(),
                            extensions: Metadata::new(),
                        })
                        .collect(),
                },
            },
        )],
    );
    let place = node(
        "place",
        "screen",
        "placement",
        vec![
            ("geometry", link(shape.id)),
            ("position", link(entrance.id)),
            ("opacity", link(opacity.id)),
        ],
    );
    let scene = node(
        "scene",
        "ordered",
        "scene",
        vec![(
            "children",
            Binding::List {
                values: vec![link(place.id)],
            },
        )],
    );
    let output = Output {
        id: ObjectId::new(),
        name: "main".into(),
        ty: Type::Scene,
        value: Reference::Node { node: scene.id }.into(),
        extensions: Metadata::new(),
    };
    let definition = Definition {
        id: ObjectId::new(),
        name: "Animated shape".into(),
        inputs: vec![size, accent, count],
        nodes: vec![width, shape, entrance, opacity, place, scene],
        instances: vec![],
        outputs: vec![output],
        extensions: Metadata::new(),
    };
    Construction {
        seed: 20261008,
        canvas: Canvas {
            width: 1920,
            height: 1080,
            rate: [24, 1],
            start: Time::ZERO,
            end: Time::new(4, 1).unwrap(),
            extensions: Metadata::new(),
        },
        root: Instance {
            id: ObjectId::new(),
            name: "main".into(),
            definition: definition.id,
            clock: Clock::default(),
            overrides: BTreeMap::new(),
            extensions: Metadata::new(),
        },
        definitions: vec![definition],
        presentation: Presentation::default(),
        extensions: Metadata::new(),
    }
}
fn scene(prepared: &Prepared, output: ObjectId, time: Time) -> Arc<Scene> {
    let Value::Scene(scene) = prepared
        .evaluate(output, time, Limits::default(), &|| Ok(()))
        .unwrap()
    else {
        panic!("Expected scene")
    };
    scene
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9, "{a} != {b}");
}
#[test]
fn animated_shape_matches_frozen_values_and_identities_in_every_seek_order() {
    let construction = fixture();
    let output = construction.definitions[0].outputs[0].id;
    let prepared = Prepared::new(construction.clone()).unwrap();
    let spec: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rewrite/fixtures/animated-shape.json"
    ))
    .unwrap();
    let times: Vec<Time> = spec["times"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| Time::new(t[0].as_i64().unwrap(), t[1].as_u64().unwrap() as u32).unwrap())
        .collect();
    let mut baseline = BTreeMap::new();
    let mut ids = None;
    for order in spec["orders"].as_array().unwrap() {
        for index in order.as_array().unwrap() {
            let time = times[index.as_u64().unwrap() as usize];
            let sampled = scene(&prepared, output, time);
            assert_eq!(sampled.entries.len(), 1);
            let entry = &sampled.entries[0];
            let placement = &entry.placement;
            let contour = &contours(&placement.geometry)[0];
            let seconds = time.numerator() as f64 / f64::from(time.denominator());
            close(screen_position(placement)[0], 0.);
            close(
                screen_position(placement)[1],
                80. * (1. - (seconds / 0.6).clamp(0., 1.)),
            );
            close(
                placement.opacity,
                if seconds <= 0.5 {
                    (seconds * 2.).max(0.)
                } else {
                    (1.5 - seconds).max(0.5)
                },
            );
            assert_eq!(
                contour
                    .points
                    .iter()
                    .map(|p| p.position)
                    .collect::<Vec<_>>(),
                vec![[0., 0.], [200., 0.], [200., 100.], [0., 100.]]
            );
            assert!(contour.closed);
            let mut observed = vec![entry.id, placement.id, placement.geometry.id, contour.id];
            observed.extend(contour.points.iter().map(|p| p.id));
            if let Some(ids) = &ids {
                assert_eq!(ids, &observed);
            } else {
                ids = Some(observed);
            }
            if let Some(previous) = baseline.insert(time, sampled.clone()) {
                assert_eq!(previous, sampled);
            }
        }
    }
    let text = fold_motion::source::canonical(&construction).unwrap();
    let parsed = fold_motion::source::parse(&text).unwrap();
    assert_eq!(parsed.construction, construction);
    assert!(parsed.unresolved.is_empty());
    assert!(parsed.spans.contains_key(&output));
    assert_eq!(
        fold_motion::source::canonical(&parsed.construction).unwrap(),
        text
    );
    let prepared = Prepared::new(parsed.construction).unwrap();
    for (time, expected) in baseline {
        assert_eq!(scene(&prepared, output, time), expected);
    }
}
#[test]
fn asset_clocks_overrides_and_caches_are_isolated_by_instance() {
    let mut construction = fixture();
    let definition = &mut construction.definitions[0];
    let placement_output = ObjectId::new();
    let color_output = ObjectId::new();
    definition.outputs.push(Output {
        id: placement_output,
        name: "placement".into(),
        ty: Type::Placement,
        value: Reference::Node {
            node: definition.nodes[4].id,
        }
        .into(),
        extensions: Metadata::new(),
    });
    definition.outputs.push(Output {
        id: color_output,
        name: "accent".into(),
        ty: definition.inputs[1].ty.clone(),
        value: Reference::Input {
            input: definition.inputs[1].id,
        }
        .into(),
        extensions: Metadata::new(),
    });
    let shape_output = definition.outputs[0].id;
    let original = Instance {
        id: ObjectId::new(),
        name: "original".into(),
        definition: definition.id,
        clock: Clock::default(),
        overrides: BTreeMap::new(),
        extensions: Metadata::new(),
    };
    let mut variant = original.clone();
    variant.id = ObjectId::new();
    variant.name = "variant".into();
    variant.clock = Clock {
        offset: Time::new(1, 4).unwrap(),
        rate: Ratio::new(-1, 1).unwrap(),
    };
    variant.overrides.insert(
        definition.inputs[0].id,
        Parameter::new(literal(Literal::Distance {
            value: 50.,
            unit: Unit::Canvas,
        }))
        .unwrap(),
    );
    let blue = Literal::Color {
        value: [0., 0., 1., 1.],
        encoding: Encoding::LinearRec709,
        alpha: Alpha::Straight,
    };
    variant.overrides.insert(
        definition.inputs[1].id,
        Parameter::new(literal(blue.clone())).unwrap(),
    );
    let combined = node(
        "scene",
        "ordered",
        "both",
        vec![(
            "children",
            Binding::List {
                values: [&original, &variant]
                    .into_iter()
                    .map(|instance| Binding::Link {
                        reference: Reference::Instance {
                            instance: instance.id,
                            output: placement_output,
                        }
                        .into(),
                    })
                    .collect(),
            },
        )],
    );
    let mut outputs: Vec<_> = [&original, &variant]
        .into_iter()
        .map(|instance| Output {
            id: ObjectId::new(),
            name: instance.name.clone(),
            ty: Type::Scene,
            value: Reference::Instance {
                instance: instance.id,
                output: shape_output,
            }
            .into(),
            extensions: Metadata::new(),
        })
        .collect();
    let original_output = outputs[0].id;
    let variant_output = outputs[1].id;
    let combined_output = ObjectId::new();
    let blue_output = ObjectId::new();
    outputs.push(Output {
        id: combined_output,
        name: "both".into(),
        ty: Type::Scene,
        value: Reference::Node { node: combined.id }.into(),
        extensions: Metadata::new(),
    });
    outputs.push(Output {
        id: blue_output,
        name: "blue".into(),
        ty: blue.ty(),
        value: Reference::Instance {
            instance: variant.id,
            output: color_output,
        }
        .into(),
        extensions: Metadata::new(),
    });
    let parent = Definition {
        id: ObjectId::new(),
        name: "Parent".into(),
        inputs: vec![],
        nodes: vec![combined],
        instances: vec![original, variant],
        outputs,
        extensions: Metadata::new(),
    };
    construction.root.definition = parent.id;
    construction.definitions.push(parent);
    let parent_id = construction.definitions[1].id;
    let fragment_text = fold_motion::source::canonical_fragment(&construction, parent_id).unwrap();
    let parent_fragment =
        fold_motion::source::parse_fragment(&fragment_text, &construction, parent_id).unwrap();
    assert_eq!(parent_fragment.definition, construction.definitions[1]);
    let document = fold_foundation::DocumentId::new();
    let mut project = fold_project::Project::new(8);
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                construction.document(document).unwrap(),
            )],
        })
        .unwrap();
    let mut renamed_parent = parent_fragment;
    renamed_parent.definition.name = "Renamed parent".into();
    let candidate = fold_motion::editing::prepare_fragment(
        &project.snapshot(),
        document,
        parent_id,
        &renamed_parent,
        &fold_motion::reconcile::IdentityMap::new(),
    )
    .unwrap();
    let expected = Construction::from_document(candidate.document()).unwrap();
    assert_eq!(expected.definitions[0], construction.definitions[0]);
    candidate.apply(&mut project).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("animated-asset-variants.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    assert_eq!(
        Construction::from_document(&reopened.snapshot().state().documents[&document]).unwrap(),
        expected
    );
    project.undo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        construction
    );
    project.redo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        expected
    );
    let printed = fold_motion::source::canonical(&construction).unwrap();
    let parsed = fold_motion::source::parse(&printed).unwrap();
    assert_eq!(parsed.construction, construction);
    let prepared = Prepared::new(parsed.construction).unwrap();
    let a = scene(&prepared, original_output, Time::new(-1, 4).unwrap());
    let b = scene(&prepared, variant_output, Time::new(-1, 4).unwrap());
    close(screen_position(&a.entries[0].placement)[1], 80.);
    close(screen_position(&b.entries[0].placement)[1], 80. / 6.);
    close(b.entries[0].placement.opacity, 1.);
    assert_eq!(
        contours(&b.entries[0].placement.geometry)[0].points[2].position,
        [100., 50.]
    );
    assert_ne!(a.entries[0].placement.id, b.entries[0].placement.id);
    let both = scene(&prepared, combined_output, Time::new(-1, 4).unwrap());
    assert_eq!(both.entries[0].placement, a.entries[0].placement);
    assert_eq!(both.entries[1].placement, b.entries[0].placement);
    assert_eq!(
        prepared
            .evaluate(blue_output, Time::ZERO, Limits::default(), &|| Ok(()))
            .unwrap(),
        Value::Primitive(blue)
    );
    let children = construction.definitions[1].nodes[0]
        .parameters
        .get_mut("children")
        .unwrap();
    let Binding::List { mut values } = children.binding().unwrap() else {
        unreachable!()
    };
    values.reverse();
    children.value = serde_json::to_value(Binding::List {
        values: values.clone(),
    })
    .unwrap();
    let reordered = Prepared::new(construction.clone()).unwrap();
    let reordered_scene = scene(&reordered, combined_output, Time::new(-1, 4).unwrap());
    assert_eq!(reordered_scene.entries[0], both.entries[1]);
    assert_eq!(reordered_scene.entries[1], both.entries[0]);
    let mut invalid = construction.clone();
    values.push(values[0].clone());
    invalid.definitions[1].nodes[0]
        .parameters
        .get_mut("children")
        .unwrap()
        .value = serde_json::to_value(Binding::List { values }).unwrap();
    assert!(
        invalid
            .validate_semantics()
            .unwrap_err()
            .contains("Duplicate native identity")
    );
    construction.root.clock.rate = Ratio::new(0, 1).unwrap();
    let frozen = Prepared::new(construction).unwrap();
    assert_eq!(
        scene(&frozen, variant_output, Time::new(-100, 1).unwrap()),
        scene(&frozen, variant_output, Time::new(100, 1).unwrap())
    );
}
#[test]
fn evaluation_budgets_cancellation_and_invalid_dimensions_are_errors() {
    let mut construction = fixture();
    let output = construction.definitions[0].outputs[0].id;
    let prepared = Prepared::new(construction.clone()).unwrap();
    assert!(
        prepared
            .evaluate(
                output,
                Time::ZERO,
                Limits {
                    steps: 1,
                    elements: 100
                },
                &|| Ok(())
            )
            .unwrap_err()
            .contains("budget")
    );
    assert!(
        prepared
            .evaluate(
                output,
                Time::ZERO,
                Limits {
                    steps: 1000,
                    elements: 1
                },
                &|| Ok(())
            )
            .unwrap_err()
            .contains("budget")
    );
    assert_eq!(
        prepared
            .evaluate(output, Time::ZERO, Limits::default(), &|| Err(
                "cancelled".into()
            ))
            .unwrap_err(),
        "cancelled"
    );
    construction.definitions[0].inputs[0].default = Literal::Distance {
        value: -1.,
        unit: Unit::Canvas,
    };
    let invalid = Prepared::new(construction).unwrap();
    assert!(
        invalid
            .evaluate(output, Time::ZERO, Limits::default(), &|| Ok(()))
            .unwrap_err()
            .contains("positive")
    );
    assert!(
        prepared
            .evaluate(ObjectId::new(), Time::ZERO, Limits::default(), &|| Ok(()))
            .unwrap_err()
            .contains("Output")
    );
}

#[test]
fn scene_and_asset_time_and_cubic_tween_remain_explicit() {
    let mut construction = fixture();
    construction.root.clock = Clock {
        offset: Time::new(1, 4).unwrap(),
        rate: Ratio::new(-1, 1).unwrap(),
    };
    let definition = &mut construction.definitions[0];
    let mut outputs = Vec::new();
    for mode in ["scene", "asset"] {
        let clock = node("time", mode, &format!("{mode}_clock"), vec![]);
        let output = ObjectId::new();
        definition.outputs.push(Output {
            id: output,
            name: format!("{mode}_clock"),
            ty: Type::Signal {
                value: Box::new(Type::Duration),
            },
            value: Reference::Node { node: clock.id }.into(),
            extensions: Metadata::new(),
        });
        definition.nodes.push(clock);
        outputs.push(output);
    }
    let tween = node(
        "tween",
        "cubic_out",
        "ease",
        vec![
            ("from", literal(Literal::Scalar { value: 0. })),
            ("to", literal(Literal::Scalar { value: 1. })),
            (
                "time",
                literal(Literal::Duration {
                    value: Time::new(1, 2).unwrap(),
                }),
            ),
        ],
    );
    let tween_output = ObjectId::new();
    definition.outputs.push(Output {
        id: tween_output,
        name: "ease".into(),
        ty: Type::Signal {
            value: Box::new(Type::Scalar),
        },
        value: Reference::Node { node: tween.id }.into(),
        extensions: Metadata::new(),
    });
    definition.nodes.push(tween);
    let prepared = Prepared::new(construction).unwrap();
    for (output, time) in [
        (outputs[0], Time::new(3, 4).unwrap()),
        (outputs[1], Time::new(-1, 2).unwrap()),
    ] {
        assert_eq!(
            prepared
                .evaluate(output, Time::new(3, 4).unwrap(), Limits::default(), &|| Ok(
                    ()
                ))
                .unwrap(),
            Value::Primitive(Literal::Duration { value: time })
        );
    }
    assert_eq!(
        prepared
            .evaluate(tween_output, Time::ZERO, Limits::default(), &|| Ok(()))
            .unwrap(),
        Value::Primitive(Literal::Scalar { value: 0.875 })
    );
}

#[test]
fn construction_projection_preserves_expression_edges_and_unresolved_records() {
    let mut construction = fixture();
    let definition = &mut construction.definitions[0];
    let mut unresolved = definition.nodes[0].clone();
    unresolved.id = ObjectId::new();
    unresolved.name = "future node".into();
    unresolved.operation = "vendor.future".into();
    unresolved.version = 73;
    unresolved.parameters = std::collections::BTreeMap::from([(
        "future".into(),
        Parameter {
            id: ObjectId::new(),
            value: serde_json::json!({"future_binding":[1,null,{"keep":"raw"}]}),
            protected: false,
            extensions: Metadata::new(),
        },
    )]);
    unresolved.extensions.insert(
        "future_layout".into(),
        serde_json::json!({"λ":[1,null,"keep"]}),
    );
    definition.nodes.push(unresolved.clone());
    let node = definition.nodes[0].id;
    construction.presentation.positions.insert(node, [14., 22.]);
    construction
        .presentation
        .annotations
        .insert(node, "artist λ".into());
    construction
        .presentation
        .groups
        .push(fold_motion::model::Group {
            id: ObjectId::new(),
            name: "Keep layout".into(),
            members: vec![node],
            locked: true,
            extensions: Metadata::new(),
        });
    construction
        .extensions
        .insert("future_document".into(), serde_json::json!({"keep":true}));
    let source = fold_motion::source::canonical(&construction).unwrap();
    assert_eq!(
        source,
        fold_motion::source::canonical(&construction).unwrap()
    );
    let parsed_document = fold_motion::source::parse(&source).unwrap();
    assert_eq!(parsed_document.construction, construction);
    assert_eq!(parsed_document.unresolved, vec![unresolved.id]);
    let document = fold_foundation::DocumentId::new();
    let mut project = fold_project::Project::new(4);
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                construction.document(document).unwrap(),
            )],
        })
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unresolved-native.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 4).unwrap();
    assert_eq!(
        Construction::from_document(&reopened.snapshot().state().documents[&document]).unwrap(),
        construction
    );
    let unresolved_line = source
        .lines()
        .find_map(|line| line.trim().strip_prefix("unresolved "))
        .unwrap();
    let recovered: fold_motion::model::Node =
        serde_json::from_str(unresolved_line.strip_suffix(';').unwrap()).unwrap();
    assert_eq!(recovered, unresolved);
    let expression_line = source
        .lines()
        .find(|line| line.contains(" = expr("))
        .unwrap();
    let text = expression_line
        .split_once(" = expr(")
        .unwrap()
        .1
        .strip_suffix(");")
        .unwrap();
    let definition = &construction.definitions[0];
    let parsed = fold_motion::expression_source::parse(text, &mut |name| {
        definition
            .inputs
            .iter()
            .find(|i| i.name == name)
            .map(|input| Reference::Input { input: input.id })
            .ok_or("Unknown input".into())
    })
    .unwrap();
    let Binding::Expression { expression } =
        definition.nodes[0].parameters["value"].binding().unwrap()
    else {
        unreachable!()
    };
    assert_eq!(parsed.expression, expression);
    let mut escaped = construction.clone();
    escaped.definitions[0].inputs[0].name = "instance:[\"name\",\"output\"]".into();
    let escaped_source = fold_motion::source::canonical(&escaped).unwrap();
    let line = escaped_source
        .lines()
        .find(|line| line.contains(" = expr("))
        .unwrap();
    let text = line
        .split_once(" = expr(")
        .unwrap()
        .1
        .strip_suffix(");")
        .unwrap();
    assert_eq!(
        fold_motion::expression_source::parse(text, &mut |name| {
            fold_motion::source::resolve_reference_name(&escaped, &escaped.definitions[0], name)
        })
        .unwrap()
        .expression,
        expression
    );
    let mut renamed = construction.clone();
    renamed.definitions[0].inputs[0].name = "new size".into();
    let renamed_source = fold_motion::source::canonical(&renamed).unwrap();
    let line = renamed_source
        .lines()
        .find(|line| line.contains(" = expr("))
        .unwrap();
    let text = line
        .split_once(" = expr(")
        .unwrap()
        .1
        .strip_suffix(");")
        .unwrap();
    assert_eq!(
        fold_motion::expression_source::parse(text, &mut |name| {
            assert_eq!(name, "new size");
            Ok(Reference::Input {
                input: definition.inputs[0].id,
            })
        })
        .unwrap()
        .expression,
        expression
    );
}

#[test]
fn id_free_source_allocates_native_defaults_and_resolves_forward_references() {
    let fixture = fixture();
    let source = format!(
        r#"motion 1;
seed 7;
canvas {};
asset "Forward" {{
  let "answer" = "fold.motion.expression"."value" @1 (
    "value" = expr(later * 2);
  );
  let "later" = "fold.motion.expression"."value" @1 (
    "value" = value(3);
  );
  let "empty" = "fold.motion.scene"."ordered" @1 ();
  output "result" : {{"kind":"signal","value":{{"kind":"scalar"}}}} = answer;
}}
root "main" = "Forward" clock {} {{}};
presentation {};
extensions {{}};
"#,
        serde_json::to_string(&fixture.canvas).unwrap(),
        serde_json::to_string(&Clock::default()).unwrap(),
        serde_json::to_string(&Presentation::default()).unwrap()
    );
    let parsed = fold_motion::source::parse(&source).unwrap();
    parsed.construction.validate_semantics().unwrap();
    let second = fold_motion::source::parse(&source).unwrap();
    assert_ne!(
        parsed.construction.definitions[0].id,
        second.construction.definitions[0].id
    );
    assert_ne!(
        parsed.construction.definitions[0].nodes[0].id,
        second.construction.definitions[0].nodes[0].id
    );
    let output = parsed.construction.definitions[0].outputs[0].id;
    assert!(
        parsed.construction.definitions[0].nodes[2]
            .parameters
            .contains_key("children")
    );
    let canonical = fold_motion::source::canonical(&parsed.construction).unwrap();
    assert_eq!(
        fold_motion::source::parse(&canonical).unwrap().construction,
        parsed.construction
    );
    let prepared = Prepared::new(parsed.construction).unwrap();
    assert_eq!(
        prepared
            .evaluate(output, Time::ZERO, Limits::default(), &|| Ok(()))
            .unwrap(),
        Value::Primitive(Literal::Scalar { value: 6. })
    );
    for invalid in [
        source.replace("motion 1;", "motion 2;"),
        source.replace("expr(later * 2)", "expr(missing * 2)"),
        source.replace("value(3)", "link(3)"),
        source.replace("\"value\" = value(3);", "\"bad\" = value(3);"),
        source.replace(
            "\"value\" = value(3);",
            "\"value\" = value(3); \"value\" = value(4);",
        ),
        source.replace("\"Forward\" clock", "\"Missing\" clock"),
        format!("{source} trailing"),
    ] {
        let error = fold_motion::source::parse(&invalid).err().unwrap();
        assert!(error.span.start <= error.span.end && error.span.end <= invalid.len());
    }
    let wrong_type = source.replace("value(3)", "value(3px)");
    let parsed = fold_motion::source::parse(&wrong_type).unwrap();
    assert!(
        parsed
            .construction
            .validate_semantics()
            .unwrap_err()
            .contains("declares")
    );
}

#[test]
fn source_transactions_retain_invalid_buffers_and_commit_one_undo_group() {
    use fold_motion::editing::{self, Scope, SourceBuffer};
    use fold_project::{EditBatch, Mutation, Project};
    let construction = fixture();
    let document = fold_foundation::DocumentId::new();
    let mut native = construction.document(document).unwrap();
    native
        .extensions
        .insert("wrapper".into(), serde_json::json!({"keep":true}));
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(native)],
        })
        .unwrap();
    let before = project.snapshot();
    let mut buffer = SourceBuffer::default();
    assert!(
        buffer
            .propose("motion invalid".into(), &before, document, Scope::Document)
            .is_err()
    );
    assert_eq!(buffer.text(), "motion invalid");
    assert!(!buffer.diagnostics().is_empty());
    assert_eq!(project.snapshot().revision(), before.revision());
    let mut changed = construction.clone();
    changed.definitions[0].inputs[0].default = Literal::Distance {
        value: 140.,
        unit: Unit::Canvas,
    };
    let mut outside = changed.clone();
    outside.root.name = "Outside scope".into();
    assert!(matches!(
        buffer.propose(
            fold_motion::source::canonical(&outside).unwrap(),
            &before,
            document,
            Scope::Definition(construction.definitions[0].id)
        ),
        Err(editing::Error::OutsideScope)
    ));
    let mut locked = changed.clone();
    locked.definitions[0].inputs[2].default = Literal::Integer { value: 99 };
    let locked_source = fold_motion::source::canonical(&locked).unwrap();
    assert!(matches!(
        buffer.propose(locked_source.clone(), &before, document, Scope::Document),
        Err(editing::Error::Locked(_))
    ));
    assert_eq!(buffer.text(), locked_source);
    assert_eq!(project.snapshot().revision(), before.revision());
    let text = fold_motion::source::canonical(&changed).unwrap();
    let candidate = buffer
        .propose(
            text.clone(),
            &before,
            document,
            Scope::Definition(construction.definitions[0].id),
        )
        .unwrap();
    assert_eq!(buffer.text(), text);
    assert!(buffer.diagnostics().is_empty());
    assert_eq!(project.snapshot().revision(), before.revision());
    let committed = candidate.apply(&mut project).unwrap();
    assert_eq!(committed.revision().0, before.revision().0 + 1);
    assert_eq!(project.undo_label(), Some("Edit Motion construction"));
    assert_eq!(
        committed.state().documents[&document].extensions,
        before.state().documents[&document].extensions
    );
    assert_eq!(
        Construction::from_document(&committed.state().documents[&document]).unwrap(),
        changed
    );
    project.undo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        construction
    );
    project.redo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        changed
    );
    let current = project.snapshot();
    let mut later = changed.clone();
    later.definitions[0].name = "Later".into();
    let stale = editing::prepare(&current, document, Scope::Document, later).unwrap();
    project.undo().unwrap();
    assert!(matches!(
        stale.apply(&mut project),
        Err(editing::Error::Conflict)
    ));
}

#[test]
fn protection_blocks_parent_override_context_and_group_bypasses() {
    use fold_motion::protection;
    let before = fixture();
    let count = before.definitions[0].inputs[2].id;
    let mut after = before.clone();
    after.definitions[0].inputs[2].protected = false;
    assert_eq!(protection::validate(&before, &after).unwrap_err().id, count);
    let mut after = before.clone();
    after.root.overrides.insert(
        count,
        Parameter::new(literal(Literal::Integer { value: 999 })).unwrap(),
    );
    assert_eq!(protection::validate(&before, &after).unwrap_err().id, count);
    let mut after = before.clone();
    after.definitions[0].id = ObjectId::new();
    after.root.definition = after.definitions[0].id;
    assert!(protection::validate(&before, &after).is_err());
    let mut after = before.clone();
    let extra = Definition {
        id: ObjectId::new(),
        name: "Replacement".into(),
        inputs: vec![],
        nodes: vec![],
        instances: vec![],
        outputs: vec![],
        extensions: Metadata::new(),
    };
    after.root.definition = extra.id;
    after.definitions.push(extra);
    assert_eq!(
        protection::validate(&before, &after).unwrap_err().id,
        before.root.id
    );
    let mut before = before;
    let parameter = before.definitions[0].nodes[0]
        .parameters
        .get_mut("value")
        .unwrap();
    parameter.protected = true;
    let parameter_id = parameter.id;
    let mut after = before.clone();
    after.definitions[0].nodes[0].version = 2;
    assert_eq!(
        protection::validate(&before, &after).unwrap_err().id,
        parameter_id
    );
    let size = before.definitions[0].inputs[0].id;
    before.presentation.groups.push(fold_motion::model::Group {
        id: ObjectId::new(),
        name: "Locked size".into(),
        members: vec![size],
        locked: true,
        extensions: Metadata::new(),
    });
    let mut after = before.clone();
    after.root.overrides.insert(
        size,
        Parameter::new(literal(Literal::Distance {
            value: 10.,
            unit: Unit::Canvas,
        }))
        .unwrap(),
    );
    assert_eq!(protection::validate(&before, &after).unwrap_err().id, size);
    let mut after = before.clone();
    after.presentation.groups.clear();
    assert!(protection::validate(&before, &after).is_err());
    let mut after = before.clone();
    after.definitions[0].name = "Renamed without replacement".into();
    protection::validate(&before, &after).unwrap();
}

#[test]
fn explicit_fragment_mapping_preserves_protected_inputs_and_artist_layout() {
    use fold_motion::{editing, reconcile, source};
    use fold_project::{EditBatch, Mutation, Project};
    let context = fixture();
    let asset = r#"asset "Counter" {
 input "count" @{"protected":true} : {"kind":"integer"} = 1i;
 let "answer" = "fold.motion.expression"."value" @1 ("value" = expr(count * 2i););
 output "main" : {"kind":"signal","value":{"kind":"integer"}} = answer;
}"#;
    let text = format!(
        "motion 1; seed 1; canvas {}; {asset} root \"main\" = \"Counter\" clock {} {{}}; presentation {}; extensions {{}};",
        serde_json::to_string(&context.canvas).unwrap(),
        serde_json::to_string(&Clock::default()).unwrap(),
        serde_json::to_string(&Presentation::default()).unwrap()
    );
    let mut before = source::parse(&text).unwrap().construction;
    let target = before.definitions[0].id;
    let original = &before.definitions[0];
    let count = original.inputs[0].id;
    let node = original.nodes[0].id;
    before.presentation.positions.insert(count, [10., 20.]);
    before.presentation.positions.insert(node, [30., 40.]);
    before
        .presentation
        .annotations
        .insert(node, "Artist annotation".into());
    let fragment_text = asset.replace("count * 2i", "count * 3i");
    let mut fragment = source::parse_fragment(&fragment_text, &before, target).unwrap();
    let old = &before.definitions[0];
    let new = &fragment.definition;
    let old_parameter = &old.nodes[0].parameters["value"];
    let new_parameter = &new.nodes[0].parameters["value"];
    let Binding::Expression {
        expression: old_expression,
    } = old_parameter.binding().unwrap()
    else {
        unreachable!()
    };
    let Binding::Expression {
        expression: new_expression,
    } = new_parameter.binding().unwrap()
    else {
        unreachable!()
    };
    let mapping = reconcile::IdentityMap::from([
        (new.id, old.id),
        (new.inputs[0].id, old.inputs[0].id),
        (new.nodes[0].id, old.nodes[0].id),
        (new_parameter.id, old_parameter.id),
        (new.outputs[0].id, old.outputs[0].id),
        (new.outputs[0].value.id, old.outputs[0].value.id),
        (
            new_expression.edges().unwrap()[0].id,
            old_expression.edges().unwrap()[0].id,
        ),
    ]);
    let opaque = serde_json::to_value(fragment.definition.nodes[0].id).unwrap();
    fragment
        .definition
        .extensions
        .insert("opaque_uuid_text".into(), opaque.clone());
    let document = fold_foundation::DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let snapshot = project.snapshot();
    assert!(
        editing::prepare_fragment(
            &snapshot,
            document,
            target,
            &fragment,
            &reconcile::IdentityMap::new()
        )
        .is_err()
    );
    let candidate =
        editing::prepare_fragment(&snapshot, document, target, &fragment, &mapping).unwrap();
    assert!(candidate.source_spans().contains_key(&node));
    assert!(
        !candidate
            .source_spans()
            .contains_key(&fragment.definition.nodes[0].id)
    );
    let after = Construction::from_document(candidate.document()).unwrap();
    assert_eq!(after.presentation, before.presentation);
    assert_eq!(
        after.definitions[0].inputs[0],
        before.definitions[0].inputs[0]
    );
    assert_eq!(after.definitions[0].extensions["opaque_uuid_text"], opaque);
    assert_eq!(
        reconcile::identities(&after.definitions[0]).unwrap(),
        reconcile::identities(&before.definitions[0]).unwrap()
    );
    let output = after.definitions[0].outputs[0].id;
    assert_eq!(
        Prepared::new(after.clone())
            .unwrap()
            .evaluate(output, Time::ZERO, Limits::default(), &|| Ok(()))
            .unwrap(),
        Value::Primitive(Literal::Integer { value: 3 })
    );
    candidate.apply(&mut project).unwrap();
    project.undo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        before
    );
    let mut wrong = mapping.clone();
    wrong.insert(fragment.definition.nodes[0].id, count);
    assert!(reconcile::definition(&before.definitions[0], &fragment.definition, &wrong).is_err());
    let mut duplicate = mapping.clone();
    duplicate.insert(
        fragment.definition.outputs[0].value.id,
        old_expression.edges().unwrap()[0].id,
    );
    assert!(
        reconcile::definition(&before.definitions[0], &fragment.definition, &duplicate)
            .unwrap_err()
            .contains("one-to-one")
    );
    let mut outside = mapping.clone();
    outside.insert(ObjectId::new(), before.definitions[0].id);
    assert!(
        reconcile::definition(&before.definitions[0], &fragment.definition, &outside)
            .unwrap_err()
            .contains("outside the fragment")
    );
    let mut locked = fragment;
    locked.definition.inputs[0].default = Literal::Integer { value: 99 };
    assert!(matches!(
        editing::prepare_fragment(&project.snapshot(), document, target, &locked, &mapping),
        Err(editing::Error::Locked(_))
    ));
}

#[test]
fn fragment_removal_cleans_only_deleted_layout_and_respects_locked_groups() {
    use fold_motion::{editing, reconcile, source};
    use fold_project::{EditBatch, Mutation, Project};
    let mut before = fixture();
    let unused = node("scene", "ordered", "unused", vec![]);
    let removed = unused.id;
    before.definitions[0].nodes.push(unused);
    before.presentation.positions.insert(removed, [50., 60.]);
    before
        .presentation
        .annotations
        .insert(removed, "Removed with this record".into());
    let retained = before.definitions[0].nodes[0].id;
    before.presentation.positions.insert(retained, [70., 80.]);
    before.presentation.groups.push(fold_motion::model::Group {
        id: ObjectId::new(),
        name: "Loose".into(),
        members: vec![removed, retained],
        locked: false,
        extensions: Metadata::new(),
    });
    let target = before.definitions[0].id;
    let original_text = source::canonical_fragment(&before, target).unwrap();
    assert_eq!(
        source::parse_fragment(&original_text, &before, target)
            .unwrap()
            .definition,
        before.definitions[0]
    );
    let mut draft = before.clone();
    draft.definitions[0].nodes.pop();
    draft.presentation = Presentation::default();
    let text = source::canonical_fragment(&draft, target).unwrap();
    let fragment = source::parse_fragment(&text, &before, target).unwrap();
    let document = fold_foundation::DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let mut buffer = editing::SourceBuffer::default();
    let candidate = buffer
        .propose_fragment(
            text,
            &project.snapshot(),
            document,
            target,
            &reconcile::IdentityMap::new(),
        )
        .unwrap();
    let after = Construction::from_document(candidate.document()).unwrap();
    assert_eq!(
        after.presentation.positions.get(&retained),
        Some(&[70., 80.])
    );
    assert!(!after.presentation.positions.contains_key(&removed));
    assert!(!after.presentation.annotations.contains_key(&removed));
    assert_eq!(after.presentation.groups[0].members, vec![retained]);
    before.presentation.groups[0].locked = true;
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    assert!(matches!(
        editing::prepare_fragment(
            &project.snapshot(),
            document,
            target,
            &fragment,
            &reconcile::IdentityMap::new()
        ),
        Err(editing::Error::Locked(_))
    ));
}

#[test]
fn rebase_preserves_artist_keys_layout_and_disjoint_controls() {
    use fold_motion::editing::{self, Scope};
    use fold_project::{EditBatch, Mutation, Project};
    let before = fixture();
    let document = fold_foundation::DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let base = project.snapshot();
    let mut proposed = before.clone();
    proposed.definitions[0].inputs[0].default = Literal::Distance {
        value: 140.,
        unit: Unit::Canvas,
    };
    let mut source_buffer = editing::SourceBuffer::default();
    let candidate = source_buffer
        .propose(
            fold_motion::source::canonical(&proposed).unwrap(),
            &base,
            document,
            Scope::Definition(before.definitions[0].id),
        )
        .unwrap();
    assert!(!candidate.source_spans().is_empty());
    let mut artist = before.clone();
    artist.definitions[0].inputs[1].default = Literal::Color {
        value: [0., 1., 0., 1.],
        encoding: Encoding::LinearRec709,
        alpha: Alpha::Straight,
    };
    let keys = &mut artist.definitions[0].nodes[3]
        .parameters
        .get_mut("track")
        .unwrap()
        .value;
    let mut decoded: Binding = serde_json::from_value(keys.clone()).unwrap();
    let Binding::Track { track } = &mut decoded else {
        unreachable!()
    };
    track.keys[1].value = Literal::Scalar { value: 0.8 };
    *keys = serde_json::to_value(decoded).unwrap();
    artist
        .presentation
        .positions
        .insert(artist.definitions[0].nodes[3].id, [88., 99.]);
    editing::prepare(&base, document, Scope::Document, artist.clone())
        .unwrap()
        .apply(&mut project)
        .unwrap();
    let live = project.snapshot();
    let rebased = candidate.rebase(&live).unwrap();
    assert!(rebased.source_spans().is_empty());
    let changes = rebased.diff().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].native_id,
        Some(before.definitions[0].inputs[0].id)
    );
    assert_eq!(changes, candidate.diff().unwrap());
    let merged = Construction::from_document(rebased.document()).unwrap();
    assert_eq!(
        merged.definitions[0].inputs[0].default,
        Literal::Distance {
            value: 140.,
            unit: Unit::Canvas
        }
    );
    assert_eq!(
        merged.definitions[0].inputs[1],
        artist.definitions[0].inputs[1]
    );
    assert_eq!(
        merged.definitions[0].nodes[3],
        artist.definitions[0].nodes[3]
    );
    assert_eq!(merged.presentation, artist.presentation);
    assert!(matches!(
        candidate.apply(&mut project),
        Err(editing::Error::Conflict)
    ));
    assert_eq!(project.snapshot().revision(), live.revision());
    rebased.apply(&mut project).unwrap();
    project.undo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        artist
    );
}

#[test]
fn native_merge_distinguishes_key_identity_parent_deletion_and_contract_changes() {
    use fold_motion::merge;
    let base = fixture();
    let mut proposed = base.clone();
    let mut current = base.clone();
    fn set_key(c: &mut Construction, index: usize, value: f64) {
        let parameter = c.definitions[0].nodes[3]
            .parameters
            .get_mut("track")
            .unwrap();
        let Binding::Track { mut track } = parameter.binding().unwrap() else {
            unreachable!()
        };
        track.keys[index].value = Literal::Scalar { value };
        parameter.value = serde_json::to_value(Binding::Track { track }).unwrap();
    }
    set_key(&mut proposed, 1, 0.8);
    set_key(&mut current, 2, 0.4);
    let merged = merge::construction(&base, &proposed, &current).unwrap();
    merged.validate_semantics().unwrap();
    let Binding::Track { track } = merged.definitions[0].nodes[3].parameters["track"]
        .binding()
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(track.keys[1].value, Literal::Scalar { value: 0.8 });
    assert_eq!(track.keys[2].value, Literal::Scalar { value: 0.4 });
    set_key(&mut current, 1, 0.8);
    let conflicts = merge::construction(&base, &proposed, &current).unwrap_err();
    assert!(
        conflicts
            .iter()
            .any(|c| c.native_id == Some(track.keys[1].id) && c.reason.contains("same key"))
    );
    let mut removed = base.clone();
    removed.definitions[0].nodes.remove(3);
    assert!(
        merge::construction(&base, &removed, &current)
            .unwrap_err()
            .iter()
            .any(|c| c.reason.contains("descendant"))
    );
    let mut changed_contract = base.clone();
    changed_contract.definitions[0].nodes[2].mode = "cubic_out".into();
    let mut changed_value = base.clone();
    changed_value.definitions[0].nodes[2]
        .parameters
        .get_mut("duration")
        .unwrap()
        .value = serde_json::to_value(literal(Literal::Duration {
        value: Time::new(2, 1).unwrap(),
    }))
    .unwrap();
    assert!(
        merge::construction(&base, &changed_value, &changed_contract)
            .unwrap_err()
            .iter()
            .any(|c| c.native_id == Some(base.definitions[0].nodes[2].id)
                && c.reason.contains("contract"))
    );
}

#[test]
fn rebase_checks_current_locks_and_merged_dependencies() {
    use fold_motion::editing::{self, Scope};
    use fold_project::{EditBatch, Mutation, Project};
    let before = fixture();
    let document = fold_foundation::DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let base = project.snapshot();
    let mut proposed = before.clone();
    proposed.definitions[0].inputs[0].default = Literal::Distance {
        value: 120.,
        unit: Unit::Canvas,
    };
    let candidate = editing::prepare(&base, document, Scope::Document, proposed).unwrap();
    let mut locked = before.clone();
    locked.definitions[0].inputs[0].protected = true;
    editing::prepare(&base, document, Scope::Document, locked)
        .unwrap()
        .apply(&mut project)
        .unwrap();
    assert!(matches!(
        candidate.rebase(&project.snapshot()),
        Err(editing::Error::Locked(_))
    ));
    // A concurrent newly connected node must not retain a dangling reference after
    // rebasing a deletion, even though the records themselves are disjoint.
    let mut before = fixture();
    let spare = node("time", "scene", "spare", vec![]);
    let spare_id = spare.id;
    before.definitions[0].nodes.push(spare);
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let base = project.snapshot();
    let mut removed = before.clone();
    removed.definitions[0].nodes.pop();
    let candidate = editing::prepare(&base, document, Scope::Document, removed).unwrap();
    let mut connected = before.clone();
    connected.definitions[0].nodes.push(node(
        "expression",
        "value",
        "new dependency",
        vec![("value", link(spare_id))],
    ));
    editing::prepare(&base, document, Scope::Document, connected)
        .unwrap()
        .apply(&mut project)
        .unwrap();
    assert!(matches!(
        candidate.rebase(&project.snapshot()),
        Err(editing::Error::RebaseConflict(_))
    ));
}

#[test]
fn native_merge_orders_independent_insertions_and_keeps_unknown_payloads_atomic() {
    use fold_motion::merge;
    let base = fixture();
    let mut proposed = base.clone();
    let mut current = base.clone();
    proposed.definitions[0]
        .nodes
        .push(node("time", "scene", "agent clock", vec![]));
    current.definitions[0]
        .nodes
        .push(node("time", "asset", "artist clock", vec![]));
    let merged = merge::construction(&base, &proposed, &current).unwrap();
    merged.validate_semantics().unwrap();
    assert_eq!(
        merged,
        merge::construction(&base, &current, &proposed).unwrap()
    );
    assert_eq!(
        merged.definitions[0].nodes.len(),
        base.definitions[0].nodes.len() + 2
    );
    let mut proposed = base.clone();
    let mut current = base.clone();
    proposed.definitions[0].nodes.swap(0, 1);
    current.definitions[0].nodes.swap(1, 2);
    assert!(
        merge::construction(&base, &proposed, &current)
            .unwrap_err()
            .iter()
            .any(|c| c.path.last() == Some(&merge::Part::Order))
    );
    let mut base = base;
    base.definitions[0]
        .extensions
        .insert("vendor".into(), serde_json::json!({"left":1,"right":1}));
    let mut proposed = base.clone();
    let mut current = base.clone();
    proposed.definitions[0]
        .extensions
        .insert("vendor".into(), serde_json::json!({"left":2,"right":1}));
    current.definitions[0]
        .extensions
        .insert("vendor".into(), serde_json::json!({"left":1,"right":2}));
    let conflicts = merge::construction(&base, &proposed, &current).unwrap_err();
    assert!(
        conflicts
            .iter()
            .any(|c| c.path.last() == Some(&merge::Part::Field("vendor".into())))
    );
    let mut proposed = base.clone();
    let mut current = base.clone();
    fn insert_key(c: &mut Construction, time: Time, value: f64) {
        let parameter = c.definitions[0].nodes[3]
            .parameters
            .get_mut("track")
            .unwrap();
        let Binding::Track { mut track } = parameter.binding().unwrap() else {
            unreachable!()
        };
        track.keys.insert(
            1,
            Key {
                id: ObjectId::new(),
                time,
                value: Literal::Scalar { value },
                interpolation: Interpolation::Linear,
                handles: Default::default(),
                extensions: Metadata::new(),
            },
        );
        parameter.value = serde_json::to_value(Binding::Track { track }).unwrap();
    }
    insert_key(&mut proposed, Time::new(1, 5).unwrap(), 0.4);
    insert_key(&mut current, Time::new(1, 10).unwrap(), 0.2);
    let merged = merge::construction(&base, &proposed, &current).unwrap();
    merged.validate_semantics().unwrap();
    let Binding::Track { track } = merged.definitions[0].nodes[3].parameters["track"]
        .binding()
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(track.keys[1].time, Time::new(1, 10).unwrap());
    assert_eq!(track.keys[2].time, Time::new(1, 5).unwrap());
}

#[test]
fn semantic_diff_preserves_native_boundaries_and_connection_intent() {
    use fold_motion::{
        diff::{self, Kind},
        merge::Part,
    };
    let before = fixture();
    let mut after = before.clone();
    let definition = &mut after.definitions[0];
    let input_id = definition.inputs[0].id;
    definition.inputs[0].protected = true;
    definition.inputs[0].default = Literal::Distance {
        value: 125.,
        unit: Unit::Canvas,
    };
    let removed = definition.inputs.pop().unwrap().id;
    let added = input("extra", Literal::Scalar { value: 1. });
    let added_id = added.id;
    definition.inputs.push(added);
    let expression = &mut definition.nodes[0]
        .parameters
        .get_mut("value")
        .unwrap()
        .value;
    let mut binding: Binding = serde_json::from_value(expression.clone()).unwrap();
    if let Binding::Expression {
        expression: Expr::Binary { left, .. },
    } = &mut binding
    {
        if let Expr::Reference { reference } = left.as_mut() {
            reference.target = Reference::Input { input: added_id };
        } else {
            panic!("Expected reference operand")
        }
    } else {
        panic!("Expected expression binding")
    }
    *expression = serde_json::to_value(binding).unwrap();
    after.extensions.insert(
        "vendor".into(),
        serde_json::json!({"nested": {"protected": true}}),
    );
    let parameter = after.definitions[0].nodes[3]
        .parameters
        .get_mut("track")
        .unwrap();
    let Binding::Track { mut track } = parameter.binding().unwrap() else {
        unreachable!()
    };
    let key_id = track.keys[1].id;
    track.keys[1].value = Literal::Scalar { value: 0.75 };
    parameter.value = serde_json::to_value(Binding::Track { track }).unwrap();
    let changes = diff::construction(&before, &after).unwrap();
    assert!(
        changes
            .iter()
            .any(|c| c.kind == Kind::Protection && c.native_id == Some(input_id))
    );
    assert!(
        changes
            .iter()
            .any(|c| c.kind == Kind::Value && c.native_id == Some(input_id))
    );
    assert_eq!(
        changes.iter().filter(|c| c.kind == Kind::Removal).count(),
        1
    );
    assert!(
        changes
            .iter()
            .any(|c| c.kind == Kind::Removal && c.native_id == Some(removed))
    );
    assert!(
        changes
            .iter()
            .any(|c| c.kind == Kind::Addition && c.native_id == Some(added_id))
    );
    assert_eq!(
        changes
            .iter()
            .filter(|c| c.kind == Kind::Connection)
            .count(),
        1
    );
    assert!(changes.iter().any(|c| c.kind == Kind::Order));
    let vendor = changes
        .iter()
        .find(|c| c.path == vec![Part::Field("vendor".into())])
        .unwrap();
    assert_eq!(vendor.after, after.extensions.get("vendor").cloned());
    let key = changes
        .iter()
        .find(|c| c.native_id == Some(key_id))
        .unwrap();
    assert_eq!(key.kind, Kind::Value);
    assert_eq!(key.path.last(), Some(&Part::Element(key_id)));
    assert!(key.after.as_ref().unwrap().get("time").is_some());
    assert_eq!(changes, diff::construction(&before, &after).unwrap());
    assert!(diff::construction(&before, &before).unwrap().is_empty());
}

#[test]
fn semantic_diff_keeps_unknown_parameters_and_extensions_atomic() {
    use fold_motion::{
        diff::{self, Kind},
        merge::Part,
    };
    let mut before = fixture();
    before.definitions[0].nodes[0].operation = "vendor.future".into();
    before.definitions[0].nodes[0].extensions.insert(
        "vendor".into(),
        serde_json::json!({"locked": false, "child": 1}),
    );
    let mut after = before.clone();
    let node = &mut after.definitions[0].nodes[0];
    let node_id = node.id;
    node.parameters.get_mut("value").unwrap().value = serde_json::json!({"unknown": true});
    node.extensions.get_mut("vendor").unwrap()["locked"] = serde_json::json!(true);
    let changes = diff::construction(&before, &after).unwrap();
    assert_eq!(changes.len(), 2);
    assert!(
        changes
            .iter()
            .all(|c| c.native_id == Some(node_id) && c.kind == Kind::Value)
    );
    assert!(
        changes
            .iter()
            .any(|c| c.path.last() == Some(&Part::Field("parameters".into())))
    );
    assert!(
        changes
            .iter()
            .any(|c| c.path.last() == Some(&Part::Field("vendor".into())))
    );
}

#[test]
fn structured_control_and_key_edits_share_candidate_undo_and_rebase() {
    use fold_motion::{
        editing::{self, Scope},
        merge::Part,
        patch::{Patch, Slot},
    };
    use fold_project::{EditBatch, Mutation, Project};
    let before = fixture();
    let def = &before.definitions[0];
    let document = fold_foundation::DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let base = project.snapshot();
    let input_path = vec![
        Part::Field("definitions".into()),
        Part::Element(def.id),
        Part::Field("inputs".into()),
        Part::Element(def.inputs[0].id),
        Part::Field("default".into()),
    ];
    let present = |value| Slot::Present { value };
    let control = Patch {
        path: input_path.clone(),
        expected: present(serde_json::to_value(&def.inputs[0].default).unwrap()),
        value: present(
            serde_json::to_value(Literal::Distance {
                value: 160.,
                unit: Unit::Canvas,
            })
            .unwrap(),
        ),
    };
    let parameter = &def.nodes[3].parameters["track"];
    let Binding::Track { track } = parameter.binding().unwrap() else {
        unreachable!()
    };
    let mut key = track.keys[1].clone();
    key.value = Literal::Scalar { value: 0.65 };
    let key_patch = Patch {
        path: vec![
            Part::Field("definitions".into()),
            Part::Element(def.id),
            Part::Field("nodes".into()),
            Part::Element(def.nodes[3].id),
            Part::Field("parameters".into()),
            Part::Field("track".into()),
            Part::Field("value".into()),
            Part::Field("track".into()),
            Part::Field("keys".into()),
            Part::Element(key.id),
        ],
        expected: present(serde_json::to_value(&track.keys[1]).unwrap()),
        value: present(serde_json::to_value(&key).unwrap()),
    };
    let mut partial_key = key_patch.clone();
    partial_key.path.push(Part::Field("value".into()));
    assert!(matches!(
        editing::prepare_patches(&base, document, Scope::Document, &[partial_key]),
        Err(editing::Error::Patch(
            fold_motion::patch::Error::Invalid { .. }
        ))
    ));
    let candidate = editing::prepare_patches(
        &base,
        document,
        Scope::Definition(def.id),
        &[control.clone(), key_patch],
    )
    .unwrap();
    assert_eq!(candidate.diff().unwrap().len(), 2);
    let mut artist = before.clone();
    artist.definitions[0].inputs[1].name = "Artist accent".into();
    editing::prepare(&base, document, Scope::Document, artist.clone())
        .unwrap()
        .apply(&mut project)
        .unwrap();
    let rebased = candidate.rebase(&project.snapshot()).unwrap();
    rebased.apply(&mut project).unwrap();
    let applied =
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap();
    assert_eq!(applied.definitions[0].inputs[1].name, "Artist accent");
    let Binding::Track { track: result } = applied.definitions[0].nodes[3].parameters["track"]
        .binding()
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(result.keys[1], key);
    let unchanged_revision = project.snapshot().revision();
    assert!(matches!(
        editing::prepare_patches(&project.snapshot(), document, Scope::Document, &[control]),
        Err(editing::Error::Patch(fold_motion::patch::Error::Conflict {
            index: 0,
            ..
        }))
    ));
    assert_eq!(project.snapshot().revision(), unchanged_revision);
    project.undo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        artist
    );
    project.redo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        applied
    );
    // Native input type checking still rejects a syntactically valid patch.
    let wrong_type = Patch {
        path: input_path,
        expected: present(serde_json::to_value(&def.inputs[0].default).unwrap()),
        value: present(serde_json::to_value(Literal::Bool { value: true }).unwrap()),
    };
    assert!(matches!(
        editing::prepare_patches(&base, document, Scope::Document, &[wrong_type]),
        Err(editing::Error::Patch(fold_motion::patch::Error::Validation(diagnostic)))
            if diagnostic.kind == fold_motion::validation::Kind::Type
                && diagnostic.native_ids == vec![def.inputs[0].id]
    ));
}

#[test]
fn structured_patches_reject_opaque_descent_overlap_and_identity_substitution() {
    use fold_motion::{
        merge::Part,
        patch::{self, Patch, Slot},
    };
    let mut before = fixture();
    before
        .extensions
        .insert("vendor".into(), serde_json::json!({"a": 1}));
    let def = &before.definitions[0];
    let vendor = Patch {
        path: vec![Part::Field("vendor".into())],
        expected: Slot::Present {
            value: serde_json::json!({"a": 1}),
        },
        value: Slot::Present {
            value: serde_json::Value::Null,
        },
    };
    let after = patch::construction(&before, std::slice::from_ref(&vendor)).unwrap();
    assert_eq!(after.extensions["vendor"], serde_json::Value::Null);
    let roundtrip: Patch = serde_json::from_value(serde_json::to_value(&vendor).unwrap()).unwrap();
    assert_eq!(roundtrip, vendor);
    let mut removal = vendor.clone();
    removal.expected = Slot::Present {
        value: serde_json::Value::Null,
    };
    removal.value = Slot::Absent {};
    assert!(
        !patch::construction(&after, &[removal])
            .unwrap()
            .extensions
            .contains_key("vendor")
    );
    let mut child = vendor.clone();
    child.path.push(Part::Field("a".into()));
    child.expected = Slot::Present {
        value: serde_json::json!(1),
    };
    assert!(matches!(
        patch::construction(&before, &[child]),
        Err(patch::Error::Invalid { .. })
    ));
    assert!(matches!(
        patch::construction(&before, &[vendor.clone(), vendor.clone()]),
        Err(patch::Error::Invalid { index: Some(1), .. })
    ));
    assert!(matches!(
        patch::construction(&before, &vec![vendor; 129]),
        Err(patch::Error::Invalid { index: None, .. })
    ));
    let mut substitute = def.nodes[0].clone();
    substitute.id = ObjectId::new();
    let replacement = Patch {
        path: vec![
            Part::Field("definitions".into()),
            Part::Element(def.id),
            Part::Field("nodes".into()),
            Part::Element(def.nodes[0].id),
        ],
        expected: Slot::Present {
            value: serde_json::to_value(&def.nodes[0]).unwrap(),
        },
        value: Slot::Present {
            value: serde_json::to_value(substitute).unwrap(),
        },
    };
    assert!(matches!(
        patch::construction(&before, &[replacement]),
        Err(patch::Error::Invalid { .. })
    ));
}

#[test]
fn structured_collection_insertion_and_explicit_order_use_one_base() {
    use fold_motion::{
        merge::Part,
        patch::{self, Patch, Slot},
    };
    let before = fixture();
    let def = &before.definitions[0];
    let node = node("time", "scene", "new clock", vec![]);
    let mut path = vec![
        Part::Field("definitions".into()),
        Part::Element(def.id),
        Part::Field("nodes".into()),
    ];
    path.push(Part::Element(node.id));
    let add = Patch {
        path: path.clone(),
        expected: Slot::Absent {},
        value: Slot::Present {
            value: serde_json::to_value(&node).unwrap(),
        },
    };
    path.pop();
    path.push(Part::Order);
    let old: Vec<_> = def.nodes.iter().map(|n| n.id).collect();
    let mut new = old.clone();
    new.insert(0, node.id);
    let reorder = Patch {
        path,
        expected: Slot::Present {
            value: serde_json::json!(old),
        },
        value: Slot::Present {
            value: serde_json::json!(new),
        },
    };
    let after = patch::construction(&before, &[reorder.clone(), add.clone()]).unwrap();
    assert_eq!(after.definitions[0].nodes[0], node);
    assert_eq!(
        after,
        patch::construction(&before, &[add.clone(), reorder.clone()]).unwrap()
    );
    let mut invalid_order = reorder;
    invalid_order.value = Slot::Present {
        value: serde_json::json!([node.id, node.id]),
    };
    assert!(matches!(
        patch::construction(&before, &[add, invalid_order]),
        Err(patch::Error::Invalid { .. })
    ));
    assert_eq!(before.definitions[0].nodes.len(), def.nodes.len());
}

#[test]
fn structured_patches_cannot_bypass_locks_scope_or_admission_limits() {
    use fold_motion::{
        editing::{self, Scope},
        merge::Part,
        patch::{self, Patch, Slot},
    };
    use fold_project::{EditBatch, Mutation, Project};
    let before = fixture();
    let def = &before.definitions[0];
    let document = fold_foundation::DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let base = project.snapshot();
    let patch = Patch {
        path: vec![
            Part::Field("definitions".into()),
            Part::Element(def.id),
            Part::Field("inputs".into()),
            Part::Element(def.inputs[2].id),
            Part::Field("protected".into()),
        ],
        expected: Slot::Present {
            value: serde_json::json!(true),
        },
        value: Slot::Present {
            value: serde_json::json!(false),
        },
    };
    let seed = Patch {
        path: vec![Part::Field("seed".into())],
        expected: Slot::Present {
            value: serde_json::json!(before.seed),
        },
        value: Slot::Present {
            value: serde_json::json!(42),
        },
    };
    assert!(matches!(
        editing::prepare_patches(&base, document, Scope::Document, &[seed.clone(), patch]),
        Err(editing::Error::Locked(_))
    ));
    assert!(matches!(
        editing::prepare_patches(
            &base,
            document,
            Scope::Definition(def.id),
            std::slice::from_ref(&seed)
        ),
        Err(editing::Error::OutsideScope)
    ));
    assert_eq!(project.snapshot().revision(), base.revision());
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        before
    );
    let oversized = Patch {
        path: vec![Part::Field("vendor".into())],
        expected: Slot::Absent {},
        value: Slot::Present {
            value: serde_json::json!("x".repeat(patch::MAX_BYTES)),
        },
    };
    assert!(matches!(
        patch::construction(&before, &[oversized]),
        Err(patch::Error::Invalid { index: None, .. })
    ));
    let mut wire = serde_json::to_value(seed).unwrap();
    wire["legacy_value"] = serde_json::json!(0);
    assert!(serde_json::from_value::<Patch>(wire).is_err());
    assert!(
        serde_json::from_value::<Slot>(serde_json::json!({"kind": "absent", "value": null}))
            .is_err()
    );
}

#[test]
fn artist_unlock_only_clears_selected_flags_and_is_revision_bound_and_undoable() {
    use fold_motion::{
        editing::{self, Scope},
        model::Group,
    };
    use fold_project::{EditBatch, Mutation, Project};
    let mut before = fixture();
    let input_id = before.definitions[0].inputs[2].id;
    let parameter = before.definitions[0].nodes[0]
        .parameters
        .get_mut("value")
        .unwrap();
    parameter.protected = true;
    let group_id = ObjectId::new();
    before.presentation.groups.push(Group {
        id: group_id,
        name: "Locked shape".into(),
        members: vec![before.definitions[0].nodes[1].id],
        locked: true,
        extensions: Metadata::new(),
    });
    let inherited_parameter = before.definitions[0].nodes[1].parameters["width"].id;
    assert!(
        fold_motion::protection::locked_ids(&before)
            .unwrap()
            .contains(&inherited_parameter)
    );
    let mut project = Project::new(8);
    let document = fold_foundation::DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let base = project.snapshot();
    assert!(matches!(
        editing::unlock_for_artist(
            &mut project,
            base.revision(),
            document,
            &[input_id, ObjectId::new()]
        ),
        Err(editing::Error::Invalid(_))
    ));
    assert_eq!(project.snapshot().revision(), base.revision());
    let mut expected = before.clone();
    expected.definitions[0].inputs[2].protected = false;
    expected.presentation.groups[0].locked = false;
    assert!(
        !fold_motion::protection::locked_ids(&expected)
            .unwrap()
            .contains(&inherited_parameter)
    );
    assert!(matches!(
        editing::prepare(&base, document, Scope::Document, expected.clone()),
        Err(editing::Error::Locked(_))
    ));
    editing::unlock_for_artist(
        &mut project,
        base.revision(),
        document,
        &[input_id, group_id],
    )
    .unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        expected
    );
    assert!(matches!(
        editing::unlock_for_artist(&mut project, base.revision(), document, &[input_id]),
        Err(editing::Error::Conflict)
    ));
    let current_revision = project.snapshot().revision();
    assert!(matches!(
        editing::unlock_for_artist(&mut project, current_revision, document, &[input_id]),
        Err(editing::Error::NoChange)
    ));
    project.undo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        before
    );
    project.redo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        expected
    );
    // Lock flags can be cleared without interpreting unresolved operation payloads.
    let mut unresolved = before.clone();
    unresolved.definitions[0].nodes[0].operation = "vendor.future".into();
    let parameter_id = unresolved.definitions[0].nodes[0].parameters["value"].id;
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                unresolved.document(document).unwrap(),
            )],
        })
        .unwrap();
    let revision = project.snapshot().revision();
    editing::unlock_for_artist(&mut project, revision, document, &[parameter_id]).unwrap();
    unresolved.definitions[0].nodes[0]
        .parameters
        .get_mut("value")
        .unwrap()
        .protected = false;
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        unresolved
    );
}

#[test]
fn native_curve_projection_preserves_fractional_times_ids_units_and_extensions() {
    let mut before = fixture();
    before.definitions[0].nodes[1]
        .parameters
        .get_mut("height")
        .unwrap()
        .value = serde_json::to_value(literal(Literal::Distance {
        value: 100.,
        unit: Unit::Canvas,
    }))
    .unwrap();
    let parameter = before.definitions[0].nodes[3]
        .parameters
        .get_mut("track")
        .unwrap();
    let Binding::Track { mut track } = parameter.binding().unwrap() else {
        unreachable!()
    };
    track
        .extensions
        .insert("vendor".into(), serde_json::json!({"track": true}));
    track.keys[1]
        .extensions
        .insert("vendor".into(), serde_json::json!({"key": true}));
    let track_id = track.id;
    let key_id = track.keys[1].id;
    parameter.value = serde_json::to_value(Binding::Track { track }).unwrap();
    let mut channels = fold_motion::keyed::channels(&before, None).unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].target.path, track_id.to_string());
    channels[0].curve.keys[1].time = Time::new(1, 3).unwrap();
    channels[0].curve.keys[1].value = 0.75;
    channels[0].curve.keys[1].interpolation = fold_animation::Interpolation::Hold;
    let after = fold_motion::keyed::replace(&before, &channels).unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    let document = before.document(fold_foundation::DocumentId::new()).unwrap();
    assert_eq!(
        registry.animation_channels(&document, None).unwrap(),
        fold_motion::keyed::channels(&before, None).unwrap()
    );
    let edited = registry
        .edit_animation(&document, &channels, Time::ZERO)
        .unwrap();
    assert_eq!(Construction::from_document(&edited).unwrap(), after);
    let Binding::Track { track } = after.definitions[0].nodes[3].parameters["track"]
        .binding()
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(track.id, track_id);
    assert_eq!(track.keys[1].id, key_id);
    assert_eq!(track.keys[1].time, Time::new(1, 3).unwrap());
    assert_eq!(
        track.keys[1].extensions["vendor"],
        serde_json::json!({"key":true})
    );
    assert_eq!(
        track.extensions["vendor"],
        serde_json::json!({"track":true})
    );
    assert_eq!(
        track.sample(Time::new(2, 3).unwrap()).unwrap(),
        Literal::Scalar { value: 0.75 }
    );
    assert_eq!(
        fold_motion::keyed::channels(&after, None).unwrap(),
        channels
    );
    let distance_id = before.definitions[0].nodes[1].parameters["height"].id;
    let animated =
        fold_motion::keyed::animate(&before, distance_id, Time::new(-1, 3).unwrap()).unwrap();
    let mut distance = fold_motion::keyed::channels(&animated, None)
        .unwrap()
        .into_iter()
        .find(|c| c.property != channels[0].property)
        .unwrap();
    distance.curve.keys[0].value = 225.;
    let result = fold_motion::keyed::replace(&animated, &[distance]).unwrap();
    let Binding::Track { track } = result.definitions[0].nodes.last().unwrap().parameters["track"]
        .binding()
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(
        track.keys[0].value,
        Literal::Distance {
            value: 225.,
            unit: Unit::Canvas
        }
    );
    assert_eq!(
        result.definitions[0].nodes[1].parameters["height"].id,
        distance_id
    );
}

#[test]
fn native_curve_edits_preserve_bezier_and_reject_retargeting_locks_and_invalid_tangents() {
    let mut before = fixture();
    before.definitions[0].nodes[1]
        .parameters
        .get_mut("height")
        .unwrap()
        .value = serde_json::to_value(literal(Literal::Distance {
        value: 100.,
        unit: Unit::Canvas,
    }))
    .unwrap();
    let channels = fold_motion::keyed::channels(&before, None).unwrap();
    let mut changed = channels.clone();
    changed[0].curve.keys[0].interpolation = fold_animation::Interpolation::Bezier;
    let bezier = fold_motion::keyed::replace(&before, &changed).unwrap();
    assert_eq!(
        fold_motion::keyed::channels(&bezier, None).unwrap(),
        changed
    );
    changed[0].curve.keys[0].outgoing.fraction = 0.75;
    assert!(fold_motion::keyed::replace(&before, &changed).is_err());
    changed = channels.clone();
    changed[0].property = "other".into();
    assert!(fold_motion::keyed::replace(&before, &changed).is_err());
    changed = channels.clone();
    changed[0].curve.keys[1].time = changed[0].curve.keys[0].time;
    assert!(fold_motion::keyed::replace(&before, &changed).is_err());
    let parameter = before.definitions[0].nodes[1].parameters["height"].id;
    let two_tracks = fold_motion::keyed::animate(&before, parameter, Time::ZERO).unwrap();
    let mut moved = fold_motion::keyed::channels(&two_tracks, None).unwrap();
    let first = moved[0].curve.keys[0].id;
    moved[0].curve.keys[0].id = moved[1].curve.keys[0].id;
    moved[1].curve.keys[0].id = first;
    assert!(
        fold_motion::keyed::replace(&two_tracks, &moved)
            .unwrap_err()
            .contains("another track")
    );
    let mut locked = before.clone();
    locked.definitions[0].nodes[3]
        .parameters
        .get_mut("track")
        .unwrap()
        .protected = true;
    changed = channels;
    changed[0].curve.keys[1].value = 0.3;
    assert!(fold_motion::keyed::replace(&locked, &changed).is_err());
    let height = locked.definitions[0].nodes[1]
        .parameters
        .get_mut("height")
        .unwrap();
    height.protected = true;
    let height_id = height.id;
    assert!(fold_motion::keyed::animate(&locked, height_id, Time::ZERO).is_err());
}

#[test]
fn animate_creates_native_vector_color_and_rotation_tracks_without_losing_parameter_identity() {
    for value in [
        vector(80.),
        Literal::Color {
            value: [-0.2, 2., 0.5, 0.8],
            encoding: Encoding::AcesCg,
            alpha: Alpha::Straight,
        },
        Literal::Rotation {
            value: [0.5; 4],
            space: Space::World,
        },
    ] {
        let mut before = fixture();
        let mut tween = node(
            "tween",
            "linear",
            "Typed entrance",
            vec![
                ("from", literal(value.clone())),
                ("to", literal(value.clone())),
            ],
        );
        let parameter = tween.parameters.get_mut("from").unwrap();
        parameter
            .extensions
            .insert("vendor".into(), serde_json::json!({"keep":true}));
        let id = parameter.id;
        let metadata = parameter.extensions.clone();
        let owner = tween.id;
        before.definitions[0].nodes.push(tween);
        let after = fold_motion::keyed::animate(&before, id, Time::new(-1, 3).unwrap()).unwrap();
        let target = &after.definitions[0]
            .nodes
            .iter()
            .find(|node| node.id == owner)
            .unwrap()
            .parameters["from"];
        assert_eq!(target.id, id);
        assert_eq!(target.extensions, metadata);
        let Binding::Track { track } =
            after.definitions[0].nodes.last().unwrap().parameters["track"]
                .binding()
                .unwrap()
        else {
            panic!("native track")
        };
        assert_eq!(track.ty, value.ty());
        assert_eq!(track.keys[0].value, value);
        assert_eq!(track.keys[0].time, Time::new(-1, 3).unwrap());
        after.validate_semantics().unwrap();
    }
}

#[test]
fn execution_failures_report_causes_without_matching_message_text() {
    use fold_motion::evaluation::FailureKind;
    let value = fixture();
    let output = value.definitions[0].outputs[0].id;
    let prepared = Prepared::new(value.clone()).unwrap();
    for limits in [
        Limits {
            steps: 0,
            elements: 100,
        },
        Limits {
            steps: 10000,
            elements: 0,
        },
        Limits {
            steps: 10000,
            elements: 1,
        },
    ] {
        let error = prepared
            .evaluate_detailed(output, Time::ZERO, limits, &|| Ok(()))
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Budget);
        if limits.steps == 0 {
            assert!(error.context.is_none());
        } else {
            assert_eq!(
                error.context.unwrap().operation,
                value.definitions[0].nodes[if limits.elements == 0 { 5 } else { 1 }].id
            );
        }
    }
    for message in [
        "Evaluation budget exhausted",
        "Generated element budget exhausted",
        "caller stopped",
    ] {
        let error = prepared
            .evaluate_detailed(output, Time::ZERO, Limits::default(), &|| {
                Err(message.into())
            })
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Cancelled);
        assert_eq!(error.message, message);
    }
    let calls = std::cell::Cell::new(0);
    let error = prepared
        .evaluate_detailed(
            output,
            Time::ZERO,
            Limits {
                steps: 0,
                elements: 100,
            },
            &|| {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    Ok(())
                } else {
                    Err("late cancellation".into())
                }
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Budget);
    assert_eq!(
        calls.get(),
        1,
        "classifying an error must not poll cancellation again"
    );
    let mut invalid = value;
    invalid.definitions[0].inputs[0].default = Literal::Distance {
        value: -1.,
        unit: Unit::Canvas,
    };
    let error = Prepared::new(invalid)
        .unwrap()
        .evaluate_detailed(output, Time::ZERO, Limits::default(), &|| Ok(()))
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert!(error.message.contains("dimensions"));
    // Failed evaluation never poisons the immutable prepared snapshot.
    assert!(
        prepared
            .evaluate_detailed(output, Time::ZERO, Limits::default(), &|| Ok(()))
            .is_ok()
    );
}

#[test]
fn override_tracks_sample_parent_time_before_child_clock() {
    let control = input("amount", Literal::Scalar { value: 0. });
    let output = ObjectId::new();
    let child = Definition {
        id: ObjectId::new(),
        name: "Child".into(),
        inputs: vec![control.clone()],
        nodes: vec![],
        instances: vec![],
        outputs: vec![Output {
            id: output,
            name: "amount".into(),
            ty: Type::Scalar,
            value: Reference::Input { input: control.id }.into(),
            extensions: Metadata::new(),
        }],
        extensions: Metadata::new(),
    };
    let track = Track {
        id: ObjectId::new(),
        ty: Type::Scalar,
        combination: Combination::Add,
        base: Some(Literal::Scalar { value: 10. }),
        extensions: Metadata::new(),
        keys: [0, 2]
            .into_iter()
            .map(|n| Key {
                id: ObjectId::new(),
                time: Time::new(n, 1).unwrap(),
                value: Literal::Scalar {
                    value: n as f64 * 100.,
                },
                interpolation: Interpolation::Linear,
                handles: Default::default(),
                extensions: Metadata::new(),
            })
            .collect(),
    };
    let instance = Instance {
        id: ObjectId::new(),
        name: "Child instance".into(),
        definition: child.id,
        clock: Clock {
            offset: Time::new(7, 1).unwrap(),
            rate: Ratio::new(0, 1).unwrap(),
        },
        overrides: BTreeMap::from([(
            control.id,
            Parameter::new(Binding::Track { track }).unwrap(),
        )]),
        extensions: Metadata::new(),
    };
    let mut construction = fixture();
    construction.definitions = vec![child];
    construction.root = instance.clone();
    let sample = |construction: &Construction, output, time| {
        Prepared::new(construction.clone())
            .unwrap()
            .evaluate(output, time, Limits::default(), &|| Ok(()))
            .unwrap()
            .primitive()
            .unwrap()
            .clone()
    };
    assert_eq!(
        sample(&construction, output, Time::new(1, 2).unwrap()),
        Literal::Scalar { value: 60. }
    );
    let parent_output = ObjectId::new();
    let parent = Definition {
        id: ObjectId::new(),
        name: "Parent".into(),
        inputs: vec![],
        nodes: vec![],
        outputs: vec![Output {
            id: parent_output,
            name: "amount".into(),
            ty: Type::Scalar,
            value: Reference::Instance {
                instance: instance.id,
                output,
            }
            .into(),
            extensions: Metadata::new(),
        }],
        instances: vec![instance],
        extensions: Metadata::new(),
    };
    construction.root = Instance {
        id: ObjectId::new(),
        name: "Main".into(),
        definition: parent.id,
        clock: Clock {
            offset: Time::new(1, 4).unwrap(),
            rate: Ratio::new(2, 1).unwrap(),
        },
        overrides: BTreeMap::new(),
        extensions: Metadata::new(),
    };
    construction.definitions.push(parent);
    assert_eq!(
        sample(&construction, parent_output, Time::new(3, 4).unwrap()),
        Literal::Scalar { value: 110. }
    );
}

/// Captures the native portion of the frozen oracle. Transport/artist evidence is
/// deliberately absent until captured by those workflows, so this is not a gate pass.
#[test]
fn capture_frozen_slice_native_evidence() {
    use fold_foundation::DocumentId;
    use fold_project::{EditBatch, Mutation, Project};
    use serde_json::json;
    let specification: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rewrite/fixtures/animated-shape.json"
    ))
    .unwrap();
    let mut authored = fixture();
    let shape = authored.definitions[0].nodes[1].id;
    authored.presentation.positions.insert(shape, [320., 180.]);
    authored
        .presentation
        .annotations
        .insert(shape, "Retain authored layout".into());
    authored.extensions.insert(
        "vendor_capture".into(),
        json!({"opaque":[1,{"future":true}]}),
    );
    let accent = authored.definitions[0].inputs[1].id;
    authored.root.overrides.insert(
        accent,
        Parameter::new(literal(Literal::Color {
            value: [0., 0., 1., 1.],
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        }))
        .unwrap(),
    );
    let output = authored.definitions[0].outputs[0].id;
    let prepared = Prepared::new(authored.clone()).unwrap();
    let times: Vec<Time> = specification["times"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| Time::new(t[0].as_i64().unwrap(), t[1].as_u64().unwrap() as u32).unwrap())
        .collect();
    let seek_runs: Vec<Vec<serde_json::Value>> = specification["orders"].as_array().unwrap().iter().map(|order| {
        order.as_array().unwrap().iter().map(|index| {
            let time = times[index.as_u64().unwrap() as usize];
            let scene = scene(&prepared, output, time);
            let placement = &scene.entries[0].placement;
            let points = &contours(&placement.geometry)[0].points;
            let bounds = points.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |mut b,p| {
                b[0] = b[0].min(p.position[0]); b[1] = b[1].min(p.position[1]);
                b[2] = b[2].max(p.position[0]); b[3] = b[3].max(p.position[1]); b
            });
            json!({"time":[time.numerator(),time.denominator()],"position":screen_position(placement),
                "opacity":placement.opacity,"size":[bounds[2]-bounds[0],bounds[3]-bounds[1]],
                "element_ids":{"entry":scene.entries[0].id,"placement":placement.id,
                    "geometry":placement.geometry.id,"contours":contours(&placement.geometry).iter().map(|c| json!({"id":c.id,"points":c.points.iter().map(|p| p.id).collect::<Vec<_>>()})).collect::<Vec<_>>()}})
        }).collect()
    }).collect();
    let valid_source = fold_motion::source::canonical(&authored).unwrap();
    let reparsed = fold_motion::source::parse(&valid_source)
        .unwrap()
        .construction;
    assert_eq!(reparsed, authored);
    let id = DocumentId::new();
    let mut before = authored.clone();
    before.definitions[0].nodes[1].name = "Before edit".into();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(id).unwrap())],
        })
        .unwrap();
    fold_motion::editing::prepare(
        &project.snapshot(),
        id,
        fold_motion::editing::Scope::Document,
        authored.clone(),
    )
    .unwrap()
    .apply(&mut project)
    .unwrap();
    let read = |project: &Project| {
        Construction::from_document(&project.snapshot().state().documents[&id]).unwrap()
    };
    project.undo().unwrap();
    let undo = read(&project);
    assert_eq!(undo, before);
    project.redo().unwrap();
    let redo = read(&project);
    assert_eq!(redo, authored);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("native.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = read(&fold_project::load(&path, 8).unwrap());
    assert_eq!(reopened, authored);
    let mut buffer = fold_motion::editing::SourceBuffer::default();
    assert!(
        buffer
            .propose(
                format!("{valid_source}\ninvalid trailing source"),
                &project.snapshot(),
                id,
                fold_motion::editing::Scope::Document
            )
            .is_err()
    );
    let invalid_source = read(&project);
    assert_eq!(invalid_source, authored);
    let mut unknown = authored.clone();
    unknown.definitions[0].nodes[0].operation = "vendor.future".into();
    unknown.definitions[0].nodes[0].version = 99;
    let mut opaque_project = Project::new(8);
    opaque_project
        .commit(EditBatch {
            base: opaque_project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(unknown.document(id).unwrap())],
        })
        .unwrap();
    let unknown_path = directory.path().join("unknown.fold");
    fold_project::save(&opaque_project.snapshot(), &unknown_path).unwrap();
    let unknown_reopened = read(&fold_project::load(&unknown_path, 8).unwrap());
    assert_eq!(unknown_reopened, unknown);
    assert!(Prepared::new(unknown_reopened.clone()).is_err());
    let evidence = json!({"fixture":specification["fixture"],"protocol":2,
        "document_type":"fold.motion.construction","capture_scope":"native evaluation and persistence only",
        "seek_runs":seek_runs,"valid_source":valid_source,"invalid_source_buffer":buffer.text(),
        "snapshots":{"authored":authored,"canonical_reparse":reparsed,"reopened":reopened,
            "redo":redo,"undo":undo,"before_edit":before,"invalid_source":invalid_source},
        "unknown_record_before":unknown.definitions[0].nodes[0],
        "unknown_record_reopened":unknown_reopened.definitions[0].nodes[0],
        "unknown_extensions_before":authored.extensions,"unknown_extensions_reopened":reopened.extensions});
    if let Some(directory) = std::env::var_os("FOLD_SLICE_CAPTURE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("native-evidence.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        std::fs::write(directory.join("animated-shape.foldsrc"), valid_source).unwrap();
    }
}

fn screen_position(placement: &fold_motion::evaluation::Placement) -> [f64; 2] {
    let fold_motion::evaluation::placement::Transform::Screen { position } = placement.transform
    else {
        panic!("expected screen placement")
    };
    position
}

#[test]
fn candidate_evaluation_retains_project_context_until_explicit_rebase() {
    use fold_motion::editing::{self, Scope};
    use fold_project::{Asset, EditBatch, Mutation, Project};
    let original = fixture();
    let id = fold_foundation::DocumentId::new();
    let sibling = fold_foundation::DocumentId::new();
    let asset = fold_foundation::AssetId::new();
    let settings = |version| {
        [(
            "color_configuration".into(),
            serde_json::json!({"version":version}),
        )]
        .into()
    };
    let resource = |version| Asset {
        id: asset,
        location: format!("resource-{version}"),
        fingerprint: format!("content-{version}"),
        extensions: Metadata::default(),
    };
    let mut project = Project::with_settings(8, settings(1));
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(original.document(id).unwrap()),
                Mutation::PutDocument(original.document(sibling).unwrap()),
                Mutation::PutAsset(resource(1)),
            ],
        })
        .unwrap();
    let base = project.snapshot();
    let mut proposed = original.clone();
    proposed.definitions[0].inputs[0].default = Literal::Distance {
        value: 140.,
        unit: Unit::Canvas,
    };
    let candidate = editing::prepare(&base, id, Scope::Document, proposed.clone()).unwrap();
    let retained = candidate.snapshot().clone();
    assert_eq!(candidate.base_revision(), base.revision());
    assert_eq!(
        Construction::from_document(&retained.state().documents[&id]).unwrap(),
        proposed
    );
    assert_eq!(retained.state().settings, base.state().settings);
    assert!(Arc::ptr_eq(
        &retained.state().documents[&sibling],
        &base.state().documents[&sibling]
    ));
    assert!(
        project.snapshot().same_content(&base),
        "preparation cannot publish an overlay"
    );
    let mut changed_sibling = original.clone();
    changed_sibling.definitions[0].name = "Changed sibling".into();
    project
        .commit(EditBatch {
            base: base.revision(),
            mutations: vec![
                Mutation::SetSettings(settings(2)),
                Mutation::PutAsset(resource(2)),
                Mutation::PutDocument(changed_sibling.document(sibling).unwrap()),
            ],
        })
        .unwrap();
    let rebased = candidate.rebase(&project.snapshot()).unwrap();
    assert_eq!(rebased.base_revision(), project.snapshot().revision());
    assert_eq!(rebased.snapshot().state().settings, settings(2));
    assert_eq!(*rebased.snapshot().state().assets[&asset], resource(2));
    assert_eq!(
        Construction::from_document(&rebased.snapshot().state().documents[&sibling]).unwrap(),
        changed_sibling
    );
    assert_eq!(
        Construction::from_document(&rebased.snapshot().state().documents[&id]).unwrap(),
        proposed
    );
    assert_eq!(candidate.snapshot().state().settings, settings(1));
    assert_eq!(*retained.state().assets[&asset], resource(1));
    assert_eq!(
        retained.state().documents[&sibling],
        base.state().documents[&sibling]
    );
    let mut store = fold_motion::candidates::Store::default();
    let now = std::time::Instant::now();
    let handle = store.insert(candidate, now).unwrap();
    let observation = store.shared(handle, now).unwrap();
    store.cancel(handle, now).unwrap();
    assert!(store.shared(handle, now).is_err());
    assert!(observation.snapshot().same_content(&retained));
    let rebased_snapshot = rebased.snapshot().clone();
    let applied = rebased.apply(&mut project).unwrap();
    assert!(applied.same_content(&rebased_snapshot));
    assert_eq!(
        Construction::from_document(&applied.state().documents[&id]).unwrap(),
        proposed
    );
    assert_eq!(retained.state().settings, settings(1));
}
