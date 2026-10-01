use fold_app::session::Session;
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::desktop::{
    DesktopClient, DesktopCommand as Command, PlaybackRange, TransportAction as Action,
    ViewLocation,
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};

fn fixture() -> (Session, [DocumentId; 3]) {
    use fold_compositor::{Composite, Node, Parameters};
    use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
    let ids = [DocumentId::new(), DocumentId::new(), DocumentId::new()];
    let mut motion = fold_motion::Motion::empty();
    motion.info.rate = [48, 1];
    motion.info.frames = 48;
    let solid = Node::new(
        Parameters::Solid {
            rgba: [0., 0., 0., 1.],
        },
        vec![],
    );
    let output = Node::new(Parameters::Output, vec![solid.id]);
    let composite = Composite {
        info: motion.info.clone(),
        output: output.id,
        nodes: vec![solid, output],
        extensions: Default::default(),
    };
    let track = Track::new(TrackKind::Video, "Motion");
    let sequence = Sequence {
        dimensions: [64, 64],
        rate: [24, 1],
        tracks: vec![track.clone()],
        clips: vec![Clip {
            id: ObjectId::new(),
            track: track.id,
            link: None,
            asset: Default::default(),
            info: SourceMedia::Document {
                source: DocumentRef {
                    document: ids[0],
                    output: "video".into(),
                    extensions: Default::default(),
                },
                info: motion.info.clone(),
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(1, 1).unwrap(),
            level: 1.,
            extensions: Default::default(),
        }],
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(motion.document(ids[0]).unwrap()),
                Mutation::PutDocument(composite.document(ids[1]).unwrap()),
                Mutation::PutDocument(sequence.document(ids[2]).unwrap()),
            ],
        })
        .unwrap();
    (Session::new(project), ids)
}
fn navigate(session: &mut Session, document: DocumentId) {
    session.command(Command::Navigate(ViewLocation {
        document,
        time: Time::ZERO,
        label: "Test".into(),
    }));
}
fn action(session: &mut Session, action: Action) {
    session.command(Command::Transport(action));
}

#[test]
fn one_transport_controls_each_view_and_keeps_review_marks_out_of_project_data() {
    let (mut session, ids) = fixture();
    let before = session.snapshot().unwrap();
    for (index, id) in ids.into_iter().enumerate() {
        navigate(&mut session, id);
        assert_eq!(session.state().viewer_document, Some(id));
        assert_eq!(session.state().playback_range, PlaybackRange::default());
        action(&mut session, Action::Jump(3 + index as u32));
        action(&mut session, Action::MarkIn);
        action(&mut session, Action::Jump(10));
        action(&mut session, Action::MarkOut);
        action(&mut session, Action::Stop);
        assert_eq!(session.state().frame, 3 + index as u32);
        assert!(!session.state().playing);
        action(&mut session, Action::NextFrame);
        assert_eq!(session.state().frame, 4 + index as u32);
        action(&mut session, Action::PreviousFrame);
        action(&mut session, Action::GoToOut);
        assert_eq!(session.state().frame, 10);
        action(&mut session, Action::GoToIn);
        assert_eq!(session.state().frame, 3 + index as u32);
        let rate = session.state().rate;
        assert_eq!(
            session.state().navigation.last().unwrap().time,
            Time::new(
                i64::from(session.state().frame) * i64::from(rate[1]),
                rate[0]
            )
            .unwrap()
        );
        action(&mut session, Action::Jump(u32::MAX));
        assert_eq!(session.state().frame, session.state().frames - 1);
    }
    for (index, id) in ids.into_iter().enumerate() {
        navigate(&mut session, id);
        assert_eq!(
            session.state().playback_range,
            PlaybackRange {
                start: Some(3 + index as u32),
                end: Some(10)
            }
        );
    }
    // Crossing a mark collapses the opposite bound, never creates a reversed range.
    action(&mut session, Action::Jump(2));
    action(&mut session, Action::MarkOut);
    assert_eq!(
        session
            .state()
            .playback_range
            .bounds(session.state().frames),
        (2, 2)
    );
    action(&mut session, Action::Jump(0));
    action(&mut session, Action::PreviousFrame);
    assert_eq!(session.state().frame, 0);
    assert_eq!(session.snapshot().unwrap().revision(), before.revision());
    assert_eq!(
        session.snapshot().unwrap().state().documents,
        before.state().documents
    );
}

#[cfg(feature = "desktop")]
#[test]
fn nested_motion_and_compositor_play_their_own_marked_range_then_hold_out() {
    use std::time::{Duration, Instant};
    let (mut session, ids) = fixture();
    for id in ids {
        navigate(&mut session, id);
        action(&mut session, Action::Jump(2));
        action(&mut session, Action::MarkIn);
        action(&mut session, Action::Jump(4));
        action(&mut session, Action::MarkOut);
        action(&mut session, Action::GoToIn);
        action(&mut session, Action::TogglePlay);
        assert!(
            session.state().playing,
            "nested views must support playback"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while session.state().playing {
            assert!(Instant::now() < deadline, "marked playback did not finish");
            session.poll();
            assert!(session.state().frame <= 4);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            session.state().frame,
            4,
            "Out is included and remains visible"
        );
        assert_eq!(session.state().viewer_document, Some(id));
        action(&mut session, Action::Stop);
        assert_eq!(session.state().frame, 2);
        action(&mut session, Action::Jump(3));
        action(&mut session, Action::TogglePlay);
        action(&mut session, Action::TogglePlay);
        assert!(!session.state().playing);
        assert_eq!(session.state().frame, 3, "Pause retains the current frame");
        action(&mut session, Action::TogglePlay);
        action(&mut session, Action::GoToOut);
        assert!(!session.state().playing);
        assert_eq!(session.state().frame, 4);
    }
}
