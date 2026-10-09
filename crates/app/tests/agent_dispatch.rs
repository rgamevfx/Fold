use fold_app::agent::dispatch::Dispatcher;
use fold_foundation::{DocumentId, ObjectId};
use fold_motion::model::Construction;
use fold_project::{EditBatch, Mutation, Project};
use serde_json::{Value, json};
use std::time::Instant;
fn setup() -> (Dispatcher, Project, DocumentId) {
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
    (Dispatcher::default(), project, id)
}
fn guard(d: &Dispatcher, p: &Project, id: DocumentId) -> Value {
    json!({"epoch":d.epoch(),"request_id":ObjectId::new(),"base":p.snapshot().revision().0,"scope":{"document":id,"definition":null}})
}
fn raw(d: &mut Dispatcher, p: &mut Project, command: Value) -> Value {
    serde_json::from_slice(&d.dispatch(
        p,
        &serde_json::to_vec(&json!({"version":2,"command":command})).unwrap(),
        Instant::now(),
    ))
    .unwrap()
}
fn call(d: &mut Dispatcher, p: &mut Project, command: Value) -> Value {
    let mut result = raw(d, p, command);
    let job = result["result"]["job"].clone();
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while result["result"]["state"] == "running" {
        assert!(Instant::now() < deadline, "job did not complete");
        std::thread::sleep(std::time::Duration::from_millis(1));
        result = raw(d, p, json!({"method":"job_status","handle":job}));
    }
    result
}

fn propose(d: &mut Dispatcher, p: &mut Project, id: DocumentId) -> Value {
    let proposed_guard = guard(d, p, id);
    let result = call(
        d,
        p,
        json!({"method":"propose","guard":proposed_guard,"edit":{"kind":"patches","patches":[{"path":[{"kind":"field","value":"seed"}],"expected":{"kind":"present","value":0},"value":{"kind":"present","value":42}}]}}),
    );
    assert!(result.get("error").is_none(), "{result}");
    result["result"]["handle"].clone()
}

#[test]
fn version_schema_and_closed_dispatch_reject_legacy_and_artist_unlock_routes() {
    let (mut d, mut p, _) = setup();
    let revision = p.snapshot().revision();
    for bytes in [
        br#"{"method":"author"}"#.as_slice(),
        br#"{"version":1,"command":{"method":"discover"}}"#,
    ] {
        let result: Value =
            serde_json::from_slice(&d.dispatch(&mut p, bytes, Instant::now())).unwrap();
        assert_eq!(result["error"]["code"], "version");
    }
    for command in [
        json!({"method":"unlock_controls"}),
        json!({"method":"discover","extra":true}),
    ] {
        assert_eq!(
            call(&mut d, &mut p, command)["error"]["code"],
            "unsupported"
        );
    }
    let duplicate: Value = serde_json::from_slice(&d.dispatch(
        &mut p,
        br#"{"version":2,"version":2,"command":{"method":"discover"}}"#,
        Instant::now(),
    ))
    .unwrap();
    assert_eq!(duplicate["error"]["code"], "unsupported");
    let discovery = call(&mut d, &mut p, json!({"method":"discover"}));
    assert_eq!(
        discovery["result"]["request_schema"]["properties"]["version"]["const"],
        2
    );
    assert!(discovery["result"]["unavailable"]["frames"].is_string());
    assert_eq!(
        discovery["result"]["documents"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        discovery["result"]["documents"][0]["native_inspection"],
        true
    );
    assert_eq!(p.snapshot().revision(), revision);
}

#[test]
fn inspect_apply_retry_undo_and_cancel_use_one_guarded_dispatcher() {
    let (mut d, mut p, id) = setup();
    let handle = propose(&mut d, &mut p, id);
    let inspection = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":handle}),
    );
    assert!(inspection["result"]["source"].is_string());
    assert_eq!(inspection["result"]["scope"]["document"], json!(id));
    assert_eq!(inspection["result"]["changes"].as_array().unwrap().len(), 1);
    let apply = json!({"version":2,"command":{"method":"apply","guard":guard(&d,&p,id),"handle":handle,"receipt":inspection["result"]["receipt"]}});
    let bytes = serde_json::to_vec(&apply).unwrap();
    let response = d.dispatch(&mut p, &bytes, Instant::now());
    let result: Value = serde_json::from_slice(&response).unwrap();
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(
        Construction::from_document(&p.snapshot().state().documents[&id])
            .unwrap()
            .seed,
        42
    );
    p.undo().unwrap();
    let revision = p.snapshot().revision();
    assert_eq!(d.dispatch(&mut p, &bytes, Instant::now()), response);
    assert_eq!(p.snapshot().revision(), revision);
    let cancel_guard = guard(&d, &p, id);
    let cancel = call(
        &mut d,
        &mut p,
        json!({"method":"cancel_candidate","guard":cancel_guard,"handle":handle}),
    );
    assert_eq!(cancel["result"]["state"], "already_committed");
    let mut malformed = apply.clone();
    malformed["command"]["unexpected"] = json!(true);
    let rejected: Value = serde_json::from_slice(&d.dispatch(
        &mut p,
        &serde_json::to_vec(&malformed).unwrap(),
        Instant::now(),
    ))
    .unwrap();
    assert_eq!(rejected["error"]["code"], "idempotency_conflict");
    let mut changed = apply;
    changed["command"]["guard"]["base"] = json!(revision.0);
    let changed: Value = serde_json::from_slice(&d.dispatch(
        &mut p,
        &serde_json::to_vec(&changed).unwrap(),
        Instant::now(),
    ))
    .unwrap();
    assert_eq!(changed["error"]["code"], "idempotency_conflict");
}

#[test]
fn source_preconditions_and_disjoint_rebase_preserve_artist_edits() {
    let (mut d, mut p, id) = setup();
    let epoch = d.epoch();
    let base = p.snapshot().revision().0;
    let inspected = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_document","epoch":epoch,"revision":base,"scope":{"document":id,"definition":null}}),
    );
    let expected = inspected["result"]["source"].as_str().unwrap();
    let mut construction = fold_motion::source::parse(expected).unwrap().construction;
    construction.seed = 9;
    let edit = json!({"kind":"source","expected":expected,"text":fold_motion::source::canonical(&construction).unwrap(),"mapping":{}});
    let proposed_guard = guard(&d, &p, id);
    let proposal = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":proposed_guard,"edit":edit}),
    );
    let handle = proposal["result"]["handle"].clone();
    assert!(!handle.is_null(), "{proposal}");
    let snapshot = p.snapshot();
    let mut artist = Construction::from_document(&snapshot.state().documents[&id]).unwrap();
    artist
        .presentation
        .positions
        .insert(artist.definitions[0].nodes[0].id, [20., 30.]);
    fold_motion::editing::prepare(
        &snapshot,
        id,
        fold_motion::editing::Scope::Document,
        artist.clone(),
    )
    .unwrap()
    .apply(&mut p)
    .unwrap();
    let rebase_guard = guard(&d, &p, id);
    let rebased = call(
        &mut d,
        &mut p,
        json!({"method":"rebase","guard":rebase_guard,"handle":handle}),
    );
    let next = rebased["result"]["handle"].clone();
    let inspected = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":next}),
    );
    let merged = fold_motion::source::parse(inspected["result"]["source"].as_str().unwrap())
        .unwrap()
        .construction;
    assert_eq!(merged.seed, 9);
    assert_eq!(merged.presentation, artist.presentation);
    let bad_guard = guard(&d, &p, id);
    let bad = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":bad_guard,"edit":{"kind":"source","expected":"wrong","text":"invalid","mapping":{}}}),
    );
    assert_eq!(bad["error"]["code"], "conflict");
    assert_eq!(
        Construction::from_document(&p.snapshot().state().documents[&id]).unwrap(),
        artist
    );
}

#[test]
fn dispatcher_preserves_locks_scopes_and_generation_inspection_guards() {
    let (mut d, mut p, id) = setup();
    let snapshot = p.snapshot();
    let mut locked = Construction::from_document(&snapshot.state().documents[&id]).unwrap();
    locked.definitions[0].inputs[0].protected = true;
    fold_motion::editing::prepare(
        &snapshot,
        id,
        fold_motion::editing::Scope::Document,
        locked.clone(),
    )
    .unwrap()
    .apply(&mut p)
    .unwrap();
    let g = guard(&d, &p, id);
    let unlock = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"patches","patches":[{
            "path":[{"kind":"field","value":"definitions"},{"kind":"element","value":locked.definitions[0].id},{"kind":"field","value":"inputs"},{"kind":"element","value":locked.definitions[0].inputs[0].id},{"kind":"field","value":"protected"}],
            "expected":{"kind":"present","value":true},"value":{"kind":"present","value":false}
        }]}}),
    );
    assert_eq!(unlock["error"]["code"], "locked", "{unlock}");
    let first = propose(&mut d, &mut p, id);
    let inspected = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":first}),
    );
    let next = propose(&mut d, &mut p, id);
    let g = guard(&d, &p, id);
    let rejected = call(
        &mut d,
        &mut p,
        json!({"method":"apply","guard":g,"handle":next,"receipt":inspected["result"]["receipt"]}),
    );
    assert_eq!(rejected["error"]["code"], "conflict");
    let mut g = guard(&d, &p, id);
    g["scope"]["definition"] = json!(locked.definitions[0].id);
    let rejected = call(
        &mut d,
        &mut p,
        json!({"method":"apply","guard":g,"handle":first,"receipt":inspected["result"]["receipt"]}),
    );
    assert_eq!(rejected["error"]["code"], "conflict");
    assert_eq!(
        Construction::from_document(&p.snapshot().state().documents[&id]).unwrap(),
        locked
    );
}

#[test]
fn scoped_source_round_trip_keeps_identity_and_invalid_drafts_return_spans() {
    let (mut d, mut p, id) = setup();
    let mut value = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let asset = value.definitions[0].id;
    let epoch = d.epoch();
    let revision = p.snapshot().revision().0;
    let inspected = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_document","epoch":epoch,"revision":revision,"scope":{"document":id,"definition":asset}}),
    );
    let expected = inspected["result"]["source"].as_str().unwrap();
    let mut g = guard(&d, &p, id);
    g["scope"]["definition"] = json!(asset);
    let bad = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"source","expected":expected,"text":"not a construction","mapping":{}}}),
    );
    assert_eq!(bad["error"]["code"], "invalid_source", "{bad}");
    assert!(bad["error"]["details"]["diagnostics"][0]["start"].is_u64());
    value.definitions[0].name = "Renamed asset".into();
    let text = fold_motion::source::canonical_fragment(&value, asset).unwrap();
    let mut g = guard(&d, &p, id);
    g["scope"]["definition"] = json!(asset);
    let proposal = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"source","expected":expected,"text":text,"mapping":{}}}),
    );
    let handle = proposal["result"]["handle"].clone();
    assert!(!handle.is_null(), "{proposal}");
    let checked = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":handle}),
    );
    let mut g = guard(&d, &p, id);
    g["scope"]["definition"] = json!(asset);
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"apply","guard":g,"handle":handle,"receipt":checked["result"]["receipt"]}),
    );
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(
        Construction::from_document(&p.snapshot().state().documents[&id]).unwrap(),
        value
    );
}

#[test]
fn queued_proposal_keeps_its_base_and_retry_does_not_publish_a_second_generation() {
    let (mut d, mut p, id) = setup();
    let before = p.snapshot();
    let command = json!({"method":"propose","guard":guard(&d,&p,id),"edit":{"kind":"patches","patches":[{"path":[{"kind":"field","value":"seed"}],"expected":{"kind":"present","value":0},"value":{"kind":"present","value":42}}]}});
    let accepted = raw(&mut d, &mut p, command.clone());
    assert_eq!(accepted["result"]["state"], "running");
    let mut artist = Construction::from_document(&before.state().documents[&id]).unwrap();
    artist
        .presentation
        .positions
        .insert(artist.definitions[0].nodes[0].id, [35., 45.]);
    fold_motion::editing::prepare(
        &before,
        id,
        fold_motion::editing::Scope::Document,
        artist.clone(),
    )
    .unwrap()
    .apply(&mut p)
    .unwrap();
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"job_status","handle":accepted["result"]["job"]}),
    );
    assert_eq!(proposed["result"]["base"], json!(before.revision()));
    assert_eq!(raw(&mut d, &mut p, command), accepted);
    let handle = proposed["result"]["handle"].clone();
    let inspected = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":handle}),
    );
    let g = guard(&d, &p, id);
    let stale = call(
        &mut d,
        &mut p,
        json!({"method":"apply","guard":g,"handle":handle,"receipt":inspected["result"]["receipt"]}),
    );
    assert_eq!(stale["error"]["code"], "conflict");
    let g = guard(&d, &p, id);
    let rebased = call(
        &mut d,
        &mut p,
        json!({"method":"rebase","guard":g,"handle":handle}),
    );
    let next = rebased["result"]["handle"].clone();
    assert_ne!(next, handle);
    let inspected = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":next}),
    );
    let g = guard(&d, &p, id);
    let applied = call(
        &mut d,
        &mut p,
        json!({"method":"apply","guard":g,"handle":next,"receipt":inspected["result"]["receipt"]}),
    );
    assert!(applied.get("error").is_none(), "{applied}");
    artist.seed = 42;
    assert_eq!(
        Construction::from_document(&p.snapshot().state().documents[&id]).unwrap(),
        artist
    );
}

#[test]
fn preparation_capacity_failure_retains_existing_generations() {
    let (mut dispatcher, mut project, id) = setup();
    let base = project.snapshot().revision();
    let mut handles = vec![];
    for _ in 0..fold_motion::candidates::MAX_LIVE {
        handles.push(propose(&mut dispatcher, &mut project, id));
    }
    let request_guard = guard(&dispatcher, &project, id);
    let rejected = call(
        &mut dispatcher,
        &mut project,
        json!({
            "method":"propose", "guard":request_guard,
            "edit":{"kind":"patches","patches":[{
                "path":[{"kind":"field","value":"seed"}],
                "expected":{"kind":"present","value":0},
                "value":{"kind":"present","value":99}
            }]}
        }),
    );
    assert_eq!(rejected["error"]["code"], "capacity");
    assert_eq!(project.snapshot().revision(), base);
    for handle in handles {
        let result = call(
            &mut dispatcher,
            &mut project,
            json!({"method":"inspect_candidate", "handle":handle}),
        );
        assert!(result.get("error").is_none(), "{result}");
        assert_eq!(result["result"]["handle"], handle);
    }
}

#[test]
fn document_inspection_retains_the_requested_revision_while_artist_edits() {
    let (mut dispatcher, mut project, id) = setup();
    let snapshot = project.snapshot();
    let mut construction = Construction::from_document(&snapshot.state().documents[&id]).unwrap();
    let expected = fold_motion::source::canonical(&construction).unwrap();
    let epoch = dispatcher.epoch();
    let accepted = raw(
        &mut dispatcher,
        &mut project,
        json!({
            "method":"inspect_document", "epoch":epoch, "revision":snapshot.revision().0,
            "scope":{"document":id,"definition":null}
        }),
    );
    assert_eq!(accepted["result"]["state"], "running");
    construction.seed = 51;
    fold_motion::editing::prepare(
        &snapshot,
        id,
        fold_motion::editing::Scope::Document,
        construction,
    )
    .unwrap()
    .apply(&mut project)
    .unwrap();
    let result = call(
        &mut dispatcher,
        &mut project,
        json!({
            "method":"job_status", "handle":accepted["result"]["job"]
        }),
    );
    assert_eq!(result["result"]["revision"], snapshot.revision().0);
    assert_eq!(result["result"]["source"], expected);
    assert_ne!(project.snapshot().revision(), snapshot.revision());
}

#[test]
fn native_validation_codes_and_identities_survive_source_and_patch_jobs() {
    use fold_motion::{binding::Binding, model::Reference, types::Literal};
    let (mut dispatcher, mut project, document) = setup();
    let snapshot = project.snapshot();
    let before = Construction::from_document(&snapshot.state().documents[&document]).unwrap();
    let definition = &before.definitions[0];
    let node = definition.nodes[0].id;
    let missing = ObjectId::new();
    for (code, target) in [
        ("cycle", node),
        ("missing_resource", missing),
        ("unsupported", node),
        ("type", node),
    ] {
        let mut after = before.clone();
        let shape = &mut after.definitions[0].nodes[0];
        match code {
            "cycle" | "missing_resource" => {
                shape.parameters.get_mut("height").unwrap().value =
                    serde_json::to_value(Binding::Link {
                        reference: Reference::Node { node: target }.into(),
                    })
                    .unwrap()
            }
            "unsupported" => shape.version = u32::MAX,
            "type" => {
                shape.parameters.get_mut("height").unwrap().value =
                    serde_json::to_value(Binding::Literal {
                        value: Literal::Bool { value: false },
                    })
                    .unwrap()
            }
            _ => unreachable!(),
        }
        let mut edits = vec![
            json!({"kind":"patches","patches":[{"path":[{"kind":"field","value":"definitions"},{"kind":"element","value":definition.id}],"expected":{"kind":"present","value":definition},"value":{"kind":"present","value":after.definitions[0]}}]}),
        ];
        if code == "missing_resource" {
            // Canonical Source requires scope-resolved symbolic references.
            assert!(fold_motion::source::canonical(&after).is_err());
        } else {
            edits.push(json!({"kind":"source","expected":fold_motion::source::canonical(&before).unwrap(),"text":fold_motion::source::canonical(&after).unwrap(),"mapping":{}}));
        }
        for edit in edits {
            let request_guard = guard(&dispatcher, &project, document);
            let result = call(
                &mut dispatcher,
                &mut project,
                json!({"method":"propose","guard":request_guard,"edit":edit}),
            );
            assert_eq!(result["error"]["code"], code, "{result}");
            assert!(
                result["error"]["details"]["native_ids"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(target)),
                "{result}"
            );
            assert_eq!(project.snapshot().revision(), snapshot.revision());
            assert_eq!(
                project.snapshot().state().documents[&document],
                snapshot.state().documents[&document]
            );
        }
    }
}

#[test]
fn structural_patch_failures_preserve_cycle_and_missing_definition_diagnostics() {
    let (mut dispatcher, mut project, document) = setup();
    let snapshot = project.snapshot();
    let before = Construction::from_document(&snapshot.state().documents[&document]).unwrap();
    let mut recursive = before.definitions[0].clone();
    let mut instance = before.root.clone();
    instance.id = ObjectId::new();
    recursive.instances.push(instance);
    let group = ObjectId::new();
    let missing = ObjectId::new();
    let field = |value: &str| json!({"kind":"field","value":value});
    let cases = [
        (
            "cycle",
            recursive.id,
            json!({"path":[field("definitions"),{"kind":"element","value":recursive.id}],"expected":{"kind":"present","value":before.definitions[0]},"value":{"kind":"present","value":recursive}}),
        ),
        (
            "cycle",
            group,
            json!({"path":[field("presentation"),field("groups")],"expected":{"kind":"present","value":[]},"value":{"kind":"present","value":[{"id":group,"name":"Recursive group","members":[group],"locked":false}]}}),
        ),
        (
            "missing_resource",
            missing,
            json!({"path":[field("root"),field("definition")],"expected":{"kind":"present","value":before.root.definition},"value":{"kind":"present","value":missing}}),
        ),
    ];
    for (code, id, patch) in cases {
        let request_guard = guard(&dispatcher, &project, document);
        let result = call(
            &mut dispatcher,
            &mut project,
            json!({"method":"propose","guard":request_guard,"edit":{"kind":"patches","patches":[patch]}}),
        );
        assert_eq!(result["error"]["code"], code, "{result}");
        assert!(
            result["error"]["details"]["native_ids"]
                .as_array()
                .unwrap()
                .contains(&json!(id)),
            "{result}"
        );
        assert_eq!(project.snapshot().revision(), snapshot.revision());
        assert_eq!(
            project.snapshot().state().documents[&document],
            snapshot.state().documents[&document]
        );
    }
}

#[test]
fn source_structural_failure_retains_native_cause_and_source_span() {
    let (mut dispatcher, mut project, document) = setup();
    let snapshot = project.snapshot();
    let before = Construction::from_document(&snapshot.state().documents[&document]).unwrap();
    let expected = fold_motion::source::canonical(&before).unwrap();
    let group = ObjectId::new();
    let mut presentation = serde_json::to_value(&before.presentation).unwrap();
    presentation["groups"] =
        json!([{"id":group,"name":"Recursive group","members":[group],"locked":false}]);
    let original = format!(
        "presentation {};",
        serde_json::to_string(&before.presentation).unwrap()
    );
    let replacement = format!("presentation {presentation};");
    assert!(expected.contains(&original));
    let text = expected.replace(&original, &replacement);
    let request_guard = guard(&dispatcher, &project, document);
    let result = call(
        &mut dispatcher,
        &mut project,
        json!({"method":"propose","guard":request_guard,"edit":{"kind":"source","expected":expected,"text":text,"mapping":{}}}),
    );
    assert_eq!(result["error"]["code"], "cycle", "{result}");
    assert!(
        result["error"]["details"]["native_ids"]
            .as_array()
            .unwrap()
            .contains(&json!(group))
    );
    let diagnostic = &result["error"]["details"]["diagnostics"][0];
    assert_eq!(diagnostic["native_id"], json!(group));
    let start = diagnostic["start"].as_u64().unwrap() as usize;
    let end = diagnostic["end"].as_u64().unwrap() as usize;
    let marked = &text[start..end];
    assert!(marked.contains(&group.to_string()), "{marked}");
    assert!(marked.contains("Recursive group"), "{marked}");
    assert_eq!(snapshot.revision(), project.snapshot().revision());
    assert_eq!(
        snapshot.state().documents[&document],
        project.snapshot().state().documents[&document]
    );
}

#[test]
fn expired_apply_captures_unchanged_frozen_construction() {
    let before = fold_motion::source::parse(include_str!(
        "../../../docs/rewrite/fixtures/animated-shape.foldsrc"
    ))
    .unwrap()
    .construction;
    let document = DocumentId::new();
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(before.document(document).unwrap())],
        })
        .unwrap();
    let mut dispatcher = Dispatcher::default();
    let revision = project.snapshot().revision();
    let mut proposed = before.clone();
    proposed.seed += 1;
    let request_guard = guard(&dispatcher, &project, document);
    let proposal = call(
        &mut dispatcher,
        &mut project,
        json!({"method":"propose","guard":request_guard,
        "edit":{"kind":"source","expected":fold_motion::source::canonical(&before).unwrap(),
        "text":fold_motion::source::canonical(&proposed).unwrap(),"mapping":{}}}),
    );
    assert!(proposal.get("error").is_none(), "{proposal}");
    let handle = proposal["result"]["handle"].clone();
    let inspected = call(
        &mut dispatcher,
        &mut project,
        json!({"method":"inspect_candidate","handle":handle}),
    );
    assert!(inspected.get("error").is_none(), "{inspected}");
    let request_guard = guard(&dispatcher, &project, document);
    let request = serde_json::to_vec(
        &json!({"version":2,"command":{"method":"apply","guard":request_guard,
        "handle":handle,"receipt":inspected["result"]["receipt"]}}),
    )
    .unwrap();
    let future =
        Instant::now() + fold_motion::candidates::IDLE_LIFETIME + std::time::Duration::from_secs(1);
    let response: Value =
        serde_json::from_slice(&dispatcher.dispatch(&mut project, &request, future)).unwrap();
    assert_eq!(response["error"]["code"], "expired", "{response}");
    let after =
        Construction::from_document(&project.snapshot().state().documents[&document]).unwrap();
    assert_eq!(after, before);
    assert_eq!(project.snapshot().revision(), revision);
    let retry: Value =
        serde_json::from_slice(&dispatcher.dispatch(&mut project, &request, future)).unwrap();
    assert_eq!(retry, response);
    assert_eq!(project.snapshot().revision(), revision);
    if let Some(directory) = std::env::var_os("FOLD_SLICE_CAPTURE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("expiry-evidence.json"), serde_json::to_vec_pretty(&json!({
            "capture_scope":"version-2 dispatcher with explicit monotonic clock; not elapsed CLI/MCP transport time",
            "source_revision":env!("FOLD_SOURCE_SHA256"),
            "build":{"target":env!("FOLD_BUILD_TARGET"),"profile":env!("FOLD_BUILD_PROFILE"),"package":env!("CARGO_PKG_VERSION")},
            "expired_candidate":{"code":response["error"]["code"],"before":before,"after":after},
            "response":response,"retry":retry,
        })).unwrap()).unwrap();
    }
}
