use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_media::VideoInfo;
use fold_platform::VideoProvider;
use fold_project::{Asset, EditBatch, Mutation, Project};
use fold_timeline::*;
fn t(n: i64) -> Time {
    Time::new(n, 1).unwrap()
}
fn setup() -> (Project, DocumentId, Sequence) {
    let mut project = Project::new(20);
    let id = DocumentId::new();
    let asset = AssetId::new();
    let tracks = vec![
        Track::new(TrackKind::Video, "V1"),
        Track::new(TrackKind::Video, "V2"),
        Track::new(TrackKind::Audio, "A1"),
        Track::new(TrackKind::Audio, "A2"),
    ];
    let link = ObjectId::new();
    let clip = |track: usize, start, duration, link| Clip {
        id: ObjectId::new(),
        track: tracks[track].id,
        link,
        asset,
        info: SourceMedia::Video(VideoInfo {
            width: 2,
            height: 2,
            rate: [24, 1],
            frames: 240,
        }),
        start: t(start),
        source_start: t(1),
        duration: t(duration),
        level: 1.0,
        extensions: Default::default(),
    };
    let mut clips = vec![
        clip(0, 0, 2, Some(link)),
        clip(2, 0, 2, Some(link)),
        clip(1, 2, 2, None),
        clip(3, 0, 9, None),
    ];
    clips[2].level = 0.5;
    let sequence = Sequence {
        tracks,
        clips,
        dimensions: [2, 2],
        rate: [24, 1],
        extensions: Default::default(),
    };
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutAsset(Asset {
                    id: asset,
                    location: "fixture.mp4".into(),
                    fingerprint: "fixture".into(),
                    extensions: Default::default(),
                }),
                Mutation::PutDocument(sequence.document(id).unwrap()),
            ],
        })
        .unwrap();
    (project, id, sequence)
}
fn edit(id: ObjectId, linked: bool, edit: ClipEdit) -> SequenceEdit {
    SequenceEdit::Clip { id, linked, edit }
}
#[test]
fn linked_cross_track_move_trim_split_lift_and_history_are_atomic() {
    let (mut project, id, original) = setup();
    let before = project.snapshot();
    let selected = original.clips[0].id;
    let move_edit = edit(
        selected,
        true,
        ClipEdit::MoveTo {
            start: t(4),
            track: original.tracks[1].id,
        },
    );
    project
        .commit(edit_sequence(&before, id, &move_edit).unwrap())
        .unwrap();
    let moved = project.snapshot();
    let sequence = Sequence::from_document(&moved.state().documents[&id]).unwrap();
    assert_eq!(sequence.clips[0].track, original.tracks[1].id);
    assert_eq!(sequence.clips[1].track, original.tracks[2].id);
    for clip in &sequence.clips[..2] {
        assert_eq!(clip.start, t(4));
        assert_eq!(clip.source_start, t(1));
    }
    project
        .commit(
            edit_sequence(
                &moved,
                id,
                &edit(
                    selected,
                    true,
                    ClipEdit::Trim {
                        start: t(5),
                        end: t(6),
                    },
                ),
            )
            .unwrap(),
        )
        .unwrap();
    let trimmed = project.snapshot();
    let seq = Sequence::from_document(&trimmed.state().documents[&id]).unwrap();
    for clip in &seq.clips[..2] {
        assert_eq!(clip.start, t(5));
        assert_eq!(clip.source_start, t(2));
        assert_eq!(clip.duration, t(1));
    }
    let right = ObjectId::new();
    project
        .commit(
            edit_sequence(
                &trimmed,
                id,
                &edit(
                    selected,
                    true,
                    ClipEdit::Split {
                        at: Time::new(11, 2).unwrap(),
                        right_id: right,
                    },
                ),
            )
            .unwrap(),
        )
        .unwrap();
    let split = project.snapshot();
    let seq = Sequence::from_document(&split.state().documents[&id]).unwrap();
    assert_eq!(seq.clips.len(), 6);
    let right_ids = seq.linked_ids(right, true).unwrap();
    assert_eq!(right_ids.len(), 2);
    assert_ne!(
        seq.clips.iter().find(|c| c.id == right).unwrap().link,
        seq.clips[0].link
    );
    project
        .commit(edit_sequence(&split, id, &edit(right, true, ClipEdit::Lift)).unwrap())
        .unwrap();
    assert_eq!(
        Sequence::from_document(&project.snapshot().state().documents[&id])
            .unwrap()
            .clips
            .len(),
        4
    );
    assert_eq!(
        project.undo().unwrap().state().documents,
        split.state().documents
    );
    assert_eq!(
        project.undo().unwrap().state().documents,
        trimmed.state().documents
    );
    assert_eq!(
        project.redo().unwrap().state().documents,
        split.state().documents
    );
    assert_eq!(
        Sequence::from_document(&before.state().documents[&id])
            .unwrap()
            .clips[0]
            .start,
        t(0)
    );
}
#[test]
fn locks_overlap_kinds_and_unlink_are_enforced_before_publication() {
    let (_, _, sequence) = setup();
    let id = sequence.clips[0].id;
    assert!(
        sequence
            .edited(&edit(
                id,
                true,
                ClipEdit::MoveTo {
                    start: t(2),
                    track: sequence.tracks[1].id
                }
            ))
            .is_err()
    );
    assert!(
        sequence
            .edited(&edit(
                id,
                true,
                ClipEdit::MoveTo {
                    start: t(4),
                    track: sequence.tracks[2].id
                }
            ))
            .is_err()
    );
    let mut locked = sequence.clone();
    locked.tracks[2].locked = true;
    assert!(
        locked
            .edited(&edit(id, true, ClipEdit::Move { start: t(4) }))
            .is_err()
    );
    assert!(
        locked
            .edited(&edit(id, false, ClipEdit::Move { start: t(4) }))
            .is_err()
    );
    assert!(locked.edited(&edit(id, true, ClipEdit::Unlink)).is_err());
    let independent = sequence
        .edited(&edit(id, false, ClipEdit::Move { start: t(4) }))
        .unwrap();
    assert_eq!(independent.clips[0].start, t(4));
    assert_eq!(independent.clips[1].start, t(0));
    assert!(independent.clips[..2].iter().all(|c| c.link.is_none()));
    let lifted = sequence.edited(&edit(id, false, ClipEdit::Lift)).unwrap();
    assert!(
        lifted
            .clips
            .iter()
            .find(|c| c.id == sequence.clips[1].id)
            .unwrap()
            .link
            .is_none()
    );
}
#[test]
fn track_visibility_solo_mute_opacity_and_audio_mix_lower_to_shared_plans() {
    let (project, id, mut sequence) = setup();
    // Put the overlay over the first video track, not after it.
    sequence.clips[2].start = t(0);
    let compile = |document: &fold_project::Document| {
        SequenceProvider.compile(fold_platform::VideoCompile {
            snapshot: &project.snapshot(),
            document,
            reference: &fold_project::DocumentRef {
                document: id,
                output: "video".into(),
                extensions: Default::default(),
            },
            time: t(0),
            dimensions: [2, 2],
            cancel: &Default::default(),
            resolve: &|_, _| Err("unexpected nested source".into()),
        })
    };
    let document = sequence.document(id).unwrap();
    let graph = compile(&document).unwrap();
    assert!(matches!(
        graph.nodes[graph.output],
        fold_render::ImageOp::Over { .. }
    ));
    sequence.tracks[1].enabled = false;
    let graph = compile(&sequence.document(id).unwrap()).unwrap();
    assert!(matches!(
        graph.nodes[graph.output],
        fold_render::ImageOp::Video { .. }
    ));
    assert_eq!(
        sequence
            .audio_plan(&project.snapshot())
            .unwrap()
            .regions
            .len(),
        2
    );
    sequence.tracks[3].solo = true;
    let audio = sequence.audio_plan(&project.snapshot()).unwrap();
    assert_eq!(audio.regions.len(), 1);
    assert_eq!(audio.regions[0].end, 9 * 48000);
    sequence.tracks[3].enabled = false;
    assert!(
        sequence
            .audio_plan(&project.snapshot())
            .unwrap()
            .regions
            .is_empty()
    );
    sequence.tracks[3].solo = false;
    assert_eq!(
        sequence
            .audio_plan(&project.snapshot())
            .unwrap()
            .regions
            .len(),
        1
    );
}
