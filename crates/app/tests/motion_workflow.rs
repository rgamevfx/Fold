use fold_app::{media_workflow, packages};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::{Cancel, Decoder};
use fold_motion::{Motion, authoring as a, fields::Datum, graph::Link};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, PreviewKey},
    packages::CommandRequest,
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
fn reference(document: DocumentId) -> DocumentRef {
    DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    }
}
fn fixture() -> (Project, DocumentId, Motion, ObjectId) {
    let mut motion = Motion::empty();
    motion.info.width = 64;
    motion.info.height = 64;
    motion.info.frames = 24;
    let rect = a::add_content(&mut motion, "fold.motion.rectangle").unwrap();
    a::node(&mut motion, rect)
        .unwrap()
        .set("height", Datum::Scalar(12.));
    let control = a::add_node(&mut motion, "fold.motion.published_control").unwrap();
    a::node(&mut motion, control)
        .unwrap()
        .set("default", Datum::Scalar(12.));
    let settings = a::node(&mut motion, control)
        .unwrap()
        .settings::<fold_motion::nodes::interface::ControlSettings>()
        .unwrap();
    a::connect(
        &mut motion,
        rect,
        "width",
        Link {
            node: control,
            socket: "value".into(),
        },
    )
    .unwrap();
    let id = DocumentId::new();
    let mut project = Project::new(32);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(motion.document(id).unwrap())],
        })
        .unwrap();
    (project, id, motion, settings.id)
}
fn pixels(project: &Project, reference: &DocumentRef) -> Vec<[f32; 4]> {
    fold_render::render(
        packages::builtins()
            .video(&project.snapshot(), reference, Time::ZERO, 64, 64)
            .unwrap(),
    )
    .unwrap()
    .pixels()
    .to_vec()
}
#[test]
fn per_reference_controls_are_isolated_and_survive_renames() {
    let (mut project, id, mut motion, control) = fixture();
    let source = reference(id);
    let before = project.snapshot();
    let base = pixels(&project, &source);
    let mut altered = source.clone();
    altered.extensions.insert(
        "fold.controls".into(),
        serde_json::to_value(std::collections::BTreeMap::from([(
            control,
            Datum::Scalar(30.),
        )]))
        .unwrap(),
    );
    assert_ne!(base, pixels(&project, &altered));
    assert_eq!(base, pixels(&project, &source));
    assert_eq!(
        before.state().documents,
        project.snapshot().state().documents
    );
    motion
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.published_control")
        .unwrap()
        .settings["name"] = serde_json::json!("Renamed Width");
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(motion.document(id).unwrap())],
        })
        .unwrap();
    assert_ne!(base, pixels(&project, &altered));
    let mut invalid = source;
    invalid.extensions.insert(
        "fold.controls".into(),
        serde_json::to_value(std::collections::BTreeMap::from([(
            ObjectId::new(),
            Datum::Scalar(1.),
        )]))
        .unwrap(),
    );
    assert!(
        packages::builtins()
            .video(&project.snapshot(), &invalid, Time::ZERO, 64, 64)
            .is_err()
    );
}
#[test]
fn motion_nests_in_compositor_and_timeline_without_flattening() {
    use fold_compositor::{Composite, Node, Parameters, Source};
    use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
    let (mut project, id, motion, _) = fixture();
    let reference = reference(id);
    let info = motion.info.clone();
    let read = Node::new(
        Parameters::Read {
            source: Source::Document {
                source: reference.clone(),
                info: info.clone(),
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(1, 1).unwrap(),
        },
        vec![],
    );
    let output = Node::new(Parameters::Output, vec![read.id]);
    let composite_id = DocumentId::new();
    let composite = Composite {
        info: info.clone(),
        output: output.id,
        nodes: vec![read, output],
        extensions: Default::default(),
    };
    let nested_ref = DocumentRef {
        document: composite_id,
        ..reference.clone()
    };
    let track = Track::new(TrackKind::Video, "Motion title");
    let seq = Sequence {
        dimensions: [64, 64],
        rate: [24, 1],
        tracks: vec![track.clone()],
        clips: vec![Clip {
            id: ObjectId::new(),
            track: track.id,
            link: None,
            asset: Default::default(),
            info: SourceMedia::Document {
                source: nested_ref.clone(),
                info,
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(1, 1).unwrap(),
            level: 1.,
            extensions: Default::default(),
        }],
        extensions: Default::default(),
    };
    let sequence_id = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(composite.document(composite_id).unwrap()),
                Mutation::PutDocument(seq.document(sequence_id).unwrap()),
            ],
        })
        .unwrap();
    let original = pixels(&project, &reference);
    assert_eq!(original, pixels(&project, &nested_ref));
    assert_eq!(original[0], [0.; 4]);
    let mut key = PreviewKey {
        output: "video".into(),
        target: Some((sequence_id, Time::ZERO)),
        content: media_workflow::content_for(&project.snapshot(), sequence_id).unwrap(),
        frame: 0,
        dimensions: [64, 64],
        view: 1,
    };
    let frame = media_workflow::evaluate(
        &project.snapshot(),
        &key,
        &mut Decoder::default(),
        &Cancel::default(),
    )
    .unwrap();
    assert!(frame.pixels().iter().all(|p| p[3] == 1.));
    assert!(frame.pixels().iter().any(|p| p[0] > 0.));
    let dir = tempfile::tempdir().unwrap();
    // Delivery selection is explicit, persisted and independent of the viewer.
    assert_eq!(
        media_workflow::output(&project.snapshot())
            .unwrap()
            .0
            .document,
        sequence_id
    );
    project
        .commit(media_workflow::select_output(&project.snapshot(), id).unwrap())
        .unwrap();
    assert_eq!(
        media_workflow::output(&project.snapshot())
            .unwrap()
            .0
            .document,
        id
    );
    project.undo().unwrap();
    assert_eq!(
        media_workflow::output(&project.snapshot())
            .unwrap()
            .0
            .document,
        sequence_id
    );
    project.redo().unwrap();
    let encoded = dir.path().join("motion.mp4");
    media_workflow::export(&project.snapshot(), &encoded, 0, 1, &Cancel::default()).unwrap();
    assert_eq!(
        fold_media::inspect(&encoded, &Cancel::default())
            .unwrap()
            .info
            .frames,
        1
    );
    let path = dir.path().join("motion.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 32).unwrap();
    assert_eq!(
        media_workflow::output(&reopened.snapshot())
            .unwrap()
            .0
            .document,
        id
    );
    assert!(media_workflow::select_output(&reopened.snapshot(), DocumentId::new()).is_err());
    assert_eq!(original, pixels(&reopened, &reference));
    key.content = media_workflow::content_for(&reopened.snapshot(), sequence_id).unwrap();
    let reopened_frame = media_workflow::evaluate(
        &reopened.snapshot(),
        &key,
        &mut Decoder::default(),
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(frame.pixels(), reopened_frame.pixels());
}
#[test]
fn motion_preview_cancel_commit_and_undo_use_the_host_transaction_path() {
    let (project, id, mut motion, _) = fixture();
    let before = project.snapshot();
    let mut session = fold_app::session::Session::new(project);
    let rect = motion
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.rectangle")
        .unwrap();
    rect.set("height", Datum::Scalar(28.));
    let request = CommandRequest {
        id: fold_motion::package::EDIT.into(),
        base: before.revision(),
        arguments: serde_json::to_vec(&fold_motion::package::EditArgs {
            document: id,
            motion: motion.clone(),
        })
        .unwrap(),
    };
    session.command(DesktopCommand::PreviewExtension(request.clone()));
    assert!(session.state().transient);
    assert_eq!(
        session.snapshot().unwrap().state().documents,
        before.state().documents
    );
    session.command(DesktopCommand::CancelPreviewEdit);
    assert!(!session.state().transient);
    session.command(DesktopCommand::Extension(request));
    assert_eq!(
        Motion::from_document(&session.snapshot().unwrap().state().documents[&id]).unwrap(),
        motion
    );
    session.command(DesktopCommand::Undo);
    assert_eq!(
        session.snapshot().unwrap().state().documents,
        before.state().documents
    );
    session.command(DesktopCommand::Redo);
    assert_eq!(
        Motion::from_document(&session.snapshot().unwrap().state().documents[&id]).unwrap(),
        motion
    );
}
