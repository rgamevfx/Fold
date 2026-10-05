use fold_app::session::Session;
use fold_platform::{
    browser::BrowserCommand,
    desktop::{DesktopClient, DesktopCommand},
};
use std::time::{Duration, Instant};
fn edit(session: &mut Session, name: &str) {
    session.command(DesktopCommand::Browser(BrowserCommand::NewBin {
        parent: Default::default(),
        name: name.into(),
    }));
}
fn finish(session: &mut Session) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.state().busy {
        assert!(Instant::now() < deadline, "{}", session.state().status);
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn save_tracks_the_written_snapshot_and_undo_returns_to_clean_content() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("project.fold");
    let mut session = Session::default();
    assert!(!session.state().dirty);
    assert!(!session.state().can_undo);
    edit(&mut session, "First");
    assert!(session.state().dirty && session.state().can_undo);
    session.command(DesktopCommand::Save(path.clone()));
    // An edit published before the save result is consumed must remain unsaved.
    edit(&mut session, "Second");
    finish(&mut session);
    assert!(session.state().dirty);
    assert_eq!(session.state().save_serial, 1);
    assert_eq!(
        session.state().project_path.as_ref(),
        Some(&std::fs::canonicalize(&path).unwrap())
    );
    session.command(DesktopCommand::Undo);
    assert!(!session.state().dirty);
    assert!(session.state().can_redo);
    session.command(DesktopCommand::Redo);
    assert!(session.state().dirty);
    session.command(DesktopCommand::Save(
        directory.path().join("missing/fail.fold"),
    ));
    finish(&mut session);
    assert!(session.state().dirty);
    assert_eq!(session.state().save_serial, 1);
    assert_eq!(session.state().project_path.as_ref(), Some(&path));
    session.command(DesktopCommand::NewProject);
    assert!(!session.state().dirty && !session.state().can_undo);
    assert!(session.state().project_path.is_none());
    assert!(
        session
            .snapshot()
            .unwrap()
            .state()
            .organization
            .bins
            .is_empty()
    );
    session.command(DesktopCommand::Open(path.clone()));
    finish(&mut session);
    assert!(!session.state().dirty);
    assert_eq!(
        session.snapshot().unwrap().state().organization.bins.len(),
        1
    );
    session.command(DesktopCommand::Open(directory.path().join("missing.fold")));
    finish(&mut session);
    assert_eq!(session.state().project_path.as_ref(), Some(&path));
    assert_eq!(
        session.snapshot().unwrap().state().organization.bins.len(),
        1
    );
}
#[test]
fn opening_does_not_discard_edits_published_while_io_runs() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("project.fold");
    let mut session = Session::default();
    session.command(DesktopCommand::Save(path.clone()));
    finish(&mut session);
    session.command(DesktopCommand::Open(path));
    edit(&mut session, "Keep me");
    finish(&mut session);
    assert!(session.state().dirty);
    assert_eq!(
        session.snapshot().unwrap().state().organization.bins.len(),
        1
    );
    assert!(session.state().status.contains("changed while opening"));
}
