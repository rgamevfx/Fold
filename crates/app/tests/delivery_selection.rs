use fold_app::{media_workflow, session::Session};
use fold_foundation::DocumentId;
use fold_motion::{Motion, authoring};
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};

#[test]
fn scene_request_is_independent_of_viewer_keys_and_retains_alpha() {
    let mut project = Project::new(8);
    let id = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(Motion::empty().document(id).unwrap())],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let mut request = media_workflow::SceneRequest {
        region: None,
        preview: None,
        source: DocumentRef {
            document: id,
            output: "video".into(),
            extensions: Default::default(),
        },
        time: fold_foundation::Time::ZERO,
        dimensions: [4, 4],
    };
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let frame = media_workflow::evaluate_scene(&snapshot, &request, &mut decoder, &cancel).unwrap();
    assert_eq!(frame.dimensions(), [4, 4]);
    assert!(frame.pixels().iter().all(|p| *p == [0.; 4]));
    assert!(
        frame.to_display().is_err(),
        "scene evaluation must not silently matte"
    );
    // Spatial preview requests are independent; the full delivery request
    // remains full-frame and alpha/timing survive extraction.
    let full_request = request.clone();
    request.region = Some(fold_render::region::Region {
        x: 1,
        y: 1,
        width: 2,
        height: 2,
    });
    let tile = media_workflow::evaluate_scene(&snapshot, &request, &mut decoder, &cancel).unwrap();
    assert_eq!(tile.dimensions(), [2, 2]);
    assert_eq!(tile.pixels(), &[[0.; 4]; 4]);
    assert_eq!(tile.descriptor().timing, frame.descriptor().timing);
    assert_eq!(
        media_workflow::evaluate_scene(&snapshot, &full_request, &mut decoder, &cancel)
            .unwrap()
            .dimensions(),
        [4, 4]
    );
    request.region = None;
    request.source.output = "missing".into();
    assert!(media_workflow::evaluate_scene(&snapshot, &request, &mut decoder, &cancel).is_err());
    request.source.output = "video".into();
    request.time = fold_foundation::Time::new(-1, 1).unwrap();
    assert!(media_workflow::evaluate_scene(&snapshot, &request, &mut decoder, &cancel).is_err());
}

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
