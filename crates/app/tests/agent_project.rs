use fold_app::agent::dispatch::Dispatcher;
use fold_foundation::{DocumentId, ObjectId};
use fold_project::Project;
use serde_json::{Value, json};
use std::time::Instant;
fn guard(d: &Dispatcher, p: &Project, id: DocumentId) -> Value {
    json!({"epoch":d.epoch(),"request_id":ObjectId::new(),"base":p.snapshot().revision().0,"scope":{"document":id}})
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

fn create(d: &mut Dispatcher, p: &mut Project) -> (DocumentId, Value) {
    let id = DocumentId::new();
    let g = guard(d, p, id);
    let before = p.snapshot();
    let proposed = call(
        d,
        p,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
            {"op":"create","document":id,"type_id":fold_composition::COMPOSITION,"name":"Composition"},
            {"op":"add_layer","composition":id,"kind":"motion"},
            {"op":"add_layer","composition":id,"kind":"image"}
        ]}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    assert_eq!(p.snapshot().state(), before.state());
    (id, proposed["result"]["handle"].clone())
}
fn apply(d: &mut Dispatcher, p: &mut Project, id: DocumentId, handle: Value) {
    let inspected = call(d, p, json!({"method":"inspect_candidate","handle":handle}));
    assert!(inspected.get("error").is_none(), "{inspected}");
    let g = guard(d, p, id);
    let result = call(
        d,
        p,
        json!({"method":"apply","guard":g,"handle":handle,"receipt":inspected["result"]["receipt"]}),
    );
    assert!(result.get("error").is_none(), "{result}");
}
fn inspected(d: &mut Dispatcher, p: &mut Project, id: DocumentId) -> Value {
    let epoch = d.epoch();
    let revision = p.snapshot().revision().0;
    let result = call(
        d,
        p,
        json!({"method":"inspect_document","epoch":epoch,"revision":revision,"scope":{"document":id}}),
    );
    assert!(result.get("error").is_none(), "{result}");
    serde_json::from_str(result["result"]["source"].as_str().unwrap()).unwrap()
}
#[test]
fn public_mixed_creation_is_inspected_atomic_undoable_and_preserved_on_reopen() {
    let mut d = Dispatcher::default();
    let mut p = Project::new(16);
    let (id, handle) = create(&mut d, &mut p);
    apply(&mut d, &mut p, id, handle);
    assert_eq!(p.snapshot().state().documents.len(), 3);
    let state = p.snapshot();
    p.undo().unwrap();
    assert!(p.snapshot().state().documents.is_empty());
    p.redo().unwrap();
    assert_eq!(p.snapshot().state().documents.len(), 3);
    for (id, document) in &state.state().documents {
        assert_eq!(p.snapshot().state().documents[id].payload, document.payload);
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scene.fold");
    let g =
        json!({"epoch":d.epoch(),"request_id":ObjectId::new(),"base":p.snapshot().revision().0});
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"save_project_copy","guard":g,"path":path}),
    );
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(
        fold_project::load(&path, 0).unwrap().snapshot().state(),
        p.snapshot().state()
    );
}
#[test]
fn explicit_layer_connections_use_discovered_ports_and_one_undo_entry() {
    let mut d = Dispatcher::default();
    let mut p = Project::new(16);
    let (id, handle) = create(&mut d, &mut p);
    apply(&mut d, &mut p, id, handle);
    let composition = inspected(&mut d, &mut p, id);
    let layers = composition["native"]["layers"].as_array().unwrap();
    let target = layers[0]["id"].clone();
    let source = layers[1]["id"].clone();
    let graph: DocumentId =
        serde_json::from_value(layers[0]["source"]["Output"]["document"].clone()).unwrap();
    let input = inspected(&mut d, &mut p, graph)["image_inputs"][0]["id"].clone();
    assert!(input.is_string());
    let before = p.snapshot();
    let g = guard(&d, &p, id);
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
            {"op":"connect_layers","composition":id,"source":source,"target":target,"port":input}
        ]}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    apply(&mut d, &mut p, id, proposed["result"]["handle"].clone());
    let connected = inspected(&mut d, &mut p, graph);
    assert_eq!(connected["image_inputs"][0]["id"], input);
    assert_eq!(connected["image_inputs"][0]["source"], layers[1]["source"]);
    assert_eq!(
        p.snapshot().state().documents[&id].payload,
        before.state().documents[&id].payload
    );
    p.undo().unwrap();
    assert_eq!(
        p.snapshot().state().documents[&graph].payload,
        before.state().documents[&graph].payload
    );
}

#[test]
fn rebase_rejects_cycles_introduced_by_concurrent_layer_connections() {
    let mut d = Dispatcher::default();
    let mut p = Project::new(16);
    let (id, handle) = create(&mut d, &mut p);
    apply(&mut d, &mut p, id, handle);
    let g = guard(&d, &p, id);
    let added = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
            {"op":"add_layer","composition":id,"kind":"image"}
        ]}}),
    );
    assert!(added.get("error").is_none(), "{added}");
    apply(&mut d, &mut p, id, added["result"]["handle"].clone());
    let composition = inspected(&mut d, &mut p, id);
    let layers = composition["native"]["layers"].as_array().unwrap();
    let a = layers[0]["id"].clone();
    let b = layers[1]["id"].clone();
    let mut ports = Vec::new();
    for layer in &layers[..2] {
        let graph = serde_json::from_value(layer["source"]["Output"]["document"].clone()).unwrap();
        ports.push(inspected(&mut d, &mut p, graph)["image_inputs"][0]["id"].clone());
    }
    let g = guard(&d, &p, id);
    let pending = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
            {"op":"connect_layers","composition":id,"source":a,"target":b,"port":ports[1]}
        ]}}),
    );
    assert!(pending.get("error").is_none(), "{pending}");
    let g = guard(&d, &p, id);
    let concurrent = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
            {"op":"connect_layers","composition":id,"source":b,"target":a,"port":ports[0]}
        ]}}),
    );
    assert!(concurrent.get("error").is_none(), "{concurrent}");
    apply(&mut d, &mut p, id, concurrent["result"]["handle"].clone());
    let before = p.snapshot();
    let g = guard(&d, &p, id);
    let rebased = call(
        &mut d,
        &mut p,
        json!({"method":"rebase","guard":g,"handle":pending["result"]["handle"]}),
    );
    assert!(
        rebased.get("error").is_some(),
        "Rebase accepted a dependency cycle: {rebased}"
    );
    assert_eq!(rebased["error"]["code"], "conflict", "{rebased}");
    assert_eq!(
        rebased["error"]["message"], "Proposal dependency changed",
        "{rebased}"
    );
    assert_eq!(p.snapshot().state(), before.state());
    let retained = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":pending["result"]["handle"]}),
    );
    assert!(retained.get("error").is_none(), "{retained}");
}

#[test]
fn public_compositor_source_rebases_disjoint_controls_and_requires_fresh_inspection() {
    let mut d = Dispatcher::default();
    let mut p = Project::new(16);
    let (id, handle) = create(&mut d, &mut p);
    apply(&mut d, &mut p, id, handle);
    let snapshot = p.snapshot();
    let original = snapshot
        .state()
        .documents
        .values()
        .find(|v| v.type_id == fold_compositor::COMPOSITE)
        .unwrap()
        .clone();
    let document = original.id;
    let base = fold_compositor::Composite::from_document(&original).unwrap();
    let node = base.nodes[0].id;
    let expected = base.source(Some(&[node])).unwrap();
    let mut fragment: Value = serde_json::from_str(&expected).unwrap();
    fragment[0]["operation"]["name"] = json!("Agent source");
    let g = guard(&d, &p, document);
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[{"op":"compositor_source","document":document,"nodes":[node],"expected":expected,"source":serde_json::to_string(&fragment).unwrap()}]}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    let mut artist = base.clone();
    artist.nodes[0].position = Some([42., 17.]);
    p.commit(fold_project::EditBatch {
        base: p.snapshot().revision(),
        mutations: vec![fold_project::Mutation::PutDocument(
            artist.document(document).unwrap(),
        )],
    })
    .unwrap();
    let g = guard(&d, &p, document);
    let rebased = call(
        &mut d,
        &mut p,
        json!({"method":"rebase","guard":g,"handle":proposed["result"]["handle"]}),
    );
    assert!(rebased.get("error").is_none(), "{rebased}");
    apply(
        &mut d,
        &mut p,
        document,
        rebased["result"]["handle"].clone(),
    );
    let merged =
        fold_compositor::Composite::from_document(&p.snapshot().state().documents[&document])
            .unwrap();
    assert_eq!(merged.nodes[0].name, "Agent source");
    assert_eq!(merged.nodes[0].position, Some([42., 17.]));
    p.undo().unwrap();
    assert_eq!(
        fold_compositor::Composite::from_document(&p.snapshot().state().documents[&document])
            .unwrap(),
        artist
    );
    p.redo().unwrap();
    assert_eq!(
        fold_compositor::Composite::from_document(&p.snapshot().state().documents[&document])
            .unwrap(),
        merged
    );
}

#[test]
fn compositor_source_retains_its_provider_budget_above_sixty_four_kibibytes() {
    let mut dispatcher = Dispatcher::default();
    let mut project = Project::new(8);
    let document = DocumentId::new();
    let g = guard(&dispatcher, &project, document);
    let created = call(
        &mut dispatcher,
        &mut project,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[{"op":"create","document":document,"type_id":fold_compositor::COMPOSITE,"name":"Source budget"}]}}),
    );
    assert!(created.get("error").is_none(), "{created}");
    apply(
        &mut dispatcher,
        &mut project,
        document,
        created["result"]["handle"].clone(),
    );
    let before = inspected(&mut dispatcher, &mut project, document);
    let expected = before["graph_source"].as_str().unwrap();
    let mut nodes: Value = serde_json::from_str(expected).unwrap();
    nodes[0]["position"] = json!([7.0, 9.0]);
    let source = format!(
        "{}{}",
        serde_json::to_string(&nodes).unwrap(),
        " ".repeat(70_000)
    );
    let g = guard(&dispatcher, &project, document);
    let proposed = call(
        &mut dispatcher,
        &mut project,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[{"op":"compositor_source","document":document,"nodes":null,"expected":expected,"source":source}]}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    apply(
        &mut dispatcher,
        &mut project,
        document,
        proposed["result"]["handle"].clone(),
    );
    let after = inspected(&mut dispatcher, &mut project, document);
    let nodes: Value = serde_json::from_str(after["graph_source"].as_str().unwrap()).unwrap();
    assert_eq!(nodes[0]["position"], json!([7.0, 9.0]));
}
