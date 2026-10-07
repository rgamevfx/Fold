use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    Motion,
    authoring::{
        self,
        composition::{self, Args},
    },
    nodes::composition::{KIND, Settings},
};
use fold_platform::{
    browser::{Placement, PlacementSource},
    packages::{CommandRequest, PackageRegistry},
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};

fn reference(document: DocumentId) -> DocumentRef {
    DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    }
}
fn setup() -> (Project, PackageRegistry, DocumentId, Vec<ObjectId>) {
    let mut motion = Motion::new_scene();
    motion.info = fold_media::VideoInfo {
        width: 64,
        height: 48,
        rate: [24, 1],
        frames: 96,
    };
    let mut objects = vec![];
    for (x, color) in [
        (12., [1., 0., 0., 1.]),
        (28., [0., 1., 0., 1.]),
        (44., [0., 0., 1., 1.]),
    ] {
        let id = authoring::scene::create_object(&mut motion, "fold.motion.rectangle").unwrap();
        authoring::node(&mut motion, id)
            .unwrap()
            .set("width", fold_motion::fields::Datum::Scalar(12.));
        authoring::node(&mut motion, id)
            .unwrap()
            .set("height", fold_motion::fields::Datum::Scalar(10.));
        let object = motion.scene.as_ref().unwrap().objects[0].clone();
        authoring::node(&mut motion, object.transform.unwrap())
            .unwrap()
            .set("translation", fold_motion::fields::Datum::Vector([x, 24.]));
        authoring::node(&mut motion, object.appearance.unwrap())
            .unwrap()
            .set("color", fold_motion::fields::Datum::Color(color));
        if x == 12. {
            let mut curve = fold_animation::Curve::default();
            curve.insert(Time::ZERO, 12.);
            curve.insert(Time::new(1, 1).unwrap(), 16.);
            let node = authoring::node(&mut motion, id).unwrap();
            node.animation.insert("width.0".into(), curve);
            node.extensions
                .insert("future.node".into(), serde_json::json!({"retain": true}));
        }
        objects.push(id);
    }
    let id = DocumentId::new();
    let mut document = motion.document(id).unwrap();
    document
        .extensions
        .insert("future.document".into(), serde_json::json!({"retain":true}));
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let mut registry = PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    (project, registry, id, objects)
}
fn request(
    project: &Project,
    document: DocumentId,
    child: DocumentId,
    objects: Vec<ObjectId>,
) -> CommandRequest {
    CommandRequest {
        id: composition::NEST.into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(&Args {
            document,
            child,
            objects,
            name: "Foreground".into(),
        })
        .unwrap(),
    }
}
fn render(
    registry: &PackageRegistry,
    project: &Project,
    id: DocumentId,
    time: Time,
) -> fold_render::Frame {
    fold_render::render(
        registry
            .video(&project.snapshot(), &reference(id), time, 64, 48)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn precompose_preserves_pixels_keys_unknown_data_and_atomic_undo_round_trip() {
    let (mut project, registry, parent, objects) = setup();
    let before = project.snapshot();
    let child = DocumentId::new();
    let expected = render(&registry, &project, parent, Time::new(1, 2).unwrap());
    let command = request(&project, parent, child, objects[..2].to_vec());
    let batch = registry
        .stage(&before, &command, &Default::default())
        .unwrap();
    project.commit(batch).unwrap();
    let after = project.snapshot();
    assert_eq!(after.revision().0, before.revision().0 + 1);
    assert_eq!(
        after
            .state()
            .organization
            .items
            .iter()
            .find(|item| item.id == fold_project::ItemId::Document(child))
            .unwrap()
            .name,
        "Foreground"
    );
    let nested = Motion::from_document(&after.state().documents[&child]).unwrap();
    assert_eq!(nested.scene.as_ref().unwrap().objects.len(), 2);
    let original = Motion::from_document(&before.state().documents[&parent]).unwrap();
    for id in &objects[..2] {
        assert_eq!(
            nested.graph.node(*id).unwrap(),
            original.graph.node(*id).unwrap()
        );
    }

    assert_eq!(
        nested.info,
        Motion::from_document(&before.state().documents[&parent])
            .unwrap()
            .info
    );
    assert_eq!(
        after.state().documents[&parent].extensions,
        before.state().documents[&parent].extensions
    );
    assert_eq!(
        after.state().documents[&parent].dependencies,
        vec![reference(child)]
    );
    let actual = render(&registry, &project, parent, Time::new(1, 2).unwrap());
    let max = actual
        .pixels()
        .iter()
        .zip(expected.pixels())
        .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
        .fold(0f32, f32::max);
    assert!(max < 0.005, "nesting changed rendered pixels: {max}");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested.fold");
    fold_project::save(&after, &path).unwrap();
    let reopened = fold_project::load(&path, 16).unwrap();
    assert_eq!(
        reopened.snapshot().state().documents,
        after.state().documents
    );
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
    assert_eq!(
        project.snapshot().state().organization,
        before.state().organization
    );
    project.redo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        after.state().documents
    );
}

#[test]
fn nested_placement_moving_trimming_and_mixed_rate_local_time_are_exact() {
    let (mut project, registry, parent, _) = setup();
    let child = DocumentId::new();
    let mut source = Motion::new_scene();
    source.info = fold_media::VideoInfo {
        width: 64,
        height: 48,
        rate: [30000, 1001],
        frames: 300,
    };
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(source.document(child).unwrap())],
        })
        .unwrap();
    let at = Time::new(1, 3).unwrap();
    let source_start = Time::new(1001, 30000).unwrap();
    let duration = Time::new(2, 1).unwrap();
    let batch = registry
        .place(
            &project.snapshot(),
            &Placement {
                graph: None,
                source: PlacementSource::Output(reference(child)),
                target: parent,
                track: None,
                at,
                source_start,
                duration: Some(duration),
                audio_only: false,
                new_track: false,
                mode: Default::default(),
            },
        )
        .unwrap();
    project.commit(batch).unwrap();
    let mut motion = Motion::from_document(&project.snapshot().state().documents[&parent]).unwrap();
    let id = motion.scene.as_ref().unwrap().objects[0].id;
    assert_eq!(motion.graph.node(id).unwrap().kind, KIND);
    let settings = |m: &Motion| m.graph.node(id).unwrap().settings::<Settings>().unwrap();
    assert_eq!(
        settings(&motion)
            .request(at)
            .unwrap()
            .unwrap()
            .mapped_time()
            .unwrap(),
        Some(source_start)
    );
    let moved = Time::new(7, 6).unwrap();
    composition::set_range(&mut motion, id, moved, moved.checked_add(duration).unwrap()).unwrap();
    assert_eq!(
        settings(&motion).request(moved).unwrap().unwrap().time,
        source_start
    );
    let trimmed = moved.checked_add(Time::new(1, 24).unwrap()).unwrap();
    composition::set_range(
        &mut motion,
        id,
        trimmed,
        moved.checked_add(duration).unwrap(),
    )
    .unwrap();
    assert_eq!(
        settings(&motion).request(trimmed).unwrap().unwrap().time,
        source_start.checked_add(Time::new(1, 24).unwrap()).unwrap()
    );
    assert_eq!(settings(&motion).start, moved);
    let cycle = registry.place(
        &project.snapshot(),
        &Placement {
            graph: None,
            source: PlacementSource::Output(reference(parent)),
            target: child,
            track: None,
            at: Time::ZERO,
            source_start: Time::ZERO,
            duration: None,
            audio_only: false,
            new_track: false,
            mode: Default::default(),
        },
    );
    assert!(cycle.is_err());
}

#[test]
fn child_duration_edits_refresh_nested_metadata_without_editing_parent() {
    let (mut project, registry, parent, objects) = setup();
    let child = DocumentId::new();
    let batch = registry
        .stage(
            &project.snapshot(),
            &request(&project, parent, child, objects[..1].to_vec()),
            &Default::default(),
        )
        .unwrap();
    project.commit(batch).unwrap();
    let parent_before = project.snapshot().state().documents[&parent].clone();
    let mut child_motion =
        Motion::from_document(&project.snapshot().state().documents[&child]).unwrap();
    child_motion.info.frames = 12;
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(child_motion.document(child).unwrap())],
        })
        .unwrap();
    // Time 1s used to be within the child's four-second duration. It must now be transparent,
    // not an out-of-range compile error based on stale placement metadata.
    let outside = render(&registry, &project, parent, Time::new(1, 1).unwrap());
    assert_eq!(outside.pixels()[24 * 64 + 12][3], 0.);
    let inside = render(&registry, &project, parent, Time::new(1, 4).unwrap());
    assert!((inside.pixels()[24 * 64 + 12][3] - 1.).abs() < 0.00001);
    assert_eq!(project.snapshot().state().documents[&parent], parent_before);
}

#[test]
fn invalid_nesting_never_changes_the_project() {
    let (mut project, registry, parent, objects) = setup();
    let before = project.snapshot();
    let disjoint = request(
        &project,
        parent,
        DocumentId::new(),
        vec![objects[0], objects[2]],
    );
    assert!(
        registry
            .stage(&before, &disjoint, &Default::default())
            .unwrap_err()
            .contains("adjacent")
    );
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
    let mut motion = Motion::from_document(&before.state().documents[&parent]).unwrap();
    motion
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == objects[0])
        .unwrap()
        .masks
        .push(fold_motion::scene::Mask::new(
            objects[1],
            fold_motion::scene::MaskSpace::Local,
        ));
    project
        .commit(EditBatch {
            base: before.revision(),
            mutations: vec![Mutation::PutDocument(motion.document(parent).unwrap())],
        })
        .unwrap();
    let before = project.snapshot();
    assert!(
        registry
            .stage(
                &before,
                &request(&project, parent, DocumentId::new(), vec![objects[0]]),
                &Default::default()
            )
            .is_err()
    );
    assert!(
        registry
            .stage(
                &before,
                &request(&project, parent, DocumentId::new(), vec![objects[1]]),
                &Default::default()
            )
            .is_err()
    );
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn nesting_groups_preserves_sibling_order_when_children_precede_their_parent() {
    let (mut project, registry, parent, objects) = setup();
    let mut motion = Motion::from_document(&project.snapshot().state().documents[&parent]).unwrap();
    let group = authoring::scene::create_group(&mut motion).unwrap();
    authoring::scene::reparent(&mut motion, objects[2], Some(group), Time::ZERO, true).unwrap();
    let scene = motion.scene.as_mut().unwrap();
    let group_object = scene.objects.remove(0);
    scene.objects.insert(1, group_object);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(motion.document(parent).unwrap())],
        })
        .unwrap();
    let child = DocumentId::new();
    let batch = registry
        .stage(
            &project.snapshot(),
            &request(&project, parent, child, vec![group]),
            &Default::default(),
        )
        .unwrap();
    project.commit(batch).unwrap();
    let parent_motion =
        Motion::from_document(&project.snapshot().state().documents[&parent]).unwrap();
    let first = &parent_motion.scene.as_ref().unwrap().objects[0];
    assert_eq!(
        parent_motion.graph.node(first.source.node).unwrap().kind,
        KIND
    );
    let child_motion =
        Motion::from_document(&project.snapshot().state().documents[&child]).unwrap();
    assert!(
        child_motion
            .scene
            .as_ref()
            .unwrap()
            .objects
            .iter()
            .any(|o| o.id == objects[2] && o.parent == Some(group))
    );

    let empty = DocumentId::new();
    let before = project.snapshot();
    let batch = registry
        .stage(
            &before,
            &request(&project, parent, empty, vec![]),
            &Default::default(),
        )
        .unwrap();
    project.commit(batch).unwrap();
    assert!(
        Motion::from_document(&project.snapshot().state().documents[&empty])
            .unwrap()
            .scene
            .unwrap()
            .objects
            .is_empty()
    );
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
    assert_eq!(
        project.snapshot().state().organization,
        before.state().organization
    );
}
