use fold_app::session::Session;
use fold_foundation::{DocumentId, ObjectId};
use fold_motion::model::Construction;
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_project::{EditBatch, Mutation, Project};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn setup() -> (Session, DocumentId) {
    let id = DocumentId::new();
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                fold_motion::package::create(id).unwrap(),
            )],
        })
        .unwrap();
    (Session::new(project), id)
}
fn raw(session: &mut Session, command: Value) -> Value {
    serde_json::from_slice(
        &session
            .dispatch_agent(&serde_json::to_vec(&json!({"version":2,"command":command})).unwrap()),
    )
    .unwrap()
}
fn call(session: &mut Session, command: Value) -> Value {
    let mut result = raw(session, command);
    let handle = result["result"]["job"].clone();
    let deadline = Instant::now() + Duration::from_secs(10);
    while result["result"]["state"] == "running" {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
        result = raw(session, json!({"method":"job_status","handle":handle}));
    }
    result
}
fn guard(session: &mut Session, id: DocumentId) -> Value {
    let info = raw(session, json!({"method":"discover"}));
    json!({"epoch":info["epoch"],"request_id":ObjectId::new(),"base":session.snapshot().unwrap().revision().0,"scope":{"document":id,"definition":null}})
}
fn proposal(session: &mut Session, id: DocumentId) -> Value {
    let guard = guard(session, id);
    let result = call(
        session,
        json!({"method":"propose","guard":guard,"edit":{"kind":"patches","patches":[{"path":[{"kind":"field","value":"seed"}],"expected":{"kind":"present","value":0},"value":{"kind":"present","value":42}}]}}),
    );
    assert!(result.get("error").is_none(), "{result}");
    result["result"]["handle"].clone()
}

#[test]
fn owner_applies_refreshes_undoes_and_preserves_active_artist_edits() {
    let (mut session, id) = setup();
    let handle = proposal(&mut session, id);
    let inspection = call(
        &mut session,
        json!({"method":"inspect_candidate","handle":handle}),
    );
    let snapshot = session.snapshot().unwrap();
    let mut artist = Construction::from_document(&snapshot.state().documents[&id]).unwrap();
    artist.seed = 12;
    session.command(DesktopCommand::PreviewExtension(
        fold_platform::packages::CommandRequest {
            id: fold_motion::package::EDIT.into(),
            base: snapshot.revision(),
            arguments: serde_json::to_vec(&fold_motion::package::EditArgs {
                document: id,
                construction: artist,
            })
            .unwrap(),
        },
    ));
    assert!(session.state().transient);
    let request_guard = guard(&mut session, id);
    let apply = json!({"method":"apply","guard":request_guard,"handle":handle,"receipt":inspection["result"]["receipt"]});
    assert_eq!(
        raw(&mut session, apply.clone())["error"]["code"],
        "conflict"
    );
    assert!(session.state().transient);
    assert_eq!(session.snapshot().unwrap().revision(), snapshot.revision());
    session.command(DesktopCommand::CancelPreviewEdit);
    // A retry preserves the original conflict even after the artist finishes.
    assert_eq!(
        raw(&mut session, apply.clone())["error"]["code"],
        "conflict"
    );
    let mut fresh = apply;
    fresh["guard"] = guard(&mut session, id);
    let result = raw(&mut session, fresh.clone());
    assert!(result.get("error").is_none(), "{result}");
    assert!(session.state().dirty && session.state().can_undo);
    assert_eq!(
        Construction::from_document(&session.snapshot().unwrap().state().documents[&id])
            .unwrap()
            .seed,
        42
    );
    session.command(DesktopCommand::Undo);
    assert_eq!(
        Construction::from_document(&session.snapshot().unwrap().state().documents[&id])
            .unwrap()
            .seed,
        0
    );
    assert_eq!(raw(&mut session, fresh), result);
    assert_eq!(
        Construction::from_document(&session.snapshot().unwrap().state().documents[&id])
            .unwrap()
            .seed,
        0
    );
}

#[test]
fn new_and_open_replace_the_protocol_epoch_and_retire_old_handles() {
    let (mut session, id) = setup();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("native.fold");
    fold_project::save(&session.snapshot().unwrap(), &path).unwrap();
    let old_handle = proposal(&mut session, id);
    let before = raw(&mut session, json!({"method":"discover"}))["epoch"].clone();
    let pending = raw(
        &mut session,
        json!({"method":"inspect_candidate","handle":old_handle}),
    );
    assert_eq!(pending["result"]["state"], "running");
    session.command(DesktopCommand::NewProject);
    let after = raw(&mut session, json!({"method":"discover"}))["epoch"].clone();
    assert_ne!(before, after);
    assert_eq!(
        raw(
            &mut session,
            json!({"method":"job_status","handle":pending["result"]["job"]})
        )["error"]["code"],
        "conflict"
    );
    assert_eq!(
        call(
            &mut session,
            json!({"method":"inspect_candidate","handle":old_handle})
        )["error"]["code"],
        "conflict"
    );
    session.command(DesktopCommand::Open(path));
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.state().busy {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        session
            .snapshot()
            .unwrap()
            .state()
            .documents
            .contains_key(&id)
    );
    assert_ne!(
        after,
        raw(&mut session, json!({"method":"discover"}))["epoch"]
    );
}
