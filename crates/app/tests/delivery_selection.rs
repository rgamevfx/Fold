use fold_app::{media_workflow, session::Session};
use fold_foundation::DocumentId;
use fold_motion::{Motion, authoring};
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_project::{EditBatch, Mutation, Project};

#[test]
fn delivery_selection_is_transactional_and_missing_selection_does_not_fall_back() {
    let mut project = Project::new(8);
    let id = DocumentId::new();
    let other = DocumentId::new();
    let mut motion = Motion::empty();
    authoring::add_content(&mut motion, "fold.motion.rectangle").unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(motion.document(id).unwrap()),
                Mutation::PutDocument(motion.document(other).unwrap()),
            ],
        })
        .unwrap();
    project
        .commit(media_workflow::select_output(&project.snapshot(), id).unwrap())
        .unwrap();
    let mut session = Session::new(project);
    session.command(DesktopCommand::SetOutput(other));
    assert_eq!(session.output_info().unwrap().0, other);
    session.command(DesktopCommand::Undo);
    assert_eq!(session.output_info().unwrap().0, id);
    session.command(DesktopCommand::SetOutput(DocumentId::new()));
    assert_eq!(session.output_info().unwrap().0, id);
    let snapshot = session.snapshot().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delivery.fold");
    fold_project::save(&snapshot, &path).unwrap();
    let mut project = fold_project::load(&path, 8).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::RemoveDocument(id)],
        })
        .unwrap();
    assert!(media_workflow::output(&project.snapshot()).is_err());
    project.undo().unwrap();
    assert_eq!(
        media_workflow::output(&project.snapshot())
            .unwrap()
            .0
            .document,
        id
    );
}
