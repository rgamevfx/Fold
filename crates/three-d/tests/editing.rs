use fold_foundation::{DocumentId, ObjectId};
use fold_graph::{
    binding::Binding,
    types::{Literal, Space, Unit},
};
use fold_media::{Cancel, VideoInfo};
use fold_platform::packages::{CommandRequest, PackageRegistry};
use fold_project::{EditBatch, Mutation, Project};
use fold_three_d::{
    TYPE,
    editing::{self, Edit, Placement},
    model::{Content, Geometry, Object, Scene, Transform},
    package::{self, EditArgs},
};

#[test]
fn artist_and_agent_use_one_guarded_transaction_path_with_undo() {
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let id = DocumentId::new();
    let mut document = registry.create(TYPE, id).unwrap();
    document
        .extensions
        .insert("vendor.document".into(), serde_json::json!([1, 2]));
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let document = &snapshot.state().documents[&id];
    let before = Scene::from_document(document).unwrap();
    let object = before.roots[0];
    let expected = before.objects[&object].transform.position.clone();
    let mut value = expected.clone();
    value.value = serde_json::to_value(Binding::Literal {
        value: Literal::Vector {
            value: vec![2., 3., 0.],
            unit: Unit::Meters,
            space: Space::World,
        },
    })
    .unwrap();
    let artist = Edit::Parameter {
        object,
        expected: expected.clone(),
        value: value.clone(),
    };
    let request = CommandRequest {
        id: package::EDIT.into(),
        base: snapshot.revision(),
        arguments: serde_json::to_vec(&EditArgs {
            document: id,
            edits: vec![artist.clone()],
        })
        .unwrap(),
    };
    let batch = registry
        .stage(&snapshot, &request, &Cancel::default())
        .unwrap();
    let stale = batch.clone();
    project.commit(batch).unwrap();
    assert!(project.commit(stale).is_err());
    let current = project.snapshot();
    let document = &current.state().documents[&id];
    assert_eq!(
        document.extensions["vendor.document"],
        serde_json::json!([1, 2])
    );
    assert_eq!(
        Scene::from_document(document).unwrap().objects[&object]
            .transform
            .position,
        value
    );
    assert!(
        registry
            .author(document, &[serde_json::to_value(artist).unwrap()])
            .is_err()
    );
    let mut agent_value = value.clone();
    agent_value.value = serde_json::to_value(Binding::Literal {
        value: Literal::Vector {
            value: vec![5., 3., 0.],
            unit: Unit::Meters,
            space: Space::World,
        },
    })
    .unwrap();
    let agent = Edit::Parameter {
        object,
        expected: value.clone(),
        value: agent_value.clone(),
    };
    let authored = registry
        .author(document, &[serde_json::to_value(agent).unwrap()])
        .unwrap();
    project
        .commit(EditBatch {
            base: current.revision(),
            mutations: vec![Mutation::PutDocument(authored)],
        })
        .unwrap();
    project.undo().unwrap();
    assert_eq!(
        Scene::from_document(&project.snapshot().state().documents[&id])
            .unwrap()
            .objects[&object]
            .transform
            .position,
        value
    );
    project.undo().unwrap();
    assert_eq!(
        Scene::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        before
    );
}

#[test]
fn atomic_batches_preserve_protected_descendants_resources_and_extensions() {
    let cancel = Cancel::default();
    let mut scene = Scene::new(VideoInfo {
        width: 640,
        height: 360,
        rate: [30, 1],
        frames: 300,
    });
    let instance = scene.roots[0];
    let Content::Instance {
        geometry, material, ..
    } = scene.objects[&instance].content
    else {
        unreachable!()
    };
    scene.objects.get_mut(&instance).unwrap().protected = true;
    let old = scene.geometry[&geometry].clone();
    assert!(
        editing::apply(
            &scene,
            &[Edit::Geometry {
                id: geometry,
                expected: Some(old.clone()),
                value: Some(Geometry::Box {
                    size: [2.; 3],
                    extensions: Default::default()
                })
            }],
            &cancel
        )
        .is_err()
    );
    let group = ObjectId::new();
    let mut parent = Object {
        name: "Parent".into(),
        visible: true,
        protected: false,
        transform: Transform::default(),
        content: Content::Group {
            children: vec![instance],
            extensions: Default::default(),
        },
        extensions: Default::default(),
    };
    scene.roots[0] = group;
    scene.objects.insert(group, parent.clone());
    let new_group = ObjectId::new();
    let replacement = parent.clone();
    let Content::Group { children, .. } = &mut parent.content else {
        unreachable!()
    };
    children.clear();
    assert!(
        editing::apply(
            &scene,
            &[
                Edit::Replace {
                    id: group,
                    expected: Box::new(replacement.clone()),
                    value: Box::new(parent)
                },
                Edit::Insert {
                    id: new_group,
                    object: Box::new(replacement),
                    placement: Placement {
                        parent: None,
                        index: 0
                    }
                }
            ],
            &cancel
        )
        .is_err()
    );
    assert!(
        editing::apply(
            &scene,
            &[Edit::Delete {
                id: group,
                expected: Box::new(scene.objects[&group].clone())
            }],
            &cancel
        )
        .is_err()
    );
    let mut changed = scene.materials[&material].clone();
    changed.color = [1., 0., 0., 1.];
    assert!(
        editing::apply(
            &scene,
            &[Edit::Material {
                id: material,
                expected: Some(scene.materials[&material].clone()),
                value: Some(changed)
            }],
            &cancel
        )
        .is_err()
    );
    scene.objects.get_mut(&instance).unwrap().protected = false;
    let Geometry::Box { extensions, .. } = scene.geometry.get_mut(&geometry).unwrap() else {
        unreachable!()
    };
    extensions.insert("vendor.geometry".into(), serde_json::json!({"future":true}));
    assert!(
        editing::apply(
            &scene,
            &[
                Edit::Geometry {
                    id: geometry,
                    expected: Some(scene.geometry[&geometry].clone()),
                    value: None
                },
                Edit::Geometry {
                    id: geometry,
                    expected: None,
                    value: Some(old)
                }
            ],
            &cancel
        )
        .is_err()
    );
    assert_eq!(scene.objects[&instance].name, "Box");
}

#[test]
fn world_parenting_preserves_animation_shear_channels_and_undo() {
    use fold_foundation::Time;
    use fold_project::Revision;
    let mut scene = Scene::new(VideoInfo {
        width: 640,
        height: 360,
        rate: [30, 1],
        frames: 150,
    });
    let instance = scene.roots[0];
    let (a, edits) =
        fold_three_d::tools::create(&scene, fold_three_d::tools::Kind::Group, None).unwrap();
    scene = editing::apply(&scene, &edits, &Cancel::default()).unwrap();
    let (b, edits) =
        fold_three_d::tools::create(&scene, fold_three_d::tools::Kind::Group, None).unwrap();
    scene = editing::apply(&scene, &edits, &Cancel::default()).unwrap();
    let literal = |value| serde_json::to_value(Binding::Literal { value }).unwrap();
    for (id, position, angle, scale) in [
        (a, vec![2., 3., 4.], 0.7_f64, vec![2.; 3]),
        (b, vec![-4., 1., 2.], -1.2_f64, vec![0.5; 3]),
    ] {
        let transform = &mut scene.objects.get_mut(&id).unwrap().transform;
        transform.position.value = literal(Literal::Vector {
            value: position,
            unit: Unit::Meters,
            space: Space::World,
        });
        transform.rotation.value = literal(Literal::Rotation {
            value: [0., 0., (angle / 2.).sin(), (angle / 2.).cos()],
            space: Space::World,
        });
        transform.scale.value = literal(Literal::Vector {
            value: scale,
            unit: Unit::Unitless,
            space: Space::World,
        });
    }
    scene = editing::apply(
        &scene,
        &fold_three_d::tools::move_to(&scene, instance, Some(a)).unwrap(),
        &Cancel::default(),
    )
    .unwrap();
    let old = scene.objects[&instance].clone();
    let geometry = scene.geometry.clone();
    let moved = editing::apply(
        &scene,
        &fold_three_d::tools::move_to_world(&scene, instance, Some(b)).unwrap(),
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(
        moved.objects[&instance].transform.position.id,
        old.transform.position.id
    );
    let semantic = editing::apply(
        &scene,
        &[Edit::MoveWorld {
            id: instance,
            expected: editing::placement(&scene, instance).unwrap(),
            parent: Some(b),
        }],
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(semantic, moved);
    assert!(
        editing::apply(
            &scene,
            &[Edit::MoveWorld {
                id: instance,
                expected: Placement {
                    parent: None,
                    index: 999
                },
                parent: Some(b)
            }],
            &Cancel::default()
        )
        .is_err()
    );
    assert_eq!(moved.objects[&instance].content, old.content);
    assert_eq!(moved.geometry, geometry);
    let document = DocumentId::new();
    for time in [
        Time::ZERO,
        Time::new(1, 2).unwrap(),
        Time::new(4, 1).unwrap(),
    ] {
        let matrix = |scene: &Scene| {
            scene
                .evaluate(document, Revision(0), time, [640, 360], &|| Ok(()))
                .unwrap()
                .0
                .instances
                .into_iter()
                .find(|i| i.id == instance)
                .unwrap()
                .transform
                .affine
                .unwrap()
        };
        for (before, after) in matrix(&scene)
            .0
            .iter()
            .flatten()
            .zip(matrix(&moved).0.iter().flatten())
        {
            assert!((before - after).abs() < 1e-9);
        }
    }
    let mut sheared = scene.clone();
    sheared.objects.get_mut(&b).unwrap().transform.scale.value = literal(Literal::Vector {
        value: vec![1., 2., 3.],
        unit: Unit::Unitless,
        space: Space::World,
    });
    use fold_graph::binding::{Interpolation, Track};
    use fold_three_d::{interaction::world, tools};
    let mut animated = sheared;
    // Animate both parents and the child. Parent rotation + nonuniform scale
    // requires affine shear; fractional samples must retain the full curves.
    for (id, distance) in [(a, 3.), (b, -2.), (instance, 0.7)] {
        let transform = &mut animated.objects.get_mut(&id).unwrap().transform;
        let Binding::Literal { value: start } = transform.position.binding().unwrap() else {
            unreachable!()
        };
        let mut track = Track::constant(start.clone(), Time::ZERO).unwrap();
        let mut end = track.keys[0].clone();
        end.id = ObjectId::new();
        end.time = Time::new(3, 1).unwrap();
        let Literal::Vector { value, .. } = &mut end.value else {
            unreachable!()
        };
        value[0] += distance;
        value[1] -= distance * 0.3;
        track.keys[0].interpolation = Interpolation::Bezier;
        track
            .extensions
            .insert("vendor.curve".into(), serde_json::json!(42));
        track.keys.push(end);
        transform.position.value = serde_json::to_value(Binding::Track { track }).unwrap();
        let Binding::Literal { value } = transform.rotation.binding().unwrap() else {
            unreachable!()
        };
        let mut rotation = Track::constant(value, Time::ZERO).unwrap();
        let mut end = rotation.keys[0].clone();
        end.id = ObjectId::new();
        end.time = Time::new(3, 1).unwrap();
        end.value = Literal::euler(
            [0.2, 0.4, distance],
            fold_graph::types::EulerOrder::Xyz,
            Space::World,
        )
        .unwrap();
        rotation.keys.push(end);
        transform.rotation.value =
            serde_json::to_value(Binding::Track { track: rotation }).unwrap();
    }
    animated.validate().unwrap();
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let authored = registry
        .author(
            &animated.document(document).unwrap(),
            &[serde_json::to_value(Edit::MoveWorld {
                id: instance,
                expected: editing::placement(&animated, instance).unwrap(),
                parent: Some(b),
            })
            .unwrap()],
        )
        .unwrap();
    let moved = Scene::from_document(&authored).unwrap();
    assert_eq!(
        moved.objects[&instance].transform.position,
        animated.objects[&instance].transform.position
    );
    assert_eq!(
        moved.objects[&instance].transform.rotation,
        animated.objects[&instance].transform.rotation
    );
    assert_eq!(
        moved.objects[&instance].transform.scale,
        animated.objects[&instance].transform.scale
    );
    let root = editing::apply(
        &moved,
        &tools::move_to_world(&moved, instance, None).unwrap(),
        &Cancel::default(),
    )
    .unwrap();
    let back = editing::apply(
        &root,
        &tools::move_to_world(&root, instance, Some(a)).unwrap(),
        &Cancel::default(),
    )
    .unwrap();
    let nested = editing::apply(
        &animated,
        &tools::move_to_world(&animated, a, Some(b)).unwrap(),
        &Cancel::default(),
    )
    .unwrap();
    let detached = editing::apply(
        &nested,
        &tools::move_to_world(&nested, instance, None).unwrap(),
        &Cancel::default(),
    )
    .unwrap();
    let (copy, edits) = tools::duplicate(&back, instance).unwrap();
    let copied = editing::apply(&back, &edits, &Cancel::default()).unwrap();
    copied.validate().unwrap(); // Includes frozen-track/key identity uniqueness.
    let close = |left: fold_foundation::affine::Affine, right: fold_foundation::affine::Affine| {
        for (a, b) in left.0.iter().flatten().zip(right.0.iter().flatten()) {
            assert!((a - b).abs() < 1e-8, "{a} != {b}");
        }
    };
    for tick in 0..137 {
        let time = Time::new(tick, 29).unwrap();
        let expected = world(&animated, instance, time).unwrap();
        for scene in [&moved, &root, &back, &nested, &detached] {
            close(expected, world(scene, instance, time).unwrap());
            let evaluated = scene
                .evaluate(document, Revision(0), time, [640, 360], &|| Ok(()))
                .unwrap()
                .0;
            close(
                expected,
                evaluated
                    .instances
                    .iter()
                    .find(|i| i.id == instance)
                    .unwrap()
                    .transform
                    .affine
                    .unwrap(),
            );
        }
        close(expected, world(&copied, copy, time).unwrap());
    }
    let time = Time::new(5, 7).unwrap();
    let translated = editing::apply(
        &moved,
        &tools::translate_world(&moved, instance, time, [1., 2., 3.], true).unwrap(),
        &Cancel::default(),
    )
    .unwrap();
    let before = world(&moved, instance, time).unwrap().point([0.; 3]);
    let after = world(&translated, instance, time).unwrap().point([0.; 3]);
    for i in 0..3 {
        assert!((after[i] - before[i] - (i + 1) as f64).abs() < 1e-8);
    }
    // The old parent is no longer a live dependency. New-parent edits contribute.
    let mut changed = moved.clone();
    changed
        .objects
        .get_mut(&a)
        .unwrap()
        .transform
        .position
        .value = literal(Literal::Vector {
        value: vec![80.; 3],
        unit: Unit::Meters,
        space: Space::World,
    });
    close(
        world(&moved, instance, time).unwrap(),
        world(&changed, instance, time).unwrap(),
    );
    let new_parent = editing::apply(
        &moved,
        &tools::translate_world(&moved, b, time, [1., 0., 0.], true).unwrap(),
        &Cancel::default(),
    )
    .unwrap();
    let point = world(&new_parent, instance, time).unwrap().point([0.; 3]);
    assert!((point[0] - before[0] - 1.).abs() < 1e-8);
    let mut locked = animated.clone();
    locked.objects.get_mut(&instance).unwrap().protected = true;
    assert!(tools::move_to_world(&locked, instance, Some(b)).is_err());
    assert!(tools::move_to_world(&animated, a, Some(a)).is_err());
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(animated.document(document).unwrap())],
        })
        .unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(moved.document(document).unwrap())],
        })
        .unwrap();
    assert_eq!(
        Scene::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        moved
    );
    project.undo().unwrap();
    assert_eq!(
        Scene::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        animated
    );
    // Whole-object edits cannot discard saved parent motion or unknown data.
    let expected = moved.objects[&instance].clone();
    let mut value = expected.clone();
    value.transform.parent_space.clear();
    assert!(
        editing::apply(
            &moved,
            &[Edit::Replace {
                id: instance,
                expected: Box::new(expected),
                value: Box::new(value)
            }],
            &Cancel::default()
        )
        .is_err()
    );
}

#[test]
fn duplicate_animated_tree_and_detach_resources_preserve_original_and_undo() {
    use fold_foundation::Time;
    use fold_graph::binding::Track;
    use fold_three_d::tools;
    let cancel = Cancel::default();
    let mut scene = Scene::new(VideoInfo {
        width: 640,
        height: 360,
        rate: [30, 1],
        frames: 150,
    });
    let source = scene.roots[0];
    let object = scene.objects.get_mut(&source).unwrap();
    object
        .extensions
        .insert("vendor.note".into(), serde_json::json!({"keep":1}));
    object.transform.position.value = serde_json::to_value(Binding::Track {
        track: Track::constant(
            Literal::Vector {
                value: vec![1., 2., 0.],
                unit: Unit::Meters,
                space: Space::World,
            },
            Time::ZERO,
        )
        .unwrap(),
    })
    .unwrap();
    let (group, edits) = tools::group(&scene, &[source]).unwrap();
    scene = editing::apply(&scene, &edits, &cancel).unwrap();
    let (copy, edits) = tools::duplicate(&scene, group).unwrap();
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let document_id = DocumentId::new();
    let document = scene.document(document_id).unwrap();
    let authored = registry
        .author(
            &document,
            &edits
                .iter()
                .map(|edit| serde_json::to_value(edit).unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let copied = Scene::from_document(&authored).unwrap();
    let Content::Group { children, .. } = &copied.objects[&copy].content else {
        unreachable!()
    };
    let cloned = children[0];
    assert_eq!(copied.objects[&source], scene.objects[&source]);
    assert_eq!(
        copied.objects[&cloned].extensions,
        scene.objects[&source].extensions
    );
    assert_eq!(
        copied.objects[&cloned].content,
        scene.objects[&source].content
    );
    let track = |scene: &Scene, id| {
        let Binding::Track { track } = scene.objects[&id].transform.position.binding().unwrap()
        else {
            unreachable!()
        };
        track
    };
    assert_ne!(track(&copied, cloned).id, track(&scene, source).id);
    assert_ne!(
        track(&copied, cloned).keys[0].id,
        track(&scene, source).keys[0].id
    );
    assert_eq!(
        track(&copied, cloned)
            .sample(Time::new(1, 2).unwrap())
            .unwrap(),
        track(&scene, source)
            .sample(Time::new(1, 2).unwrap())
            .unwrap()
    );
    assert_eq!(copied.geometry, scene.geometry);
    assert_eq!(copied.materials, scene.materials);
    let mut copied = copied;
    copied.objects.get_mut(&source).unwrap().protected = true;
    let Content::Instance {
        geometry, material, ..
    } = copied.objects[&cloned].content
    else {
        unreachable!()
    };
    let edits = tools::make_unique(&copied, cloned, false).unwrap();
    let detached = editing::apply(&copied, &edits, &cancel).unwrap();
    let Content::Instance {
        material: unique,
        geometry: retained,
        ..
    } = detached.objects[&cloned].content
    else {
        unreachable!()
    };
    assert_ne!(unique, material);
    assert_eq!(retained, geometry);
    assert_eq!(detached.materials[&unique], copied.materials[&material]);
    assert_eq!(detached.objects[&source], copied.objects[&source]);
    let edits = tools::make_unique(&detached, cloned, true).unwrap();
    let detached_geometry = editing::apply(&detached, &edits, &cancel).unwrap();
    assert_eq!(detached_geometry.geometry.len(), scene.geometry.len() + 1);
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(copied.document(document_id).unwrap())],
        })
        .unwrap();
    let batch = editing::stage(
        &project.snapshot(),
        document_id,
        &tools::make_unique(&copied, cloned, false).unwrap(),
        &cancel,
    )
    .unwrap();
    project.commit(batch).unwrap();
    project.undo().unwrap();
    assert_eq!(
        Scene::from_document(&project.snapshot().state().documents[&document_id]).unwrap(),
        copied
    );
    // A tree cannot steal an existing child or contain disconnected objects.
    let rogue = ObjectId::new();
    let mut objects = std::collections::BTreeMap::new();
    objects.insert(
        rogue,
        Object::new(
            "Invalid",
            Content::Group {
                children: vec![source],
                extensions: Default::default(),
            },
        ),
    );
    assert!(
        editing::apply(
            &scene,
            &[Edit::InsertTree {
                root: rogue,
                objects,
                placement: Placement {
                    parent: None,
                    index: 0
                }
            }],
            &cancel
        )
        .is_err()
    );
    // Copying protection preserves it, including a protected group hierarchy.
    let mut protected = scene.clone();
    protected.objects.get_mut(&group).unwrap().protected = true;
    let (id, edits) = tools::duplicate(&protected, group).unwrap();
    assert!(editing::apply(&protected, &edits, &cancel).unwrap().objects[&id].protected);
}
