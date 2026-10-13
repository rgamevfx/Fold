use fold_app::session::Session;
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion_canvas::{
    model::{Canvas, Content, Contour, Object, Point},
    tools::{self, Part},
};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand as C},
    packages::CommandRequest,
};
use fold_project::{EditBatch, Mutation, Project};

#[test]
fn released_motion_preview_commits_the_displayed_curve_once() {
    let object = ObjectId::new();
    let point = ObjectId::new();
    let document = DocumentId::new();
    let canvas = Canvas {
        roots: vec![object],
        objects: [(
            object,
            Object::new(
                "Curve",
                Content::Path {
                    contours: vec![Contour {
                        id: ObjectId::new(),
                        closed: false,
                        points: vec![Point {
                            id: point,
                            position: [360., 540.],
                            incoming: [0.; 2],
                            outgoing: [220., -320.],
                            position_track: None,
                            incoming_track: None,
                            outgoing_track: None,
                            extensions: Default::default(),
                        }],
                        extensions: Default::default(),
                    }],
                    extensions: Default::default(),
                },
            ),
        )]
        .into(),
        ..Default::default()
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(canvas.document(document).unwrap())],
        })
        .unwrap();
    let mut host = Session::new(project);
    let original = host.snapshot().unwrap();
    let mut last = None;
    for position in [[400., 600.], [480., 800.]] {
        let edit = tools::move_point(
            &canvas,
            object,
            point,
            Part::Anchor,
            position,
            Time::ZERO,
            false,
            false,
        )
        .unwrap();
        let request = CommandRequest {
            id: fold_motion_context::package::EDIT.into(),
            base: original.revision(),
            arguments: serde_json::to_vec(&fold_motion_context::package::EditArgs {
                document,
                edits: vec![edit],
            })
            .unwrap(),
        };
        host.command(C::PreviewExtension(request.clone()));
        assert!(host.state().transient);
        last = Some(request);
    }
    let preview_content = host.state().content.clone();
    let request = last.unwrap();
    host.command(C::Extension(request.clone()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while host.extension_pending(&request) {
        host.poll();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let after = host.snapshot().unwrap();
    assert_ne!(
        after.revision(),
        original.revision(),
        "release discarded the drag: {}",
        host.state().status
    );
    assert!(!host.state().transient);
    assert_eq!(host.state().content, preview_content);
    let result = Canvas::from_document(&after.state().documents[&document]).unwrap();
    let Content::Path { contours, .. } = &result.objects[&object].content else {
        panic!()
    };
    assert_eq!(contours[0].points[0].position, [480., 800.]);
    host.command(C::Undo);
    assert_eq!(
        host.snapshot().unwrap().state().documents[&document].payload,
        original.state().documents[&document].payload
    );
    host.command(C::Redo);
    assert_eq!(
        host.snapshot().unwrap().state().documents[&document].payload,
        after.state().documents[&document].payload
    );
}

#[test]
#[ignore = "manual CPU profile using FOLD_DRAG_PROJECT"]
fn profile_motion_preview_submission() {
    let project = fold_project::load(std::env::var("FOLD_DRAG_PROJECT").unwrap(), 8).unwrap();
    let mut host = Session::new(project);
    let snapshot = host.snapshot().unwrap();
    let doc = snapshot
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_motion_canvas::TYPE)
        .unwrap();
    let canvas = Canvas::from_document(doc).unwrap();
    let (&object, path) = canvas
        .objects
        .iter()
        .find(|(_, o)| matches!(o.content, Content::Path { .. }))
        .unwrap();
    let Content::Path { contours, .. } = &path.content else {
        unreachable!()
    };
    let point = &contours[0].points[0];
    let mut times = Vec::new();
    for i in 0..120 {
        let edit = tools::move_point(
            &canvas,
            object,
            point.id,
            Part::Anchor,
            [point.position[0] + f64::from(i), point.position[1]],
            Time::ZERO,
            false,
            false,
        )
        .unwrap();
        let request = CommandRequest {
            id: fold_motion_context::package::EDIT.into(),
            base: snapshot.revision(),
            arguments: serde_json::to_vec(&fold_motion_context::package::EditArgs {
                document: doc.id,
                edits: vec![edit],
            })
            .unwrap(),
        };
        let start = std::time::Instant::now();
        host.command(C::PreviewExtension(request));
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "120 preview submissions: median {:.3} ms; p95 {:.3} ms; max {:.3} ms",
        times[60], times[114], times[119]
    );
}
