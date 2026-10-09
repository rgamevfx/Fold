//! Shared exact-time sampling across provider-owned source placements.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::{Binding, Track},
    model::Construction,
    types::{Literal, Unit},
};
use fold_platform::{image_source::Source, parameters};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};

#[test]
fn nested_sources_map_one_root_time_and_repeated_uses_remain_distinct() {
    let registry = fold_app::packages::builtins();
    let motion_id = DocumentId::new();
    let mut motion =
        Construction::from_document(&fold_motion::package::create(motion_id).unwrap()).unwrap();
    motion.canvas.start = Time::new(10, 1).unwrap();
    motion.canvas.end = Time::new(20, 1).unwrap();
    let parameter = motion.definitions[0].nodes[0]
        .parameters
        .get_mut("height")
        .unwrap();
    let id = parameter.id;
    let mut track = Track::constant(
        Literal::Distance {
            value: 100.,
            unit: Unit::Canvas,
        },
        Time::new(10, 1).unwrap(),
    )
    .unwrap();
    let mut end = track.keys[0].clone();
    end.id = ObjectId::new();
    end.time = Time::new(20, 1).unwrap();
    end.value = Literal::Distance {
        value: 200.,
        unit: Unit::Canvas,
    };
    track.keys.push(end);
    parameter.value = serde_json::to_value(Binding::Track { track }).unwrap();
    let reference = DocumentRef {
        document: motion_id,
        output: motion.definitions[0].outputs[0].id.to_string(),
        extensions: Default::default(),
    };
    let mut composition = fold_composition::Composition::default();
    let info = fold_media::VideoInfo {
        width: 1280,
        height: 720,
        rate: [24, 1],
        frames: 240,
    };
    let mut a = fold_composition::Layer::new(
        "First",
        Some(Source::Output(reference.clone())),
        info.clone(),
    )
    .unwrap();
    a.origin = Time::new(2, 1).unwrap();
    a.source_start = Time::new(1, 1).unwrap();
    let mut b =
        fold_composition::Layer::new("Second", Some(Source::Output(reference)), info).unwrap();
    b.origin = Time::new(2, 1).unwrap();
    b.source_start = Time::new(3, 1).unwrap();
    b.locked = true;
    let (a_id, b_id) = (a.id, b.id);
    composition.layers = vec![a, b];
    let composition_id = DocumentId::new();
    let mut outer = fold_composition::Composition::default();
    let mut placement = fold_composition::Layer::new(
        "Nested",
        Some(Source::Output(DocumentRef {
            document: composition_id,
            output: "video".into(),
            extensions: Default::default(),
        })),
        composition.info.clone(),
    )
    .unwrap();
    placement.origin = Time::new(1, 1).unwrap();
    placement.source_start = Time::new(2, 1).unwrap();
    let placement_id = placement.id;
    outer.layers.push(placement);
    let outer_id = DocumentId::new();
    let timeline_id = DocumentId::new();
    let track = fold_timeline::Track::new(fold_timeline::TrackKind::Video, "Track");
    let clip_id = ObjectId::new();
    let timeline = fold_timeline::Sequence {
        dimensions: [1280, 720],
        rate: [24, 1],
        tracks: vec![track.clone()],
        transitions: vec![],
        extensions: Default::default(),
        clips: vec![fold_timeline::Clip {
            id: clip_id,
            track: track.id,
            link: None,
            asset: fold_foundation::AssetId::new(),
            info: fold_timeline::SourceMedia::Document {
                source: DocumentRef {
                    document: outer_id,
                    output: "video".into(),
                    extensions: Default::default(),
                },
                info: outer.info.clone(),
            },
            start: Time::new(2, 1).unwrap(),
            source_start: Time::new(1, 1).unwrap(),
            duration: Time::new(2, 1).unwrap(),
            level: 1.,
            effects: vec![],
            extensions: Default::default(),
        }],
    };
    let mut project = Project::new(8);
    let snapshot = project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(motion.document(motion_id).unwrap()),
                Mutation::PutDocument(composition.document(composition_id).unwrap()),
                Mutation::PutDocument(outer.document(outer_id).unwrap()),
                Mutation::PutDocument(timeline.document(timeline_id).unwrap()),
            ],
        })
        .unwrap();
    let entries = registry
        .document_parameters(
            &snapshot,
            &snapshot.state().documents[&outer_id],
            &Default::default(),
        )
        .unwrap();
    let find = |owner| {
        entries
            .iter()
            .find(|e| {
                e.target.parameter.native() == Some(id)
                    && e.target
                        .placement
                        .as_ref()
                        .is_some_and(|p| p.owners == [placement_id, owner])
            })
            .unwrap()
    };
    let (a, b) = (find(a_id), find(b_id));
    assert_ne!(a.target, b.target);
    assert!(a.editable);
    assert!(!b.editable);
    assert_eq!(
        parameters::resolve(&entries, &[], &parameters::absolute_path(a)).unwrap(),
        a
    );
    // Root 2 -> outer source 3 -> inner source 2/4 -> Motion authored 12/14.
    for (entry, expected) in [(a, 120.), (b, 140.)] {
        assert_eq!(
            registry
                .sample_placed_parameter(
                    &snapshot,
                    &entry.target,
                    Time::new(2, 1).unwrap(),
                    &Default::default()
                )
                .unwrap(),
            Literal::Distance {
                value: expected,
                unit: Unit::Canvas
            }
        );
        assert!(
            registry
                .sample_parameter(
                    &snapshot.state().documents[&motion_id],
                    &entry.target,
                    Time::ZERO,
                    &Default::default()
                )
                .is_err()
        );
    }
    let placed = registry
        .document_parameters(
            &snapshot,
            &snapshot.state().documents[&timeline_id],
            &Default::default(),
        )
        .unwrap();
    let placed = placed
        .iter()
        .find(|e| {
            e.target.parameter.native() == Some(id)
                && e.target
                    .placement
                    .as_ref()
                    .is_some_and(|p| p.owners == [clip_id, placement_id, a_id])
        })
        .unwrap();
    assert_eq!(
        registry
            .sample_placed_parameter(
                &snapshot,
                &placed.target,
                Time::new(3, 1).unwrap(),
                &Default::default()
            )
            .unwrap(),
        Literal::Distance {
            value: 120.,
            unit: Unit::Canvas
        }
    );
    let mut stale = a.target.clone();
    stale.placement.as_mut().unwrap().owners[1] = ObjectId::new();
    assert!(
        registry
            .sample_placed_parameter(&snapshot, &stale, Time::ZERO, &Default::default())
            .is_err()
    );
    composition.layers[0].source = Some(Source::Output(DocumentRef {
        document: motion_id,
        output: "missing-output".into(),
        extensions: Default::default(),
    }));
    let broken = project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(
                composition.document(composition_id).unwrap(),
            )],
        })
        .unwrap();
    assert!(
        registry
            .sample_placed_parameter(
                &broken,
                &a.target,
                Time::new(2, 1).unwrap(),
                &Default::default()
            )
            .unwrap_err()
            .contains("source output")
    );
}
