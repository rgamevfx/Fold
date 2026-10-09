use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{FailureKind, Limits, Prepared, Value},
    model::{Construction, Parameter, Reference},
    registry, source,
    types::{Literal, Space, Type, Unit},
};
use std::collections::BTreeMap;

fn fixture() -> (Construction, ObjectId) {
    let mut c =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    let grid = registry::resolve("fold.motion.distribute", 2, "grid")
        .unwrap()
        .node("Grid", Default::default())
        .unwrap();
    let id = grid.id;
    c.definitions[0].nodes = vec![grid];
    c.definitions[0].outputs[0].ty = Type::Geometry;
    c.definitions[0].outputs[0].value = Reference::Node { node: id }.into();
    (c, id)
}
fn set(c: &mut Construction, key: &str, value: Literal) {
    c.definitions[0].nodes[0].parameters.insert(
        key.into(),
        Parameter::new(Binding::Literal { value }).unwrap(),
    );
}
#[derive(Clone)]
struct Sample {
    id: u64,
    cell: [u32; 2],
    position: [f64; 3],
}
struct GridView {
    id: u64,
    points: Vec<Sample>,
    space: Space,
    unit: Unit,
}
fn samples(g: &fold_motion::evaluation::Geometry, columns: usize) -> Vec<Sample> {
    use fold_motion::evaluation::attributes::Domain;
    (0..g.count(Domain::Point).unwrap())
        .map(|i| Sample {
            id: g.element_id(Domain::Point, i).unwrap(),
            cell: [(i % columns) as u32, (i / columns) as u32],
            position: g.position(Domain::Point, i).unwrap(),
        })
        .collect()
}
fn points(c: &Construction, time: Time) -> GridView {
    let Value::Geometry(points) = Prepared::new(c.clone())
        .unwrap()
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
    let Binding::Literal {
        value: Literal::Integer { value: columns },
    } = c.definitions[0].nodes[0].parameters["columns"]
        .binding()
        .unwrap()
    else {
        panic!()
    };
    GridView {
        id: points.id,
        points: samples(&points, columns as usize),
        space: points.space,
        unit: points.unit,
    }
}
#[test]
fn centered_grid_preserves_cell_identity_through_growth_spacing_and_time() {
    let (mut c, _) = fixture();
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let initial = points(&c, Time::ZERO);
    assert_eq!(initial.points.len(), 240);
    assert_eq!(initial.space, Space::World);
    assert_eq!(initial.unit, Unit::Meters);
    let ids: BTreeMap<_, _> = initial.points.iter().map(|p| (p.cell, p.id)).collect();
    assert_eq!(ids.len(), 240);
    assert_eq!(
        initial
            .points
            .iter()
            .map(|p| p.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        240
    );
    for (index, p) in initial.points.iter().enumerate() {
        assert_eq!(p.cell, [(index % 20) as u32, (index / 20) as u32]);
        assert!((p.position[0] - (p.cell[0] as f64 * 0.12 - 1.14)).abs() < 1e-6);
        assert!((p.position[1] - (p.cell[1] as f64 * 0.12 - 0.66)).abs() < 1e-6);
        assert_eq!(p.position[2], 0.);
    }
    set(&mut c, "columns", Literal::Integer { value: 21 });
    set(&mut c, "rows", Literal::Integer { value: 13 });
    set(
        &mut c,
        "spacing",
        Literal::Vector {
            value: vec![0.2, 0.3],
            unit: Unit::Meters,
            space: Space::World,
        },
    );
    set(
        &mut c,
        "center",
        Literal::Vector {
            value: vec![1., 2., 3.],
            unit: Unit::Meters,
            space: Space::World,
        },
    );
    for time in [
        Time::new(2, 1).unwrap(),
        Time::ZERO,
        Time::new(-1, 24).unwrap(),
    ] {
        let grown = points(&c, time);
        assert_eq!(grown.id, initial.id);
        assert_eq!(grown.points.len(), 273);
        for p in &grown.points {
            if let Some(old) = ids.get(&p.cell) {
                assert_eq!(*old, p.id);
            }
            assert!((p.position[0] - (p.cell[0] as f64 * 0.2 - 1.)).abs() < 1e-6);
            assert!((p.position[1] - (p.cell[1] as f64 * 0.3 + 0.2)).abs() < 1e-6);
            assert_eq!(p.position[2], 3.);
        }
    }
    set(&mut c, "columns", Literal::Integer { value: 1 });
    set(&mut c, "rows", Literal::Integer { value: 1 });
    let single = points(&c, Time::ZERO);
    assert_eq!(single.points[0].position, [1., 2., 3.]);
    assert_eq!(single.points[0].id, ids[&[0, 0]]);
    set(&mut c, "columns", Literal::Integer { value: 0 });
    assert!(points(&c, Time::ZERO).points.is_empty());
}
#[test]
fn grid_rejects_invalid_counts_extents_and_obeys_shared_budgets() {
    let (c, id) = fixture();
    for (name, value) in [
        ("columns", Literal::Integer { value: -1 }),
        ("columns", Literal::Integer { value: 100_001 }),
        ("columns", Literal::Integer { value: 100_000 }),
        (
            "spacing",
            Literal::Vector {
                value: vec![0., 0.1],
                unit: Unit::Meters,
                space: Space::World,
            },
        ),
        (
            "spacing",
            Literal::Vector {
                value: vec![1e6; 2],
                unit: Unit::Meters,
                space: Space::World,
            },
        ),
    ] {
        let mut bad = c.clone();
        set(&mut bad, name, value);
        let error = Prepared::new(bad)
            .unwrap()
            .evaluate_detailed(
                c.definitions[0].outputs[0].id,
                Time::ZERO,
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, id);
    }
    let prepared = Prepared::new(c.clone()).unwrap();
    let output = c.definitions[0].outputs[0].id;
    for limits in [
        Limits {
            steps: 20,
            elements: 1000,
        },
        Limits {
            steps: 1000,
            elements: 20,
        },
    ] {
        assert_eq!(
            prepared
                .evaluate_detailed(output, Time::ZERO, limits, &|| Ok(()))
                .unwrap_err()
                .kind,
            FailureKind::Budget
        );
    }
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
    assert_eq!(points(&c, Time::ZERO).points.len(), 240);
}

fn cloner_fixture() -> (Construction, ObjectId) {
    let (mut c, grid) = fixture();
    let link = |node| Binding::Link {
        reference: Reference::Node { node }.into(),
    };
    let node = |key: &str, mode: &str, name: &str, bindings| {
        registry::resolve(
            &format!("fold.motion.{key}"),
            if matches!(key, "distribute" | "cloner") {
                2
            } else {
                1
            },
            mode,
        )
        .unwrap()
        .node(name, bindings)
        .unwrap()
    };
    let prototype = node("mesh", "box", "Box", BTreeMap::new());
    let prototype_id = prototype.id;
    let paint = node(
        "material",
        "unlit",
        "Surface",
        [("geometry".into(), link(prototype.id))].into(),
    );
    let cloner = node(
        "cloner",
        "points",
        "Cloner",
        [
            ("prototype".into(), link(paint.id)),
            ("points".into(), link(grid)),
        ]
        .into(),
    );
    let world = node(
        "place",
        "world",
        "World",
        [("geometry".into(), link(cloner.id))].into(),
    );
    let camera = node("camera", "perspective", "Camera", BTreeMap::new());
    let scene = node(
        "scene",
        "spatial",
        "Scene",
        [
            ("camera".into(), link(camera.id)),
            (
                "children".into(),
                Binding::List {
                    values: vec![link(world.id)],
                },
            ),
        ]
        .into(),
    );
    c.definitions[0].outputs[0].ty = Type::Scene;
    c.definitions[0].outputs[0].value = Reference::Node { node: scene.id }.into();
    c.definitions[0]
        .nodes
        .extend([prototype, paint, cloner, world, camera, scene]);
    (c, prototype_id)
}
#[test]
fn cloner_keeps_one_prototype_and_cell_correspondence_through_growth_and_mesh_edits() {
    use fold_motion::{evaluation::GeometryData, rendering};
    use fold_project::{EditBatch, Mutation, Project};
    let (mut c, prototype) = cloner_fixture();
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let document = DocumentId::new();
    let mut project = Project::new(8);
    let mut identities = BTreeMap::new();
    let mut held = None;
    for (columns, size) in [(20, 0.06), (21, 0.06), (21, 0.09), (0, 0.09)] {
        set(&mut c, "columns", Literal::Integer { value: columns });
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == prototype)
            .unwrap()
            .parameters
            .insert(
                "size".into(),
                Parameter::new(Binding::Literal {
                    value: Literal::Vector {
                        value: vec![size; 3],
                        unit: Unit::Meters,
                        space: Space::World,
                    },
                })
                .unwrap(),
            );
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(c.document(document).unwrap())],
            })
            .unwrap();
        let frame = rendering::spatial(
            &project.snapshot(),
            document,
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            [640, 360],
            Default::default(),
            &|| Ok(()),
        )
        .unwrap();
        let GeometryData::Instances(collection) = &frame.native.entries[0].placement.geometry.data
        else {
            panic!()
        };
        assert_eq!(collection.instances.len(), (columns * 12) as usize);
        assert_eq!(frame.scene.instances.len(), collection.instances.len());
        assert_eq!(frame.scene.meshes.len(), usize::from(columns != 0));
        let GeometryData::Mesh(mesh) = &collection.prototype.data else {
            panic!()
        };
        if let Some(render_mesh) = frame.scene.meshes.values().next() {
            assert!(std::sync::Arc::ptr_eq(render_mesh, &mesh.prepared.mesh));
        }
        for ((instance, point), rendered) in collection
            .instances
            .iter()
            .zip(samples(&collection.distribution, columns as usize))
            .zip(&frame.scene.instances)
        {
            assert_eq!(instance.source_point, point.id);
            assert_eq!(instance.position, point.position);
            assert_eq!(
                rendered.transform.translation,
                point.position.map(|v| v as f32)
            );
            if let Some(old) = identities.insert(point.cell, (instance.id, rendered.id)) {
                assert_eq!(old, (instance.id, rendered.id));
            }
        }
        if let Some(old) = &held {
            let old: &rendering::SpatialFrame = old;
            assert_eq!(old.scene.instances.len(), 240);
            assert!(
                old.scene
                    .meshes
                    .values()
                    .next()
                    .unwrap()
                    .positions
                    .iter()
                    .all(|p| p.map(f32::abs) == [0.03f32; 3])
            );
        } else {
            held = Some(frame);
        }
    }
}
#[test]
fn cloner_rejects_unqualified_prototypes_and_rendering_rejects_instance_overflow() {
    use fold_motion::rendering;
    use fold_project::{EditBatch, Mutation, Project};
    let (mut c, _) = cloner_fixture();
    set(&mut c, "columns", Literal::Integer { value: 400 });
    let id = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
    assert!(matches!(
        rendering::spatial(
            &project.snapshot(),
            id,
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            [640, 360],
            Default::default(),
            &|| Ok(())
        ),
        Err(rendering::Error::Budget)
    ));
    let planar = registry::resolve("fold.motion.shape", 1, "rectangle")
        .unwrap()
        .node("Planar", Default::default())
        .unwrap();
    let cloner = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.cloner")
        .unwrap();
    let cloner_id = cloner.id;
    cloner.parameters.insert(
        "prototype".into(),
        Parameter::new(Binding::Link {
            reference: Reference::Node { node: planar.id }.into(),
        })
        .unwrap(),
    );
    c.definitions[0].nodes.push(planar);
    let error = Prepared::new(c.clone())
        .unwrap()
        .evaluate_detailed(
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, cloner_id);
}

fn scale_fixture(factor: Binding) -> (Construction, ObjectId) {
    let (mut c, _) = cloner_fixture();
    let cloner = c.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.cloner")
        .unwrap()
        .id;
    let animator = registry::resolve("fold.motion.animator", 1, "scale_multiply")
        .unwrap()
        .node(
            "Scale",
            [
                (
                    "geometry".into(),
                    Binding::Link {
                        reference: Reference::Node { node: cloner }.into(),
                    },
                ),
                ("factor".into(), factor),
            ]
            .into(),
        )
        .unwrap();
    let id = animator.id;
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .insert(
            "geometry".into(),
            Parameter::new(Binding::Link {
                reference: Reference::Node { node: id }.into(),
            })
            .unwrap(),
        );
    c.definitions[0].nodes.push(animator);
    (c, id)
}
#[test]
fn moving_radial_field_samples_upstream_instances_and_reaches_shared_render_mesh() {
    use fold_motion::{evaluation::GeometryData, rendering};
    use fold_project::{EditBatch, Mutation, Project};
    let vector = |value: Vec<f64>| Binding::Literal {
        value: Literal::Vector {
            value,
            unit: Unit::Meters,
            space: Space::World,
        },
    };
    let center = registry::resolve("fold.motion.tween", 1, "linear")
        .unwrap()
        .node(
            "Center",
            [
                ("from".into(), vector(vec![-1., -0.5, 0.])),
                ("to".into(), vector(vec![1., 0.5, 0.])),
                (
                    "duration".into(),
                    Binding::Literal {
                        value: Literal::Duration {
                            value: Time::new(2, 1).unwrap(),
                        },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let field = registry::resolve("fold.motion.field", 1, "radial")
        .unwrap()
        .node(
            "Influence",
            [(
                "center".into(),
                Binding::Link {
                    reference: Reference::Node { node: center.id }.into(),
                },
            )]
            .into(),
        )
        .unwrap();
    let response = registry::resolve("fold.motion.ramp", 1, "scalar_linear")
        .unwrap()
        .node(
            "Scale response",
            [
                (
                    "position".into(),
                    Binding::Link {
                        reference: Reference::Node { node: field.id }.into(),
                    },
                ),
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
    let factor = Binding::Link {
        reference: Reference::Node { node: response.id }.into(),
    };
    let (mut c, animator) = scale_fixture(factor);
    c.definitions[0].nodes.extend([center, field, response]);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let document = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(document).unwrap())],
        })
        .unwrap();
    let prepared = Prepared::new(c.clone()).unwrap();
    let mut ids = None;
    for time in [
        Time::new(2, 1).unwrap(),
        Time::ZERO,
        Time::new(1, 1).unwrap(),
        Time::new(1, 2).unwrap(),
        Time::new(3, 1).unwrap(),
    ] {
        let frame = rendering::spatial(
            &project.snapshot(),
            document,
            c.definitions[0].outputs[0].id,
            time,
            [640, 360],
            Default::default(),
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(frame.scene.meshes.len(), 1);
        let GeometryData::Instances(collection) = &frame.native.entries[0].placement.geometry.data
        else {
            panic!()
        };
        let current: Vec<_> = collection.instances.iter().map(|v| v.id).collect();
        if let Some(ids) = &ids {
            assert_eq!(ids, &current);
        }
        ids = Some(current);
        let t = (time.numerator() as f64 / f64::from(time.denominator())).clamp(0., 2.);
        let center = [-1. + t, -0.5 + t * 0.5, 0.];
        let mut low = 1f64;
        let mut high = 0f64;
        for ((instance, point), rendered) in collection
            .instances
            .iter()
            .zip(samples(&collection.distribution, 20))
            .zip(&frame.scene.instances)
        {
            let distance = (0..3)
                .map(|i| (point.position[i] - center[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            let expected = 0.2 + 0.8 * (1. - distance).clamp(0., 1.);
            assert!((instance.scale - expected).abs() < 1e-12);
            assert_eq!(instance.position, point.position);
            assert_eq!(rendered.transform.scale, [expected as f32; 3]);
            low = low.min(instance.scale);
            high = high.max(instance.scale);
        }
        assert!(low <= 0.21 && high > 0.9);
    }
    let calls = std::cell::Cell::new(0usize);
    let error = prepared
        .evaluate_detailed(
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            Limits::default(),
            &|| {
                calls.set(calls.get() + 1);
                if calls.get() > 4000 {
                    Err("stop during instance sampling".into())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert!(calls.get() > 4000);
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == animator)
        .unwrap()
        .parameters
        .insert(
            "factor".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Scalar { value: 0. },
            })
            .unwrap(),
        );
    let error = Prepared::new(c.clone())
        .unwrap()
        .evaluate_detailed(
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap_err();
    assert_eq!(error.context.unwrap().operation, animator);
}
#[test]
fn scale_sampling_uses_stable_instance_random_and_explicit_multiplication() {
    use fold_motion::{
        evaluation::GeometryData,
        expression::{Binary, Expr},
    };
    let (mut c, animator) = scale_fixture(Binding::Expression {
        expression: Expr::Binary {
            op: Binary::Add,
            left: Box::new(Expr::Literal {
                value: Literal::Scalar { value: 0.5 },
            }),
            right: Box::new(Expr::Random {
                version: 1,
                stream: "scale".into(),
            }),
        },
    });
    let mut values = BTreeMap::new();
    for columns in [20, 21, 20] {
        set(&mut c, "columns", Literal::Integer { value: columns });
        let Value::Scene(scene) = Prepared::new(c.clone())
            .unwrap()
            .evaluate(
                c.definitions[0].outputs[0].id,
                Time::new(1, 2).unwrap(),
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap()
        else {
            panic!()
        };
        let GeometryData::Instances(collection) = &scene.entries[0].placement.geometry.data else {
            panic!()
        };
        for instance in &collection.instances {
            let expected = 0.5
                + fold_motion::execution::random_v1(
                    c.seed,
                    animator.to_bytes(),
                    Some(instance.id),
                    "scale",
                );
            assert_eq!(instance.scale, expected);
            if let Some(old) = values.insert(instance.id, instance.scale) {
                assert_eq!(old, instance.scale);
            }
        }
    }
    let second = registry::resolve("fold.motion.animator", 1, "scale_multiply")
        .unwrap()
        .node(
            "Double",
            [
                (
                    "geometry".into(),
                    Binding::Link {
                        reference: Reference::Node { node: animator }.into(),
                    },
                ),
                (
                    "factor".into(),
                    Binding::Literal {
                        value: Literal::Scalar { value: 2. },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .insert(
            "geometry".into(),
            Parameter::new(Binding::Link {
                reference: Reference::Node { node: second.id }.into(),
            })
            .unwrap(),
        );
    c.definitions[0].nodes.push(second);
    let Value::Scene(scene) = Prepared::new(c.clone())
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
    let GeometryData::Instances(collection) = &scene.entries[0].placement.geometry.data else {
        panic!()
    };
    for instance in &collection.instances {
        assert_eq!(instance.scale, values[&instance.id] * 2.);
    }
}

#[test]
fn color_ramp_animates_instances_preserving_scale_prototype_and_material_settings() {
    use fold_motion::{
        evaluation::GeometryData,
        ramp::Ramp,
        rendering,
        types::{Alpha, Encoding},
    };
    use fold_project::{EditBatch, Mutation, Project};
    let (mut c, scale) = scale_fixture(Binding::Literal {
        value: Literal::Scalar { value: 0.5 },
    });
    let link = |node| Binding::Link {
        reference: Reference::Node { node }.into(),
    };
    let node = |key: &str, mode: &str, name: &str, bindings| {
        registry::resolve(
            &format!("fold.motion.{key}"),
            if matches!(key, "distribute" | "cloner") {
                2
            } else {
                1
            },
            mode,
        )
        .unwrap()
        .node(name, bindings)
        .unwrap()
    };
    let vector = |value| Binding::Literal {
        value: Literal::Vector {
            value,
            unit: Unit::Meters,
            space: Space::World,
        },
    };
    let center = node(
        "tween",
        "linear",
        "Color center",
        [
            ("from".into(), vector(vec![-1., -0.5, 0.])),
            ("to".into(), vector(vec![1., 0.5, 0.])),
            (
                "duration".into(),
                Binding::Literal {
                    value: Literal::Duration {
                        value: Time::new(2, 1).unwrap(),
                    },
                },
            ),
        ]
        .into(),
    );
    let field = node(
        "field",
        "radial",
        "Color influence",
        [("center".into(), link(center.id))].into(),
    );
    let color = |value| Literal::Color {
        value,
        encoding: Encoding::LinearRec709,
        alpha: Alpha::Straight,
    };
    let response = node(
        "ramp",
        "color_linear",
        "Palette",
        [
            ("position".into(), link(field.id)),
            (
                "stops".into(),
                Binding::Ramp {
                    ramp: Ramp::two(color([2., 0., 0., 0.25]), color([0., 0., 4., 0.75])).unwrap(),
                },
            ),
        ]
        .into(),
    );
    let animator = node(
        "animator",
        "color_replace",
        "Color",
        [
            ("geometry".into(), link(scale)),
            ("color".into(), link(response.id)),
        ]
        .into(),
    );
    let animator_id = animator.id;
    let world = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap();
    world.parameters.insert(
        "geometry".into(),
        Parameter::new(link(animator.id)).unwrap(),
    );
    world.parameters.insert(
        "opacity".into(),
        Parameter::new(Binding::Literal {
            value: Literal::Scalar { value: 0.5 },
        })
        .unwrap(),
    );
    c.definitions[0]
        .nodes
        .extend([center, field, response, animator]);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let document = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(document).unwrap())],
        })
        .unwrap();
    let mut ids = None;
    for seconds in [2, 0, 1, 2] {
        let frame = rendering::spatial(
            &project.snapshot(),
            document,
            c.definitions[0].outputs[0].id,
            Time::new(seconds, 1).unwrap(),
            [640, 360],
            Default::default(),
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(frame.scene.meshes.len(), 1);
        assert_eq!(frame.scene.materials.len(), 240);
        let GeometryData::Instances(collection) = &frame.native.entries[0].placement.geometry.data
        else {
            panic!()
        };
        assert_eq!(
            collection
                .prototype
                .appearance
                .material
                .as_ref()
                .unwrap()
                .color
                .value,
            [1.; 4]
        );
        let current: Vec<_> = frame
            .scene
            .instances
            .iter()
            .map(|v| (v.id, v.mesh, v.material))
            .collect();
        if let Some(previous) = &ids {
            assert_eq!(previous, &current);
        }
        ids = Some(current);
        let center = [-1. + seconds as f64, -0.5 + seconds as f64 * 0.5, 0.];
        for (instance, rendered) in collection.instances.iter().zip(&frame.scene.instances) {
            let distance = (0..3)
                .map(|axis| (instance.position[axis] - center[axis]).powi(2))
                .sum::<f64>()
                .sqrt();
            let influence = (1. - distance).clamp(0., 1.);
            let expected = [
                2. * (1. - influence),
                0.,
                4. * influence,
                0.25 + 0.5 * influence,
            ];
            let authored = instance.color.as_ref().unwrap();
            for (actual, expected) in authored.value.iter().zip(expected) {
                assert!((actual - expected).abs() < 1e-12);
            }
            assert_eq!(instance.scale, 0.5);
            assert_eq!(rendered.transform.scale, [0.5; 3]);
            let surface = &frame.scene.materials[&rendered.material];
            for axis in 0..4 {
                let expected = if axis == 3 {
                    expected[axis] * 0.5
                } else {
                    expected[axis]
                };
                assert!((f64::from(surface.color[axis]) - expected).abs() < 2e-7);
            }
            assert!(surface.unlit);
            assert_eq!(surface.roughness, 0.5);
            assert_eq!(surface.reflectance, 0.5);
        }
    }
    // A later Color Animator replaces the previous contribution; it does not
    // multiply alpha or overwrite scale, topology or the prototype surface.
    let second = node(
        "animator",
        "color_replace",
        "Replacement",
        [
            ("geometry".into(), link(animator_id)),
            (
                "color".into(),
                Binding::Literal {
                    value: color([0., 2., 0., 0.4]),
                },
            ),
        ]
        .into(),
    );
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .insert("geometry".into(), Parameter::new(link(second.id)).unwrap());
    c.definitions[0].nodes.push(second);
    let Value::Scene(scene) = Prepared::new(c.clone())
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
    let GeometryData::Instances(collection) = &scene.entries[0].placement.geometry.data else {
        panic!()
    };
    assert!(
        collection
            .instances
            .iter()
            .all(|v| v.scale == 0.5 && v.color.as_ref().unwrap().value == [0., 2., 0., 0.4])
    );
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == animator_id)
        .unwrap()
        .parameters
        .insert(
            "color".into(),
            Parameter::new(Binding::Literal {
                value: color([-1., 0., 0., 1.]),
            })
            .unwrap(),
        );
    let failure = Prepared::new(c.clone())
        .unwrap()
        .evaluate_detailed(
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap_err();
    assert_eq!(failure.kind, FailureKind::Invalid);
    assert_eq!(failure.context.unwrap().operation, animator_id);
}

#[test]
fn linear_field_projects_animated_endpoints_clamps_and_preserves_instances() {
    use fold_motion::{evaluation::GeometryData, ramp::Ramp};
    let vector = |value: [f64; 3]| Binding::Literal {
        value: Literal::Vector {
            value: value.to_vec(),
            unit: Unit::Meters,
            space: Space::World,
        },
    };
    let link = |node| Binding::Link {
        reference: Reference::Node { node }.into(),
    };
    let tween = registry::resolve("fold.motion.tween", 1, "linear")
        .unwrap()
        .node(
            "End",
            [
                ("from".into(), vector([0.2, 0.1, 0.1])),
                ("to".into(), vector([-0.2, 0.5, -0.2])),
                (
                    "duration".into(),
                    Binding::Literal {
                        value: Literal::Duration {
                            value: Time::new(2, 1).unwrap(),
                        },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let field = registry::resolve("fold.motion.field", 1, "linear")
        .unwrap()
        .node(
            "Linear influence",
            [
                ("start".into(), vector([-0.2, -0.1, 0.1])),
                ("end".into(), link(tween.id)),
            ]
            .into(),
        )
        .unwrap();
    let field_id = field.id;
    let response = registry::resolve("fold.motion.ramp", 1, "scalar_linear")
        .unwrap()
        .node(
            "Response",
            [
                ("position".into(), link(field.id)),
                (
                    "stops".into(),
                    Binding::Ramp {
                        ramp: Ramp::two(
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
    let (mut c, _) = scale_fixture(link(response.id));
    c.definitions[0].nodes.extend([tween, field, response]);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let prepared = Prepared::new(c.clone()).unwrap();
    let mut retained = None;
    for seconds in [2, 0, 1, 2] {
        let Value::Scene(scene) = prepared
            .evaluate(
                c.definitions[0].outputs[0].id,
                Time::new(seconds, 1).unwrap(),
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap()
        else {
            panic!()
        };
        let GeometryData::Instances(collection) = &scene.entries[0].placement.geometry.data else {
            panic!()
        };
        let mut low = 1_f64;
        let mut high = 0_f64;
        // Independent projection onto the authored animated segment in 3D.
        let start = [-0.2, -0.1, 0.1];
        let end = [
            0.2 - 0.2 * seconds as f64,
            0.1 + 0.2 * seconds as f64,
            0.1 - 0.15 * seconds as f64,
        ];
        let delta: [f64; 3] = std::array::from_fn(|i| end[i] - start[i]);
        for (index, instance) in collection.instances.iter().enumerate() {
            let x = ((index % 20) as f64 * 0.12 - 19. * 0.06) as f32 as f64;
            let y = ((index / 20) as f64 * 0.12 - 11. * 0.06) as f32 as f64;
            let amount = ((x - start[0]) * delta[0] + (y - start[1]) * delta[1]
                - start[2] * delta[2])
                / delta.iter().map(|v| v * v).sum::<f64>();
            let expected = 0.2 + 0.8 * amount.clamp(0., 1.);
            assert!((instance.scale - expected).abs() < 1e-12);
            low = low.min(instance.scale);
            high = high.max(instance.scale);
        }
        assert_eq!(low, 0.2);
        assert_eq!(high, 1.);
        let identities: Vec<_> = collection
            .instances
            .iter()
            .map(|v| (v.id, v.position, v.source_point))
            .collect();
        if let Some(previous) = &retained {
            assert_eq!(previous, &identities);
        }
        retained = Some(identities);
    }
    for invalid in [
        [-0.2, -0.1, 0.1],
        [-0.2 + 1e-8, -0.1, 0.1],
        [1e6 + 1., 0., 0.],
    ] {
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == field_id)
            .unwrap()
            .parameters
            .insert("end".into(), Parameter::new(vector(invalid)).unwrap());
        let error = Prepared::new(c.clone())
            .unwrap()
            .evaluate_detailed(
                c.definitions[0].outputs[0].id,
                Time::ZERO,
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, field_id);
    }
}

#[test]
fn native_random_preserves_named_instance_values_across_time_count_and_source() {
    use fold_motion::evaluation::GeometryData;
    let literal = |value| Binding::Literal { value };
    let random = registry::resolve("fold.motion.random", 1, "uniform")
        .unwrap()
        .node(
            "Variation",
            [
                ("minimum".into(), literal(Literal::Scalar { value: 0.5 })),
                ("maximum".into(), literal(Literal::Scalar { value: 1.5 })),
                (
                    "stream".into(),
                    literal(Literal::Text {
                        value: "rotation".into(),
                    }),
                ),
            ]
            .into(),
        )
        .unwrap();
    let random_id = random.id;
    let (mut c, _) = scale_fixture(Binding::Link {
        reference: Reference::Node { node: random_id }.into(),
    });
    c.definitions[0].nodes.push(random);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let seed = c.seed;
    let mut baseline = BTreeMap::new();
    for (stream, seed) in [("rotation", seed), ("rotation", seed + 1), ("color", seed)] {
        c.seed = seed;
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == random_id)
            .unwrap()
            .parameters
            .insert(
                "stream".into(),
                Parameter::new(literal(Literal::Text {
                    value: stream.into(),
                }))
                .unwrap(),
            );
        let mut values = BTreeMap::new();
        for (columns, seconds) in [(20, 2), (21, 0), (20, 1), (20, 2)] {
            set(&mut c, "columns", Literal::Integer { value: columns });
            let Value::Scene(scene) = Prepared::new(c.clone())
                .unwrap()
                .evaluate(
                    c.definitions[0].outputs[0].id,
                    Time::new(seconds, 1).unwrap(),
                    Limits::default(),
                    &|| Ok(()),
                )
                .unwrap()
            else {
                panic!()
            };
            let GeometryData::Instances(collection) = &scene.entries[0].placement.geometry.data
            else {
                panic!()
            };
            for instance in &collection.instances {
                let expected = 0.5
                    + fold_motion::execution::random_v1(
                        seed,
                        random_id.to_bytes(),
                        Some(instance.id),
                        stream,
                    );
                assert_eq!(instance.scale, expected);
                assert!((0.5..1.5).contains(&instance.scale));
                if let Some(old) = values.insert(instance.id, instance.scale) {
                    assert_eq!(old, instance.scale);
                }
            }
        }
        if baseline.is_empty() {
            baseline = values;
        } else {
            assert_ne!(baseline, values);
        }
    }
    for (key, value) in [
        (
            "stream",
            Literal::Text {
                value: String::new(),
            },
        ),
        (
            "stream",
            Literal::Text {
                value: "x".repeat(257),
            },
        ),
        ("minimum", Literal::Scalar { value: 2. }),
        ("maximum", Literal::Scalar { value: 1e6 + 1. }),
    ] {
        let mut invalid = c.clone();
        invalid.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == random_id)
            .unwrap()
            .parameters
            .insert(key.into(), Parameter::new(literal(value)).unwrap());
        let error = Prepared::new(invalid)
            .unwrap()
            .evaluate_detailed(
                c.definitions[0].outputs[0].id,
                Time::ZERO,
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, random_id);
    }
    c.definitions[0].outputs[0].ty = Type::Field {
        value: Box::new(Type::Scalar),
        domain: fold_motion::types::Domain::Instance,
        space: Space::World,
    };
    c.definitions[0].outputs[0].value = Reference::Node { node: random_id }.into();
    let error = Prepared::new(c.clone())
        .unwrap()
        .evaluate_detailed(
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Invalid);
    assert_eq!(error.context.unwrap().operation, random_id);
}

#[test]
fn native_random_z_rotation_preserves_instances_and_reaches_render_transforms() {
    use fold_motion::{evaluation::GeometryData, rendering};
    use fold_project::{EditBatch, Mutation, Project};
    let scalar = |value| Binding::Literal {
        value: Literal::Scalar { value },
    };
    let link = |node| Binding::Link {
        reference: Reference::Node { node }.into(),
    };
    let (mut c, scale) = scale_fixture(scalar(0.5));
    let random = registry::resolve("fold.motion.random", 1, "uniform")
        .unwrap()
        .node(
            "Rotation variation",
            [
                ("maximum".into(), scalar(std::f64::consts::TAU)),
                (
                    "stream".into(),
                    Binding::Literal {
                        value: Literal::Text {
                            value: "rotation".into(),
                        },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let random_id = random.id;
    let rotation = registry::resolve("fold.motion.animator", 1, "rotation_z_add")
        .unwrap()
        .node(
            "Rotate",
            [
                ("geometry".into(), link(scale)),
                ("angle".into(), link(random.id)),
            ]
            .into(),
        )
        .unwrap();
    let rotation_id = rotation.id;
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .insert(
            "geometry".into(),
            Parameter::new(link(rotation.id)).unwrap(),
        );
    c.definitions[0].nodes.extend([random, rotation]);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let id = DocumentId::new();
    let mut project = Project::new(8);
    let mut identities = BTreeMap::new();
    for (columns, seconds) in [(20, 2), (21, 0), (20, 1)] {
        set(&mut c, "columns", Literal::Integer { value: columns });
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
            })
            .unwrap();
        let frame = rendering::spatial(
            &project.snapshot(),
            id,
            c.definitions[0].outputs[0].id,
            Time::new(seconds, 1).unwrap(),
            [640, 360],
            Default::default(),
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(frame.scene.meshes.len(), 1);
        let GeometryData::Instances(collection) = &frame.native.entries[0].placement.geometry.data
        else {
            panic!()
        };
        for (instance, rendered) in collection.instances.iter().zip(&frame.scene.instances) {
            let angle = std::f64::consts::TAU
                * fold_motion::execution::random_v1(
                    c.seed,
                    random_id.to_bytes(),
                    Some(instance.id),
                    "rotation",
                );
            assert_eq!(instance.rotation_z, angle);
            assert_eq!(instance.scale, 0.5);
            let q = rendered.transform.rotation;
            assert_eq!(
                q,
                [
                    0.,
                    0.,
                    (angle * 0.5).sin() as f32,
                    (angle * 0.5).cos() as f32
                ]
            );
            let point = instance.point([0.06, 0.03, 0.01]);
            let expected = [
                instance.position[0] + 0.03 * angle.cos() - 0.015 * angle.sin(),
                instance.position[1] + 0.03 * angle.sin() + 0.015 * angle.cos(),
                instance.position[2] + 0.005,
            ];
            for axis in 0..3 {
                assert!((point[axis] - expected[axis]).abs() < 1e-12);
            }
            if let Some(old) = identities.insert(instance.id, (angle, rendered.id, rendered.mesh)) {
                assert_eq!(old, (angle, rendered.id, rendered.mesh));
            }
        }
    }
    // Same-axis rotations compose additively around the prototype origin.
    let second = registry::resolve("fold.motion.animator", 1, "rotation_z_add")
        .unwrap()
        .node(
            "Quarter turn",
            [
                ("geometry".into(), link(rotation_id)),
                ("angle".into(), scalar(std::f64::consts::FRAC_PI_2)),
            ]
            .into(),
        )
        .unwrap();
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .insert("geometry".into(), Parameter::new(link(second.id)).unwrap());
    c.definitions[0].nodes.push(second);
    let Value::Scene(scene) = Prepared::new(c.clone())
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
    let GeometryData::Instances(collection) = &scene.entries[0].placement.geometry.data else {
        panic!()
    };
    for instance in &collection.instances {
        assert_eq!(
            instance.rotation_z,
            (identities[&instance.id].0 + std::f64::consts::FRAC_PI_2)
                .rem_euclid(std::f64::consts::TAU)
        );
    }
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .insert(
            "scale".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Vector {
                    value: vec![2., 1., 1.],
                    unit: Unit::Unitless,
                    space: Space::World,
                },
            })
            .unwrap(),
        );
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
    let frame = rendering::spatial(
        &project.snapshot(),
        id,
        c.definitions[0].outputs[0].id,
        Time::ZERO,
        [640, 360],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    let fold_motion::evaluation::placement::Transform::World(world) =
        &frame.native.entries[0].placement.transform
    else {
        panic!()
    };
    let GeometryData::Instances(collection) = &frame.native.entries[0].placement.geometry.data
    else {
        panic!()
    };
    assert_eq!(frame.scene.instances.len(), collection.instances.len());
    for (rendered, native) in frame.scene.instances.iter().zip(&collection.instances) {
        let matrix = rendered
            .transform
            .affine
            .expect("nonuniform parent and rotated instance retain affine shear");
        matrix.validate().unwrap();
        let (sin, cos) = native.rotation_z.sin_cos();
        let expected = world.point([
            native.position[0] + cos * native.scale,
            native.position[1] + sin * native.scale,
            native.position[2],
        ]);
        assert!(
            matrix
                .point([1., 0., 0.])
                .into_iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-10)
        );
    }
}
