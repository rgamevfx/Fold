use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{Geometry, GeometryData, Limits, Prepared, Value},
    model::{Construction, Node, Parameter, Reference},
    registry, source,
    types::{Literal, Space, Type, Unit},
};
use std::sync::Arc;
fn link(id: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node: id }.into(),
    }
}
fn literal(value: Literal) -> Binding {
    Binding::Literal { value }
}
fn node(key: &str, mode: &str, bindings: Vec<(&str, Binding)>) -> Node {
    registry::resolve(&format!("fold.motion.{key}"), 1, mode)
        .unwrap()
        .node(
            format!("{key}_{mode}"),
            bindings.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        )
        .unwrap()
}
fn fixture(nodes: Vec<Node>) -> Construction {
    let mut c =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    let d = &mut c.definitions[0];
    d.outputs[0].ty = Type::Geometry;
    d.outputs[0].value = Reference::Node {
        node: nodes.last().unwrap().id,
    }
    .into();
    d.nodes = nodes;
    c
}
fn geometry(c: &Construction) -> Arc<Geometry> {
    let restored = source::parse(&source::canonical(c).unwrap())
        .unwrap()
        .construction;
    assert_eq!(&restored, c);
    let Value::Geometry(g) = Prepared::new(restored)
        .unwrap()
        .evaluate(
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap()
    else {
        panic!()
    };
    g
}
fn vector(values: &[f64], unit: Unit, space: Space) -> Binding {
    literal(Literal::Vector {
        value: values.to_vec(),
        unit,
        space,
    })
}
#[test]
fn profile_rectangle_caps_regions_uv_and_depth_correspondence() {
    let shape = node("shape", "rectangle", vec![]);
    let tess = node("tessellate", "planar", vec![("profile", link(shape.id))]);
    let solid = node(
        "extrude",
        "closed_profile",
        vec![("profile", link(shape.id))],
    );
    let cap = geometry(&fixture(vec![shape.clone(), tess]));
    let GeometryData::Mesh(cap) = &cap.data else {
        panic!()
    };
    let c = fixture(vec![shape, solid]);
    let output = geometry(&c);
    let GeometryData::Mesh(solid) = &output.data else {
        panic!()
    };
    assert_eq!(cap.prepared.mesh.indices.len(), 6);
    assert_eq!(solid.prepared.mesh.indices.len(), 36);
    assert_eq!(solid.prepared.regions["front"], 0..6);
    assert_eq!(solid.prepared.regions["back"], 6..12);
    assert_eq!(solid.prepared.regions["sides"], 12..36);
    for (i, &source) in solid.prepared.source_vertices.iter().enumerate() {
        assert_eq!(solid.prepared.uv[i], cap.prepared.uv[source as usize]);
        assert_eq!(
            solid.prepared.mesh.positions[i][..2],
            cap.prepared.mesh.positions[source as usize][..2]
        );
    }
    let mut changed = c.clone();
    changed.definitions[0].nodes[1].parameters.insert(
        "depth".into(),
        Parameter::new(literal(Literal::Distance {
            value: 0.3,
            unit: Unit::Meters,
        }))
        .unwrap(),
    );
    let deeper = geometry(&changed);
    let GeometryData::Mesh(deeper) = &deeper.data else {
        panic!()
    };
    assert_eq!(deeper.vertex_ids, solid.vertex_ids);
    assert_eq!(deeper.face_ids, solid.face_ids);
    assert!(
        deeper
            .prepared
            .mesh
            .positions
            .iter()
            .any(|p| (p[2] + 0.3).abs() < 1e-6)
    );
}
fn area(mesh: &fold_render::spatial::Mesh) -> f64 {
    mesh.indices
        .chunks_exact(3)
        .map(|t| {
            let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[t[i] as usize]);
            f64::from((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.
        })
        .sum()
}
#[test]
fn public_profile_text_holes_match_explicit_outline_cap() {
    let text = node(
        "text",
        "packaged",
        vec![(
            "text",
            literal(Literal::Text {
                value: "O B 8".into(),
            }),
        )],
    );
    let outline = node("outline_text", "curves", vec![("text", link(text.id))]);
    let mut areas = vec![];
    for even_odd in [false, true] {
        let tess = node(
            "tessellate",
            "planar",
            vec![
                ("profile", link(outline.id)),
                ("even_odd", literal(Literal::Bool { value: even_odd })),
            ],
        );
        let g = geometry(&fixture(vec![text.clone(), outline.clone(), tess]));
        let GeometryData::Mesh(m) = &g.data else {
            panic!()
        };
        assert!(m.prepared.mesh.positions.len() > 40);
        areas.push(area(&m.prepared.mesh));
        let solid = node(
            "extrude",
            "closed_profile",
            vec![
                ("profile", link(outline.id)),
                ("even_odd", literal(Literal::Bool { value: even_odd })),
            ],
        );
        let g = geometry(&fixture(vec![text.clone(), outline.clone(), solid]));
        let GeometryData::Mesh(m) = &g.data else {
            panic!()
        };
        let front = &m.prepared.regions["front"];
        let cap = fold_render::spatial::Mesh {
            positions: m.prepared.mesh.positions.clone(),
            normals: m.prepared.mesh.normals.clone(),
            indices: m.prepared.mesh.indices[front.clone()].to_vec(),
        };
        assert!((area(&cap) - areas.last().unwrap()).abs() < 1e-8);
        // Euler characteristic: five glyph holes across three disjoint letter components.
        let vertices: std::collections::BTreeSet<_> = cap.indices.iter().copied().collect();
        let edges: std::collections::BTreeSet<_> = cap
            .indices
            .chunks_exact(3)
            .flat_map(|t| {
                [
                    (t[0].min(t[1]), t[0].max(t[1])),
                    (t[1].min(t[2]), t[1].max(t[2])),
                    (t[2].min(t[0]), t[2].max(t[0])),
                ]
            })
            .collect();
        assert_eq!(
            vertices.len() as isize - edges.len() as isize + (cap.indices.len() / 3) as isize,
            -2
        ); // O=1, B=2, 8=2 holes.
    }
    assert!((areas[0] - areas[1]).abs() < 1e-8);
}
#[test]
fn screen_transform_pivot_and_mesh_inverse_transpose_normals() {
    let shape = node("shape", "rectangle", vec![]);
    let mut construction = fixture(vec![shape.clone()]);
    let original = geometry(&construction);
    let transform = node(
        "transform",
        "screen",
        vec![
            ("geometry", link(shape.id)),
            ("pivot", vector(&[50., 50.], Unit::Canvas, Space::Screen)),
            (
                "translation",
                vector(&[10., 20.], Unit::Canvas, Space::Screen),
            ),
            ("scale", vector(&[2., 1.], Unit::Unitless, Space::Screen)),
            (
                "angle",
                literal(Literal::Scalar {
                    value: std::f64::consts::FRAC_PI_2,
                }),
            ),
        ],
    );
    construction.definitions[0].outputs[0].value = Reference::Node { node: transform.id }.into();
    construction.definitions[0].nodes.push(transform);
    let g = geometry(&construction);
    let (GeometryData::Contours(a), GeometryData::Contours(b)) = (&original.data, &g.data) else {
        panic!()
    };
    assert_eq!(a[0].points[0].id, b[0].points[0].id);
    assert!((b[0].points[0].position[0] - 110.).abs() < 1e-8);
    assert!((b[0].points[0].position[1] + 30.).abs() < 1e-8);
    let mesh = node("mesh", "box", vec![]);
    let transform = node(
        "transform",
        "world_mesh",
        vec![
            ("geometry", link(mesh.id)),
            ("pivot", vector(&[1., 0., 0.], Unit::Meters, Space::World)),
            ("scale", vector(&[2., 3., 4.], Unit::Unitless, Space::World)),
            (
                "rotation",
                literal(Literal::Rotation {
                    value: [0., 0., 0.5f64.sqrt(), 0.5f64.sqrt()],
                    space: Space::World,
                }),
            ),
        ],
    );
    let g = geometry(&fixture(vec![mesh, transform]));
    let GeometryData::Mesh(m) = &g.data else {
        panic!()
    };
    for t in m.prepared.mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| m.prepared.mesh.positions[t[i] as usize]);
        let n = m.prepared.mesh.normals[t[0] as usize];
        for p in [b, c] {
            assert!((0..3).map(|i| (p[i] - a[i]) * n[i]).sum::<f32>().abs() < 1e-5);
        }
        assert!((n.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 1e-5);
    }
}

#[test]
fn transformed_instances_share_prototype_and_fields_sample_the_selected_upstream_positions() {
    let mesh = node("mesh", "box", vec![]);
    let grid = node("distribute", "grid", vec![]);
    let cloner = node(
        "cloner",
        "points",
        vec![("prototype", link(mesh.id)), ("points", link(grid.id))],
    );
    let mut c = fixture(vec![mesh, grid, cloner.clone()]);
    let original = geometry(&c);
    let transform = node(
        "transform",
        "world_instances",
        vec![
            ("geometry", link(cloner.id)),
            (
                "translation",
                vector(&[2., 0., 0.], Unit::Meters, Space::World),
            ),
            ("pivot", vector(&[0.5, 0., 0.], Unit::Meters, Space::World)),
            (
                "angle",
                literal(Literal::Scalar {
                    value: std::f64::consts::FRAC_PI_2,
                }),
            ),
            ("scale", literal(Literal::Scalar { value: 2. })),
        ],
    );
    let field = node(
        "field",
        "radial",
        vec![("center", vector(&[0., 0., 0.], Unit::Meters, Space::World))],
    );
    let ramp = registry::resolve("fold.motion.ramp", 1, "scalar_linear")
        .unwrap()
        .node(
            "Response",
            [
                ("position".into(), link(field.id)),
                (
                    "stops".into(),
                    Binding::Ramp {
                        ramp: fold_motion::ramp::Ramp::two(
                            Literal::Scalar { value: 0.2 },
                            Literal::Scalar { value: 1. },
                        )
                        .unwrap(),
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let animate = node(
        "animator",
        "scale_multiply",
        vec![("geometry", link(transform.id)), ("factor", link(ramp.id))],
    );
    c.definitions[0].outputs[0].value = Reference::Node { node: animate.id }.into();
    c.definitions[0]
        .nodes
        .extend([transform, field, ramp, animate]);
    let g = geometry(&c);
    let (GeometryData::Instances(before), GeometryData::Instances(after)) =
        (&original.data, &g.data)
    else {
        panic!()
    };
    assert_eq!(before.prototype, after.prototype);
    assert_eq!(before.distribution, after.distribution);
    for (a, b) in before.instances.iter().zip(&after.instances) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.source_point, b.source_point);
        assert!((b.position[0] - (2.5 - 2. * a.position[1])).abs() < 1e-8);
        assert!((b.position[1] - (2. * a.position[0] - 1.)).abs() < 1e-8);
        let influence = (1. - b.position.iter().map(|x| x * x).sum::<f64>().sqrt()).clamp(0., 1.);
        assert!((b.scale - 2. * (0.2 + 0.8 * influence)).abs() < 1e-8);
    }
}

#[test]
fn conversions_and_transforms_reject_invalid_controls_and_never_return_partial_results() {
    use fold_motion::evaluation::FailureKind;
    let shape = node("shape", "rectangle", vec![]);
    let solid = node(
        "extrude",
        "closed_profile",
        vec![("profile", link(shape.id))],
    );
    let c = fixture(vec![shape, solid]);
    for (parameter, value) in [
        (
            "depth",
            Literal::Distance {
                value: -0.1,
                unit: Unit::Meters,
            },
        ),
        ("segments", Literal::Integer { value: 4097 }),
        (
            "canvas_unit",
            Literal::Distance {
                value: 0.,
                unit: Unit::Meters,
            },
        ),
    ] {
        let mut invalid = c.clone();
        let operation = &mut invalid.definitions[0].nodes[1];
        operation
            .parameters
            .insert(parameter.into(), Parameter::new(literal(value)).unwrap());
        let failure = Prepared::new(invalid.clone())
            .unwrap()
            .evaluate_detailed(
                invalid.definitions[0].outputs[0].id,
                Time::ZERO,
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap_err();
        assert_eq!(failure.kind, FailureKind::Invalid);
        assert_eq!(
            failure.context.unwrap().operation,
            invalid.definitions[0].nodes[1].id
        );
    }
    let prepared = Prepared::new(c.clone()).unwrap();
    let output = c.definitions[0].outputs[0].id;
    let failure = prepared
        .evaluate_detailed(
            output,
            Time::ZERO,
            Limits {
                steps: 10,
                elements: 100_000,
            },
            &|| Ok(()),
        )
        .unwrap_err();
    assert_eq!(failure.kind, FailureKind::Budget);
    let failure = prepared
        .evaluate_detailed(output, Time::ZERO, Limits::default(), &|| {
            Err("cancelled by user".into())
        })
        .unwrap_err();
    assert_eq!(failure.kind, FailureKind::Cancelled);
    assert!(
        prepared
            .evaluate(output, Time::ZERO, Limits::default(), &|| Ok(()))
            .is_ok()
    );
}
