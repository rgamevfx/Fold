use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{FailureKind, Limits, Prepared, Value},
    model::{Construction, Node, Parameter, Reference},
    registry, source,
    types::{Literal, Space, Unit},
};
use std::sync::Arc;
fn link(id: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node: id }.into(),
    }
}
fn node(key: &str, mode: &str, name: &str) -> Node {
    registry::resolve(&format!("fold.motion.{key}"), 1, mode)
        .unwrap()
        .node(name, Default::default())
        .unwrap()
}
fn set(node: &mut Node, key: &str, binding: Binding) {
    node.parameters
        .insert(key.into(), Parameter::new(binding).unwrap());
}
fn literal(value: Literal) -> Binding {
    Binding::Literal { value }
}
fn position(y: f64) -> Literal {
    Literal::Vector {
        value: vec![0., y, 5.],
        unit: Unit::Meters,
        space: Space::World,
    }
}
fn fixture() -> Construction {
    let mut c =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    let d = &mut c.definitions[0];
    let mut camera = node("camera", "perspective", "Camera");
    let light = node("light", "directional", "Sun");
    let tween = registry::resolve("fold.motion.tween", 1, "linear")
        .unwrap()
        .node(
            "Camera move",
            [
                ("from".into(), literal(position(1.))),
                ("to".into(), literal(position(2.))),
                (
                    "duration".into(),
                    literal(Literal::Duration {
                        value: Time::new(1, 1).unwrap(),
                    }),
                ),
            ]
            .into(),
        )
        .unwrap();
    set(&mut camera, "position", link(tween.id));
    let scene = d
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    scene.mode = "spatial".into();
    set(scene, "camera", link(camera.id));
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.extend([link(camera.id), link(light.id), link(light.id)]);
    set(scene, "children", Binding::List { values });
    d.nodes.extend([camera, light, tween]);
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
fn spatial_controls_round_trip_sample_signals_and_preserve_occurrence_identity() {
    let c = fixture();
    c.validate_semantics().unwrap();
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let mut identities = None;
    for (time, y) in [
        (Time::ZERO, 1.),
        (Time::new(1, 1).unwrap(), 2.),
        (Time::new(1, 2).unwrap(), 1.5),
        (Time::ZERO, 1.),
    ] {
        let Value::Scene(scene) = evaluate(&c, time).unwrap() else {
            panic!()
        };
        let camera = scene.camera.as_ref().unwrap();
        assert_eq!(camera.position, [0., y, 5.]);
        assert_eq!(camera.near, 0.01);
        assert_eq!(camera.far, 100.);
        assert!(Arc::ptr_eq(camera, &scene.cameras[0].camera));
        assert_eq!(scene.entries.len(), 1);
        assert_eq!(scene.lights.len(), 2);
        assert_eq!(scene.lights[0].light.illuminance, 1000.);
        assert!(Arc::ptr_eq(&scene.lights[0].light, &scene.lights[1].light));
        assert_ne!(scene.lights[0].id, scene.lights[1].id);
        let ids = (
            camera.id,
            scene.cameras[0].id,
            scene.lights[0].id,
            scene.lights[1].id,
        );
        if let Some(expected) = identities {
            assert_eq!(ids, expected)
        } else {
            identities = Some(ids)
        }
    }
    let mut reversed = c.clone();
    let scene = reversed.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.reverse();
    scene.parameters.get_mut("children").unwrap().value =
        serde_json::to_value(Binding::List { values }).unwrap();
    let Value::Scene(scene) = evaluate(&reversed, Time::ZERO).unwrap() else {
        panic!()
    };
    assert_eq!(scene.lights[0].id, identities.unwrap().3);
}
#[test]
fn invalid_spatial_controls_fail_with_native_operation_context() {
    let c = fixture();
    for (operation, parameter, value) in [
        (
            "camera",
            "near",
            Literal::Distance {
                value: 0.,
                unit: Unit::Meters,
            },
        ),
        (
            "camera",
            "far",
            Literal::Distance {
                value: 0.001,
                unit: Unit::Meters,
            },
        ),
        ("camera", "vertical_fov", Literal::Scalar { value: 0. }),
        ("camera", "ev100", Literal::Scalar { value: 21. }),
        (
            "camera",
            "up",
            Literal::Vector {
                value: vec![0.; 3],
                unit: Unit::Unitless,
                space: Space::World,
            },
        ),
        ("camera", "target", position(1.)),
        (
            "camera",
            "up",
            Literal::Vector {
                value: vec![0., -1., -5.],
                unit: Unit::Unitless,
                space: Space::World,
            },
        ),
        ("light", "illuminance", Literal::Scalar { value: -1. }),
        ("light", "illuminance", Literal::Scalar { value: 100001. }),
        (
            "light",
            "color",
            Literal::Color {
                value: [1., 1., 1., 0.5],
                encoding: fold_motion::types::Encoding::LinearRec709,
                alpha: fold_motion::types::Alpha::Straight,
            },
        ),
        (
            "light",
            "direction",
            Literal::Vector {
                value: vec![0.; 3],
                unit: Unit::Unitless,
                space: Space::World,
            },
        ),
    ] {
        let mut invalid = c.clone();
        let node = invalid.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.operation == format!("fold.motion.{operation}"))
            .unwrap();
        let id = node.id;
        set(node, parameter, literal(value));
        let error = evaluate(&invalid, Time::ZERO).unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, id);
    }
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

#[test]
fn orthographic_size_is_typed_animated_and_retained_through_spatial_conversion() {
    use fold_motion::evaluation::spatial::Projection;
    use fold_project::{EditBatch, Mutation, Project};
    let mut c = fixture();
    let camera = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.camera")
        .unwrap();
    camera.mode = "orthographic".into();
    camera.parameters.remove("vertical_fov");
    let camera_id = camera.id;
    let size = registry::resolve("fold.motion.tween", 1, "linear")
        .unwrap()
        .node(
            "View size",
            [
                (
                    "from".into(),
                    literal(Literal::Distance {
                        value: 2.,
                        unit: Unit::Meters,
                    }),
                ),
                (
                    "to".into(),
                    literal(Literal::Distance {
                        value: 4.,
                        unit: Unit::Meters,
                    }),
                ),
                (
                    "duration".into(),
                    literal(Literal::Duration {
                        value: Time::new(1, 1).unwrap(),
                    }),
                ),
            ]
            .into(),
        )
        .unwrap();
    set(camera, "vertical_size", link(size.id));
    c.definitions[0].nodes.push(size);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let mut identity = None;
    for (time, size) in [
        (Time::ZERO, 2.),
        (Time::new(1, 1).unwrap(), 4.),
        (Time::new(1, 2).unwrap(), 3.),
        (Time::ZERO, 2.),
    ] {
        let Value::Scene(scene) = evaluate(&c, time).unwrap() else {
            panic!()
        };
        let camera = scene.camera.as_ref().unwrap();
        assert_eq!(
            camera.projection,
            Projection::Orthographic {
                vertical_size: size
            }
        );
        if let Some(id) = identity {
            assert_eq!(camera.id, id);
        } else {
            identity = Some(camera.id);
        }
    }
    for size in [0., -1., 1e-7, 1e6 + 1.] {
        let mut invalid = c.clone();
        let camera = invalid.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == camera_id)
            .unwrap();
        set(
            camera,
            "vertical_size",
            literal(Literal::Distance {
                value: size,
                unit: Unit::Meters,
            }),
        );
        let error = evaluate(&invalid, Time::ZERO).unwrap_err();
        assert_eq!(error.kind, FailureKind::Invalid);
        assert_eq!(error.context.unwrap().operation, camera_id);
    }
    // Isolate camera conversion from the starter's Screen placement.
    let scene = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    set(scene, "children", Binding::List { values: vec![] });
    let mut project = Project::new(8);
    let document = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(document).unwrap())],
        })
        .unwrap();
    let prepared = fold_motion::rendering::spatial(
        &project.snapshot(),
        document,
        c.definitions[0].outputs[0].id,
        Time::new(1, 2).unwrap(),
        [160, 90],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(
        prepared.scene.camera.projection,
        fold_render::spatial::Projection::Orthographic { vertical_size: 3. }
    );
    assert_eq!(
        prepared.binding.camera_content,
        prepared.scene.camera.identity()
    );
}

#[test]
fn camera_plane_source_retains_named_camera_sizing_depth_and_exact_time() {
    use fold_motion::evaluation::placement::{PlaneDepth, PlaneSizing, Transform};
    for (mode, physical) in [
        ("camera_plane_viewport", false),
        ("camera_plane_physical", true),
    ] {
        let mut c = fixture();
        let d = &mut c.definitions[0];
        let camera = d
            .nodes
            .iter()
            .find(|n| n.operation == "fold.motion.camera")
            .unwrap()
            .id;
        let placement = d
            .nodes
            .iter_mut()
            .find(|n| n.operation == "fold.motion.place")
            .unwrap();
        placement.mode = mode.into();
        set(placement, "camera", link(camera));
        set(
            placement,
            "distance",
            literal(Literal::Distance {
                value: 3.,
                unit: Unit::Meters,
            }),
        );
        if physical {
            set(
                placement,
                "meters_per_unit",
                literal(Literal::Scalar { value: 0.02 }),
            );
        }
        assert_eq!(
            source::parse(&source::canonical(&c).unwrap())
                .unwrap()
                .construction,
            c
        );
        let mut identity = None;
        for (time, y) in [
            (Time::ZERO, 1.),
            (Time::new(1, 2).unwrap(), 1.5),
            (Time::new(1, 1).unwrap(), 2.),
            (Time::ZERO, 1.),
        ] {
            let Value::Scene(scene) = evaluate(&c, time).unwrap() else {
                panic!()
            };
            let placement = &scene.entries[0].placement;
            let Transform::CameraPlane(plane) = &placement.transform else {
                panic!()
            };
            assert!(Arc::ptr_eq(&plane.camera, scene.camera.as_ref().unwrap()));
            assert_eq!(plane.camera.position, [0., y, 5.]);
            assert_eq!(plane.distance, 3.);
            assert_eq!(plane.depth, PlaneDepth::Scene);
            assert_eq!(
                plane.sizing,
                if physical {
                    PlaneSizing::Physical {
                        meters_per_unit: 0.02,
                    }
                } else {
                    PlaneSizing::Viewport
                }
            );
            if let Some(id) = identity {
                assert_eq!(placement.id, id);
            } else {
                identity = Some(placement.id);
            }
        }
        for value in [0., -1., 1e7] {
            let mut bad = c.clone();
            let placement = bad.definitions[0]
                .nodes
                .iter_mut()
                .find(|n| n.operation == "fold.motion.place")
                .unwrap();
            set(
                placement,
                "distance",
                literal(Literal::Distance {
                    value,
                    unit: Unit::Meters,
                }),
            );
            assert!(evaluate(&bad, Time::ZERO).is_err());
        }
        if physical {
            let placement = c.definitions[0]
                .nodes
                .iter_mut()
                .find(|n| n.operation == "fold.motion.place")
                .unwrap();
            set(
                placement,
                "meters_per_unit",
                literal(Literal::Scalar { value: 0. }),
            );
            assert!(evaluate(&c, Time::ZERO).is_err());
        }
    }
}

#[test]
fn physical_camera_plane_conversion_keeps_attachment_under_other_view_camera() {
    use fold_motion::rendering;
    let mut c = fixture();
    let d = &mut c.definitions[0];
    let camera = d
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.camera")
        .unwrap()
        .id;
    let shape = d
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.shape")
        .unwrap()
        .id;
    let fill = registry::resolve("fold.motion.fill", 1, "solid")
        .unwrap()
        .node("Plane Fill", [("geometry".into(), link(shape))].into())
        .unwrap();
    let fill_id = fill.id;
    d.nodes.push(fill);
    let placement = d
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap();
    placement.mode = "camera_plane_physical".into();
    set(placement, "geometry", link(fill_id));
    set(placement, "camera", link(camera));
    set(
        placement,
        "distance",
        literal(Literal::Distance {
            value: 2.,
            unit: Unit::Meters,
        }),
    );
    set(
        placement,
        "meters_per_unit",
        literal(Literal::Scalar { value: 0.01 }),
    );
    let document = DocumentId::new();
    let mut project = fold_project::Project::new(8);
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                c.document(document).unwrap(),
            )],
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
    let positions = frame
        .scene
        .meshes
        .values()
        .flat_map(|m| m.positions.iter())
        .collect::<Vec<_>>();
    let left = positions.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let right = positions
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(((right - left) - 2.).abs() < 2e-5);
    let other = node("camera", "orthographic", "Other Camera");
    let scene = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    set(scene, "camera", link(other.id));
    c.definitions[0].nodes.push(other);
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                c.document(document).unwrap(),
            )],
        })
        .unwrap();
    let other_frame = rendering::spatial(
        &project.snapshot(),
        document,
        c.definitions[0].outputs[0].id,
        Time::ZERO,
        [640, 360],
        Default::default(),
        &|| Ok(()),
    )
    .unwrap();
    assert_ne!(frame.binding.camera, other_frame.binding.camera);
    assert_eq!(frame.scene.meshes, other_frame.scene.meshes);
    assert_eq!(frame.scene.instances, other_frame.scene.instances);
}
