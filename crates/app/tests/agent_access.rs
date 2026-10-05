use fold_app::{
    automation::{self, Batch},
    session::Session,
};
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_project::{Project, Revision};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn batch(revision: Revision) -> Batch {
    serde_json::from_value(json!({
    "request_id":"create","revision":revision,"project_epoch":0,"label":"Create title","ids":["motion","text"],
    "operations":[{"op":"create","id":{"$ref":"motion"},"kind":"fold.motion.document","name":"Agent title"},
    {"op":"edit","document":{"$ref":"motion"},"edits":[{"op":"object","id":{"$ref":"text"},"kind":"fold.motion.text","name":"Title"},
    {"op":"set","node":{"$ref":"text"},"input":"text","value":{"Text":"$literal artwork"}}]}]
})).unwrap()
}
#[test]
fn batched_creation_is_atomic_preserves_extensions_and_one_undo() {
    let mut project = Project::new(8);
    let before = project.snapshot();
    let request = batch(before.revision());
    let (proposal, result) = automation::stage(&before, &request, &Default::default()).unwrap();
    assert!(project.snapshot().state().documents.is_empty());
    project.commit_named(proposal, request.label).unwrap();
    assert_eq!(project.undo_label(), Some("Create title"));
    let document = project
        .snapshot()
        .state()
        .documents
        .values()
        .next()
        .unwrap()
        .as_ref()
        .clone();
    let motion = fold_motion::Motion::from_document(&document).unwrap();
    let text: fold_foundation::ObjectId =
        serde_json::from_value(result["ids"]["text"].clone()).unwrap();
    assert_eq!(
        motion.graph.node(text).unwrap().inputs["text"],
        fold_motion::graph::Input::Value(fold_motion::fields::Datum::Text(
            "$literal artwork".into()
        ))
    );
    let mut document = document;
    document
        .extensions
        .insert("future".into(), json!({"opaque":true}));
    let mut m = fold_motion::Motion::from_document(&document).unwrap();
    m.extensions
        .insert("future.payload".into(), json!([1, 2, 3]));
    document.payload = serde_json::to_vec(&m).unwrap();
    let registry = fold_app::packages::builtins();
    let changed = registry
        .author(&document, &[json!({"op":"info","value":{"frames":144}})])
        .unwrap();
    assert_eq!(changed.extensions["future"], json!({"opaque":true}));
    assert_eq!(
        fold_motion::Motion::from_document(&changed)
            .unwrap()
            .extensions["future.payload"],
        json!([1, 2, 3])
    );
    project.undo().unwrap();
    assert!(project.snapshot().state().documents.is_empty());
    project.redo().unwrap();
    assert_eq!(project.snapshot().state().documents.len(), 1);
    assert!(
        automation::stage(
            &project.snapshot(),
            &batch(Revision(0)),
            &Default::default()
        )
        .is_err()
    );
}
#[test]
fn invalid_second_operation_never_publishes_and_cancel_is_respected() {
    let project = Project::new(8);
    let mut request = batch(project.snapshot().revision());
    request.operations.push(json!({"op":"edit","document":{"$ref":"motion"},"edits":[{"op":"set","node":{"$ref":"text"},"input":"size","value":{"Color":[1,0,0,1]}}]}));
    assert!(automation::stage(&project.snapshot(), &request, &Default::default()).is_err());
    assert!(project.snapshot().state().documents.is_empty());
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(automation::stage(&project.snapshot(), &batch(Revision(0)), &cancel).is_err());
}
fn pump_call(session: &mut Session, id: &str, method: &str, params: Value) -> Value {
    let id = id.to_owned();
    let method = method.to_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        tx.send(automation::client::call(&id, &method, params))
            .unwrap();
    });
    let start = Instant::now();
    loop {
        session.poll();
        if let Ok(reply) = rx.try_recv() {
            return reply.unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn wait_job(session: &mut Session, id: &str, job: Value) -> Value {
    let start = Instant::now();
    loop {
        let reply = pump_call(session, id, "jobs", json!({"job":job}));
        if reply["result"]["status"] == "complete" {
            return reply["result"]["outcome"].clone();
        }
        assert!(start.elapsed() < Duration::from_secs(10));
    }
}
#[test]
#[cfg(unix)]
fn live_socket_retries_conflicts_undo_and_disconnect() {
    let mut session = Session::new(Project::new(8));
    session.set_agent_access(true);
    let start = Instant::now();
    let id = loop {
        session.poll();
        let access = session.agent_access();
        assert!(access["error"].is_null(), "{access}");
        if let Some(id) = access["session"].as_str() {
            break id.to_owned();
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    };
    let request = serde_json::to_value(batch(Revision(0))).unwrap();
    let first = pump_call(&mut session, &id, "batch", request.clone());
    assert_eq!(first["ok"], true, "{first}");
    let outcome = wait_job(&mut session, &id, first["result"]["job"].clone());
    assert_eq!(outcome["ok"], true, "{outcome}");
    let duplicate = pump_call(&mut session, &id, "batch", request.clone());
    assert_eq!(first, duplicate);
    assert_eq!(session.snapshot().unwrap().revision(), Revision(1));
    let mut wrong = request.clone();
    wrong["label"] = json!("different");
    assert_eq!(pump_call(&mut session, &id, "batch", wrong)["ok"], false);
    let stale = pump_call(
        &mut session,
        &id,
        "undo",
        json!({"request_id":"stale","revision":0,"project_epoch":0}),
    );
    assert_eq!(stale["ok"], false);
    let undo = pump_call(
        &mut session,
        &id,
        "undo",
        json!({"request_id":"undo","revision":1,"project_epoch":0}),
    );
    assert_eq!(undo["ok"], true);
    assert!(session.snapshot().unwrap().state().documents.is_empty());
    session.command(DesktopCommand::NewProject);
    let mut old = request;
    old["request_id"] = json!("old-project");
    assert_eq!(pump_call(&mut session, &id, "batch", old)["ok"], false);
    session.set_agent_access(false);
    assert!(automation::client::call(&id, "state", json!({})).is_err());
}
#[test]
fn mcp_negotiates_versions_lists_tools_and_reports_errors() {
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"missing\"}}\n"
    );
    let mut out = Vec::new();
    automation::mcp::serve(&mut input.as_bytes(), &mut out).unwrap();
    let responses: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-06-18");
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "fold_capture"));
    assert_eq!(responses[2]["error"]["code"], -32602);
    assert!(automation::wire::line(&mut &b"12345\n"[..], 5).is_err());
}
