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
        let reply = pump_call(session, id, "jobs", json!({"job":job,"wait_ms":1000}));
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

#[test]
fn graph_receipts_queries_and_revision_changes_share_transactions() {
    let mut project = Project::new(8);
    let before = project.snapshot();
    let request = batch(before.revision());
    let (edit, result) = automation::stage(&before, &request, &Default::default()).unwrap();
    let object = &result["receipts"][0]["changes"]["objects"][0];
    assert!(!object["transform"].is_null());
    assert!(!object["appearance"].is_null());
    project.commit(edit).unwrap();
    let created = project.snapshot();
    let document = result["ids"]["motion"].clone();
    let text = result["ids"]["text"].clone();
    let inspected = automation::inspect(
        &created,
        &json!({"document":document,"query":{"node":text}}),
    )
    .unwrap();
    assert_eq!(inspected["data"]["total"], 1);
    let req:Batch=serde_json::from_value(json!({"request_id":"edit-font-size","revision":created.revision(),"project_epoch":0,"label":"Size","operations":[{"op":"edit","document":document,"edits":[{"op":"set","node":text,"input":"size","value":{"Scalar":60}}]}]})).unwrap();
    let (edit, _) = automation::stage(&created, &req, &Default::default()).unwrap();
    project.commit(edit).unwrap();
    let changes = automation::inspect_changes(
        &project.retained_snapshot(created.revision()).unwrap(),
        &project.snapshot(),
        &json!({"document":document}),
    )
    .unwrap();
    assert_eq!(changes["changes"]["total_changed_nodes"], 1);
    project.undo().unwrap();
    let undo =
        automation::inspect_changes(&created, &project.snapshot(), &json!({"document":document}))
            .unwrap();
    assert_eq!(undo["changes"]["total_changed_nodes"], 0);
}

#[test]
fn rust_wrangle_batch_is_atomic_and_undoable() {
    let mut project = Project::new(8);
    let req:Batch=serde_json::from_value(json!({"request_id":"wrangle-create","revision":project.snapshot().revision(),"project_epoch":0,"label":"Rust Wrangle","ids":["motion","points","code"],"operations":[
        {"op":"create","id":{"$ref":"motion"},"kind":"fold.motion.document","name":"Wrangle"},
        {"op":"edit","document":{"$ref":"motion"},"edits":[
            {"op":"create_node","id":{"$ref":"points"},"kind":"fold.motion.linear_points"},
            {"op":"create_node","id":{"$ref":"code"},"kind":"fold.motion.wrangle","settings":{"source":"position[1] += chf(\"height\");"},"inputs":{"content":{"Link":{"node":{"$ref":"points"},"socket":"points"}}}}
        ]}
    ]})).unwrap();
    let (edit, result) = automation::stage(&project.snapshot(), &req, &Default::default()).unwrap();
    project.commit_named(edit, "Rust Wrangle".into()).unwrap();
    let before = project.snapshot();
    let bad:Batch=serde_json::from_value(json!({"request_id":"bad-code","revision":before.revision(),"project_epoch":0,"label":"Bad source","operations":[{"op":"edit","document":result["ids"]["motion"],"edits":[{"op":"wrangle","node":result["ids"]["code"],"source":"position[0] = missing_value;"}]}]})).unwrap();
    let error = automation::stage(&before, &bad, &Default::default()).unwrap_err();
    let error = automation::error(error);
    assert_eq!(error["kind"], "rust_compile");
    assert!(
        error["diagnostics"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    );
    assert_eq!(project.snapshot().revision(), before.revision());
    project.undo().unwrap();
    assert!(project.snapshot().state().documents.is_empty());
    project.redo().unwrap();
    assert_eq!(project.snapshot().state().documents.len(), 1);
}

#[test]
#[cfg(unix)]
fn candidates_observe_privately_reject_stale_generations_and_commit_once() {
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
    let begin = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"begin","request_id":"begin","revision":0,"project_epoch":0}),
    );
    assert_eq!(begin["ok"], true, "{begin}");
    let candidate = begin["result"]["candidate"].clone();
    let edit = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"edit","request_id":"private-edit","candidate":candidate,"generation":0,"project_epoch":0,"batch":batch(Revision(0))}),
    );
    assert_eq!(edit["ok"], true, "{edit}");
    let result = wait_job(&mut session, &id, edit["result"]["job"].clone());
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["result"]["generation"], 1);
    assert!(session.snapshot().unwrap().state().documents.is_empty());
    let document = result["result"]["ids"]["motion"].clone();
    let inspect = pump_call(
        &mut session,
        &id,
        "inspect",
        json!({"document":document,"candidate":candidate,"generation":0,"project_epoch":0}),
    );
    assert_eq!(inspect["ok"], false);
    let inspect = pump_call(
        &mut session,
        &id,
        "inspect",
        json!({"document":document,"candidate":candidate,"generation":1,"project_epoch":0}),
    );
    let inspect = wait_job(&mut session, &id, inspect["result"]["job"].clone());
    assert_eq!(inspect["ok"], true, "{inspect}");
    assert_eq!(inspect["result"]["outputs"][0]["source"]["output"], "video");
    assert_eq!(inspect["result"]["outputs"][0]["available"], true);
    let observed = pump_call(
        &mut session,
        &id,
        "observe",
        json!({"source":{"document":document,"output":"video"},"times":[{"numerator":0,"denominator":1},{"numerator":1,"denominator":2}],"dimensions":[96,54],"candidate":candidate,"generation":1,"project_epoch":0,"queries":[{"mode":"evaluated","fields":["counts"],"limit":1}]}),
    );
    assert_eq!(observed["ok"], true, "{observed}");
    let observed = wait_job(&mut session, &id, observed["result"]["job"].clone());
    assert_eq!(observed["ok"], true, "{observed}");
    let metadata = &observed["result"]["metadata"];
    assert_eq!(metadata["generation"], 1);
    assert_eq!(metadata["frames"].as_array().unwrap().len(), 2);
    assert_eq!(
        metadata["frames"][0]["measurements"][0]["status"],
        "evaluated"
    );
    assert!(
        metadata["frames"][0]["measurements"][0]["data"]["counts"]["glyphs"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(session.snapshot().unwrap().state().documents.is_empty());
    // A failed later edit preserves the complete prior candidate generation.
    let mut bad = batch(Revision(1));
    bad.operations = vec![json!({"op":"remove","document":"not-a-uuid"})];
    let failed = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"edit","request_id":"failed-edit","candidate":candidate,"generation":1,"project_epoch":0,"batch":bad}),
    );
    let failed = wait_job(&mut session, &id, failed["result"]["job"].clone());
    assert_eq!(failed["ok"], false);
    let commit_args = json!({"action":"commit","request_id":"commit","candidate":candidate,"generation":1,"revision":0,"project_epoch":0,"label":"Observed title"});
    let committed = pump_call(&mut session, &id, "candidate", commit_args.clone());
    assert_eq!(committed["ok"], true, "{committed}");
    assert_eq!(
        committed,
        pump_call(&mut session, &id, "candidate", commit_args)
    );
    assert_eq!(session.snapshot().unwrap().revision(), Revision(1));
    let undo = pump_call(
        &mut session,
        &id,
        "undo",
        json!({"request_id":"undo-candidate","revision":1,"project_epoch":0}),
    );
    assert_eq!(undo["ok"], true, "{undo}");
    assert!(session.snapshot().unwrap().state().documents.is_empty());
    session.set_agent_access(false);
}

#[test]
fn discovery_filters_and_request_validation_are_public_contracts() {
    let result = automation::catalog(&json!({"query":"oscillator"})).unwrap();
    let catalogs = result["catalogs"].as_array().unwrap();
    let motion = catalogs
        .iter()
        .find(|c| c["type"] == "fold.motion.document")
        .unwrap();
    assert_eq!(motion["catalog"]["nodes"].as_array().unwrap().len(), 1);
    assert!(automation::catalog(&json!({"typo":true})).is_err());
    assert!(automation::catalog(&json!({"nodes":"no"})).is_err());
    assert!(automation::catalog(&json!({"method":"render","query":"ignored"})).is_err());
    let tools = automation::mcp::tools();
    for name in ["fold_candidate", "fold_observe"] {
        assert!(tools.as_array().unwrap().iter().any(|t| t["name"] == name));
    }
    let batch_schema = automation::catalog(&json!({"method":"batch"})).unwrap();
    assert!(
        !batch_schema.to_string().contains("#/$defs/"),
        "embedded operation schema must resolve its local references"
    );
    let candidate_schema = automation::catalog(&json!({"method":"candidate"})).unwrap();
    assert_eq!(
        candidate_schema["inputSchema"]["properties"]["batch"],
        batch_schema["inputSchema"]
    );
    let request = automation::Request {
        version: 1,
        method: "observe".into(),
        params: json!({"source":{"document":"x","output":"video"},"times":[],"dimensions":[0,20]}),
    };
    assert!(automation::validate_request(&request).is_err());
}

#[test]
fn binding_patch_replaces_variant_and_query_typos_fail() {
    let mut project = Project::new(8);
    let (proposal, result) = automation::stage(
        &project.snapshot(),
        &batch(Revision(0)),
        &Default::default(),
    )
    .unwrap();
    project.commit(proposal).unwrap();
    let snapshot = project.snapshot();
    let document_id: fold_foundation::DocumentId =
        serde_json::from_value(result["ids"]["motion"].clone()).unwrap();
    let document = &snapshot.state().documents[&document_id];
    let node = result["ids"]["text"].clone();
    let source = fold_foundation::ObjectId::new();
    let registry = fold_app::packages::builtins();
    let changed=registry.author(document,&[
        json!({"op":"create_node","id":source,"kind":"fold.motion.value","settings":{"value_type":"Scalar"}}),
        json!({"op":"node","node":node,"value":{"inputs":{"size":{"Link":{"node":source,"socket":"value"}}}}}),
    ]).unwrap();
    let changed = registry
        .author(
            &changed,
            &[json!({"op":"node","node":node,"value":{"inputs":{"size":{"Value":{"Scalar":72}}}}})],
        )
        .unwrap();
    let motion = fold_motion::Motion::from_document(&changed).unwrap();
    let node = serde_json::from_value(node).unwrap();
    assert_eq!(
        motion.graph.node(node).unwrap().inputs["size"],
        fold_motion::graph::Input::Value(fold_motion::fields::Datum::Scalar(72.))
    );
    assert!(
        registry
            .query(&changed, &json!({"mode":"nodes","nod":"typo"}))
            .is_err()
    );
    assert!(
        registry
            .query(&changed, &json!({"mode":"nodes","limit":0}))
            .is_err()
    );
    assert!(
        registry
            .query(
                &changed,
                &json!({"mode":"signature","kind":"fold.motion.math","node":node})
            )
            .is_err()
    );
}

#[test]
#[cfg(unix)]
fn native_code_authoring_is_atomic_retry_safe_and_preserves_untargeted_properties() {
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
    let private = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"begin","request_id":"begin-code","revision":0,"project_epoch":0}),
    );
    let candidate = private["result"]["candidate"].clone();
    let args = json!({"request_id":"code","candidate":candidate,"generation":0,"project_epoch":0,"label":"SDK title","source":"const d=plan.document('title'); d.text('text','Hello',{size:72,position:[25,40]});"});
    let accepted = pump_call(&mut session, &id, "author", args.clone());
    assert_eq!(accepted["ok"], true, "{accepted}");
    let result = wait_job(&mut session, &id, accepted["result"]["job"].clone());
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(accepted, pump_call(&mut session, &id, "author", args));
    assert!(session.snapshot().unwrap().state().documents.is_empty());
    let handles = result["result"]["ids"].clone();
    let document = handles["title"].clone();
    let failed = pump_call(
        &mut session,
        &id,
        "author",
        json!({"request_id":"throw","candidate":candidate,"generation":1,"project_epoch":0,"label":"Fail","source":"plan.document('discarded'); throw Error('abort');"}),
    );
    assert_eq!(
        wait_job(&mut session, &id, failed["result"]["job"].clone())["ok"],
        false
    );
    let update = pump_call(
        &mut session,
        &id,
        "author",
        json!({"request_id":"update","candidate":candidate,"generation":1,"project_epoch":0,"label":"Continue","handles":handles,"source":"plan.document('title').text('text','ignored default').set('text','Updated');"}),
    );
    let updated = wait_job(&mut session, &id, update["result"]["job"].clone());
    assert_eq!(updated["ok"], true, "{updated}");
    assert_eq!(updated["result"]["generation"], 2);
    let committed = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"commit","request_id":"commit-code","candidate":candidate,"generation":2,"project_epoch":0,"revision":0,"label":"SDK title"}),
    );
    assert_eq!(committed["ok"], true, "{committed}");
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.state().documents.len(), 1);
    let doc = serde_json::from_value(document).unwrap();
    let motion = fold_motion::Motion::from_document(&snapshot.state().documents[&doc]).unwrap();
    let node = motion
        .graph
        .node(serde_json::from_value(handles["title/text"].clone()).unwrap())
        .unwrap();
    assert_eq!(
        node.inputs["size"],
        fold_motion::graph::Input::Value(fold_motion::fields::Datum::Scalar(72.))
    );
    assert_eq!(
        node.inputs["text"],
        fold_motion::graph::Input::Value(fold_motion::fields::Datum::Text("Updated".into()))
    );
    session.set_agent_access(false);
}

#[test]
fn candidate_preview_is_separate_conflict_checked_and_undoable() {
    let mut session = Session::new(Project::new(16));
    session.set_agent_access(true);
    let start = Instant::now();
    let id = loop {
        session.poll();
        let access = session.agent_access();
        assert!(access["error"].is_null(), "{access}");
        if let Some(id) = access["session"].as_str() {
            break id.to_owned();
        };
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    };
    let begin = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"begin","request_id":"preview-begin","revision":0,"project_epoch":0}),
    );
    let candidate = begin["result"]["candidate"].clone();
    let accepted = pump_call(
        &mut session,
        &id,
        "author",
        json!({"request_id":"preview-author","candidate":candidate,"generation":0,"project_epoch":0,"namespace":"preview","label":"Title","source":"plan.document('title').text('label','Preview');"}),
    );
    let result = wait_job(&mut session, &id, accepted["result"]["job"].clone());
    assert_eq!(result["ok"], true, "{result}");
    let source = json!({"document":result["result"]["handles"]["title"],"output":"video"});
    let show = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"preview","request_id":"show-preview","candidate":candidate,"generation":1,"project_epoch":0,"source":source,"time":{"numerator":0,"denominator":1}}),
    );
    assert_eq!(show["ok"], true, "{show}");
    let navigation = session.state().navigation.clone();
    assert!(session.snapshot().unwrap().state().documents.is_empty());
    let preview = session.agent_preview().unwrap();
    assert_eq!(preview.generation, 1);
    assert!(!preview.can_compare);
    assert!(!preview.conflict);
    session.agent_preview_action("frame", Some(10)).unwrap();
    assert_eq!(session.agent_preview().unwrap().key.frame, 10);
    assert_eq!(session.state().navigation, navigation);
    session.agent_preview_action("apply", None).unwrap();
    assert!(session.agent_preview().is_none());
    assert_eq!(session.snapshot().unwrap().state().documents.len(), 1);
    let state = pump_call(&mut session, &id, "state", json!({}))["result"].clone();
    let begin = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"begin","request_id":"begin-stale","revision":state["revision"],"project_epoch":0}),
    );
    let candidate = begin["result"]["candidate"].clone();
    let show = pump_call(
        &mut session,
        &id,
        "candidate",
        json!({"action":"preview","request_id":"show-stale","candidate":candidate,"generation":0,"project_epoch":0,"source":source,"time":{"numerator":0,"denominator":1}}),
    );
    assert_eq!(show["ok"], true, "{show}");
    assert!(session.agent_preview().unwrap().can_compare);
    session.agent_preview_action("compare", None).unwrap();
    assert!(session.agent_preview().unwrap().original);
    let undo = pump_call(
        &mut session,
        &id,
        "undo",
        json!({"request_id":"undo-preview","revision":state["revision"],"project_epoch":0}),
    );
    assert_eq!(undo["ok"], true, "{undo}");
    assert!(session.snapshot().unwrap().state().documents.is_empty());
    assert!(session.agent_preview().unwrap().conflict);
    assert!(
        !session.agent_preview().unwrap().original,
        "deleting the live source must leave the candidate visible"
    );
    assert!(session.agent_preview_action("apply", None).is_err());
    session.agent_preview_action("discard", None).unwrap();
    assert!(session.agent_preview().is_none());
    assert!(session.snapshot().unwrap().state().documents.is_empty());
    session.set_agent_access(false);
}

#[test]
fn portable_asset_public_roundtrip_is_idempotent_and_one_undo() {
    let mut m = fold_motion::Motion::empty();
    let definition = fold_foundation::ObjectId::new();
    fold_motion::program::define(
        &mut m,
        definition,
        "let mut a=Asset::new(\"Portable\");let t=a.text(\"text\",\"Hello\",50.);a.output(&t);a",
        &Default::default(),
    )
    .unwrap();
    let document = fold_foundation::DocumentId::new();
    let composition = fold_foundation::DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                fold_project::Mutation::PutDocument(m.document(document).unwrap()),
                fold_project::Mutation::PutDocument(
                    fold_composition::Composition::default()
                        .document(composition)
                        .unwrap(),
                ),
            ],
        })
        .unwrap();
    let mut session = Session::new(project);
    session.set_agent_access(true);
    let start = Instant::now();
    let id = loop {
        session.poll();
        let a = session.agent_access();
        if let Some(id) = a["session"].as_str() {
            break id.to_owned();
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("portable.foldasset");
    let export = json!({"action":"export","request_id":"export-asset","revision":1,"project_epoch":0,"document":document,"definition":definition,"path":path});
    let job = pump_call(&mut session, &id, "asset", export.clone());
    assert_eq!(job["ok"], true, "{job}");
    let result = wait_job(&mut session, &id, job["result"]["job"].clone());
    assert_eq!(result["ok"], true, "{result}");
    assert!(path.exists());
    assert_eq!(pump_call(&mut session, &id, "asset", export), job);
    let before = session.snapshot().unwrap();
    let graph = fold_foundation::DocumentId::new();
    let job = pump_call(
        &mut session,
        &id,
        "asset",
        json!({"action":"import","request_id":"import-asset","revision":1,"project_epoch":0,"composition":composition,"graph":graph,"instance":fold_foundation::ObjectId::new(),"layer":fold_foundation::ObjectId::new(),"path":path}),
    );
    assert_eq!(job["ok"], true, "{job}");
    let result = wait_job(&mut session, &id, job["result"]["job"].clone());
    assert_eq!(result["ok"], true, "{result}");
    let imported =
        fold_motion::Motion::from_document(&session.snapshot().unwrap().state().documents[&graph])
            .unwrap();
    assert_eq!(imported.groups, m.groups);
    assert_eq!(imported.fonts, m.fonts);
    let undo = pump_call(
        &mut session,
        &id,
        "undo",
        json!({"request_id":"undo-import","revision":2,"project_epoch":0}),
    );
    assert_eq!(undo["ok"], true, "{undo}");
    assert_eq!(
        session.snapshot().unwrap().state().documents,
        before.state().documents
    );
    session.set_agent_access(false);
}
