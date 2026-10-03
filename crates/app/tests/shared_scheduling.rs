//! End-to-end worker routing and concurrent export/ingest on the CPU reference.
use fold_app::{media_workflow, session::Session};
use fold_foundation::{DocumentId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, PreviewDemand, PreviewKey},
    workspace::PanelInstanceId,
};
use fold_project::{EditBatch, Mutation, Parent, Project};
use std::time::{Duration, Instant};

fn fixture() -> (Session, DocumentId) {
    let id = DocumentId::new();
    let mut project = Project::new(8);
    let mut motion = fold_motion::Motion::empty();
    motion.info.width = 32;
    motion.info.height = 32;
    motion.info.frames = 8;
    fold_motion::authoring::add_content(&mut motion, "fold.motion.rectangle").unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(motion.document(id).unwrap())],
        })
        .unwrap();
    project
        .commit(media_workflow::select_output(&project.snapshot(), id).unwrap())
        .unwrap();
    (Session::new(project), id)
}
fn demand(
    session: &Session,
    document: DocumentId,
    consumer: u64,
    generation: u64,
    frame: u32,
) -> PreviewDemand {
    let output = fold_project::DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    let state = session.preview_state(&output, Time::new(i64::from(frame), 24).unwrap());
    PreviewDemand {
        consumer: PanelInstanceId(consumer),
        generation,
        background: false,
        key: state.preview_key(frame, 1).unwrap(),
    }
}
fn ready(session: &mut Session) -> (PreviewKey, Vec<(PanelInstanceId, u64)>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        session.poll();
        if let Some(result) = session.take_preview() {
            result.frame.unwrap();
            return (result.key, result.consumers);
        }
        assert!(Instant::now() < deadline, "preview worker timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn equivalent_demands_share_one_result_and_independent_replacement_routes_correctly() {
    let (mut session, document) = fixture();
    let a = demand(&session, document, 1, 1, 0);
    let b = demand(&session, document, 2, 9, 0);
    session.preview_demands(vec![a.clone(), b.clone()]);
    let (key, owners) = ready(&mut session);
    assert_eq!(key, a.key);
    assert_eq!(owners, [(PanelInstanceId(1), 1), (PanelInstanceId(2), 9)]);
    session.preview_demands(vec![a.clone(), b.clone()]);
    assert!(session.take_preview().is_none());
    let next = demand(&session, document, 1, 2, 7);
    session.preview_demands(vec![next.clone(), b]);
    let (key, owners) = ready(&mut session);
    assert_eq!(key, next.key);
    assert_eq!(owners, [(PanelInstanceId(1), 2)]);
    session.preview_demands(vec![]);
    assert!(session.take_preview().is_none());
}
#[test]
fn two_consumers_continue_while_export_and_asset_only_ingest_run() {
    let (mut session, document) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("linked.ppm");
    std::fs::write(&image, b"P6\n1 1\n255\n\x10\x20\x30").unwrap();
    let destination = directory.path().join("delivery.mp4");
    session.command(DesktopCommand::Export {
        path: destination.clone(),
        start: 0,
        end: 8,
    });
    session.command(DesktopCommand::Browser(
        fold_platform::browser::BrowserCommand::Import {
            paths: vec![image],
            destination: Parent::Root,
        },
    ));
    assert!(session.resource_statistics().contains("jobs 2/2"));
    for frame in [0, 7, 1, 6, 2, 5] {
        let a = demand(&session, document, 1, u64::from(frame) + 1, frame);
        let b = demand(&session, document, 2, u64::from(frame) + 10, 7 - frame);
        session.preview_demands(vec![a, b]);
        ready(&mut session);
    }
    session.preview_demands(vec![]);
    let deadline = Instant::now() + Duration::from_secs(20);
    while session.state().busy {
        session.poll();
        assert!(Instant::now() < deadline, "{}", session.state().status);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(destination.exists(), "{}", session.state().status);
    assert_eq!(session.snapshot().unwrap().state().assets.len(), 1);
    assert_eq!(session.output_info().unwrap().0, document);
}
