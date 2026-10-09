use fold_foundation::{DocumentId, Time};
use fold_motion::{
    binding::Binding,
    scene::{self, Scene},
    types::{Literal, Space, Unit},
};
fn fixture() -> (DocumentId, Scene) {
    let id = DocumentId::new();
    let canvas = fold_motion::model::Canvas {
        width: 640,
        height: 360,
        rate: [24000, 1001],
        start: Time::ZERO,
        end: Time::new(4, 1).unwrap(),
        extensions: Default::default(),
    };
    let scene = Scene::from_document(&scene::package::create(id, canvas).unwrap()).unwrap();
    (id, scene)
}
fn position(x: f64) -> Binding {
    Binding::Literal {
        value: Literal::Vector {
            value: vec![x, 0., 0.],
            unit: Unit::Meters,
            space: Space::World,
        },
    }
}
#[test]
fn scene_hierarchy_assembles_distinct_graphs_and_preserves_opaque_extensions() {
    let (id, mut scene) = fixture();
    assert_eq!(scene.objects.len(), 3);
    for object in &scene.objects {
        assert!(
            object
                .graph
                .definitions
                .iter()
                .all(|d| d.nodes.iter().all(|n| n.operation != "fold.motion.scene"))
        );
    }
    let root = scene.objects[2].graph.root.id;
    scene.objects[2]
        .graph
        .presentation
        .annotations
        .insert(root, "Preserve root identity".into());
    let parent = scene.objects[2].id;
    scene.objects[2].position = position(2.);
    let mut child = scene::package::object(&scene.canvas, "box").unwrap();
    // Reuse the complete graph, including root annotations and authored IDs.
    child.graph = scene.objects[2].graph.clone();
    child.output = scene.objects[2].output;
    child.parent = Some(parent);
    child.position = position(3.);
    let child_id = child.id;
    scene.objects.push(child);
    scene
        .extensions
        .insert("vendor".into(), serde_json::json!({"future":true}));
    let document = scene.document(id).unwrap();
    assert_eq!(Scene::from_document(&document).unwrap(), scene);
    let evaluated = scene
        .evaluate(Time::ZERO, Default::default(), &|| Ok(()))
        .unwrap();
    assert_eq!(evaluated.entries.len(), 2);
    assert_eq!(evaluated.cameras.len(), 1);
    assert_eq!(evaluated.lights.len(), 1);
    let fold_motion::evaluation::placement::Transform::World(world) =
        &evaluated.entries[1].placement.transform
    else {
        panic!()
    };
    assert_eq!(world.position, [5., 0., 0.]);
    assert_ne!(evaluated.entries[0].id, evaluated.entries[1].id);
    assert_ne!(
        evaluated.entries[0].placement.geometry.id,
        evaluated.entries[1].placement.geometry.id
    );
    scene.objects[2].parent = Some(child_id);
    assert!(scene.validate().unwrap_err().contains("cycle"));
}
#[test]
fn scene_parent_locks_visibility_camera_and_resource_budgets_are_enforced() {
    let (id, mut scene) = fixture();
    let parent = scene.objects[2].id;
    let mut child = scene::package::object(&scene.canvas, "box").unwrap();
    child.parent = Some(parent);
    let child_id = child.id;
    scene.objects.push(child);
    scene.objects[2].locked = true;
    let document = scene.document(id).unwrap();
    let mut changed = scene.clone();
    changed.objects[3].parent = None;
    assert!(changed.replace(&document).is_err());
    assert!(scene.locked(child_id));
    scene.objects[2].locked = false;
    scene.objects[2].visible = false;
    assert!(
        scene
            .evaluate(Time::ZERO, Default::default(), &|| Ok(()))
            .unwrap()
            .entries
            .is_empty()
    );
    scene.objects[0].visible = false;
    assert!(
        scene
            .evaluate(Time::ZERO, Default::default(), &|| Ok(()))
            .is_err()
    );
    let (_, scene) = fixture();
    assert!(
        scene
            .evaluate(
                Time::ZERO,
                fold_motion::evaluation::Limits {
                    steps: 1,
                    elements: 1
                },
                &|| Ok(())
            )
            .is_err()
    );
    assert!(
        scene
            .evaluate(Time::ZERO, Default::default(), &|| Err("cancelled".into()))
            .is_err()
    );
}
#[test]
fn object_clock_and_graph_source_roundtrip_preserve_the_scene() {
    let (id, mut scene) = fixture();
    let object = &mut scene.objects[2];
    let mut track =
        fold_motion::binding::Track::constant(Literal::Scalar { value: 1. }, Time::ZERO).unwrap();
    track.keys.push(fold_motion::binding::Key {
        id: fold_foundation::ObjectId::new(),
        time: Time::new(1, 1).unwrap(),
        value: Literal::Scalar { value: 3. },
        interpolation: fold_motion::binding::Interpolation::Linear,
        handles: Default::default(),
        extensions: Default::default(),
    });
    object.scale = Binding::Track { track };
    object.clock.offset = Time::new(1, 2).unwrap();
    let source = fold_motion::source::canonical(&object.graph).unwrap();
    object.graph = fold_motion::source::parse(&source).unwrap().construction;
    let sampled = scene
        .evaluate(Time::new(1, 1).unwrap(), Default::default(), &|| Ok(()))
        .unwrap();
    let fold_motion::evaluation::placement::Transform::World(t) =
        &sampled.entries[0].placement.transform
    else {
        panic!()
    };
    assert_eq!(t.matrix().vector([1.; 3]), [2.; 3]);
    assert_eq!(
        Scene::from_document(&scene.document(id).unwrap()).unwrap(),
        scene
    );
    let text = scene::package::object(&scene.canvas, "text").unwrap();
    text.graph.validate_semantics().unwrap();
}

#[test]
fn scene_curves_roundtrip_nested_clocks_coupled_vectors_and_locks() {
    use fold_motion::{
        binding::{Interpolation, Key, Track},
        types::Ratio,
    };
    let (id, mut scene) = fixture();
    scene.canvas.start = Time::new(1, 3).unwrap();
    let parent = scene.objects[2].id;
    scene.objects[2].clock.offset = Time::new(1, 7).unwrap();
    scene.objects[2].clock.rate = Ratio::new(3, 2).unwrap();
    let mut child = scene::package::object(&scene.canvas, "box").unwrap();
    child.parent = Some(parent);
    child.clock.offset = Time::new(2, 9).unwrap();
    child.clock.rate = Ratio::new(5, 3).unwrap();
    let value = |x, y| Literal::Vector {
        value: vec![x, y, 0.],
        unit: Unit::Meters,
        space: Space::World,
    };
    let mut track = Track::constant(value(1., 2.), Time::ZERO).unwrap();
    track.keys[0]
        .extensions
        .insert("vendor".into(), serde_json::json!([1, 2]));
    track.keys.push(Key {
        id: fold_foundation::ObjectId::new(),
        time: Time::new(7, 13).unwrap(),
        value: value(3., 4.),
        interpolation: Interpolation::Linear,
        handles: Default::default(),
        extensions: Default::default(),
    });
    let key = track.keys[0].id;
    child.position = Binding::Track {
        track: track.clone(),
    };
    let child_id = child.id;
    scene.objects.push(child);
    let original = scene.document(id).unwrap();
    let channels = scene::animation::channels(&scene, Some(child_id)).unwrap();
    assert_eq!(channels.len(), 3);
    let mut x = channels
        .iter()
        .find(|c| c.component == "X")
        .unwrap()
        .clone();
    let displayed = x.curve.keys[0].time;
    let mut t = displayed.checked_add(scene.canvas.start).unwrap();
    for object in scene.ancestors(child_id).unwrap() {
        t = object.clock.at(t).unwrap();
    }
    assert_eq!(t, Time::ZERO);
    x.curve.keys[0].time = displayed.checked_add(Time::new(1, 100).unwrap()).unwrap();
    x.curve.keys[0].value = 8.;
    let edited = scene::animation::replace(&scene, &[x.clone()]).unwrap();
    edited.replace(&original).unwrap();
    let Binding::Track { track: after } = &edited.objects[3].position else {
        panic!()
    };
    assert_eq!(after.keys[0].id, key);
    assert_eq!(after.keys[0].value, value(8., 2.));
    assert_eq!(after.keys[0].extensions, track.keys[0].extensions);
    assert_eq!(after.keys[0].time, Time::new(1, 40).unwrap());
    let y = scene::animation::channels(&edited, Some(child_id))
        .unwrap()
        .into_iter()
        .find(|c| c.component == "Y")
        .unwrap();
    assert_eq!(y.curve.keys[0].time, x.curve.keys[0].time);
    let mut conflict = channels[1].clone();
    conflict.curve.keys[0].time = Time::new(2, 1).unwrap();
    conflict.curve.keys.sort_by_key(|k| k.time);
    assert!(scene::animation::replace(&scene, &[x.clone(), conflict]).is_err());
    scene.objects[2].locked = true;
    assert!(scene::animation::replace(&scene, &[x]).is_err());
}

#[test]
fn scene_provider_hierarchy_undo_named_outputs_and_artist_unlock() {
    use fold_platform::{
        hierarchy::Edit,
        packages::{CommandRequest, PackageRegistry},
    };
    use fold_project::{EditBatch, Mutation, Project};
    let (id, scene) = fixture();
    let mut registry = PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    let document = scene.document(id).unwrap();
    assert_eq!(registry.hierarchy(&document).unwrap().rows.len(), 3);
    assert_eq!(
        registry.document_outputs(&document)[0].reference.output,
        scene.output.to_string()
    );
    let object = scene.objects[2].id;
    let locked = registry
        .edit_hierarchy(
            &document,
            &[Edit::Lock {
                object,
                value: true,
            }],
        )
        .unwrap();
    assert!(
        registry
            .edit_hierarchy(
                &locked,
                &[
                    Edit::Lock {
                        object,
                        value: false
                    },
                    Edit::Rename {
                        object,
                        name: "bypass".into()
                    }
                ]
            )
            .is_err()
    );
    let unlocked = registry
        .edit_hierarchy(
            &locked,
            &[Edit::Lock {
                object,
                value: false,
            }],
        )
        .unwrap();
    assert!(!Scene::from_document(&unlocked).unwrap().locked(object));
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document.clone())],
        })
        .unwrap();
    let mut graph = scene.objects[2].graph.clone();
    graph.definitions[0].nodes[0].name = "My box".into();
    let request = CommandRequest {
        id: scene::GRAPH_EDIT.into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(&scene::package::GraphEditArgs {
            document: id,
            object,
            construction: graph.clone(),
        })
        .unwrap(),
    };
    let batch = registry
        .stage(&project.snapshot(), &request, &Default::default())
        .unwrap();
    project.commit(batch).unwrap();
    let changed = Scene::from_document(&project.snapshot().state().documents[&id]).unwrap();
    assert_eq!(changed.objects[2].graph, graph);
    assert_eq!(changed.objects[0], scene.objects[0]);
    project.undo().unwrap();
    assert_eq!(
        Scene::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        scene
    );
}

#[test]
fn scene_nonuniform_parenting_keeps_child_rotation_and_shear() {
    let (id, mut scene) = fixture();
    let parent = scene.objects[2].id;
    scene.objects[2].scale = Binding::Literal {
        value: Literal::Vector {
            value: vec![2., 1., 3.],
            unit: Unit::Unitless,
            space: Space::World,
        },
    };
    let mut child = scene::package::object(&scene.canvas, "box").unwrap();
    child.parent = Some(parent);
    child.position = position(1.);
    let angle = std::f64::consts::FRAC_PI_8;
    child.rotation = Binding::Literal {
        value: Literal::Rotation {
            value: [0., 0., angle.sin(), angle.cos()],
            space: Space::World,
        },
    };
    scene.objects.push(child);
    let scene = Scene::from_document(&scene.document(id).unwrap()).unwrap();
    let result = scene
        .evaluate(Time::ZERO, Default::default(), &|| Ok(()))
        .unwrap();
    let fold_motion::evaluation::placement::Transform::World(world) =
        &result.entries[1].placement.transform
    else {
        panic!()
    };
    let expected = [
        2. + 2. * std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
        0.,
    ];
    assert!(
        world
            .point([1., 0., 0.])
            .into_iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 1e-12)
    );
    assert_eq!(world.matrix().vector([0., 0., 1.]), [0., 0., 3.]);
}

#[test]
fn reusable_graph_occurrences_and_override_tracks_keep_native_ownership() {
    use fold_foundation::ObjectId;
    use fold_motion::{
        model::{Definition, Input, Instance, Output, Parameter, Reference},
        types::{Clock, Ratio, Type},
    };
    let (_, mut scene) = fixture();
    let graph = &mut scene.objects[2].graph;
    let parameter = graph.definitions[0].nodes[0].parameters["size"].id;
    *graph = fold_motion::keyed::animate(graph, parameter, Time::ZERO).unwrap();
    let input = Input {
        id: ObjectId::new(),
        name: "Strength".into(),
        ty: Type::Scalar,
        default: Literal::Scalar { value: 1. },
        protected: false,
        extensions: Default::default(),
    };
    graph.definitions[0].inputs.push(input.clone());
    let mut first = graph.root.clone();
    first.name = "First".into();
    first.clock.offset = Time::new(1, 1).unwrap();
    first.overrides.insert(
        input.id,
        Parameter::new(Binding::Track {
            track: fold_motion::binding::Track::constant(
                input.default.clone(),
                Time::new(1, 2).unwrap(),
            )
            .unwrap(),
        })
        .unwrap(),
    );
    let second = Instance {
        id: ObjectId::new(),
        name: "Second".into(),
        definition: first.definition,
        clock: Clock {
            offset: Time::new(2, 1).unwrap(),
            rate: Ratio::new(-1, 1).unwrap(),
        },
        overrides: Default::default(),
        extensions: Default::default(),
    };
    let nested_output = graph.definitions[0].outputs[0].id;
    let outer = Definition {
        id: ObjectId::new(),
        name: "Object".into(),
        inputs: vec![],
        nodes: vec![],
        instances: vec![first.clone(), second],
        outputs: vec![Output {
            id: ObjectId::new(),
            name: "main".into(),
            ty: Type::Geometry,
            value: Reference::Instance {
                instance: first.id,
                output: nested_output,
            }
            .into(),
            extensions: Default::default(),
        }],
        extensions: Default::default(),
    };
    graph.root = Instance {
        id: ObjectId::new(),
        name: "Object".into(),
        definition: outer.id,
        clock: Clock::default(),
        overrides: Default::default(),
        extensions: Default::default(),
    };
    let output = outer.outputs[0].id;
    graph.definitions.push(outer);
    graph.validate_semantics().unwrap();
    scene.objects[2].output = output;
    let object = scene.objects[2].id;
    let channels = scene::animation::channels(&scene, Some(object)).unwrap();
    let nodes: Vec<_> = channels.iter().filter(|c| c.component == "X").collect();
    assert_eq!(nodes.len(), 2);
    assert_ne!(nodes[0].target, nodes[1].target);
    assert_eq!(nodes[0].curve.keys[0].id, nodes[1].curve.keys[0].id);
    assert_eq!(nodes[0].curve.keys[0].time, Time::new(1, 1).unwrap());
    assert_eq!(nodes[1].curve.keys[0].time, Time::new(2, 1).unwrap());
    let mut a = nodes[0].clone();
    a.curve.keys[0].value = 3.;
    let edited = scene::animation::replace(&scene, &[a.clone()]).unwrap();
    let after = scene::animation::channels(&edited, Some(object)).unwrap();
    assert!(
        after
            .iter()
            .filter(|c| c.component == "X")
            .all(|c| c.curve.keys[0].value == 3.)
    );
    let mut b = nodes[1].clone();
    b.curve.keys[0].value = 4.;
    assert!(
        scene::animation::replace(&scene, &[a, b])
            .unwrap_err()
            .contains("conflicting occurrence")
    );
    let mut input = channels
        .iter()
        .find(|c| c.target.object == first.id)
        .unwrap()
        .clone();
    assert_eq!(
        input.curve.keys[0].time,
        Time::new(1, 2).unwrap(),
        "overrides use parent time, not instance time"
    );
    input.curve.keys[0].value = 8.;
    let edited = scene::animation::replace(&scene, &[input.clone()]).unwrap();
    let after = scene::animation::channels(&edited, Some(object)).unwrap();
    assert_eq!(
        after
            .iter()
            .find(|c| c.target == input.target)
            .unwrap()
            .curve
            .keys[0]
            .value,
        8.
    );
}

#[test]
fn paused_clock_projects_a_constant_sample_and_preserves_hidden_keys_on_value_edit() {
    use fold_motion::types::Ratio;
    let (_, mut scene) = fixture();
    let value = |x| Literal::Vector {
        value: vec![x, 0., 0.],
        unit: Unit::Meters,
        space: Space::World,
    };
    let mut track =
        fold_motion::binding::Track::constant(value(1.), Time::new(-1, 1).unwrap()).unwrap();
    let mut key = track.keys[0].clone();
    key.id = fold_foundation::ObjectId::new();
    key.time = Time::new(1, 1).unwrap();
    key.value = value(3.);
    track.keys.push(key);
    scene.objects[2].position = Binding::Track {
        track: track.clone(),
    };
    scene.objects[2].clock.rate = Ratio::new(0, 1).unwrap();
    let object = scene.objects[2].id;
    let channels = scene::animation::channels(&scene, Some(object)).unwrap();
    assert_eq!(
        channels,
        scene::animation::channels(&scene, Some(object)).unwrap()
    );
    let mut x = channels
        .iter()
        .find(|c| c.component == "X")
        .unwrap()
        .clone();
    assert!(x.property_label.contains("Paused"));
    assert_eq!(x.curve.keys.len(), 1);
    assert_eq!(x.curve.sample(Time::new(20, 1).unwrap()), Some(2.));
    x.curve.keys[0].value = 5.;
    let changed = scene::animation::replace(&scene, &[x.clone()]).unwrap();
    let Binding::Track { track: updated } = &changed.objects[2].position else {
        panic!()
    };
    assert_eq!(updated.keys.len(), 3);
    assert_eq!(updated.keys[0], track.keys[0]);
    assert_eq!(updated.keys[2], track.keys[1]);
    assert_eq!(updated.sample(Time::ZERO).unwrap(), value(5.));
    x.curve.keys[0].time = Time::new(1, 1).unwrap();
    assert!(
        scene::animation::replace(&scene, &[x])
            .unwrap_err()
            .contains("Resume the clock")
    );
}
