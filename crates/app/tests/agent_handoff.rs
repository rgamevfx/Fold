//! Cross-transport native editing and persistence handoff. Rendering and physical
//! input acceptance remain separate from this owner/service integration test.
#![cfg(target_os = "linux")]
use fold_app::session::Session;
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{binding::Binding, model::Construction, types::Literal};
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_project::{Document, EditBatch, Mutation, Project};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
enum Transport {
    Cli,
    Mcp,
}
fn request(session: &mut Session, path: &Path, transport: Transport, command: Value) -> Value {
    request_wire(
        session,
        path,
        transport,
        json!({"version":2,"command":command}),
    )
}
fn request_wire(session: &mut Session, path: &Path, transport: Transport, request: Value) -> Value {
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut child = Command::new(env!("CARGO_BIN_EXE_fold-cli"))
                .arg(match transport { Transport::Cli => "protocol", Transport::Mcp => "mcp" })
                .arg(path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
                .spawn().unwrap();
            let mut input = child.stdin.take().unwrap();
            match transport {
                Transport::Cli => write!(input, "{request}").unwrap(),
                Transport::Mcp => {
                    for envelope in [
                        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"handoff","version":"1"}}}),
                        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"fold_request","arguments":request}}),
                    ] { writeln!(input, "{envelope}").unwrap(); }
                }
            }
            drop(input);
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            match transport {
                Transport::Cli => serde_json::from_slice(&output.stdout).unwrap(),
                Transport::Mcp => {
                    let responses: Vec<Value> = String::from_utf8(output.stdout).unwrap().lines()
                        .map(|line| serde_json::from_str(line).unwrap()).collect();
                    assert_eq!(responses.len(), 2);
                    assert_eq!(responses[1]["id"], 2);
                    assert!(responses[1].get("error").is_none(), "{:?}", responses[1]);
                    responses[1]["result"]["structuredContent"].clone()
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        while !worker.is_finished() {
            assert!(Instant::now() < deadline, "public transport timed out");
            session.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
        worker.join().unwrap()
    })
}
fn terminal(session: &mut Session, path: &Path, transport: Transport, command: Value) -> Value {
    let mut response = request(session, path, transport, command);
    let handle = response["result"]["job"].clone();
    let deadline = Instant::now() + Duration::from_secs(30);
    while response["result"]["state"] == "running" {
        assert!(Instant::now() < deadline, "public job timed out");
        response = request(
            session,
            path,
            transport,
            json!({"method":"job_status","handle":handle,"wait_ms":1000}),
        );
    }
    response
}
fn call(session: &mut Session, path: &Path, transport: Transport, command: Value) -> Value {
    let response = terminal(session, path, transport, command);
    assert!(response.get("error").is_none(), "{response}");
    response
}
fn capture_rejection(
    session: &mut Session,
    path: &Path,
    transport: Transport,
    document: DocumentId,
    command: Value,
    case: (&str, &str),
    records: &mut serde_json::Map<String, Value>,
) -> Value {
    let before = construction(session, document);
    let revision = session.snapshot().unwrap().revision();
    let response = terminal(session, path, transport, command);
    assert_eq!(response["error"]["code"], case.1, "{response}");
    let after = construction(session, document);
    assert_eq!(before, after);
    assert_eq!(session.snapshot().unwrap().revision(), revision);
    records.insert(
        case.0.into(),
        json!({"code":response["error"]["code"],"before":before,"after":after}),
    );
    response
}
fn construction(session: &Session, id: DocumentId) -> Construction {
    Construction::from_document(&session.snapshot().unwrap().state().documents[&id]).unwrap()
}
fn artist_edit(session: &mut Session, id: DocumentId, value: Construction) {
    session.command(DesktopCommand::Extension(
        fold_platform::packages::CommandRequest {
            id: fold_motion::package::EDIT.into(),
            base: session.snapshot().unwrap().revision(),
            arguments: serde_json::to_vec(&fold_motion::package::EditArgs {
                document: id,
                construction: value.clone(),
            })
            .unwrap(),
        },
    ));
    assert_eq!(construction(session, id), value);
}
fn finish_file_work(session: &mut Session) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.state().busy || session.state().file_busy {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        session.state().project_error.is_none(),
        "{:?}",
        session.state().project_error
    );
}
fn fixture() -> Document {
    let id = DocumentId::new();
    let mut value = fold_motion::source::parse(include_str!(
        "../../../docs/rewrite/fixtures/animated-shape.foldsrc"
    ))
    .unwrap()
    .construction;
    // The artist locks Count after the preceding native edits in this handoff.
    let count = value.definitions[0]
        .inputs
        .iter_mut()
        .find(|input| input.name == "count")
        .unwrap();
    count.protected = false;
    value.root.overrides.insert(
        count.id,
        fold_motion::model::Parameter {
            id: ObjectId::new(),
            protected: false,
            extensions: [("vendor_override".into(), json!({"opaque":[1,2,3]}))].into(),
            value: serde_json::to_value(Binding::Literal {
                value: Literal::Integer { value: 1 },
            })
            .unwrap(),
        },
    );
    value
        .extensions
        .insert("vendor_document".into(), json!({"opaque":{"version":99}}));
    value.document(id).unwrap()
}

#[test]
fn cli_and_mcp_preserve_native_handoff_through_undo_and_reopen() {
    let document = fixture();
    let id = document.id;
    let mut outcomes = vec![];
    for transport in [Transport::Cli, Transport::Mcp] {
        let mut errors = serde_json::Map::new();
        let mut project = Project::new(32);
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(document.clone())],
            })
            .unwrap();
        let mut session = Session::new(project);
        let directory = tempfile::tempdir().unwrap();
        let path = session.enable_agent_transport(directory.path()).unwrap();
        // Observe the frozen shape through each real transport before artist edits.
        let frozen = construction(&session, id);
        let revision = session.snapshot().unwrap().revision();
        let legacy = request_wire(
            &mut session,
            &path,
            transport,
            json!({"version":1,"command":{"method":"discover"}}),
        );
        assert_eq!(legacy["error"]["code"], "version", "{legacy}");
        let after = construction(&session, id);
        assert_eq!(after, frozen);
        assert_eq!(session.snapshot().unwrap().revision(), revision);
        errors.insert(
            "old_protocol".into(),
            json!({"code":legacy["error"]["code"],"before":frozen,"after":after}),
        );
        let initial = call(&mut session, &path, transport, json!({"method":"discover"}));
        let specification: Value = serde_json::from_str(include_str!(
            "../../../docs/rewrite/fixtures/animated-shape.json"
        ))
        .unwrap();
        let times: Vec<Time> = specification["times"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| Time::new(t[0].as_i64().unwrap(), t[1].as_u64().unwrap() as u32).unwrap())
            .collect();
        let mut seek_runs = vec![];
        let mut identities = None;
        for order in specification["orders"].as_array().unwrap() {
            let requested: Vec<Time> = order
                .as_array()
                .unwrap()
                .iter()
                .map(|index| times[index.as_u64().unwrap() as usize])
                .collect();
            let observation = call(
                &mut session,
                &path,
                transport,
                json!({"method":"observe",
                "target":{"kind":"committed","epoch":initial["epoch"],"revision":initial["result"]["revision"],"document":id},
                "output":frozen.definitions[0].outputs[0].id,"times":requested}),
            );
            let samples = observation["result"]["samples"].as_array().unwrap();
            assert_eq!(samples.len(), requested.len());
            for (sample, time) in samples.iter().zip(requested) {
                assert_eq!(sample["time"], json!(time));
                let entry = &sample["value"]["entries"][0];
                let placement = &entry["placement"];
                let seconds = time.numerator() as f64 / f64::from(time.denominator());
                let y = 80. * (1. - (seconds / 0.6).clamp(0., 1.));
                let opacity = if seconds <= 0.5 {
                    (seconds * 2.).max(0.)
                } else {
                    (1.5 - seconds).max(0.5)
                };
                assert!((placement["position"][1].as_f64().unwrap() - y).abs() <= 1e-9);
                assert!((placement["opacity"].as_f64().unwrap() - opacity).abs() <= 1e-9);
                let geometry = sample["geometries"]
                    .as_object()
                    .unwrap()
                    .values()
                    .next()
                    .unwrap();
                assert_eq!(geometry["bounds"], json!([[0., 0.], [200., 100.]]));
                let ids = json!([
                    entry["id"],
                    placement["id"],
                    geometry["id"],
                    geometry["contours"][0]["point_ids"]
                ]);
                if let Some(before) = &identities {
                    assert_eq!(&ids, before);
                } else {
                    identities = Some(ids);
                }
            }
            seek_runs.push(observation["result"]["samples"].clone());
        }
        assert_eq!(construction(&session, id), frozen);
        // Source and graph edits go through the same provider used by native panels.
        let mut value = construction(&session, id);
        let node = value.definitions[0].nodes[0].id;
        value.definitions[0].nodes[0].name = "Artist card".into();
        let snapshot = session.snapshot().unwrap();
        let candidate = fold_motion::editing::SourceBuffer::default()
            .propose(
                fold_motion::source::canonical(&value).unwrap(),
                &snapshot,
                id,
                fold_motion::editing::Scope::Document,
            )
            .unwrap();
        artist_edit(
            &mut session,
            id,
            Construction::from_document(candidate.document()).unwrap(),
        );
        value = construction(&session, id);
        value.presentation.positions.insert(node, [320., 180.]);
        value
            .presentation
            .annotations
            .insert(node, "Keep this card aligned".into());
        artist_edit(&mut session, id, value);
        let mut channels = fold_motion::keyed::channels(&construction(&session, id), None).unwrap();
        channels[0].curve.keys[1].time = Time::new(3, 5).unwrap();
        channels[0].curve.keys[1].value = 0.8;
        value = fold_motion::keyed::replace(&construction(&session, id), &channels).unwrap();
        value.definitions[0]
            .inputs
            .iter_mut()
            .find(|input| input.name == "count")
            .unwrap()
            .protected = true;
        artist_edit(&mut session, id, value);
        session.command(DesktopCommand::Select(Selection {
            document: Some(id),
            objects: vec![node],
        }));
        let artist_selection = session.state().selection.clone();
        let artist_frame = session.state().frame;
        let before = construction(&session, id);
        let info = call(&mut session, &path, transport, json!({"method":"discover"}));
        let epoch = info["epoch"].clone();
        let base = session.snapshot().unwrap().revision().0;
        let guard = || json!({"epoch":epoch,"request_id":ObjectId::new(),"base":base,"scope":{"document":id,"definition":null}});
        let inspected = call(
            &mut session,
            &path,
            transport,
            json!({"method":"inspect_document","epoch":epoch,"revision":base,"scope":{"document":id,"definition":null}}),
        );
        assert_eq!(
            fold_motion::source::parse(inspected["result"]["source"].as_str().unwrap())
                .unwrap()
                .construction,
            before
        );
        let track_node = before.definitions[0]
            .nodes
            .iter()
            .find(|node| node.id == channels[0].target.object)
            .unwrap();
        let Binding::Track { track } = track_node.parameters["track"].binding().unwrap() else {
            panic!("expected native track")
        };
        let key = track.keys[1].id;
        let mut proposed_key = track.keys[1].clone();
        proposed_key.time = Time::new(3, 4).unwrap();
        let field = |name: &str| json!({"kind":"field","value":name});
        let element = |id| json!({"kind":"element","value":id});
        // Both direct instance overrides and parent replacement must retain locks.
        let count_index = before.definitions[0]
            .inputs
            .iter()
            .position(|input| input.name == "count")
            .unwrap();
        let input = before.definitions[0].inputs[count_index].id;
        let mut override_value = before.root.overrides[&input].clone();
        override_value.value = serde_json::to_value(Binding::Literal {
            value: Literal::Integer { value: 999 },
        })
        .unwrap();
        let mut parent = before.definitions[0].clone();
        parent.inputs[count_index].default = Literal::Integer { value: 999 };
        for (case, patch) in [
            (
                "override_lock_bypass",
                json!({"path":[field("root"),field("overrides"),field(&input.to_string())],"expected":{"kind":"present","value":before.root.overrides[&input]},"value":{"kind":"present","value":override_value}}),
            ),
            (
                "parent_lock_bypass",
                json!({"path":[field("definitions"),element(before.root.definition)],"expected":{"kind":"present","value":before.definitions[0]},"value":{"kind":"present","value":parent}}),
            ),
        ] {
            let rejected = capture_rejection(
                &mut session,
                &path,
                transport,
                id,
                json!({"method":"propose","guard":guard(),"edit":{"kind":"patches","patches":[patch]}}),
                (case, "locked"),
                &mut errors,
            );
            assert_eq!(rejected["error"]["code"], "locked", "{rejected}");
            assert_eq!(session.snapshot().unwrap().revision().0, base);
            assert_eq!(construction(&session, id), before);
        }
        for (case, code) in [
            ("cycle", "cycle"),
            ("recursive_asset", "cycle"),
            ("unknown_operation", "unsupported"),
        ] {
            let mut definition = before.definitions[0].clone();
            match case {
                "cycle" => {
                    let operation = &mut definition.nodes[0];
                    operation.parameters.get_mut("value").unwrap().value =
                        serde_json::to_value(Binding::Link {
                            reference: fold_motion::model::Reference::Node { node: operation.id }
                                .into(),
                        })
                        .unwrap();
                }
                "recursive_asset" => {
                    let mut recursive = before.root.clone();
                    recursive.id = ObjectId::new();
                    recursive.name = "Recursive instance".into();
                    recursive.overrides.clear();
                    definition.instances.push(recursive);
                }
                "unknown_operation" => definition.nodes[0].operation = "vendor.unsupported".into(),
                _ => unreachable!(),
            }
            capture_rejection(
                &mut session,
                &path,
                transport,
                id,
                json!({"method":"propose","guard":guard(),"edit":{"kind":"patches","patches":[{
                    "path":[field("definitions"),element(definition.id)],
                    "expected":{"kind":"present","value":before.definitions[0]},
                    "value":{"kind":"present","value":definition}
                }]}}),
                (case, code),
                &mut errors,
            );
        }
        let proposal = call(
            &mut session,
            &path,
            transport,
            json!({"method":"propose","guard":guard(),"edit":{"kind":"patches","patches":[{
                "path":[field("definitions"),element(before.root.definition),field("nodes"),element(track_node.id),field("parameters"),field("track"),field("value"),field("track"),field("keys"),element(key)],
                "expected":{"kind":"present","value":track.keys[1]},"value":{"kind":"present","value":proposed_key}
            }]}}),
        );
        let original_handle = proposal["result"]["handle"].clone();
        let original_inspection = call(
            &mut session,
            &path,
            transport,
            json!({"method":"inspect_candidate","handle":original_handle}),
        );
        // Even edits to different fields of the same key must conflict.
        let mut raced_channels = channels.clone();
        raced_channels[0].curve.keys[1].value = 0.9;
        let raced = fold_motion::keyed::replace(&before, &raced_channels).unwrap();
        artist_edit(&mut session, id, raced.clone());
        let race_base = session.snapshot().unwrap().revision().0;
        let rejected = capture_rejection(
            &mut session,
            &path,
            transport,
            id,
            json!({"method":"rebase","guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":race_base,"scope":{"document":id,"definition":null}},"handle":original_handle}),
            ("same_property", "conflict"),
            &mut errors,
        );
        assert_eq!(rejected["error"]["code"], "conflict", "{rejected}");
        assert!(
            rejected["error"]["details"]["conflicts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|conflict| conflict["native_id"] == json!(key))
        );
        assert_eq!(session.snapshot().unwrap().revision().0, race_base);
        assert_eq!(construction(&session, id), raced);
        session.command(DesktopCommand::Undo);
        assert_eq!(construction(&session, id), before);
        // An artist can continue arranging the graph while the proposal exists.
        // Rebase must publish a new immutable generation and preserve that edit.
        let mut before = before.clone();
        before.presentation.positions.insert(node, [480., 240.]);
        artist_edit(&mut session, id, before.clone());
        let base = session.snapshot().unwrap().revision().0;
        let guard = || json!({"epoch":epoch,"request_id":ObjectId::new(),"base":base,"scope":{"document":id,"definition":null}});
        let rebased = call(
            &mut session,
            &path,
            transport,
            json!({"method":"rebase","guard":guard(),"handle":original_handle}),
        );
        let handle = rebased["result"]["handle"].clone();
        assert_ne!(handle, original_handle);
        let inspection = call(
            &mut session,
            &path,
            transport,
            json!({"method":"inspect_candidate","handle":handle}),
        );
        let wrong_receipt = capture_rejection(
            &mut session,
            &path,
            transport,
            id,
            json!({"method":"apply","guard":guard(),"handle":handle,"receipt":original_inspection["result"]["receipt"]}),
            ("stale_generation", "conflict"),
            &mut errors,
        );
        assert_eq!(
            wrong_receipt["error"]["code"], "conflict",
            "{wrong_receipt}"
        );
        assert_eq!(construction(&session, id), before);
        call(
            &mut session,
            &path,
            transport,
            json!({"method":"cancel_candidate","guard":guard(),"handle":original_handle}),
        );
        let cancelled = capture_rejection(
            &mut session,
            &path,
            transport,
            id,
            json!({"method":"apply","guard":guard(),"handle":original_handle,"receipt":original_inspection["result"]["receipt"]}),
            ("cancelled_candidate", "cancelled"),
            &mut errors,
        );
        assert_eq!(cancelled["error"]["code"], "cancelled", "{cancelled}");
        assert_eq!(construction(&session, id), before);
        let mut expected_channels = channels.clone();
        expected_channels[0].curve.keys[1].time = Time::new(3, 4).unwrap();
        let expected = fold_motion::keyed::replace(&before, &expected_channels).unwrap();
        assert_eq!(
            fold_motion::source::parse(inspection["result"]["source"].as_str().unwrap())
                .unwrap()
                .construction,
            expected
        );
        call(
            &mut session,
            &path,
            transport,
            json!({"method":"observe","target":{"kind":"candidate","handle":handle},"output":before.definitions[0].outputs[0].id,"times":[Time::new(1,4).unwrap()]}),
        );
        assert_eq!(session.state().frame, artist_frame);
        assert_eq!(
            session.state().selection.document,
            artist_selection.document
        );
        assert_eq!(session.state().selection.objects, artist_selection.objects);
        let apply = json!({"method":"apply","guard":guard(),"handle":handle,"receipt":inspection["result"]["receipt"]});
        let applied = call(&mut session, &path, transport, apply.clone());
        assert_eq!(construction(&session, id), expected);
        assert_eq!(call(&mut session, &path, transport, apply.clone()), applied);
        let mut reused = apply.clone();
        reused["handle"] = original_handle;
        let rejected = capture_rejection(
            &mut session,
            &path,
            transport,
            id,
            reused,
            ("changed_retry", "idempotency_conflict"),
            &mut errors,
        );
        assert_eq!(
            rejected["error"]["code"], "idempotency_conflict",
            "{rejected}"
        );
        assert_eq!(construction(&session, id), expected);
        session.command(DesktopCommand::Undo);
        assert_eq!(
            construction(&session, id),
            before,
            "one undo must remove exactly the agent edit"
        );
        assert_eq!(call(&mut session, &path, transport, apply), applied);
        assert_eq!(
            construction(&session, id),
            before,
            "retry after Undo must not apply again"
        );
        session.command(DesktopCommand::Redo);
        assert_eq!(construction(&session, id), expected);
        let saved = directory.path().join("handoff.fold");
        session.command(DesktopCommand::Save(saved.clone()));
        finish_file_work(&mut session);
        assert!(saved.exists());
        session.command(DesktopCommand::NewProject);
        session.command(DesktopCommand::Open(saved));
        finish_file_work(&mut session);
        assert_eq!(construction(&session, id), expected);
        let reopened = call(&mut session, &path, transport, json!({"method":"discover"}));
        let inspected = call(
            &mut session,
            &path,
            transport,
            json!({"method":"inspect_document","epoch":reopened["epoch"],"revision":reopened["result"]["revision"],"scope":{"document":id,"definition":null}}),
        );
        outcomes.push((
            info["result"]["request_schema"].clone(),
            inspected["result"]["source"].clone(),
            seek_runs,
            errors,
        ));
    }
    assert_eq!(
        outcomes[0], outcomes[1],
        "CLI/MCP schemas, frozen seek samples and complete reopened source must agree"
    );
    if let Some(directory) = std::env::var_os("FOLD_SLICE_CAPTURE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("transport-evidence.json"), serde_json::to_vec_pretty(&json!({
            "capture_scope":"frozen seek samples and source/key/layout/count-lock handoff; ramp/field and expiry capture pending",
            "cli_schema":outcomes[0].0,"mcp_schema":outcomes[1].0,
            "cli_handoff":outcomes[0].1,"mcp_handoff":outcomes[1].1,
            "cli_seek_runs":outcomes[0].2,"mcp_seek_runs":outcomes[1].2,
            "cli_errors":outcomes[0].3,"mcp_errors":outcomes[1].3,
        })).unwrap()).unwrap();
    }
}

// Count actual native Undo operations to reach the captured artist state, then
// let the caller Redo. This checks history behavior rather than revision deltas.
fn undo_to(session: &mut Session, document: DocumentId, target: &Construction) -> usize {
    let mut count = 0;
    while construction(session, document) != *target {
        assert!(count < 8, "Undo failed to reach captured artist state");
        session.command(DesktopCommand::Undo);
        count += 1;
    }
    count
}

fn handoff_properties(value: &Construction) -> Value {
    let definition = &value.definitions[0];
    let input = |name: &str| {
        definition
            .inputs
            .iter()
            .find(|input| input.name == name)
            .unwrap()
    };
    let palette = definition
        .nodes
        .iter()
        .find(|node| node.name == "Palette")
        .unwrap();
    json!({
        "ramp":palette.parameters["stops"],
        "artist_key":fold_motion::keyed::channels(value,None).unwrap(),
        "count_lock":input("Columns"),
        "graph_layout":value.presentation,
        "asset_overrides":value.root.overrides,
        "field_direction":[input("Field start"),input("Field end"),input("Field duration")],
    })
}

#[test]
fn integrated_ramp_key_lock_and_layout_survive_public_field_handoff() {
    let fixture = fold_project::load(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/rewrite/evidence/m5b-controls-baseline.fold"),
        32,
    )
    .unwrap();
    let document = fixture
        .snapshot()
        .state()
        .documents
        .values()
        .next()
        .unwrap()
        .clone();
    let id = document.id;
    let original = Construction::from_document(&document).unwrap();
    let title = original.definitions[0]
        .nodes
        .iter()
        .find(|n| n.name == "Title screen")
        .unwrap();
    // Create the native track once so both transport runs retain identical IDs.
    let mut prepared =
        fold_motion::keyed::animate(&original, title.parameters["opacity"].id, Time::ZERO).unwrap();
    let title_color = prepared.definitions[0]
        .inputs
        .iter()
        .find(|input| input.name == "Title color")
        .unwrap();
    prepared.root.overrides.insert(
        title_color.id,
        fold_motion::model::Parameter::new(Binding::Literal {
            value: Literal::Color {
                value: [0.1, 0.3, 0.8, 1.],
                encoding: fold_motion::types::Encoding::LinearRec709,
                alpha: fold_motion::types::Alpha::Straight,
            },
        })
        .unwrap(),
    );
    let mut outcomes = vec![];
    for transport in [Transport::Cli, Transport::Mcp] {
        let mut project = Project::new(32);
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument((*document).clone())],
            })
            .unwrap();
        let mut session = Session::new(project);
        let directory = tempfile::tempdir().unwrap();
        let path = session.enable_agent_transport(directory.path()).unwrap();
        artist_edit(&mut session, id, prepared.clone());
        let mut artist = construction(&session, id);
        let mut channels = fold_motion::keyed::channels(&artist, None).unwrap();
        channels[0].curve.keys[0].value = 0.8;
        channels[0].curve.keys[0].time = Time::new(1, 4).unwrap();
        artist = fold_motion::keyed::replace(&artist, &channels).unwrap();
        artist.definitions[0]
            .inputs
            .iter_mut()
            .find(|i| i.name == "Columns")
            .unwrap()
            .protected = true;
        let palette = artist.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.name == "Palette")
            .unwrap();
        let palette_id = palette.id;
        let stops = palette.parameters.get_mut("stops").unwrap();
        let Binding::Ramp { mut ramp } = stops.binding().unwrap() else {
            panic!("native Ramp required")
        };
        let Literal::Color { value, .. } = &mut ramp.stops[0].value else {
            panic!("color stop required")
        };
        *value = [0.02, 1., 0.1, 1.];
        stops.value = serde_json::to_value(Binding::Ramp { ramp }).unwrap();
        artist
            .presentation
            .positions
            .insert(palette_id, [640., 320.]);
        artist
            .presentation
            .annotations
            .insert(palette_id, "Artist palette: preserve green".into());
        artist_edit(&mut session, id, artist.clone());
        session.command(DesktopCommand::Select(Selection {
            document: Some(id),
            objects: vec![palette_id],
        }));
        let selection = session.state().selection.clone();
        let frame = session.state().frame;
        let selection_before = json!({"document":selection.document,"objects":selection.objects});
        let info = call(&mut session, &path, transport, json!({"method":"discover"}));
        let epoch = info["epoch"].clone();
        let base = session.snapshot().unwrap().revision().0;
        let guard = |base| json!({"epoch":epoch,"request_id":ObjectId::new(),"base":base,"scope":{"document":id,"definition":null}});
        let field = |name: &str| json!({"kind":"field","value":name});
        let element = |id: ObjectId| json!({"kind":"element","value":id});
        let mut expected = artist.clone();
        let mut patches = vec![];
        for name in ["Field start", "Field end", "Field duration"] {
            let input = expected.definitions[0]
                .inputs
                .iter_mut()
                .find(|i| i.name == name)
                .unwrap();
            let before = input.default.clone();
            match &mut input.default {
                Literal::Vector { value, .. } => {
                    value[0] = -value[0];
                    value[1] = -value[1];
                }
                Literal::Duration { value } => *value = Time::new(3, 1).unwrap(),
                _ => panic!("unexpected field input"),
            }
            patches.push(json!({
                "path":[field("definitions"),element(artist.root.definition),field("inputs"),element(input.id),field("default")],
                "expected":{"kind":"present","value":before},"value":{"kind":"present","value":input.default}
            }));
        }
        let proposed = call(
            &mut session,
            &path,
            transport,
            json!({"method":"propose","guard":guard(base),"edit":{"kind":"patches","patches":patches}}),
        );
        let initial_handle = proposed["result"]["handle"].clone();
        // A further native ramp/layout edit races with the pending field proposal.
        let palette = artist.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == palette_id)
            .unwrap();
        let stops = palette.parameters.get_mut("stops").unwrap();
        let Binding::Ramp { mut ramp } = stops.binding().unwrap() else {
            unreachable!()
        };
        ramp.stops[1].position = 0.9;
        stops.value = serde_json::to_value(Binding::Ramp { ramp }).unwrap();
        artist
            .presentation
            .positions
            .insert(palette_id, [720., 360.]);
        artist_edit(&mut session, id, artist.clone());
        expected.presentation = artist.presentation.clone();
        expected.definitions[0].nodes = artist.definitions[0].nodes.clone();
        let base = session.snapshot().unwrap().revision().0;
        let rebased = call(
            &mut session,
            &path,
            transport,
            json!({"method":"rebase","guard":guard(base),"handle":initial_handle}),
        );
        let handle = rebased["result"]["handle"].clone();
        assert_ne!(handle, initial_handle);
        let inspected = call(
            &mut session,
            &path,
            transport,
            json!({"method":"inspect_candidate","handle":handle}),
        );
        assert_eq!(
            fold_motion::source::parse(inspected["result"]["source"].as_str().unwrap())
                .unwrap()
                .construction,
            expected
        );
        let observe = |target| json!({"method":"observe","target":target,"output":artist.definitions[0].outputs[0].id,"times":[Time::ZERO,Time::new(1,1).unwrap(),Time::new(3,1).unwrap()]});
        let observation = call(
            &mut session,
            &path,
            transport,
            observe(json!({"kind":"candidate","handle":handle})),
        );
        assert_eq!(session.state().selection.objects, selection.objects);
        assert_eq!(session.state().selection.document, selection.document);
        assert_eq!(session.state().frame, frame);
        let selection_after = json!({"document":session.state().selection.document,"objects":session.state().selection.objects});
        let frame_after = session.state().frame;
        // Independent reversed, three-second radial sweep and artist's compressed ramp.
        for sample in observation["result"]["samples"].as_array().unwrap() {
            let t: Time = serde_json::from_value(sample["time"].clone()).unwrap();
            let u = (t.numerator() as f64 / f64::from(t.denominator()) / 3.).clamp(0., 1.);
            let center = [1. - 2. * u, 0.5 - u, 0.];
            let copies = sample["geometries"]
                .as_object()
                .unwrap()
                .values()
                .find_map(|g| g.get("instances"))
                .unwrap()
                .as_array()
                .unwrap();
            assert_eq!(copies.len(), 240);
            for copy in copies {
                let position: [f64; 3] = serde_json::from_value(copy["position"].clone()).unwrap();
                let distance = position
                    .iter()
                    .zip(center)
                    .map(|(p, c)| (p - c).powi(2))
                    .sum::<f64>()
                    .sqrt();
                let influence = (1. - distance).clamp(0., 1.);
                assert!((copy["scale"].as_f64().unwrap() - (0.2 + 0.8 * influence)).abs() < 1e-12);
                let factor = (influence / 0.9).min(1.);
                for (index, (a, b)) in [0.02, 1., 0.1, 1.]
                    .into_iter()
                    .zip([0.02, 0.1, 1., 1.])
                    .enumerate()
                {
                    assert!(
                        (copy["color"]["value"][index].as_f64().unwrap() - (a + (b - a) * factor))
                            .abs()
                            < 1e-12
                    );
                }
            }
        }
        let apply = json!({"method":"apply","guard":guard(base),"handle":handle,"receipt":inspected["result"]["receipt"]});
        let applied = call(&mut session, &path, transport, apply.clone());
        assert_eq!(construction(&session, id), expected);
        let applied_state = construction(&session, id);
        let undo_groups = undo_to(&mut session, id, &artist);
        assert_eq!(undo_groups, 1);
        session.command(DesktopCommand::Redo);
        assert_eq!(construction(&session, id), applied_state);
        assert_eq!(call(&mut session, &path, transport, apply), applied);
        let retry_undo_groups = undo_to(&mut session, id, &artist);
        assert_eq!(retry_undo_groups, undo_groups);
        session.command(DesktopCommand::Redo);
        assert_eq!(construction(&session, id), applied_state);
        let saved = directory.path().join("integrated-handoff.fold");
        let save_base = session.snapshot().unwrap().revision().0;
        let saved_result = call(
            &mut session,
            &path,
            transport,
            json!({
                "method":"save_project_copy",
                "guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":save_base},
                "path":saved,
            }),
        );
        assert_eq!(saved_result["result"]["state"], "saved");
        session.command(DesktopCommand::NewProject);
        session.command(DesktopCommand::Open(saved.clone()));
        finish_file_work(&mut session);
        assert_eq!(construction(&session, id), expected);
        let reopened = call(&mut session, &path, transport, json!({"method":"discover"}));
        let after = call(
            &mut session,
            &path,
            transport,
            observe(
                json!({"kind":"committed","epoch":reopened["epoch"],"revision":reopened["result"]["revision"],"document":id}),
            ),
        );
        assert_eq!(after["result"]["samples"], observation["result"]["samples"]);
        outcomes.push(json!({
            "source_revision":env!("FOLD_SOURCE_SHA256"),
            "build":{"target":env!("FOLD_BUILD_TARGET"),"profile":env!("FOLD_BUILD_PROFILE"),"package":env!("CARGO_PKG_VERSION")},
            "before":artist,"after":construction(&session, id),
            "handoff":{
                "before":handoff_properties(&artist),"after":handoff_properties(&construction(&session,id)),
                "undo_groups_added":undo_groups,
                "artist_clock_before":frame,"artist_clock_after":frame_after,
                "artist_selection_before":selection_before,"artist_selection_after":selection_after,
            },
            "retry_undo_groups_added":retry_undo_groups-undo_groups,
            "samples":after["result"]["samples"],"schema":info["result"]["request_schema"]
        }));
        if let Some(destination) = std::env::var_os("FOLD_M5_HANDOFF_DIR") {
            let destination = std::path::PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            let name = match transport {
                Transport::Cli => "cli",
                Transport::Mcp => "mcp",
            };
            std::fs::copy(saved, destination.join(format!("{name}.fold"))).unwrap();
            std::fs::write(
                destination.join(format!("{name}.json")),
                serde_json::to_vec_pretty(outcomes.last().unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
    assert_eq!(
        outcomes[0], outcomes[1],
        "complete handoff state, samples and schemas must match"
    );
}

#[cfg(feature = "spatial")]
#[test]
#[ignore = "requires a native Vulkan adapter and private local sockets"]
fn cli_and_mcp_return_identical_revision_bound_frames() {
    let project = fold_project::load(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/rewrite/evidence/m5b-controls-baseline.fold"),
        32,
    )
    .unwrap();
    let snapshot = project.snapshot();
    let document = *snapshot.state().documents.keys().next().unwrap();
    let source = fold_app::packages::builtins()
        .default_output(&snapshot, document)
        .unwrap();
    let (host, info) =
        pollster::block_on(fold_render::gpu::Host::headless(512 * 1024 * 1024)).unwrap();
    eprintln!("Public transport frame adapter: {info:?}");
    let mut session = Session::new(project);
    session.set_render_host(host);
    session.command(DesktopCommand::Select(Selection {
        document: Some(document),
        objects: vec![],
    }));
    let frame = session.state().frame;
    let directory = tempfile::tempdir().unwrap();
    let socket = session.enable_agent_transport(directory.path()).unwrap();
    let mut results = vec![];
    for transport in [Transport::Cli, Transport::Mcp] {
        let discovery = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"discover"}),
        );
        assert_eq!(discovery["result"]["frame_observation"]["available"], true);
        let result = call(
            &mut session,
            &socket,
            transport,
            json!({
                "method":"observe_frame",
                "target":{"kind":"committed","epoch":discovery["epoch"],"revision":snapshot.revision(),"document":document},
                "output":source.output,"time":Time::new(1,4).unwrap(),"dimensions":[480,270]
            }),
        );
        assert_eq!(
            result["result"]["image"]["data"].as_str().unwrap().len(),
            480 * 270 * 8
        );
        assert_eq!(
            result["result"]["provenance"]["revision"],
            json!(snapshot.revision())
        );
        assert_eq!(
            result["result"]["provenance"]["time"],
            json!(Time::new(1, 4).unwrap())
        );
        let crop = call(
            &mut session,
            &socket,
            transport,
            json!({
                "method":"observe_frame",
                "target":{"kind":"committed","epoch":discovery["epoch"],"revision":snapshot.revision(),"document":document},
                "output":source.output,"time":Time::new(1,4).unwrap(),"dimensions":[480,270],
                "region":{"x":37,"y":43,"width":361,"height":192}
            }),
        );
        assert_eq!(crop["result"]["image"]["dimensions"], json!([361, 192]));
        assert_eq!(
            crop["result"]["image"]["data"].as_str().unwrap().len(),
            361 * 192 * 8
        );
        results.push(json!({"full":result,"crop":crop}));
        assert_eq!(session.state().frame, frame);
        assert_eq!(session.state().selection.document, Some(document));
        assert!(session.snapshot().unwrap().same_content(&snapshot));
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn cli_and_mcp_save_committed_project_copies_with_exact_retry_and_reopen() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/rewrite/evidence/m5b-controls-baseline.fold");
    let mut archives = vec![];
    for transport in [Transport::Cli, Transport::Mcp] {
        let project = fold_project::load(&fixture, 32).unwrap();
        let baseline = project.snapshot();
        let document = *baseline.state().documents.keys().next().unwrap();
        let mut session = Session::new(project);
        let directory = tempfile::tempdir().unwrap();
        let socket = session.enable_agent_transport(directory.path()).unwrap();
        let destination = directory.path().join("copy.fold");
        let discovery = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"discover"}),
        );
        let epoch = discovery["epoch"].clone();
        let original_path = session.state().project_path.clone();
        let guard = json!({"epoch":epoch,"request_id":ObjectId::new(),"base":baseline.revision()});
        let save = json!({"method":"save_project_copy","guard":guard,"path":destination});
        let accepted = request(&mut session, &socket, transport, save.clone());
        assert_eq!(accepted["result"]["state"], "running");
        assert_eq!(accepted["result"]["scope"], Value::Null);
        let job = accepted["result"]["job"].clone();
        let mut artist = construction(&session, document);
        artist.seed += 1;
        artist_edit(&mut session, document, artist.clone());
        let saved = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"job_status","handle":job}),
        );
        assert_eq!(saved["result"]["state"], "saved");
        assert_eq!(saved["result"]["revision"], json!(baseline.revision()));
        assert_eq!(saved["result"]["durable"], true);
        assert_eq!(saved["result"]["session_file_changed"], false);
        assert_eq!(session.state().project_path, original_path);
        assert!(session.state().dirty);
        assert_eq!(construction(&session, document), artist);
        assert_eq!(
            fold_project::load(&destination, 32)
                .unwrap()
                .snapshot()
                .state(),
            baseline.state()
        );
        // The same request returns its retained job/result even after artist edits.
        assert_eq!(call(&mut session, &socket, transport, save.clone()), saved);
        let base = session.snapshot().unwrap().revision();
        let fresh_guard = || json!({"epoch":epoch,"request_id":ObjectId::new(),"base":base});
        let cancelled = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"cancel_job","guard":fresh_guard(),"handle":job}),
        );
        assert_eq!(cancelled["result"]["state"], "already_committed");
        let wrong_scope = terminal(
            &mut session,
            &socket,
            transport,
            json!({"method":"cancel_job","guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":base,"scope":{"document":document,"definition":null}},"handle":job}),
        );
        assert_eq!(wrong_scope["error"]["code"], "conflict");
        let existing = terminal(
            &mut session,
            &socket,
            transport,
            json!({"method":"save_project_copy","guard":fresh_guard(),"path":destination}),
        );
        assert_eq!(existing["error"]["code"], "conflict");
        let other = directory.path().join("other.fold");
        let mut changed_retry = save;
        changed_retry["path"] = json!(other);
        assert_eq!(
            terminal(&mut session, &socket, transport, changed_retry)["error"]["code"],
            "idempotency_conflict"
        );
        assert!(!other.exists());
        let stale = terminal(
            &mut session,
            &socket,
            transport,
            json!({"method":"save_project_copy","guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":baseline.revision()},"path":other}),
        );
        assert_eq!(stale["error"]["code"], "conflict");
        assert!(!other.exists());
        assert_eq!(construction(&session, document), artist);
        archives.push(std::fs::read(&destination).unwrap());
        session.command(DesktopCommand::Open(destination));
        finish_file_work(&mut session);
        assert!(session.snapshot().unwrap().same_content(&baseline));
        let reopened = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"discover"}),
        );
        assert_ne!(reopened["epoch"], epoch);
        let inspected = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"inspect_document","epoch":reopened["epoch"],"revision":reopened["result"]["revision"],"scope":{"document":document,"definition":null}}),
        );
        assert_eq!(
            fold_motion::source::parse(inspected["result"]["source"].as_str().unwrap())
                .unwrap()
                .construction,
            construction(&session, document)
        );
    }
    assert_eq!(
        archives[0], archives[1],
        "both transports must persist the identical committed archive"
    );
}

#[test]
fn cli_and_mcp_author_relative_parameter_expressions_without_primitive_nodes() {
    for transport in [Transport::Cli, Transport::Mcp] {
        let id = DocumentId::new();
        let document = fold_motion::package::create(id).unwrap();
        let mut project = Project::new(16);
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(document)],
            })
            .unwrap();
        let mut session = Session::new(project);
        let directory = tempfile::tempdir().unwrap();
        let path = session.enable_agent_transport(directory.path()).unwrap();
        let initial = call(&mut session, &path, transport, json!({"method":"discover"}));
        assert_eq!(
            initial["result"]["catalog"]["parameter_references"]["container_node"],
            "Channel References"
        );
        let epoch = initial["epoch"].clone();
        let base = initial["result"]["revision"].clone();
        let scope = json!({"document":id,"definition":null});
        let inspected = call(
            &mut session,
            &path,
            transport,
            json!({"method":"inspect_document","epoch":epoch,"revision":base,"scope":scope}),
        );
        let source = inspected["result"]["source"].as_str().unwrap();
        let mut replaced = false;
        let source = source
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("\"height\" @") {
                    replaced = true;
                    format!(
                        "{} = expr(ch(\"width\") * 0.25);",
                        line.split_once(" = ").unwrap().0
                    )
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(replaced);
        let before = construction(&session, id);
        let guard =
            || json!({"epoch":epoch,"request_id":ObjectId::new(),"base":base,"scope":scope});
        let proposed = call(
            &mut session,
            &path,
            transport,
            json!({"method":"propose","guard":guard(),"edit":{"kind":"source","expected":inspected["result"]["source"],"text":source,"mapping":{}}}),
        );
        let handle = proposed["result"]["handle"].clone();
        let inspected = call(
            &mut session,
            &path,
            transport,
            json!({"method":"inspect_candidate","handle":handle}),
        );
        call(
            &mut session,
            &path,
            transport,
            json!({"method":"apply","guard":guard(),"handle":handle,"receipt":inspected["result"]["receipt"]}),
        );
        let after = construction(&session, id);
        assert_eq!(
            after.definitions[0].nodes.len(),
            before.definitions[0].nodes.len()
        );
        let height = after.definitions[0].nodes[0].parameters["height"].id;
        let sampled = fold_motion::evaluation::Prepared::new(after.clone())
            .unwrap()
            .sample_parameters(&[], &[height], Time::ZERO, &|| Ok(()))
            .unwrap();
        assert_eq!(
            sampled[&height],
            Literal::Distance {
                value: 50.,
                unit: fold_motion::types::Unit::Canvas
            }
        );
        let revision = session.snapshot().unwrap().revision();
        let catalog = call(
            &mut session,
            &path,
            transport,
            json!({
                "method":"inspect_parameters", "epoch":epoch, "revision":revision,
                "document":id, "offset":0, "limit":256
            }),
        );
        let parameter = catalog["result"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| {
                entry["target"]["parameter"]["id"] == json!(height)
                    && entry["target"]["component"].is_null()
            })
            .unwrap();
        let segments = parameter["path"].as_array().unwrap();
        let resolved = call(
            &mut session,
            &path,
            transport,
            json!({
                "method":"resolve_parameter", "epoch":epoch, "revision":revision,
                "owner": &segments[..segments.len()-1], "path":"height"
            }),
        );
        assert_eq!(
            resolved["result"]["parameter"]["target"],
            parameter["target"]
        );
        let public_sample = call(
            &mut session,
            &path,
            transport,
            json!({
                "method":"sample_parameter", "epoch":epoch, "revision":revision,
                "target":parameter["target"], "time":Time::ZERO
            }),
        );
        assert_eq!(public_sample["result"]["value"], json!(sampled[&height]));
        let stale = terminal(
            &mut session,
            &path,
            transport,
            json!({
                "method":"sample_parameter", "epoch":epoch, "revision":base,
                "target":parameter["target"], "time":Time::ZERO
            }),
        );
        assert_eq!(stale["error"]["code"], "conflict");
        assert_eq!(undo_to(&mut session, id, &before), 1);
        session.command(DesktopCommand::Redo);
        assert_eq!(construction(&session, id), after);
    }
}

#[test]
fn cli_and_mcp_inspect_layer_and_timeline_properties_through_one_service() {
    for transport in [Transport::Cli, Transport::Mcp] {
        let motion_id = DocumentId::new();
        let motion_document = fold_motion::package::create(motion_id).unwrap();
        let motion = Construction::from_document(&motion_document).unwrap();
        let width = motion.definitions[0].nodes[0].parameters["width"].id;
        let mut composition = fold_composition::Composition::default();
        let mut layer =
            fold_composition::Layer::new("Layer / One", None, composition.info.clone()).unwrap();
        layer.source = Some(fold_platform::image_source::Source::Output(
            fold_project::DocumentRef {
                document: motion_id,
                output: motion.definitions[0].outputs[0].id.to_string(),
                extensions: Default::default(),
            },
        ));
        layer.opacity = 0.4;
        let layer_id = layer.id;
        composition.layers.push(layer);
        let composition_id = DocumentId::new();
        let timeline_id = DocumentId::new();
        let track = fold_timeline::Track::new(fold_timeline::TrackKind::Video, "Track / One");
        let track_id = track.id;
        let sequence = fold_timeline::Sequence {
            dimensions: [16, 16],
            rate: [24, 1],
            tracks: vec![track],
            clips: vec![],
            transitions: vec![],
            extensions: Default::default(),
        };
        let mut project = Project::new(8);
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![
                    Mutation::PutDocument(motion_document),
                    Mutation::PutDocument(composition.document(composition_id).unwrap()),
                    Mutation::PutDocument(sequence.document(timeline_id).unwrap()),
                ],
            })
            .unwrap();
        let mut session = Session::new(project);
        let directory = tempfile::tempdir().unwrap();
        let socket = session.enable_agent_transport(directory.path()).unwrap();
        let discovered = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"discover"}),
        );
        let epoch = &discovered["epoch"];
        let revision = &discovered["result"]["revision"];
        for (document, object, key, expected) in [
            (
                composition_id,
                layer_id,
                "opacity",
                json!(Literal::Scalar { value: 0.4 }),
            ),
            (
                timeline_id,
                track_id,
                "enabled",
                json!(Literal::Bool { value: true }),
            ),
        ] {
            let catalog = call(
                &mut session,
                &socket,
                transport,
                json!({"method":"inspect_parameters","epoch":epoch,"revision":revision,"document":document,"offset":0,"limit":256}),
            );
            let entry = catalog["result"]["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| {
                    e["target"]["object"] == json!(object)
                        && e["target"]["parameter"]["name"] == key
                })
                .unwrap();
            let path = entry["path"].as_array().unwrap();
            let resolved = call(
                &mut session,
                &socket,
                transport,
                json!({"method":"resolve_parameter","epoch":epoch,"revision":revision,"owner":&path[..path.len()-1],"path":key}),
            );
            assert_eq!(resolved["result"]["parameter"]["target"], entry["target"]);
            let sample = call(
                &mut session,
                &socket,
                transport,
                json!({"method":"sample_parameter","epoch":epoch,"revision":revision,"target":entry["target"],"time":Time::ZERO}),
            );
            assert_eq!(sample["result"]["value"], expected);
        }
        let catalog = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"inspect_parameters","epoch":epoch,"revision":revision,"document":composition_id,"offset":0,"limit":256}),
        );
        let source = catalog["result"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["target"]["parameter"]["id"] == json!(width))
            .unwrap();
        assert_eq!(source["target"]["placement"]["root"], json!(composition_id));
        assert_eq!(source["target"]["placement"]["owners"], json!([layer_id]));
        let sample = call(
            &mut session,
            &socket,
            transport,
            json!({"method":"sample_parameter","epoch":epoch,"revision":revision,"target":source["target"],"time":Time::ZERO}),
        );
        assert_eq!(
            sample["result"]["value"],
            json!(Literal::Distance {
                value: 200.,
                unit: fold_motion::types::Unit::Canvas
            })
        );
    }
}
