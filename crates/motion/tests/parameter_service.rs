use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    model::Construction,
    types::{Literal, Space, Type, Unit},
};
use fold_platform::{packages::PackageRegistry, parameters};
use fold_project::{EditBatch, Mutation, Project};

fn registry() -> PackageRegistry {
    let mut registry = PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    registry
}

#[test]
fn parameter_service_keeps_identity_after_rename_and_reopen() {
    let registry = registry();
    let document = registry
        .create(fold_motion::CONSTRUCTION, DocumentId::new())
        .unwrap();
    let mut graph = Construction::from_document(&document).unwrap();
    let entries = registry.parameters(&document).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.path.last().unwrap() == "height")
        .unwrap()
        .clone();
    let owner = &entry.path[..entry.path.len() - 1];
    assert_eq!(
        parameters::resolve(&entries, owner, "height").unwrap(),
        &entry
    );
    let before = registry
        .sample_parameter(&document, &entry.target, Time::ZERO, &Default::default())
        .unwrap();
    graph.root.name = "a/b.~".into();
    graph.definitions[0].nodes[0].name = "Moved name".into();
    let changed = graph.replace_document(&document).unwrap();
    let bytes = serde_json::to_vec(&changed).unwrap();
    let restored = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        registry
            .sample_parameter(&restored, &entry.target, Time::ZERO, &Default::default())
            .unwrap(),
        before
    );
    let entries = registry.parameters(&restored).unwrap();
    let renamed = entries.iter().find(|e| e.target == entry.target).unwrap();
    assert_eq!(
        parameters::resolve(&entries, &[], &parameters::absolute_path(renamed)).unwrap(),
        renamed
    );
    assert!(parameters::resolve(&entries, &[], &parameters::absolute_path(&entry)).is_err());
    let mut wrong = entry.target.clone();
    wrong.document = DocumentId::new();
    assert!(
        registry
            .sample_parameter(&restored, &wrong, Time::ZERO, &Default::default())
            .is_err()
    );
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(
        registry
            .sample_parameter(&restored, &entry.target, Time::ZERO, &cancel)
            .is_err()
    );
}

#[test]
fn container_components_sample_root_overrides_with_units() {
    let registry = registry();
    let document = registry
        .create(fold_motion::CONSTRUCTION, DocumentId::new())
        .unwrap();
    let mut graph = Construction::from_document(&document).unwrap();
    let input = ObjectId::new();
    graph.definitions[0].inputs.push(fold_motion::model::Input {
        id: input,
        name: "Offset".into(),
        ty: Type::Vector {
            dimensions: 2,
            unit: Unit::Canvas,
            space: Space::Screen,
        },
        default: Literal::Vector {
            value: vec![1., 2.],
            unit: Unit::Canvas,
            space: Space::Screen,
        },
        protected: false,
        extensions: Default::default(),
    });
    graph.root.overrides.insert(
        input,
        fold_motion::model::Parameter {
            id: ObjectId::new(),
            value: serde_json::to_value(Binding::Literal {
                value: Literal::Vector {
                    value: vec![40., 60.],
                    unit: Unit::Canvas,
                    space: Space::Screen,
                },
            })
            .unwrap(),
            protected: true,
            extensions: Default::default(),
        },
    );
    let document = graph.replace_document(&document).unwrap();
    let entries = registry.parameters(&document).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.target.parameter.native() == Some(input) && e.target.component == Some(1))
        .unwrap();
    assert!(
        !entry.editable,
        "Protected root overrides lock the effective control"
    );
    assert_eq!(
        entry.value_type,
        Some(Type::Distance { unit: Unit::Canvas })
    );
    assert_eq!(
        registry
            .sample_parameter(&document, &entry.target, Time::ZERO, &Default::default())
            .unwrap(),
        Literal::Distance {
            value: 60.,
            unit: Unit::Canvas
        }
    );
    let mut invalid = entry.target.clone();
    invalid.component = Some(3);
    assert!(
        registry
            .sample_parameter(&document, &invalid, Time::ZERO, &Default::default())
            .is_err()
    );
}

#[test]
fn project_catalog_retains_document_identity_and_rejects_ambiguous_names() {
    let registry = registry();
    let a = registry
        .create(fold_motion::CONSTRUCTION, DocumentId::new())
        .unwrap();
    let b = registry
        .create(fold_motion::CONSTRUCTION, DocumentId::new())
        .unwrap();
    let mut project = Project::new(8);
    let snapshot = project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(a), Mutation::PutDocument(b)],
        })
        .unwrap();
    let catalog = registry
        .project_parameters(&snapshot, &Default::default())
        .unwrap();
    let entry = &catalog[0];
    assert_eq!(
        parameters::resolve(&catalog, &[], &parameters::absolute_path(entry))
            .unwrap()
            .target,
        entry.target
    );
    // Names are projections. A collision must never pick the first target.
    let mut duplicate = entry.clone();
    duplicate.target.document = DocumentId::new();
    let pair = vec![entry.clone(), duplicate];
    assert!(
        parameters::resolve(&pair, &[], &parameters::absolute_path(entry))
            .unwrap_err()
            .contains("Ambiguous")
    );
}

#[test]
fn scene_parameter_service_uses_object_clock_and_parent_paths() {
    let registry = registry();
    let document = registry
        .create(fold_motion::scene::TYPE, DocumentId::new())
        .unwrap();
    let mut scene = fold_motion::scene::Scene::from_document(&document).unwrap();
    scene.objects[0].name = "Parent".into();
    scene.objects[0].clock.rate = fold_motion::types::Ratio::new(2, 1).unwrap();
    let mut object = fold_motion::scene::package::object(&scene.canvas, "box").unwrap();
    object.parent = Some(scene.objects[0].id);
    let size = object.graph.definitions[0].nodes[0].parameters["size"].id;
    object.graph = fold_motion::keyed::animate(&object.graph, size, Time::ZERO).unwrap();
    let parameter = object.graph.definitions[0].nodes[0]
        .parameters
        .get_mut("size")
        .unwrap();
    let Binding::Track { mut track } = parameter.binding().unwrap() else {
        panic!()
    };
    let mut end = track.keys[0].clone();
    end.id = ObjectId::new();
    end.time = Time::new(2, 1).unwrap();
    end.value = Literal::Vector {
        value: vec![3.; 3],
        unit: Unit::Meters,
        space: Space::World,
    };
    track.keys.push(end);
    parameter.value = serde_json::to_value(Binding::Track { track }).unwrap();
    let mut position = fold_motion::binding::Track::constant(
        Literal::Vector {
            value: vec![0.; 3],
            unit: Unit::Meters,
            space: Space::World,
        },
        Time::ZERO,
    )
    .unwrap();
    let mut position_end = position.keys[0].clone();
    position_end.id = ObjectId::new();
    position_end.time = Time::new(2, 1).unwrap();
    position_end.value = Literal::Vector {
        value: vec![4., 0., 0.],
        unit: Unit::Meters,
        space: Space::World,
    };
    position.keys.push(position_end);
    object.position = Binding::Track { track: position };
    object.clock.offset = Time::new(2, 1).unwrap();
    object.clock.rate = fold_motion::types::Ratio::new(-1, 1).unwrap();
    let id = object.id;
    scene.objects.push(object);
    let changed = scene.replace(&document).unwrap();
    let entries = registry.parameters(&changed).unwrap();
    let entry = entries
        .iter()
        .find(|e| {
            e.target.object == Some(id)
                && e.target.parameter.native() == Some(size)
                && e.target.component == Some(0)
        })
        .unwrap();
    assert_eq!(entry.path[0], "Parent");
    let placement = entries
        .iter()
        .find(|e| {
            e.target.object == Some(id)
                && e.target.parameter.property() == Some("position")
                && e.target.component == Some(0)
        })
        .unwrap();
    assert_eq!(
        registry
            .sample_parameter(&changed, &placement.target, Time::ZERO, &Default::default())
            .unwrap(),
        Literal::Distance {
            value: 4.,
            unit: Unit::Meters
        }
    );
    assert_eq!(
        registry
            .sample_parameter(
                &changed,
                &placement.target,
                Time::new(1, 1).unwrap(),
                &Default::default()
            )
            .unwrap(),
        Literal::Distance {
            value: 0.,
            unit: Unit::Meters
        }
    );

    assert_eq!(
        registry
            .sample_parameter(&changed, &entry.target, Time::ZERO, &Default::default())
            .unwrap(),
        Literal::Distance {
            value: 3.,
            unit: Unit::Meters
        }
    );
    assert_eq!(
        registry
            .sample_parameter(
                &changed,
                &entry.target,
                Time::new(1, 1).unwrap(),
                &Default::default()
            )
            .unwrap(),
        Literal::Distance {
            value: 1.,
            unit: Unit::Meters
        }
    );
    scene.objects.iter_mut().find(|o| o.id == id).unwrap().name = "New / name".into();
    let renamed = scene.replace(&changed).unwrap();
    assert_eq!(
        registry
            .sample_parameter(&renamed, &entry.target, Time::ZERO, &Default::default())
            .unwrap(),
        Literal::Distance {
            value: 3.,
            unit: Unit::Meters
        }
    );
}
