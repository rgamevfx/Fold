use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{FailureKind, GeometryData, Limits, Prepared, Value},
    model::{Construction, Node, Parameter, Reference},
    registry, source,
    types::{Literal, Unit},
};
use std::sync::Arc;
fn link(node: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node }.into(),
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
    let text = node("text", "packaged", "Text", vec![]);
    let fill = node("fill", "solid", "Fill", vec![("geometry", link(text.id))]);
    let stroke = node(
        "stroke",
        "solid_centered_butt_miter",
        "Stroke",
        vec![("geometry", link(fill.id))],
    );
    let alternate = node(
        "place",
        "screen",
        "Fill only",
        vec![("geometry", link(fill.id))],
    );
    let definition = &mut c.definitions[0];
    // This fixture supplies its own text paint and origin placement. Remove
    // the interactive rectangle's starter paint so operation queries select
    // the text branch being qualified.
    definition
        .nodes
        .retain(|n| n.operation != "fold.motion.fill");
    let placement = definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap();
    placement.parameters.get_mut("geometry").unwrap().value =
        serde_json::to_value(link(stroke.id)).unwrap();
    placement.parameters.get_mut("position").unwrap().value =
        serde_json::to_value(Binding::Literal {
            value: Literal::Vector {
                value: vec![0., 0.],
                unit: Unit::Canvas,
                space: fold_motion::types::Space::Screen,
            },
        })
        .unwrap();
    let scene = definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.push(link(alternate.id));
    scene.parameters.get_mut("children").unwrap().value =
        serde_json::to_value(Binding::List { values }).unwrap();
    definition.nodes.extend([text, fill, stroke, alternate]);
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
fn text(c: &Construction) -> Arc<fold_motion::evaluation::text::Text> {
    let Value::Scene(scene) = evaluate(c, Time::ZERO).unwrap() else {
        panic!()
    };
    let GeometryData::Text(text) = &scene.entries[0].placement.geometry.data else {
        panic!()
    };
    text.clone()
}
#[test]
fn native_text_round_trips_and_shares_one_layout_with_style_outline_and_mesh_consumers() {
    let c = fixture();
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let Value::Scene(scene) = evaluate(&c, Time::ZERO).unwrap() else {
        panic!()
    };
    let first = &scene.entries[0].placement.geometry;
    let second = &scene.entries[1].placement.geometry;
    let (GeometryData::Text(text), GeometryData::Text(other)) = (&first.data, &second.data) else {
        panic!()
    };
    assert!(Arc::ptr_eq(text, other));
    assert_eq!(first.id, second.id);
    assert!(first.appearance.stroke.is_some() && second.appearance.stroke.is_none());
    assert_eq!(text.resource, fold_motion::evaluation::text::FONT_RESOURCE);
    assert_eq!(text.layout.text.as_ref(), "MAKE IT MOVE");
    assert_eq!(text.layout.line_count, 1);
    assert_eq!(text.glyph_ids.len(), text.layout.glyphs.len());
    assert_eq!(text.cluster_ids.len(), text.layout.clusters.len());
    assert_eq!(text.line_ids.len(), text.layout.line_count);
    assert_eq!(text.word_ids.len(), 3);
    let reference = fold_render::text::Service::new(
        include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
        "Noto Sans",
    )
    .unwrap()
    .layout("MAKE IT MOVE", 140., None)
    .unwrap();
    assert_eq!(text.layout, reference);
    let cancel = fold_media::Cancel::default();
    let outlines =
        Arc::new(fold_render::text::outlines::prepare(text.layout.clone(), &cancel).unwrap());
    assert!(Arc::ptr_eq(&outlines.layout, &text.layout));
    let mesh = fold_render::text::mesh::Cache::default()
        .prepare(outlines, 1., &cancel)
        .unwrap();
    assert!(Arc::ptr_eq(&mesh.outlines.layout, &text.layout));
    assert!(mesh.glyphs.iter().any(|g| g.mesh.is_some()));
}
#[test]
fn native_text_metrics_animate_at_exact_times_and_keep_glyph_slots() {
    let mut c = fixture();
    let duration = Binding::Literal {
        value: Literal::Duration {
            value: Time::new(1, 1).unwrap(),
        },
    };
    let distance = |value| Binding::Literal {
        value: Literal::Distance {
            value,
            unit: Unit::Canvas,
        },
    };
    let tween = node(
        "tween",
        "linear",
        "Size",
        vec![
            ("from", distance(140.)),
            ("to", distance(70.)),
            ("duration", duration),
        ],
    );
    let t = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap();
    t.parameters.get_mut("size").unwrap().value = serde_json::to_value(link(tween.id)).unwrap();
    c.definitions[0].nodes.push(tween);
    let mut ids = None;
    for (time, size) in [
        (Time::ZERO, 140.),
        (Time::new(1, 1).unwrap(), 70.),
        (Time::new(1, 2).unwrap(), 105.),
        (Time::ZERO, 140.),
    ] {
        let Value::Scene(scene) = evaluate(&c, time).unwrap() else {
            panic!()
        };
        let GeometryData::Text(text) = &scene.entries[0].placement.geometry.data else {
            panic!()
        };
        assert!(text.layout.runs.iter().all(|r| r.font_size == size));
        if let Some(ids) = &ids {
            assert_eq!(ids, &text.glyph_ids)
        } else {
            ids = Some(text.glyph_ids.clone())
        }
    }
    let t = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap();
    t.parameters.get_mut("wrap").unwrap().value = serde_json::to_value(Binding::Literal {
        value: Literal::Bool { value: true },
    })
    .unwrap();
    t.parameters.get_mut("width").unwrap().value = serde_json::to_value(distance(350.)).unwrap();
    assert!(text(&c).layout.line_count > 1);
}
#[test]
fn text_reports_unsupported_fonts_missing_glyphs_invalid_metrics_and_budgets() {
    let c = fixture();
    for (parameter, value) in [
        (
            "font",
            Literal::Text {
                value: "Uninstalled font".into(),
            },
        ),
        (
            "size",
            Literal::Distance {
                value: 0.,
                unit: Unit::Canvas,
            },
        ),
        (
            "width",
            Literal::Distance {
                value: 10001.,
                unit: Unit::Canvas,
            },
        ),
    ] {
        let mut bad = c.clone();
        let t = bad.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.operation == "fold.motion.text")
            .unwrap();
        let id = t.id;
        t.parameters.insert(
            parameter.into(),
            Parameter::new(Binding::Literal { value }).unwrap(),
        );
        let error = evaluate(&bad, Time::ZERO).unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, id);
    }
    let mut missing = c.clone();
    let t = missing.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap();
    t.parameters.get_mut("text").unwrap().value = serde_json::to_value(Binding::Literal {
        value: Literal::Text {
            value: "A🛸B".into(),
        },
    })
    .unwrap();
    assert!(!text(&missing).layout.diagnostics.is_empty());
    let prepared = Prepared::new(c.clone()).unwrap();
    let output = c.definitions[0].outputs[0].id;
    assert_eq!(
        prepared
            .evaluate_detailed(
                output,
                Time::ZERO,
                Limits {
                    elements: 0,
                    ..Limits::default()
                },
                &|| Ok(())
            )
            .unwrap_err()
            .kind,
        FailureKind::Budget
    );
    assert_eq!(
        prepared
            .evaluate_detailed(output, Time::ZERO, Limits::default(), &|| Err(
                "cancel".into()
            ))
            .unwrap_err()
            .kind,
        FailureKind::Cancelled
    );
}

fn outline_fixture(wording: &str) -> (Construction, ObjectId) {
    let mut c = fixture();
    let definition = &mut c.definitions[0];
    let text = definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap();
    text.parameters.insert(
        "text".into(),
        Parameter::new(Binding::Literal {
            value: Literal::Text {
                value: wording.into(),
            },
        })
        .unwrap(),
    );
    let outline = node(
        "outline_text",
        "curves",
        "Outline Text",
        vec![("text", link(text.id))],
    );
    let id = outline.id;
    let fill = definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.fill")
        .unwrap();
    fill.parameters
        .insert("geometry".into(), Parameter::new(link(id)).unwrap());
    definition.nodes.push(outline);
    (c, id)
}
#[test]
fn native_outlines_retain_shared_curves_source_associations_and_stable_topology() {
    let (c, _) = outline_fixture("O B 8 O");
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let Value::Scene(scene) = evaluate(&c, Time::ZERO).unwrap() else {
        panic!()
    };
    let first = &scene.entries[0].placement.geometry;
    let second = &scene.entries[1].placement.geometry;
    let (GeometryData::Outlines(outlines), GeometryData::Outlines(other)) =
        (&first.data, &second.data)
    else {
        panic!()
    };
    assert!(Arc::ptr_eq(outlines, other));
    assert!(Arc::ptr_eq(
        &outlines.source.layout,
        &outlines.curves.layout
    ));
    assert!(Arc::ptr_eq(
        &outlines.curves.glyphs[0].path,
        &outlines.curves.glyphs[6].path
    ));
    assert_eq!(
        outlines
            .topology
            .iter()
            .map(|g| g.contour_ids.len())
            .collect::<Vec<_>>(),
        [2, 0, 3, 0, 3, 0, 2]
    );
    assert_ne!(
        outlines.topology[0].command_ids,
        outlines.topology[6].command_ids
    );
    assert!(first.appearance.stroke.is_some());
    assert!(second.appearance.stroke.is_none());
    for (index, topology) in outlines.topology.iter().enumerate() {
        assert_eq!(topology.source_glyph, outlines.source.glyph_ids[index]);
        assert_eq!(
            topology.command_ids.len(),
            outlines.curves.glyphs[index].path.len()
        );
    }
    let mut smaller = c.clone();
    smaller.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap()
        .parameters
        .insert(
            "size".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Distance {
                    value: 70.,
                    unit: Unit::Canvas,
                },
            })
            .unwrap(),
        );
    let Value::Scene(small) = evaluate(&smaller, Time::new(7, 24).unwrap()).unwrap() else {
        panic!()
    };
    let GeometryData::Outlines(small_outlines) = &small.entries[0].placement.geometry.data else {
        panic!()
    };
    assert_eq!(outlines.topology, small_outlines.topology);
    assert_eq!(first.id, small.entries[0].placement.geometry.id);
    assert_ne!(outlines.curves, small_outlines.curves);
}
#[test]
fn native_outline_failures_have_operation_context_and_never_publish_partial_geometry() {
    let (c, id) = outline_fixture("A🛸B");
    let error = evaluate(&c, Time::ZERO).unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, id);
    let (mut c, id) = outline_fixture("O B 8");
    let shape = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.shape")
        .unwrap()
        .id;
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .parameters
        .insert("text".into(), Parameter::new(link(shape)).unwrap());
    let error = evaluate(&c, Time::ZERO).unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, id);
    let (c, id) = outline_fixture("O B 8");
    let prepared = Prepared::new(c.clone()).unwrap();
    let output = c.definitions[0].outputs[0].id;
    // Find an actual step inside Outline Text, then stop the same evaluation
    // with cancellation at that checkpoint. This covers native error mapping.
    let stop = (1..1000)
        .find(|steps| {
            prepared
                .evaluate_detailed(
                    output,
                    Time::ZERO,
                    Limits {
                        steps: *steps,
                        ..Limits::default()
                    },
                    &|| Ok(()),
                )
                .is_err_and(|e| {
                    e.kind == FailureKind::Budget && e.context.is_some_and(|c| c.operation == id)
                })
        })
        .unwrap();
    let calls = std::cell::Cell::new(0);
    let error = prepared
        .evaluate_detailed(output, Time::ZERO, Limits::default(), &|| {
            calls.set(calls.get() + 1);
            if calls.get() > stop {
                Err("cancel".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert_eq!(error.context.unwrap().operation, id);
}

fn solid_fixture() -> (Construction, ObjectId) {
    let (mut c, outline) = outline_fixture("O B 8 O");
    let extrude = node(
        "extrude",
        "straight_text",
        "Extrude",
        vec![("profile", link(outline))],
    );
    let id = extrude.id;
    c.definitions[0].outputs[0].ty = fold_motion::types::Type::Geometry;
    c.definitions[0].outputs[0].value = Reference::Node { node: id }.into();
    c.definitions[0].nodes.push(extrude);
    (c, id)
}
#[test]
fn native_extrude_round_trips_retains_metric_solids_and_topology_correspondence() {
    let (c, id) = solid_fixture();
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let Value::Geometry(geometry) = evaluate(&c, Time::ZERO).unwrap() else {
        panic!()
    };
    assert_eq!(geometry.unit, Unit::Meters);
    assert_eq!(geometry.space, fold_motion::types::Space::World);
    let GeometryData::Solids(solids) = &geometry.data else {
        panic!()
    };
    let GeometryData::Outlines(outlines) = &solids.source.data else {
        panic!()
    };
    assert!(Arc::ptr_eq(
        &outlines.curves,
        &solids.prepared.planar.outlines
    ));
    assert_eq!(solids.prepared.depth_meters, 0.15);
    assert_eq!(solids.prepared.meters_per_canvas, 0.01);
    assert!(Arc::ptr_eq(
        solids.glyphs[0].topology.as_ref().unwrap(),
        solids.glyphs[6].topology.as_ref().unwrap()
    ));
    assert_ne!(solids.glyphs[0].id, solids.glyphs[6].id);
    for (index, glyph) in solids.glyphs.iter().enumerate() {
        assert_eq!(glyph.source_glyph, outlines.source.glyph_ids[index]);
        if let Some(topology) = &glyph.topology {
            let solid = solids.prepared.glyphs[index].solid.as_ref().unwrap();
            assert_eq!(topology.vertex_ids.len(), solid.mesh.positions.len());
            assert_eq!(topology.face_ids.len() * 3, solid.mesh.indices.len());
            assert!(solid.regions.front.end < solid.regions.back.end);
            assert!(solid.regions.back.end < solid.regions.sides.end);
        } else {
            assert!(index % 2 == 1);
        }
    }
    let mut changed = c.clone();
    changed.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .parameters
        .insert(
            "depth".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Distance {
                    value: 0.3,
                    unit: Unit::Meters,
                },
            })
            .unwrap(),
        );
    let Value::Geometry(deep) = evaluate(&changed, Time::new(3, 24).unwrap()).unwrap() else {
        panic!()
    };
    let GeometryData::Solids(deep) = &deep.data else {
        panic!()
    };
    assert_eq!(solids.glyphs, deep.glyphs);
    assert_ne!(
        solids.prepared.glyphs[0].solid,
        deep.prepared.glyphs[0].solid
    );
    changed.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .parameters
        .insert(
            "segments".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Integer { value: 3 },
            })
            .unwrap(),
        );
    let Value::Geometry(segmented) = evaluate(&changed, Time::ZERO).unwrap() else {
        panic!()
    };
    let GeometryData::Solids(segmented) = &segmented.data else {
        panic!()
    };
    assert_eq!(solids.glyphs[0].id, segmented.glyphs[0].id);
    assert_ne!(
        solids.glyphs[0].topology.as_ref().unwrap().id,
        segmented.glyphs[0].topology.as_ref().unwrap().id
    );
}
#[test]
fn native_extrude_rejects_invalid_controls_and_maps_preparation_stops() {
    let (c, id) = solid_fixture();
    for (name, value) in [
        ("segments", Literal::Integer { value: 0 }),
        (
            "depth",
            Literal::Distance {
                value: -1.,
                unit: Unit::Meters,
            },
        ),
        (
            "canvas_unit",
            Literal::Distance {
                value: 0.,
                unit: Unit::Meters,
            },
        ),
    ] {
        let mut bad = c.clone();
        bad.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == id)
            .unwrap()
            .parameters
            .insert(
                name.into(),
                Parameter::new(Binding::Literal { value }).unwrap(),
            );
        let error = evaluate(&bad, Time::ZERO).unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, id);
    }
    let prepared = Prepared::new(c.clone()).unwrap();
    let output = c.definitions[0].outputs[0].id;
    let calls = std::cell::Cell::new(0);
    prepared
        .evaluate_detailed(output, Time::ZERO, Limits::default(), &|| {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap();
    let stop = calls.get() * 3 / 4;
    let error = prepared
        .evaluate_detailed(
            output,
            Time::ZERO,
            Limits {
                steps: stop,
                ..Limits::default()
            },
            &|| Ok(()),
        )
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Budget);
    assert_eq!(error.context.unwrap().operation, id);
    calls.set(0);
    let error = prepared
        .evaluate_detailed(output, Time::ZERO, Limits::default(), &|| {
            calls.set(calls.get() + 1);
            if calls.get() > stop {
                Err("cancel".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert_eq!(error.context.unwrap().operation, id);
}

#[test]
fn native_extrude_depth_signal_samples_exact_time_without_changing_topology() {
    let (mut c, id) = solid_fixture();
    let metric = |value| Binding::Literal {
        value: Literal::Distance {
            value,
            unit: Unit::Meters,
        },
    };
    let tween = node(
        "tween",
        "linear",
        "Depth",
        vec![
            ("from", metric(0.15)),
            ("to", metric(0.3)),
            (
                "duration",
                Binding::Literal {
                    value: Literal::Duration {
                        value: Time::new(1, 1).unwrap(),
                    },
                },
            ),
        ],
    );
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .parameters
        .insert("depth".into(), Parameter::new(link(tween.id)).unwrap());
    c.definitions[0].nodes.push(tween);
    let mut topology = None;
    for (time, depth) in [
        (Time::new(1, 1).unwrap(), 0.3),
        (Time::ZERO, 0.15),
        (Time::new(1, 3).unwrap(), 0.2),
    ] {
        let Value::Geometry(geometry) = evaluate(&c, time).unwrap() else {
            panic!()
        };
        let GeometryData::Solids(solids) = &geometry.data else {
            panic!()
        };
        assert!((solids.prepared.depth_meters - depth).abs() < 1e-12);
        match &topology {
            None => topology = Some(solids.glyphs.clone()),
            Some(expected) => assert_eq!(&solids.glyphs, expected),
        }
    }
}

#[test]
fn native_extrude_requires_outlined_input_and_explicit_world_placement() {
    let (c, id) = solid_fixture();
    let mut raw = c.clone();
    let text = raw.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap()
        .id;
    raw.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .parameters
        .insert("profile".into(), Parameter::new(link(text)).unwrap());
    let error = evaluate(&raw, Time::ZERO).unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, id);
    let mut screen = c;
    let place = node(
        "place",
        "screen",
        "Wrong space",
        vec![("geometry", link(id))],
    );
    let place_id = place.id;
    screen.definitions[0].outputs[0].ty = fold_motion::types::Type::Placement;
    screen.definitions[0].outputs[0].value = Reference::Node { node: place_id }.into();
    screen.definitions[0].nodes.push(place);
    let error = evaluate(&screen, Time::ZERO).unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, place_id);
}

fn material_fixture(mode: &str) -> (Construction, ObjectId, ObjectId) {
    let (mut c, extrude) = solid_fixture();
    let material = node(
        "material",
        mode,
        "Surface",
        vec![("geometry", link(extrude))],
    );
    let id = material.id;
    c.definitions[0].outputs[0].value = Reference::Node { node: id }.into();
    c.definitions[0].nodes.push(material);
    (c, id, extrude)
}
#[test]
fn native_material_round_trips_animates_and_preserves_solid_topology() {
    use fold_motion::types::{Alpha, Encoding};
    let (mut c, id, extrude) = material_fixture("lit");
    let color = |value| Binding::Literal {
        value: Literal::Color {
            value,
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        },
    };
    let tween = node(
        "tween",
        "linear",
        "Surface color",
        vec![
            ("from", color([2., 0., 0., 0.25])),
            ("to", color([4., 1., 0.5, 0.75])),
        ],
    );
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .parameters
        .insert("color".into(), Parameter::new(link(tween.id)).unwrap());
    c.definitions[0].nodes.push(tween);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let Value::Geometry(styled) = evaluate(&c, Time::new(1, 2).unwrap()).unwrap() else {
        panic!()
    };
    let material = styled.appearance.material.as_ref().unwrap();
    assert_eq!(material.color.value, [3., 0.5, 0.25, 0.5]);
    assert!(!material.unlit);
    assert_eq!((material.roughness, material.reflectance), (0.5, 0.5));
    let mut plain = c.clone();
    plain.definitions[0].outputs[0].value = Reference::Node { node: extrude }.into();
    let Value::Geometry(plain) = evaluate(&plain, Time::new(1, 2).unwrap()).unwrap() else {
        panic!()
    };
    assert_eq!(plain.id, styled.id);
    assert_eq!(plain.data, styled.data);
    assert!(plain.appearance.material.is_none());
    let (unlit, _, _) = material_fixture("unlit");
    let Value::Geometry(unlit) = evaluate(&unlit, Time::ZERO).unwrap() else {
        panic!()
    };
    assert!(unlit.appearance.material.as_ref().unwrap().unlit);
}
#[test]
fn native_material_rejects_unsupported_geometry_and_invalid_surface_controls() {
    use fold_motion::types::{Alpha, Encoding};
    let (c, id, _) = material_fixture("lit");
    for (name, value) in [
        ("roughness", Literal::Scalar { value: -0.01 }),
        ("reflectance", Literal::Scalar { value: 1.01 }),
        (
            "color",
            Literal::Color {
                value: [-0.01, 0., 0., 1.],
                encoding: Encoding::LinearRec709,
                alpha: Alpha::Straight,
            },
        ),
        (
            "color",
            Literal::Color {
                value: [65505., 0., 0., 1.],
                encoding: Encoding::LinearRec709,
                alpha: Alpha::Straight,
            },
        ),
    ] {
        let mut bad = c.clone();
        bad.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == id)
            .unwrap()
            .parameters
            .insert(
                name.into(),
                Parameter::new(Binding::Literal { value }).unwrap(),
            );
        let error = evaluate(&bad, Time::ZERO).unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, id);
    }
    let mut planar = c;
    let text = planar.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap()
        .id;
    planar.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .parameters
        .insert("geometry".into(), Parameter::new(link(text)).unwrap());
    let error = evaluate(&planar, Time::ZERO).unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, id);
}

fn world_fixture() -> (Construction, ObjectId) {
    use fold_motion::types::{Space, Type};
    let (mut c, material, extrude) = material_fixture("lit");
    let vector = |value: [f64; 3], unit| Binding::Literal {
        value: Literal::Vector {
            value: value.to_vec(),
            unit,
            space: Space::World,
        },
    };
    let motion = node(
        "tween",
        "linear",
        "World move",
        vec![
            ("from", vector([0.; 3], Unit::Meters)),
            ("to", vector([2., 4., 6.], Unit::Meters)),
        ],
    );
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let world = node(
        "place",
        "world",
        "World",
        vec![
            ("geometry", link(material)),
            ("position", link(motion.id)),
            ("scale", vector([2., 3., 4.], Unit::Unitless)),
            (
                "rotation",
                Binding::Literal {
                    value: Literal::Rotation {
                        value: [0., 0., q, q],
                        space: Space::World,
                    },
                },
            ),
        ],
    );
    let id = world.id;
    let unlit = node(
        "material",
        "unlit",
        "Unlit",
        vec![("geometry", link(extrude))],
    );
    let alternate = node(
        "place",
        "world",
        "Alternate world",
        vec![("geometry", link(unlit.id))],
    );
    let screen = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .id;
    let camera = node("camera", "perspective", "Camera", vec![]);
    let light = node("light", "directional", "Sun", vec![]);
    let scene = node(
        "scene",
        "spatial",
        "Integrated",
        vec![
            ("camera", link(camera.id)),
            (
                "children",
                Binding::List {
                    values: vec![
                        link(screen),
                        link(world.id),
                        link(alternate.id),
                        link(light.id),
                    ],
                },
            ),
        ],
    );
    c.definitions[0].outputs[0].ty = Type::Scene;
    c.definitions[0].outputs[0].value = Reference::Node { node: scene.id }.into();
    c.definitions[0]
        .nodes
        .extend([motion, world, unlit, alternate, camera, light, scene]);
    (c, id)
}
#[test]
fn native_world_scene_retains_order_shared_solids_materials_and_exact_transforms() {
    use fold_motion::evaluation::placement::Transform;
    let (c, _) = world_fixture();
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let mut ids = None;
    for time in [
        Time::new(1, 2).unwrap(),
        Time::ZERO,
        Time::new(1, 1).unwrap(),
    ] {
        let Value::Scene(scene) = evaluate(&c, time).unwrap() else {
            panic!()
        };
        assert!(scene.camera.is_some());
        assert_eq!(scene.lights.len(), 1);
        assert_eq!(scene.entries.len(), 3);
        assert!(matches!(
            scene.entries[0].placement.transform,
            Transform::Screen { .. }
        ));
        let a = &scene.entries[1].placement;
        let b = &scene.entries[2].placement;
        let Transform::World(transform) = &a.transform else {
            panic!()
        };
        let t = time.numerator() as f64 / time.denominator() as f64;
        assert_eq!(transform.position, [2. * t, 4. * t, 6. * t]);
        let actual = transform.point([1., 2., 3.]);
        for (actual, expected) in actual
            .into_iter()
            .zip([-6. + 2. * t, 2. + 4. * t, 12. + 6. * t])
        {
            assert!((actual - expected).abs() < 1e-12);
        }
        let (GeometryData::Solids(first), GeometryData::Solids(second)) =
            (&a.geometry.data, &b.geometry.data)
        else {
            panic!()
        };
        assert!(Arc::ptr_eq(first, second));
        assert_eq!(a.geometry.id, b.geometry.id);
        assert!(!a.geometry.appearance.material.as_ref().unwrap().unlit);
        assert!(b.geometry.appearance.material.as_ref().unwrap().unlit);
        assert_ne!(a.id, b.id);
        let actual = [a.id, b.id];
        if let Some(expected) = ids {
            assert_eq!(actual, expected);
        } else {
            ids = Some(actual);
        }
    }
}
#[test]
fn native_world_controls_reject_invalid_ranges_and_planar_input() {
    use fold_motion::types::Space;
    let (c, id) = world_fixture();
    for (name, value) in [
        (
            "scale",
            Literal::Vector {
                value: vec![1., 0., 1.],
                unit: Unit::Unitless,
                space: Space::World,
            },
        ),
        (
            "position",
            Literal::Vector {
                value: vec![1e7, 0., 0.],
                unit: Unit::Meters,
                space: Space::World,
            },
        ),
        ("opacity", Literal::Scalar { value: -0.1 }),
    ] {
        let mut bad = c.clone();
        bad.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == id)
            .unwrap()
            .parameters
            .insert(
                name.into(),
                Parameter::new(Binding::Literal { value }).unwrap(),
            );
        let error = evaluate(&bad, Time::ZERO).unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, id);
    }
    let mut bad = c;
    let text = bad.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap()
        .id;
    bad.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .parameters
        .insert("geometry".into(), Parameter::new(link(text)).unwrap());
    let error = evaluate(&bad, Time::ZERO).unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, id);
}

fn render_snapshot(c: &Construction) -> (fold_project::Project, DocumentId) {
    let mut project = fold_project::Project::new(8);
    let id = DocumentId::new();
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
    (project, id)
}
fn spatial_only(c: &mut Construction) {
    let scene = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "Integrated")
        .unwrap();
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.remove(0);
    scene.parameters.insert(
        "children".into(),
        Parameter::new(Binding::List { values }).unwrap(),
    );
}
#[test]
fn spatial_conversion_binds_snapshot_and_shares_metric_meshes_with_stable_renderer_ids() {
    use fold_motion::rendering;
    let (mut c, world) = world_fixture();
    spatial_only(&mut c);
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == world)
        .unwrap()
        .parameters
        .insert(
            "opacity".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Scalar { value: 0.4 },
            })
            .unwrap(),
        );
    let output = c.definitions[0].outputs[0].id;
    let (mut project, id) = render_snapshot(&c);
    let snapshot = project.snapshot();
    let time = Time::new(1, 2).unwrap();
    let frame = rendering::spatial(
        &snapshot,
        id,
        output,
        time,
        [640, 220],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(frame.binding.document, id);
    assert_eq!(frame.binding.revision, snapshot.revision().0);
    assert_eq!(frame.binding.time, time);
    assert_eq!(frame.binding.duration, Time::new(1, 24).unwrap());
    assert_eq!(frame.binding.camera_content, frame.scene.camera.identity());
    assert_eq!(frame.scene.instances.len(), 8);
    assert_eq!(frame.scene.meshes.len(), 3);
    assert_eq!(frame.scene.materials.len(), 2);
    assert_eq!(frame.scene.lights.len(), 1);
    assert_eq!(frame.scene.lights[0].illuminance, 1000.);
    let GeometryData::Solids(solids) = &frame.native.entries[0].placement.geometry.data else {
        panic!()
    };
    for glyph in &solids.prepared.glyphs {
        if let Some(solid) = &glyph.solid {
            assert!(
                frame
                    .scene
                    .meshes
                    .values()
                    .any(|mesh| Arc::ptr_eq(mesh, &solid.mesh))
            );
        }
    }
    let first = &frame.scene.instances[0];
    assert_eq!(frame.scene.materials[&first.material].color[3], 0.4f32);
    assert!(!frame.scene.materials[&first.material].unlit);
    let last = &frame.scene.instances[7];
    assert!(frame.scene.materials[&last.material].unlit);
    let fold_motion::evaluation::placement::Transform::World(transform) =
        &frame.native.entries[0].placement.transform
    else {
        panic!()
    };
    let glyph = &solids.prepared.glyphs[0];
    let expected = transform.point([glyph.transform[4], glyph.transform[5], 0.]);
    assert_eq!(first.transform.translation, expected.map(|v| v as f32));
    c.seed += 1;
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
    let again = rendering::spatial(
        &snapshot,
        id,
        output,
        time,
        [640, 220],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(again.binding, frame.binding);
    assert_eq!(again.scene.instances, frame.scene.instances);
    assert!(project.snapshot().revision().0 > frame.binding.revision);
    let current = rendering::spatial(
        &project.snapshot(),
        id,
        output,
        time,
        [640, 220],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(current.binding.revision, project.snapshot().revision().0);
}
#[test]
fn spatial_conversion_rejects_screen_passes_missing_material_and_budget_or_cancellation() {
    use fold_motion::rendering::{self, Error};
    let (mut c, world) = world_fixture();
    let output = c.definitions[0].outputs[0].id;
    let (project, id) = render_snapshot(&c);
    assert!(matches!(
        rendering::spatial(
            &project.snapshot(),
            id,
            output,
            Time::ZERO,
            [640, 220],
            Default::default(),
            &|| Ok(())
        ),
        Err(Error::Invalid(_))
    ));
    spatial_only(&mut c);
    let (project, id) = render_snapshot(&c);
    assert!(matches!(
        rendering::spatial(
            &project.snapshot(),
            id,
            output,
            Time::ZERO,
            [640, 220],
            rendering::Limits {
                conversion_steps: 0,
                ..Default::default()
            },
            &|| Ok(())
        ),
        Err(Error::Budget)
    ));
    assert!(matches!(
        rendering::spatial(
            &project.snapshot(),
            id,
            output,
            Time::ZERO,
            [640, 220],
            Default::default(),
            &|| Err("cancel".into())
        ),
        Err(Error::Cancelled)
    ));
    let extrude = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.extrude")
        .unwrap()
        .id;
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == world)
        .unwrap()
        .parameters
        .insert("geometry".into(), Parameter::new(link(extrude)).unwrap());
    let (project, id) = render_snapshot(&c);
    assert!(matches!(
        rendering::spatial(
            &project.snapshot(),
            id,
            output,
            Time::ZERO,
            [640, 220],
            Default::default(),
            &|| Ok(())
        ),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn spatial_conversion_rejects_f32_underflow_and_observes_conversion_cancellation() {
    use fold_motion::{
        rendering::{self, Error},
        types::Space,
    };
    let (mut c, world) = world_fixture();
    spatial_only(&mut c);
    let output = c.definitions[0].outputs[0].id;
    let (project, id) = render_snapshot(&c);
    let calls = std::cell::Cell::new(0);
    let result = rendering::spatial(
        &project.snapshot(),
        id,
        output,
        Time::ZERO,
        [640, 220],
        rendering::Limits {
            conversion_steps: 0,
            ..Default::default()
        },
        &|| {
            calls.set(calls.get() + 1);
            Ok(())
        },
    );
    assert!(matches!(result, Err(Error::Budget)));
    let stop = calls.get();
    calls.set(0);
    let result = rendering::spatial(
        &project.snapshot(),
        id,
        output,
        Time::ZERO,
        [640, 220],
        Default::default(),
        &|| {
            calls.set(calls.get() + 1);
            if calls.get() == stop {
                Err("stop conversion".into())
            } else {
                Ok(())
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == world)
        .unwrap()
        .parameters
        .insert(
            "scale".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Vector {
                    value: vec![1e-100; 3],
                    unit: Unit::Unitless,
                    space: Space::World,
                },
            })
            .unwrap(),
        );
    evaluate(&c, Time::ZERO).unwrap();
    let (project, id) = render_snapshot(&c);
    let result = rendering::spatial(
        &project.snapshot(),
        id,
        output,
        Time::ZERO,
        [640, 220],
        Default::default(),
        &|| Ok(()),
    );
    assert!(
        matches!(result,Err(Error::Invalid(ref message)) if message.contains("Invalid spatial instance"))
    );
}

#[test]
fn screen_preparation_shares_native_curves_and_matches_text_and_outline_pixels() {
    use fold_motion::rendering;
    let (mut c, _) = outline_fixture("O B 8 O");
    c.canvas.width = 640;
    c.canvas.height = 220;
    for n in &mut c.definitions[0].nodes {
        if n.operation == "fold.motion.place" {
            n.parameters.insert(
                "opacity".into(),
                Parameter::new(Binding::Literal {
                    value: Literal::Scalar { value: 0.5 },
                })
                .unwrap(),
            );
        }
    }
    let (mut project, id) = render_snapshot(&c);
    let snapshot = project.snapshot();
    let output = c.definitions[0].outputs[0].id;
    let prepared = rendering::screen(
        &snapshot,
        id,
        output,
        Time::ZERO,
        [640, 220],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    let GeometryData::Outlines(outlines) = &prepared.native.entries[0].placement.geometry.data
    else {
        panic!()
    };
    let count = outlines.curves.glyphs.len();
    assert_eq!(prepared.drawings.len(), count * 2);
    for (i, glyph) in outlines.curves.glyphs.iter().enumerate() {
        assert!(Arc::ptr_eq(&glyph.path, &prepared.drawings[i].path));
        assert!(Arc::ptr_eq(
            &prepared.drawings[i].path,
            &prepared.drawings[i + count].path
        ));
        assert_eq!(prepared.drawings[i].transform, glyph.transform);
        assert_eq!(prepared.drawings[i].fill.unwrap()[3], 0.5);
        assert_eq!(prepared.drawings[i].stroke.as_ref().unwrap().color[3], 0.5);
        assert!(prepared.drawings[i + count].stroke.is_none());
    }
    let raster = |drawings| {
        fold_render::render(fold_render::RenderGraph {
            width: 640,
            height: 220,
            nodes: vec![fold_render::ImageOp::Vector(drawings)],
            output: 0,
        })
        .unwrap()
    };
    let outline_pixels = raster(prepared.drawings.clone());
    assert!(
        outline_pixels
            .pixels()
            .iter()
            .filter(|p| p[3] > 0.5)
            .count()
            > 5000
    );
    assert!(
        outline_pixels
            .pixels()
            .iter()
            .filter(|p| p[3] == 0.)
            .count()
            > 100_000
    );
    let text_id = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap()
        .id;
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.fill")
        .unwrap()
        .parameters
        .insert("geometry".into(), Parameter::new(link(text_id)).unwrap());
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
    let direct = rendering::screen(
        &project.snapshot(),
        id,
        output,
        Time::ZERO,
        [640, 220],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert!(direct.drawings == prepared.drawings);
    assert!(raster(direct.drawings).pixels() == outline_pixels.pixels());
    assert_eq!(prepared.revision, snapshot.revision());
    assert_ne!(prepared.revision, project.snapshot().revision());
    assert_eq!(prepared.document, id);
    assert_eq!(prepared.output, output);
    assert_eq!(prepared.duration, Time::new(1, 24).unwrap());
    let half = rendering::screen(
        &snapshot,
        id,
        output,
        Time::new(1, 2).unwrap(),
        [320, 110],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(half.time, Time::new(1, 2).unwrap());
    assert_eq!(half.dimensions, [320, 110]);
    for (full, half) in prepared.drawings.iter().zip(half.drawings.iter()) {
        assert_eq!(half.transform, full.transform.map(|v| v * 0.5));
        assert_eq!(half.path, full.path);
        assert_eq!(half.stroke, full.stroke); // width is in local canvas units
    }
}

#[test]
fn screen_preparation_rejects_world_and_stops_without_partial_publication() {
    use fold_motion::rendering::{self, Error};
    let (c, _) = world_fixture();
    let (project, id) = render_snapshot(&c);
    assert!(
        matches!(rendering::screen(&project.snapshot(), id, c.definitions[0].outputs[0].id,
        Time::ZERO, [640, 220], Default::default(), &|| Ok(())), Err(Error::Invalid(message)) if message.contains("World"))
    );
    let c = fixture();
    let (project, id) = render_snapshot(&c);
    let output = c.definitions[0].outputs[0].id;
    for steps in [0, 1, 10, 100] {
        assert!(matches!(
            rendering::screen(
                &project.snapshot(),
                id,
                output,
                Time::ZERO,
                [640, 220],
                rendering::Limits {
                    conversion_steps: steps,
                    ..Default::default()
                },
                &|| Ok(())
            ),
            Err(Error::Budget)
        ));
    }
    assert!(matches!(
        rendering::screen(
            &project.snapshot(),
            id,
            output,
            Time::ZERO,
            [640, 220],
            Default::default(),
            &|| Err("stop".into())
        ),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        rendering::screen(
            &project.snapshot(),
            id,
            output,
            Time::ZERO,
            [0, 220],
            Default::default(),
            &|| Ok(())
        ),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn screen_contours_preserve_order_translation_and_straight_paint() {
    use fold_motion::{
        rendering,
        types::{Alpha, Encoding, Space},
    };
    let mut c =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    c.canvas.width = 320;
    c.canvas.height = 200;
    let distance = |value| Binding::Literal {
        value: Literal::Distance {
            value,
            unit: Unit::Canvas,
        },
    };
    let shape = node(
        "shape",
        "rectangle",
        "Box",
        vec![("width", distance(200.)), ("height", distance(100.))],
    );
    let paint = |color| Binding::Literal {
        value: Literal::Color {
            value: color,
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        },
    };
    let red = node(
        "fill",
        "solid",
        "Red",
        vec![
            ("geometry", link(shape.id)),
            ("color", paint([1., 0., 0., 1.])),
        ],
    );
    let blue = node(
        "fill",
        "solid",
        "Blue",
        vec![
            ("geometry", link(shape.id)),
            ("color", paint([0., 0., 1., 0.5])),
        ],
    );
    let background = node(
        "place",
        "screen",
        "Background",
        vec![("geometry", link(red.id))],
    );
    let foreground = node(
        "place",
        "screen",
        "Foreground",
        vec![
            ("geometry", link(blue.id)),
            (
                "position",
                Binding::Literal {
                    value: Literal::Vector {
                        value: vec![100., 50.],
                        unit: Unit::Canvas,
                        space: Space::Screen,
                    },
                },
            ),
        ],
    );
    let scene = node(
        "scene",
        "ordered",
        "Scene",
        vec![(
            "children",
            Binding::List {
                values: vec![link(background.id), link(foreground.id)],
            },
        )],
    );
    c.definitions[0].outputs[0].value = Reference::Node { node: scene.id }.into();
    c.definitions[0].inputs.clear();
    c.definitions[0].nodes = vec![shape, red, blue, background, foreground, scene];
    let (project, id) = render_snapshot(&c);
    let prepared = rendering::screen(
        &project.snapshot(),
        id,
        c.definitions[0].outputs[0].id,
        Time::ZERO,
        [320, 200],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert!(Arc::ptr_eq(
        &prepared.drawings[0].path,
        &prepared.drawings[1].path
    ));
    assert_eq!(prepared.drawings[1].fill, Some([0., 0., 1., 0.5]));
    let frame = fold_render::render(fold_render::RenderGraph {
        width: 320,
        height: 200,
        nodes: vec![fold_render::ImageOp::Vector(prepared.drawings)],
        output: 0,
    })
    .unwrap();
    assert_eq!(frame.pixels()[10 * 320 + 10], [1., 0., 0., 1.]);
    assert_eq!(frame.pixels()[60 * 320 + 110], [0.5, 0., 0.5, 1.]);
    assert_eq!(frame.pixels()[120 * 320 + 250], [0., 0., 0.5, 0.5]);
    assert_eq!(frame.pixels()[180 * 320 + 310], [0.; 4]);
}

#[test]
fn image_preparation_preserves_screen_order_world_depth_runs_and_exact_bindings() {
    use fold_motion::rendering;
    use fold_render::ImageOp;
    let (mut c, _) = world_fixture();
    let scene = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "Integrated")
        .unwrap();
    let Binding::List { values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    // Background; two World objects sharing depth; foreground; another World pass.
    let ordered = [0, 1, 2, 0, 1, 3]
        .map(|i| {
            let Binding::Link { reference } = &values[i] else {
                panic!()
            };
            Binding::Link {
                reference: reference.target.into(),
            }
        })
        .to_vec();
    scene.parameters.insert(
        "children".into(),
        Parameter::new(Binding::List { values: ordered }).unwrap(),
    );
    let (project, id) = render_snapshot(&c);
    let snapshot = project.snapshot();
    let time = Time::new(1, 7).unwrap();
    let output = c.definitions[0].outputs[0].id;
    let graph = rendering::image(
        &snapshot,
        id,
        output,
        time,
        [640, 220],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(graph.nodes.len(), 7);
    assert_eq!(graph.output, 6);
    let ImageOp::Vector(background) = &graph.nodes[0] else {
        panic!()
    };
    let ImageOp::Spatial {
        scene: first,
        binding,
    } = &graph.nodes[1]
    else {
        panic!()
    };
    assert_eq!(binding.document, id);
    assert_eq!(binding.revision, snapshot.revision().0);
    assert_eq!(binding.time, time);
    assert_eq!(binding.duration, Time::new(1, 24).unwrap());
    assert_eq!(binding.dimensions, [640, 220]);
    assert_eq!(first.lights.len(), 1);
    assert!(matches!(
        graph.nodes[2],
        ImageOp::Over {
            foreground: 1,
            background: 0
        }
    ));
    let ImageOp::Vector(foreground) = &graph.nodes[3] else {
        panic!()
    };
    assert_eq!(background, foreground);
    assert!(matches!(
        graph.nodes[4],
        ImageOp::Over {
            foreground: 3,
            background: 2
        }
    ));
    let ImageOp::Spatial {
        scene: second,
        binding: other,
    } = &graph.nodes[5]
    else {
        panic!()
    };
    assert_eq!(binding, other);
    assert_eq!(first.instances.len(), second.instances.len() * 2);
    assert!(matches!(
        graph.nodes[6],
        ImageOp::Over {
            foreground: 5,
            background: 4
        }
    ));
    assert!(matches!(
        rendering::image(
            &snapshot,
            id,
            output,
            time,
            [640, 220],
            rendering::Limits {
                conversion_steps: 2,
                ..Default::default()
            },
            &|| Ok(())
        ),
        Err(rendering::Error::Budget)
    ));
    assert!(matches!(
        rendering::image(
            &snapshot,
            id,
            output,
            time,
            [640, 220],
            Default::default(),
            &|| Err("cancel".into())
        ),
        Err(rendering::Error::Cancelled)
    ));
}

#[test]
fn native_word_ids_survive_metric_changes_and_reflow() {
    let mut c = fixture();
    let before = text(&c);
    let node = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap();
    for (name, value) in [
        ("wrap", Literal::Bool { value: true }),
        (
            "width",
            Literal::Distance {
                value: 180.,
                unit: Unit::Canvas,
            },
        ),
        (
            "size",
            Literal::Distance {
                value: 70.,
                unit: Unit::Canvas,
            },
        ),
    ] {
        node.parameters.get_mut(name).unwrap().value =
            serde_json::to_value(Binding::Literal { value }).unwrap();
    }
    let after = text(&c);
    assert_eq!(before.word_ids, after.word_ids);
    assert_eq!(after.word_ids.len(), 3);
    assert!(after.layout.line_count > before.layout.line_count);
    assert_ne!(before.layout.words[0].pivot, after.layout.words[0].pivot);
    assert_eq!(
        before
            .layout
            .words
            .iter()
            .map(|w| w.text.clone())
            .collect::<Vec<_>>(),
        after
            .layout
            .words
            .iter()
            .map(|w| w.text.clone())
            .collect::<Vec<_>>()
    );
}

fn animated_words_fixture() -> Construction {
    let mut c = fixture();
    let source = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap()
        .id;
    let position = |y| Binding::Literal {
        value: Literal::Vector {
            value: vec![0., y],
            unit: Unit::Canvas,
            space: fold_motion::types::Space::Screen,
        },
    };
    let tween = node(
        "tween",
        "cubic_out",
        "Entrance",
        vec![
            ("from", position(80.)),
            ("to", position(0.)),
            (
                "duration",
                Binding::Literal {
                    value: Literal::Duration {
                        value: Time::new(3, 5).unwrap(),
                    },
                },
            ),
        ],
    );
    let animator = node(
        "text_animator",
        "word_position",
        "Words",
        vec![("text", link(source)), ("position", link(tween.id))],
    );
    let original = node(
        "place",
        "screen",
        "Original",
        vec![("geometry", link(source)), ("position", link(tween.id))],
    );
    let definition = &mut c.definitions[0];
    definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.fill")
        .unwrap()
        .parameters
        .get_mut("geometry")
        .unwrap()
        .value = serde_json::to_value(link(animator.id)).unwrap();
    let scene = definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.push(link(original.id));
    scene.parameters.get_mut("children").unwrap().value =
        serde_json::to_value(Binding::List { values }).unwrap();
    definition.nodes.extend([tween, animator, original]);
    c
}

#[test]
fn word_animator_samples_tween_per_word_without_reshaping_or_cache_contamination() {
    let c = animated_words_fixture();
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let prepared = Prepared::new(c.clone()).unwrap();
    let mut identities = None;
    for time in [
        Time::new(1, 2).unwrap(),
        Time::ZERO,
        Time::new(7, 100).unwrap(),
        Time::new(1, 4).unwrap(),
        Time::new(3, 4).unwrap(),
        Time::new(4, 1).unwrap(),
        Time::new(-1, 24).unwrap(),
    ] {
        let Value::Scene(scene) = prepared
            .evaluate(
                c.definitions[0].outputs[0].id,
                time,
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap()
        else {
            panic!()
        };
        let GeometryData::Text(animated) = &scene.entries[0].placement.geometry.data else {
            panic!()
        };
        let GeometryData::Text(original) = &scene.entries[2].placement.geometry.data else {
            panic!()
        };
        assert!(Arc::ptr_eq(&animated.layout, &original.layout));
        assert_eq!(animated.glyph_ids, original.glyph_ids);
        assert_eq!(animated.word_ids, original.word_ids);
        assert!(original.offsets.iter().all(|p| *p == [0.; 2]));
        if let Some((id, words)) = &identities {
            assert_eq!(*id, scene.entries[0].placement.geometry.id);
            assert_eq!(words, &animated.word_ids);
        }
        identities = Some((
            scene.entries[0].placement.geometry.id,
            animated.word_ids.clone(),
        ));
        let seconds = time.numerator() as f64 / f64::from(time.denominator());
        for (index, word) in animated.layout.words.iter().enumerate() {
            let progress = ((seconds - index as f64 * 0.07) / 0.6).clamp(0., 1.);
            let y = 80. * (1. - progress).powi(3);
            for &glyph in &word.glyphs {
                assert!((animated.offsets[glyph][1] - y).abs() < 1e-10);
                assert_eq!(animated.offsets[glyph][0], 0.);
            }
        }
        let fold_motion::evaluation::placement::Transform::Screen { position } =
            scene.entries[2].placement.transform
        else {
            panic!()
        };
        assert!((position[1] - 80. * (1. - (seconds / 0.6).clamp(0., 1.)).powi(3)).abs() < 1e-10);
    }
}

#[test]
fn animated_text_and_outline_screen_pixels_agree_with_retained_layout() {
    use fold_motion::rendering;
    let mut c = animated_words_fixture();
    let time = Time::new(1, 4).unwrap();
    let (project, id) = render_snapshot(&c);
    let direct = rendering::screen(
        &project.snapshot(),
        id,
        c.definitions[0].outputs[0].id,
        time,
        [640, 360],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    let animator = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.text_animator")
        .unwrap()
        .id;
    let outlines = node(
        "outline_text",
        "curves",
        "Animated curves",
        vec![("text", link(animator))],
    );
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.fill")
        .unwrap()
        .parameters
        .get_mut("geometry")
        .unwrap()
        .value = serde_json::to_value(link(outlines.id)).unwrap();
    c.definitions[0].nodes.push(outlines);
    let (project, id) = render_snapshot(&c);
    let curves = rendering::screen(
        &project.snapshot(),
        id,
        c.definitions[0].outputs[0].id,
        time,
        [640, 360],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(direct.drawings, curves.drawings);
    let render = |drawings| {
        fold_render::render(fold_render::RenderGraph {
            width: 640,
            height: 360,
            nodes: vec![fold_render::ImageOp::Vector(drawings)],
            output: 0,
        })
        .unwrap()
    };
    assert!(render(direct.drawings).pixels() == render(curves.drawings).pixels());
}

#[test]
fn word_animator_keeps_scene_time_global_and_rejects_invalid_delay_or_limits() {
    use fold_motion::{
        expression::{ClockScope, Expr},
        rendering,
    };
    let mut c = animated_words_fixture();
    c.root.clock.offset = Time::new(1, 10).unwrap();
    c.root.clock.rate = fold_motion::types::Ratio::new(2, 1).unwrap();
    // Scene 0.3 maps to asset 0.4. Stagger is measured in that local clock,
    // after the mapping, rather than being scaled a second time.
    let Value::Scene(local) = evaluate(&c, Time::new(3, 10).unwrap()).unwrap() else {
        panic!()
    };
    let GeometryData::Text(text) = &local.entries[0].placement.geometry.data else {
        panic!()
    };
    for (index, word) in text.layout.words.iter().enumerate() {
        let progress = ((0.4 - index as f64 * 0.07) / 0.6).clamp(0., 1.);
        for &glyph in &word.glyphs {
            assert!((text.offsets[glyph][1] - 80. * (1. - progress).powi(3)).abs() < 1e-10);
        }
    }
    let animator = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.text_animator")
        .unwrap()
        .id;
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.tween")
        .unwrap()
        .parameters
        .get_mut("time")
        .unwrap()
        .value = serde_json::to_value(Binding::Expression {
        expression: Expr::Time {
            scope: ClockScope::Scene,
        },
    })
    .unwrap();
    let Value::Scene(scene) = evaluate(&c, Time::new(3, 10).unwrap()).unwrap() else {
        panic!()
    };
    let GeometryData::Text(text) = &scene.entries[0].placement.geometry.data else {
        panic!()
    };
    for word in &text.layout.words {
        for &glyph in &word.glyphs {
            assert!((text.offsets[glyph][1] - 10.).abs() < 1e-10);
        }
    }
    let prepared = Prepared::new(c.clone()).unwrap();
    let output = c.definitions[0].outputs[0].id;
    assert_eq!(
        prepared
            .evaluate_detailed(
                output,
                Time::ZERO,
                Limits {
                    steps: 1,
                    ..Default::default()
                },
                &|| Ok(())
            )
            .unwrap_err()
            .kind,
        FailureKind::Budget
    );
    assert_eq!(
        prepared
            .evaluate_detailed(
                output,
                Time::ZERO,
                Limits::default(),
                &|| Err("stop".into())
            )
            .unwrap_err()
            .kind,
        FailureKind::Cancelled
    );
    let (project, id) = render_snapshot(&c);
    assert!(matches!(
        rendering::screen(
            &project.snapshot(),
            id,
            output,
            Time::ZERO,
            [640, 360],
            rendering::Limits {
                conversion_steps: 1,
                ..Default::default()
            },
            &|| Ok(())
        ),
        Err(rendering::Error::Budget)
    ));
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == animator)
        .unwrap()
        .parameters
        .get_mut("stagger")
        .unwrap()
        .value = serde_json::to_value(Binding::Literal {
        value: Literal::Duration {
            value: Time::new(-1, 100).unwrap(),
        },
    })
    .unwrap();
    let failure = evaluate(&c, Time::ZERO).unwrap_err();
    assert_eq!(failure.kind, FailureKind::Invalid);
    assert_eq!(failure.context.unwrap().operation, animator);
}

#[test]
fn word_animation_reaches_metric_extrusion_without_changing_layout_or_topology_ids() {
    let (mut c, _) = world_fixture();
    let text_id = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.text")
        .unwrap()
        .id;
    let animated = animated_words_fixture();
    let mut nodes: Vec<_> = animated.definitions[0]
        .nodes
        .iter()
        .filter(|n| {
            n.operation == "fold.motion.tween" || n.operation == "fold.motion.text_animator"
        })
        .cloned()
        .collect();
    let animator = nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.text_animator")
        .unwrap();
    animator.parameters.get_mut("text").unwrap().value =
        serde_json::to_value(link(text_id)).unwrap();
    let animator_id = animator.id;
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.outline_text")
        .unwrap()
        .parameters
        .get_mut("text")
        .unwrap()
        .value = serde_json::to_value(link(animator_id)).unwrap();
    c.definitions[0].nodes.extend(nodes);
    let solids = |time| {
        let Value::Scene(scene) = evaluate(&c, time).unwrap() else {
            panic!()
        };
        let GeometryData::Solids(solids) = &scene.entries[1].placement.geometry.data else {
            panic!()
        };
        solids.clone()
    };
    let initial = solids(Time::ZERO);
    // This extrusion fixture has four words, so its last entrance ends at 0.81s.
    let settled = solids(Time::new(1, 1).unwrap());
    let unit = initial.prepared.meters_per_canvas;
    for (a, b) in initial.prepared.glyphs.iter().zip(&settled.prepared.glyphs) {
        if a.solid.is_some() {
            assert!((a.transform[5] - b.transform[5] + 80. * unit).abs() < 1e-10);
            assert_eq!(a.transform[4], b.transform[4]);
        }
    }
    for (a, b) in initial.glyphs.iter().zip(&settled.glyphs) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.source_glyph, b.source_glyph);
        assert_eq!(
            a.topology.as_ref().map(|v| v.id),
            b.topology.as_ref().map(|v| v.id)
        );
    }
}

#[test]
fn native_box_preserves_topology_and_shares_mesh_through_material_and_world() {
    use fold_motion::{rendering, types::Space};
    let (mut c, _) = world_fixture();
    spatial_only(&mut c);
    let box_node = node("mesh", "box", "Box prototype", vec![]);
    let box_id = box_node.id;
    for material in c.definitions[0]
        .nodes
        .iter_mut()
        .filter(|n| n.operation == "fold.motion.material")
    {
        material
            .parameters
            .insert("geometry".into(), Parameter::new(link(box_id)).unwrap());
    }
    c.definitions[0].nodes.push(box_node);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let mut ids = None;
    for size in [[0.06; 3], [0.12, 0.03, 0.09]] {
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == box_id)
            .unwrap()
            .parameters
            .insert(
                "size".into(),
                Parameter::new(Binding::Literal {
                    value: Literal::Vector {
                        value: size.to_vec(),
                        unit: Unit::Meters,
                        space: Space::World,
                    },
                })
                .unwrap(),
            );
        let Value::Scene(scene) = evaluate(&c, Time::new(1, 2).unwrap()).unwrap() else {
            panic!()
        };
        let GeometryData::Mesh(first) = &scene.entries[0].placement.geometry.data else {
            panic!()
        };
        let GeometryData::Mesh(second) = &scene.entries[1].placement.geometry.data else {
            panic!()
        };
        assert!(Arc::ptr_eq(first, second));
        let current = (
            scene.entries[0].placement.geometry.id,
            first.vertex_ids.clone(),
            first.face_ids.clone(),
        );
        if let Some(previous) = &ids {
            assert_eq!(previous, &current);
        }
        ids = Some(current);
        assert_eq!(first.prepared.mesh.positions.len(), 24);
        assert_eq!(first.prepared.uv.len(), 24);
        assert!(
            first
                .prepared
                .mesh
                .positions
                .iter()
                .all(|p| p.map(f32::abs) == size.map(|v| v as f32 * 0.5))
        );
        let (project, id) = render_snapshot(&c);
        let frame = rendering::spatial(
            &project.snapshot(),
            id,
            c.definitions[0].outputs[0].id,
            Time::new(1, 2).unwrap(),
            [640, 360],
            rendering::Limits::default(),
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(frame.scene.meshes.len(), 1);
        assert_eq!(frame.scene.instances.len(), 2);
        assert_eq!(frame.scene.materials.len(), 2);
        assert_eq!(frame.scene.instances[0].mesh, frame.scene.instances[1].mesh);
        assert_eq!(frame.scene.instances[0].transform.translation, [1., 2., 3.]);
        assert_eq!(frame.scene.instances[0].transform.scale, [2., 3., 4.]);
        assert_eq!(frame.scene.instances[1].transform.translation, [0.; 3]);
        let GeometryData::Mesh(retained) = &frame.native.entries[0].placement.geometry.data else {
            panic!()
        };
        assert!(Arc::ptr_eq(
            frame.scene.meshes.values().next().unwrap(),
            &retained.prepared.mesh
        ));
    }
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == box_id)
        .unwrap()
        .parameters
        .insert(
            "size".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Vector {
                    value: vec![0., 0.06, 0.06],
                    unit: Unit::Meters,
                    space: Space::World,
                },
            })
            .unwrap(),
        );
    let error = evaluate(&c, Time::ZERO).unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, box_id);
}

#[test]
fn camera_plane_preparation_preserves_text_paths_paint_snapshot_and_pass_order() {
    use fold_motion::{model::Parameter, rendering};
    use fold_render::ImageOp;
    let mut c = fixture();
    let camera = node("camera", "perspective", "Plane Camera", vec![]);
    let camera_id = camera.id;
    let d = &mut c.definitions[0];
    for placement in d
        .nodes
        .iter_mut()
        .filter(|n| n.operation == "fold.motion.place")
    {
        placement.mode = "camera_plane_viewport".into();
        placement
            .parameters
            .insert("camera".into(), Parameter::new(link(camera_id)).unwrap());
        placement.parameters.insert(
            "distance".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Distance {
                    value: 2.,
                    unit: Unit::Meters,
                },
            })
            .unwrap(),
        );
    }
    let scene = d
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    scene.mode = "spatial".into();
    scene
        .parameters
        .insert("camera".into(), Parameter::new(link(camera_id)).unwrap());
    d.nodes.push(camera);
    let (mut project, id) = render_snapshot(&c);
    let output = c.definitions[0].outputs[0].id;
    let snapshot = project.snapshot();
    let prepared = rendering::spatial(
        &snapshot,
        id,
        output,
        Time::ZERO,
        [640, 360],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert!(!prepared.scene.instances.is_empty());
    assert!(prepared.scene.materials.values().all(|m| m.unlit));
    let normal = {
        let camera = &prepared.scene.camera;
        let v: [f32; 3] = std::array::from_fn(|i| camera.position[i] - camera.target[i]);
        let length = v.iter().map(|v| v * v).sum::<f32>().sqrt();
        v.map(|v| v / length)
    };
    for mesh in prepared.scene.meshes.values() {
        for point in &mesh.positions {
            let depth: f32 = (0..3)
                .map(|i| (prepared.scene.camera.position[i] - point[i]) * normal[i])
                .sum();
            assert!((depth - 2.).abs() < 2e-5);
        }
        for ids in mesh.indices.chunks_exact(3) {
            // Measure the published f32 coordinates in f64: near-collinear
            // triangles lose the sign through cancellation in f32 products.
            let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[ids[i] as usize].map(f64::from));
            let u: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
            let v: [f64; 3] = std::array::from_fn(|i| c[i] - a[i]);
            let cross = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            assert!((0..3).map(|i| cross[i] * f64::from(normal[i])).sum::<f64>() >= 0.);
        }
    }
    let graph = rendering::image(
        &snapshot,
        id,
        output,
        Time::ZERO,
        [640, 360],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(graph.nodes.len(), 1);
    assert!(
        matches!(&graph.nodes[0],ImageOp::Spatial {scene,..} if scene.instances.len()==prepared.scene.instances.len())
    );
    let placement = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap();
    placement.mode = "screen".into();
    placement.parameters.remove("camera");
    placement.parameters.remove("distance");
    project
        .commit(fold_project::EditBatch {
            base: snapshot.revision(),
            mutations: vec![fold_project::Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
    let mixed = rendering::image(
        &project.snapshot(),
        id,
        output,
        Time::ZERO,
        [640, 360],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert!(matches!(
        mixed.nodes.as_slice(),
        [
            ImageOp::Vector(_),
            ImageOp::Spatial { .. },
            ImageOp::Over { .. }
        ]
    ));
    let held = rendering::spatial(
        &snapshot,
        id,
        output,
        Time::ZERO,
        [640, 360],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(held.scene.meshes, prepared.scene.meshes);
    for steps in [0, 100] {
        assert!(matches!(
            rendering::spatial(
                &snapshot,
                id,
                output,
                Time::ZERO,
                [640, 360],
                rendering::Limits {
                    conversion_steps: steps,
                    ..Default::default()
                },
                &|| Ok(())
            ),
            Err(rendering::Error::Budget)
        ));
    }
    assert!(matches!(
        rendering::spatial(
            &snapshot,
            id,
            output,
            Time::ZERO,
            [640, 360],
            Default::default(),
            &|| Err("stop".into())
        ),
        Err(rendering::Error::Cancelled)
    ));
}

#[test]
fn native_stroke_round_joins_change_screen_pixels_and_shared_planar_meshes() {
    use fold_motion::{rendering, types::Space};
    let mut c =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    c.canvas.width = 160;
    c.canvas.height = 160;
    let shape = node("shape", "rectangle", "Rectangle", vec![]);
    let stroke = node(
        "stroke",
        "solid_centered_butt_miter",
        "Stroke",
        vec![
            ("geometry", link(shape.id)),
            (
                "width",
                Binding::Literal {
                    value: Literal::Distance {
                        value: 20.,
                        unit: Unit::Canvas,
                    },
                },
            ),
        ],
    );
    let place = node(
        "place",
        "screen",
        "Place",
        vec![
            ("geometry", link(stroke.id)),
            (
                "position",
                Binding::Literal {
                    value: Literal::Vector {
                        value: vec![30., 30.],
                        unit: Unit::Canvas,
                        space: Space::Screen,
                    },
                },
            ),
        ],
    );
    let scene = node(
        "scene",
        "ordered",
        "Scene",
        vec![(
            "children",
            Binding::List {
                values: vec![link(place.id)],
            },
        )],
    );
    c.definitions[0].outputs[0].value = Reference::Node { node: scene.id }.into();
    c.definitions[0].nodes = vec![shape, stroke, place, scene];
    let mut pixels = vec![];
    for mode in ["solid_centered_butt_miter", "solid_centered_round_round"] {
        c.definitions[0].nodes[1].mode = mode.into();
        let (project, id) = render_snapshot(&c);
        let frame = rendering::screen(
            &project.snapshot(),
            id,
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            [160, 160],
            Default::default(),
            &|| Ok(()),
        )
        .unwrap();
        let drawing = frame.drawings[0].clone();
        let mesh = fold_render::spatial::planar::Cache::default()
            .prepare_with(&drawing, 1., &mut || Ok(()))
            .unwrap()
            .stroke
            .unwrap();
        let frame = fold_render::render(fold_render::RenderGraph {
            width: 160,
            height: 160,
            nodes: vec![fold_render::ImageOp::Vector(frame.drawings)],
            output: 0,
        })
        .unwrap();
        pixels.push((frame.pixels()[21 * 160 + 21][3], mesh.positions.len()));
    }
    assert_eq!(pixels[0].0, 1.);
    assert_eq!(pixels[1].0, 0.);
    assert_ne!(pixels[0].1, pixels[1].1);
}
