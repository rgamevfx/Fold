use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_media::VideoInfo;
use fold_platform::VideoProvider;
use fold_project::{Asset, EditBatch, Mutation, Project, Snapshot};
use fold_render::ImageOp;
use fold_timeline::{
    Clip, ClipEdit, Sequence, SequenceProvider, SourceMedia, Track, TrackKind, edit_clip,
};
use serde_json::json;

fn t(frame: i64) -> Time {
    Time::new(frame * 1001, 24000).unwrap()
}
fn setup() -> (Project, DocumentId, ObjectId) {
    setup_kind(TrackKind::Video)
}
fn setup_kind(kind: TrackKind) -> (Project, DocumentId, ObjectId) {
    let mut project = Project::new(20);
    let asset = AssetId::new();
    let id = DocumentId::new();
    let clip = ObjectId::new();
    let track = Track::new(kind, "Track 1");
    let sequence = Sequence {
        tracks: vec![track.clone()],
        dimensions: [2, 2],
        rate: [24000, 1001],
        clips: vec![Clip {
            id: clip,
            track: track.id,
            link: None,
            level: 1.0,
            asset,
            info: SourceMedia::Video(VideoInfo {
                width: 2,
                height: 2,
                rate: [24000, 1001],
                frames: 100,
            }),
            start: t(10),
            source_start: t(20),
            duration: t(30),
            extensions: [("future-clip".into(), json!({"gain": 0.7}))].into(),
        }],
        extensions: [("future-sequence".into(), json!([1, 2]))].into(),
    };
    let mut document = sequence.document(id).unwrap();
    document
        .extensions
        .insert("future-envelope".into(), json!(42));
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutAsset(Asset {
                    id: asset,
                    location: "unused.mp4".into(),
                    fingerprint: "test".into(),
                    extensions: Default::default(),
                }),
                Mutation::PutDocument(document),
            ],
        })
        .unwrap();
    (project, id, clip)
}
fn sequence(snapshot: &Snapshot, id: DocumentId) -> Sequence {
    Sequence::from_document(&snapshot.state().documents[&id]).unwrap()
}

#[test]
fn move_trim_split_preserve_mapping_extensions_and_history() {
    let (mut project, id, clip) = setup();
    let before = project.snapshot();
    project
        .commit(edit_clip(&before, id, clip, ClipEdit::Move { start: t(15) }).unwrap())
        .unwrap();
    let moved = project.snapshot();
    let c = &sequence(&moved, id).clips[0];
    assert_eq!(c.source_time(t(15)).unwrap(), Some(t(20)));
    assert_eq!(c.duration, t(30));
    project
        .commit(
            edit_clip(
                &moved,
                id,
                clip,
                ClipEdit::Trim {
                    start: t(17),
                    end: t(40),
                },
            )
            .unwrap(),
        )
        .unwrap();
    let trimmed = project.snapshot();
    let c = &sequence(&trimmed, id).clips[0];
    assert_eq!(c.source_time(t(17)).unwrap(), Some(t(22)));
    assert_eq!(c.source_time(t(39)).unwrap(), Some(t(44)));
    assert_eq!(c.source_time(t(40)).unwrap(), None);
    let right = ObjectId::new();
    project
        .commit(
            edit_clip(
                &trimmed,
                id,
                clip,
                ClipEdit::Split {
                    at: t(25),
                    right_id: right,
                },
            )
            .unwrap(),
        )
        .unwrap();
    let split = project.snapshot();
    let seq = sequence(&split, id);
    assert_eq!(seq.clips.len(), 2);
    assert_eq!(seq.clips[0].id, clip);
    assert_eq!(seq.clips[1].id, right);
    for frame in 17..40 {
        let mapped: Vec<_> = seq
            .clips
            .iter()
            .filter_map(|c| c.source_time(t(frame)).unwrap())
            .collect();
        assert_eq!(mapped, vec![t(frame + 5)]);
    }
    assert_eq!(seq.clips[0].extensions, seq.clips[1].extensions);
    assert_eq!(seq.extensions, sequence(&before, id).extensions);
    assert_eq!(
        split.state().documents[&id].extensions,
        before.state().documents[&id].extensions
    );
    assert_eq!(
        project.undo().unwrap().state().documents,
        trimmed.state().documents
    );
    assert_eq!(
        project.redo().unwrap().state().documents,
        split.state().documents
    );
    assert_eq!(sequence(&before, id).clips[0].start, t(10));
}

#[test]
fn invalid_edits_and_stale_proposals_publish_nothing() {
    let (mut project, id, clip) = setup();
    let before = project.snapshot();
    for edit in [
        ClipEdit::Move { start: t(-1) },
        ClipEdit::Trim {
            start: t(20),
            end: t(20),
        },
        ClipEdit::Trim {
            start: t(0),
            end: t(100),
        },
        ClipEdit::Split {
            at: t(10),
            right_id: ObjectId::new(),
        },
        ClipEdit::Split {
            at: t(40),
            right_id: ObjectId::new(),
        },
        ClipEdit::Split {
            at: t(25),
            right_id: clip,
        },
    ] {
        assert!(edit_clip(&before, id, clip, edit).is_err());
    }
    assert_eq!(project.snapshot().revision(), before.revision());
    let stale = edit_clip(&before, id, clip, ClipEdit::Move { start: t(0) }).unwrap();
    let right = ObjectId::new();
    project
        .commit(
            edit_clip(
                &before,
                id,
                clip,
                ClipEdit::Split {
                    at: t(25),
                    right_id: right,
                },
            )
            .unwrap(),
        )
        .unwrap();
    let split = project.snapshot();
    assert!(edit_clip(&split, id, right, ClipEdit::Move { start: t(24) }).is_err());
    assert!(project.commit(stale).is_err());
    assert_eq!(project.snapshot().revision(), split.revision());
}

#[test]
fn source_handles_and_video_lowering_use_exact_half_open_ranges() {
    let (mut project, id, clip) = setup();
    project
        .commit(
            edit_clip(
                &project.snapshot(),
                id,
                clip,
                ClipEdit::Trim {
                    start: t(0),
                    end: t(50),
                },
            )
            .unwrap(),
        )
        .unwrap();
    let snapshot = project.snapshot();
    let doc = &snapshot.state().documents[&id];
    let compile = |output: &str, time| {
        SequenceProvider.compile(fold_platform::VideoCompile {
            snapshot: &snapshot,
            document: doc,
            reference: &fold_project::DocumentRef {
                document: id,
                output: output.into(),
                extensions: Default::default(),
            },
            time,
            dimensions: [2, 2],
            cancel: &Default::default(),
            resolve: &|_, _| Err("unexpected nested source".into()),
            resolve_image: &|_, _| Err("unexpected nested image".into()),
            describe_channels: &|_| Err("unexpected nested channel catalog".into()),
        })
    };
    for frame in [0, 25, 49] {
        let graph = compile("video", t(frame)).unwrap();
        match &graph.nodes[graph.output] {
            ImageOp::Video { source, time } => {
                assert_eq!(*time, t(frame + 10));
                assert_eq!(source.fingerprint, "test");
            }
            _ => panic!("expected mapped video"),
        }
    }
    let graph = compile("video", t(50)).unwrap();
    assert!(matches!(graph.nodes[0], ImageOp::Solid { .. }));
    assert!(compile("audio", t(0)).is_err());
}

#[test]
fn edited_sequence_survives_save_reopen() {
    let (mut project, id, clip) = setup();
    let batch = edit_clip(
        &project.snapshot(),
        id,
        clip,
        ClipEdit::Split {
            at: t(25),
            right_id: ObjectId::new(),
        },
    )
    .unwrap();
    let edited = project.commit(batch).unwrap();
    let path = std::env::temp_dir().join(format!("fold-sequence-{:?}.fold", DocumentId::new()));
    fold_project::save(&edited, &path).unwrap();
    let loaded = fold_project::load(&path, 20).unwrap().snapshot();
    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded.state().documents, edited.state().documents);
    assert_eq!(sequence(&loaded, id).clips.len(), 2);
}

#[test]
fn sub_sample_split_uses_one_shared_boundary_without_rounding_drift() {
    let (mut project, id, clip) = setup_kind(TrackKind::Audio);
    let before = project.snapshot();
    let original = sequence(&before, id).audio_plan(&before).unwrap();
    let at = Time::new(3, 7).unwrap();
    project
        .commit(
            edit_clip(
                &before,
                id,
                clip,
                ClipEdit::Split {
                    at,
                    right_id: ObjectId::new(),
                },
            )
            .unwrap(),
        )
        .unwrap();
    let snapshot = project.snapshot();
    let plan = sequence(&snapshot, id).audio_plan(&snapshot).unwrap();
    assert_eq!(plan.regions[0].end, 20572);
    assert_eq!(plan.regions[1].start, 20572);
    assert_eq!(
        plan.regions[0].source_offset,
        original.regions[0].source_offset
    );
    assert_eq!(
        plan.regions[1].source_offset,
        original.regions[0].source_offset
    );
    assert_eq!(plan.end, original.end);
}

#[test]
fn malformed_documents_and_shadowed_extensions_are_rejected() {
    let (project, id, _) = setup();
    let snapshot = project.snapshot();
    let mut document = (*snapshot.state().documents[&id]).clone();
    document.assets.clear();
    assert!(Sequence::from_document(&document).is_err());
    let mut seq = sequence(&snapshot, id);
    seq.clips[0].extensions.insert("start".into(), json!(0));
    assert!(seq.document(id).is_err());
    let mut seq = sequence(&snapshot, id);
    seq.clips[0].source_start = t(90);
    assert!(seq.document(id).is_err());
    let mut seq = sequence(&snapshot, id);
    seq.clips[0].start = Time::new(600, 1).unwrap();
    assert!(seq.document(id).is_err());
    let mut seq = sequence(&snapshot, id);
    if let SourceMedia::Video(info) = &mut seq.clips[0].info {
        info.frames = 18000;
    } // exceeds PCM budget
    assert!(seq.document(id).is_err());
}
