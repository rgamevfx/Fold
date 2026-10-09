use fold_app::agent::dispatch::Dispatcher;
use fold_foundation::{DocumentId, ObjectId};
use fold_project::Project;
use serde_json::{Value, json};
use std::time::Instant;
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

fn create(d: &mut Dispatcher, p: &mut Project) -> (DocumentId, Value) {
    let id = DocumentId::new();
    let g = guard(d, p, id);
    let before = p.snapshot();
    let proposed = call(
        d,
        p,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
            {"op":"create","document":id,"type_id":fold_composition::COMPOSITION,"name":"Composition"},
            {"op":"add_layer","composition":id,"kind":"scene"},
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
        json!({"method":"inspect_document","epoch":epoch,"revision":revision,"scope":{"document":id,"definition":null}}),
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
fn public_scene_hierarchy_rejects_stale_values_unlocks_and_partial_edits() {
    let mut d = Dispatcher::default();
    let mut p = Project::new(16);
    let (id, handle) = create(&mut d, &mut p);
    apply(&mut d, &mut p, id, handle);
    let scene = p
        .snapshot()
        .state()
        .documents
        .values()
        .find(|v| v.type_id == fold_motion::scene::TYPE)
        .unwrap()
        .id;
    let expected = inspected(&mut d, &mut p, scene);
    let g = guard(&d, &p, scene);
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
            {"op":"hierarchy","document":scene,"expected":expected,"edits":[{"op":"create","kind":"sphere"}]}
        ]}}),
    );
    assert!(result.get("error").is_none(), "{result}");
    apply(&mut d, &mut p, scene, result["result"]["handle"].clone());
    let current = inspected(&mut d, &mut p, scene);
    assert_ne!(expected, current);
    let record =
        fold_motion::scene::Scene::from_document(&p.snapshot().state().documents[&scene]).unwrap();
    let object = record.objects.last().unwrap().id;
    for (expected, edits) in [
        (
            expected,
            json!([{"op":"rename","object":object,"name":"Stale"}]),
        ),
        (
            current,
            json!([{"op":"rename","object":object,"name":"Forbidden"},{"op":"lock","object":object,"value":false}]),
        ),
    ] {
        let before = p.snapshot();
        let g = guard(&d, &p, scene);
        let result = call(
            &mut d,
            &mut p,
            json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
                {"op":"hierarchy","document":scene,"expected":expected,"edits":edits}
            ]}}),
        );
        assert!(result.get("error").is_some(), "{result}");
        assert_eq!(before.state(), p.snapshot().state());
    }
}

#[test]
fn scoped_object_source_rebases_disjoint_artist_edits_and_honors_new_locks() {
    let mut d = Dispatcher::default();
    let mut p = Project::new(16);
    let (id, handle) = create(&mut d, &mut p);
    apply(&mut d, &mut p, id, handle);
    let scene = p
        .snapshot()
        .state()
        .documents
        .values()
        .find(|v| v.type_id == fold_motion::scene::TYPE)
        .unwrap()
        .id;
    let original = p.snapshot().state().documents[&scene].clone();
    let record = fold_motion::scene::Scene::from_document(&original).unwrap();
    let object = record.objects[0].id;
    let scope = json!({"document":scene,"definition":null,"object":object});
    let epoch = d.epoch();
    let revision = p.snapshot().revision().0;
    let inspection = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_document","epoch":epoch,"revision":revision,"scope":scope}),
    );
    let expected = inspection["result"]["source"].as_str().unwrap();
    let mut graph = fold_motion::source::parse(expected).unwrap().construction;
    graph.seed += 1;
    let mut g = guard(&d, &p, scene);
    g["scope"] = scope.clone();
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":g,"edit":{"kind":"source","expected":expected,"text":fold_motion::source::canonical(&graph).unwrap(),"mapping":{}}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    let handle = proposed["result"]["handle"].clone();
    let mut artist = record.clone();
    artist.objects[0].name = "Artist camera".into();
    p.commit(fold_project::EditBatch {
        base: p.snapshot().revision(),
        mutations: vec![fold_project::Mutation::PutDocument(
            artist.replace(&original).unwrap(),
        )],
    })
    .unwrap();
    let mut g = guard(&d, &p, scene);
    g["scope"] = scope.clone();
    let rebased = call(
        &mut d,
        &mut p,
        json!({"method":"rebase","guard":g,"handle":handle}),
    );
    assert!(rebased.get("error").is_none(), "{rebased}");
    let next = rebased["result"]["handle"].clone();
    let inspection = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":next}),
    );
    let parsed =
        fold_motion::source::parse(inspection["result"]["source"].as_str().unwrap()).unwrap();
    assert_eq!(parsed.construction.seed, graph.seed);
    // A fresh artist lock after preparation must invalidate rebasing, without
    // retiring either immutable generation or editing the artist's scene.
    let current = p.snapshot().state().documents[&scene].clone();
    artist.objects[0].locked = true;
    p.commit(fold_project::EditBatch {
        base: p.snapshot().revision(),
        mutations: vec![fold_project::Mutation::PutDocument(
            artist.replace(&current).unwrap(),
        )],
    })
    .unwrap();
    let mut g = guard(&d, &p, scene);
    g["scope"] = scope;
    let locked = call(
        &mut d,
        &mut p,
        json!({"method":"rebase","guard":g,"handle":next}),
    );
    assert_eq!(locked["error"]["code"], "conflict", "{locked}");
    let retained = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":next}),
    );
    assert!(retained.get("error").is_none(), "{retained}");
    assert_eq!(
        fold_motion::scene::Scene::from_document(&p.snapshot().state().documents[&scene])
            .unwrap()
            .objects[0]
            .graph
            .seed,
        record.objects[0].graph.seed
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
    assert_eq!(rebased["error"]["message"], "Cycle", "{rebased}");
    assert_eq!(p.snapshot().state(), before.state());
    let retained = call(
        &mut d,
        &mut p,
        json!({"method":"inspect_candidate","handle":pending["result"]["handle"]}),
    );
    assert!(retained.get("error").is_none(), "{retained}");
}

#[test]
fn replacing_a_scene_output_cannot_leave_existing_consumers_dangling() {
    let mut d = Dispatcher::default();
    let mut p = Project::new(16);
    let (id, handle) = create(&mut d, &mut p);
    apply(&mut d, &mut p, id, handle);
    let scene = p
        .snapshot()
        .state()
        .documents
        .values()
        .find(|document| document.type_id == fold_motion::scene::TYPE)
        .unwrap()
        .id;
    let record = inspected(&mut d, &mut p, scene);
    let mut next = record["native"].clone();
    next["output"] = json!(ObjectId::new());
    let before = p.snapshot();
    let g = guard(&d, &p, scene);
    let proposed = call(
        &mut d,
        &mut p,
        json!({
            "method":"propose","guard":g,"edit":{"kind":"project","edits":[
                {"op":"author","document":scene,"expected":record,"edits":[
                    {"op":"replace_scene","expected":record["native"],"value":next}
                ]}
            ]}
        }),
    );
    assert!(
        proposed.get("error").is_some(),
        "Accepted a dangling Composition source: {proposed}"
    );
    assert_eq!(p.snapshot().state(), before.state());
}

#[test]
fn rebase_revalidates_named_outputs_after_an_artist_replaces_a_publication() {
    let mut d = Dispatcher::default();
    let mut p = Project::new(16);
    let (id, handle) = create(&mut d, &mut p);
    apply(&mut d, &mut p, id, handle);
    let composition = inspected(&mut d, &mut p, id);
    let layers = composition["native"]["layers"].as_array().unwrap();
    let graph = serde_json::from_value(layers[0]["source"]["Output"]["document"].clone()).unwrap();
    let port = inspected(&mut d, &mut p, graph)["image_inputs"][0]["id"].clone();
    let g = guard(&d, &p, id);
    let pending = call(
        &mut d,
        &mut p,
        json!({
            "method":"propose","guard":g,"edit":{"kind":"project","edits":[
                {"op":"connect_layers","composition":id,"source":layers[1]["id"],"target":layers[0]["id"],"port":port}
            ]}
        }),
    );
    assert!(pending.get("error").is_none(), "{pending}");
    // The artist changes a scene publication and its existing Composition use
    // together. The unpublished candidate still refers to the old output.
    let mut root =
        fold_composition::Composition::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let Some(fold_platform::image_source::Source::Output(source)) = &mut root.layers[1].source
    else {
        panic!()
    };
    let document = p.snapshot().state().documents[&source.document].clone();
    let mut scene = fold_motion::scene::Scene::from_document(&document).unwrap();
    scene.output = ObjectId::new();
    source.output = scene.output.to_string();
    p.commit(fold_project::EditBatch {
        base: p.snapshot().revision(),
        mutations: vec![
            fold_project::Mutation::PutDocument(scene.replace(&document).unwrap()),
            fold_project::Mutation::PutDocument(root.document(id).unwrap()),
        ],
    })
    .unwrap();
    let before = p.snapshot();
    let g = guard(&d, &p, id);
    let rebased = call(
        &mut d,
        &mut p,
        json!({
            "method":"rebase","guard":g,"handle":pending["result"]["handle"]
        }),
    );
    assert!(
        rebased.get("error").is_some(),
        "Accepted a removed source output: {rebased}"
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
