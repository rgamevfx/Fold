use fold_app::agent::dispatch::Dispatcher;
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::{Binding, Interpolation, Key},
    model::Construction,
    types::{Literal, Unit},
};
use fold_project::{EditBatch, Mutation, Project};
use serde_json::{Value, json};
use std::time::Instant;
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
    if result["result"]["state"] == "running" {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "job did not complete");
            result = raw(d, p, json!({"method":"job_status","handle":job}));
            if result["result"]["state"] != "running" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    result
}

fn setup() -> (Dispatcher, Project, DocumentId, ObjectId) {
    let id = DocumentId::new();
    let base = Construction::from_document(&fold_motion::package::create(id).unwrap()).unwrap();
    let mut animated = fold_motion::keyed::animate(
        &base,
        base.definitions[0].nodes[0].parameters["height"].id,
        Time::ZERO,
    )
    .unwrap();
    let parameter = animated.definitions[0]
        .nodes
        .last_mut()
        .unwrap()
        .parameters
        .get_mut("track")
        .unwrap();
    let Binding::Track { mut track } = parameter.binding().unwrap() else {
        panic!("native track")
    };
    track.keys.push(Key {
        handles: Default::default(),
        id: ObjectId::new(),
        time: Time::new(1, 1).unwrap(),
        value: Literal::Distance {
            value: 200.,
            unit: Unit::Canvas,
        },
        interpolation: Interpolation::Linear,
        extensions: Default::default(),
    });
    parameter.value = serde_json::to_value(Binding::Track { track }).unwrap();
    let output = animated.definitions[0].outputs[0].id;
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(animated.document(id).unwrap())],
        })
        .unwrap();
    (Dispatcher::default(), project, id, output)
}
#[test]
fn committed_observations_keep_exact_time_ids_bounds_and_snapshot_provenance() {
    let (mut d, mut p, id, output) = setup();
    let snapshot = p.snapshot();
    let epoch = d.epoch();
    let times = vec![
        Time::new(-1, 1).unwrap(),
        Time::new(1, 2).unwrap(),
        Time::new(2, 1).unwrap(),
    ];
    let request = json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":snapshot.revision().0,"document":id},"output":output,"times":times});
    let observed = call(&mut d, &mut p, request.clone());
    assert!(observed.get("error").is_none(), "{observed}");
    assert_eq!(observed, call(&mut d, &mut p, request));
    let data = &observed["result"];
    let samples = data["samples"].as_array().unwrap();
    let mut ids = None;
    for (index, height) in [100., 150., 200.].into_iter().enumerate() {
        let sample = &samples[index];
        assert_eq!(sample["time"], json!(times[index]));
        assert_eq!(sample["counts"]["unique_points"], 4);
        assert_eq!(sample["counts"]["placements"], 1);
        let geometry = sample["geometries"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap();
        assert_eq!(geometry["bounds"], json!([[0., 0.], [200., height]]));
        if let Some(ids) = &ids {
            assert_eq!(&geometry["contours"][0]["point_ids"], ids);
        } else {
            ids = Some(geometry["contours"][0]["point_ids"].clone());
        }
    }
    let provenance = &data["provenance"];
    assert_eq!(provenance["epoch"], json!(epoch));
    assert_eq!(provenance["revision"], json!(snapshot.revision()));
    assert_eq!(provenance["frames_available"], false);
    assert!(provenance["camera"].is_null());
    for name in ["construction", "assets", "fonts", "configuration"] {
        assert_eq!(provenance["fingerprints"][name].as_str().unwrap().len(), 64);
    }
    assert_eq!(p.snapshot().revision(), snapshot.revision());
    let mut stale = json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":0,"document":id},"output":output,"times":[Time::ZERO]});
    assert_eq!(
        call(&mut d, &mut p, stale.clone())["error"]["code"],
        "conflict"
    );
    stale["target"]["revision"] = json!(snapshot.revision());
    stale["times"] = json!([]);
    assert_eq!(call(&mut d, &mut p, stale)["error"]["code"], "budget");
}
#[test]
fn candidate_observation_does_not_inspect_or_commit_the_proposal() {
    let (mut d, mut p, id, output) = setup();
    let before = p.snapshot();
    let epoch = d.epoch();
    let guard = || json!({"epoch":epoch,"request_id":ObjectId::new(),"base":before.revision(),"scope":{"document":id,"definition":null}});
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":guard(),"edit":{"kind":"patches","patches":[{"path":[{"kind":"field","value":"seed"}],"expected":{"kind":"present","value":0},"value":{"kind":"present","value":123}}]}}),
    );
    let handle = proposed["result"]["handle"].clone();
    assert!(!handle.is_null(), "{proposed}");
    let observed = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"candidate","handle":handle},"output":output,"times":[Time::new(1,2).unwrap()]}),
    );
    assert_eq!(observed["result"]["provenance"]["seed"], 123);
    assert_eq!(observed["result"]["provenance"]["candidate"], handle);
    assert!(observed["result"].get("receipt").is_none());
    let apply = call(
        &mut d,
        &mut p,
        json!({"method":"apply","guard":guard(),"handle":handle,"receipt":ObjectId::new()}),
    );
    assert_eq!(apply["error"]["code"], "conflict");
    assert_eq!(p.snapshot().revision(), before.revision());
}

#[test]
fn nonfinite_transformed_bounds_fail_instead_of_serializing_null_measurements() {
    let (mut d, mut p, id, output) = setup();
    let snapshot = p.snapshot();
    let mut value = Construction::from_document(&snapshot.state().documents[&id]).unwrap();
    value.definitions[0].inputs[0].default = Literal::Distance {
        value: f64::MAX,
        unit: Unit::Canvas,
    };
    value.definitions[0].nodes[1]
        .parameters
        .get_mut("position")
        .unwrap()
        .value = serde_json::to_value(Binding::Literal {
        value: Literal::Vector {
            value: vec![f64::MAX, 0.],
            unit: Unit::Canvas,
            space: fold_motion::types::Space::Screen,
        },
    })
    .unwrap();
    fold_motion::editing::prepare(&snapshot, id, fold_motion::editing::Scope::Document, value)
        .unwrap()
        .apply(&mut p)
        .unwrap();
    let revision = p.snapshot().revision();
    let epoch = d.epoch();
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":revision,"document":id},"output":output,"times":[Time::ZERO]}),
    );
    assert_eq!(result["error"]["code"], "type", "{result}");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Non-finite")
    );
    assert!(result.get("result").is_none());
    assert_eq!(p.snapshot().revision(), revision);
}

#[test]
fn asynchronous_observation_retains_its_snapshot_across_artist_edits() {
    let (mut d, mut p, id, output) = setup();
    let epoch = d.epoch();
    let before = p.snapshot();
    let accepted = raw(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":before.revision(),"document":id},"output":output,"times":[Time::ZERO]}),
    );
    assert_eq!(accepted["result"]["state"], "running");
    let mut artist = Construction::from_document(&before.state().documents[&id]).unwrap();
    artist.seed = 321;
    fold_motion::editing::prepare(
        &before,
        id,
        fold_motion::editing::Scope::Document,
        artist.clone(),
    )
    .unwrap()
    .apply(&mut p)
    .unwrap();
    let revision = p.snapshot().revision();
    let finished = call(
        &mut d,
        &mut p,
        json!({"method":"job_status","handle":accepted["result"]["job"]}),
    );
    assert_eq!(
        finished["result"]["provenance"]["revision"],
        json!(before.revision())
    );
    assert_eq!(finished["result"]["provenance"]["seed"], 0);
    assert_eq!(p.snapshot().revision(), revision);
    assert_eq!(
        Construction::from_document(&p.snapshot().state().documents[&id]).unwrap(),
        artist
    );
    let mut replacement = Dispatcher::default();
    let rejected = raw(
        &mut replacement,
        &mut p,
        json!({"method":"job_status","handle":accepted["result"]["job"]}),
    );
    assert_eq!(rejected["error"]["code"], "conflict");
}

#[test]
fn job_cancellation_checks_scope_and_replays_its_original_outcome() {
    let (mut d, mut p, id, output) = setup();
    let epoch = d.epoch();
    let revision = p.snapshot().revision();
    let accepted = raw(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":revision,"document":id},"output":output,"times":[Time::ZERO]}),
    );
    let job = accepted["result"]["job"].clone();
    assert_eq!(accepted["result"]["scope"]["document"], json!(id));
    let bad = raw(
        &mut d,
        &mut p,
        json!({"method":"cancel_job","handle":job,"guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":revision,"scope":{"document":DocumentId::new(),"definition":null}}}),
    );
    assert_eq!(bad["error"]["code"], "conflict");
    let command = json!({"method":"cancel_job","handle":job,"guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":revision,"scope":accepted["result"]["scope"]}});
    let cancellation = raw(&mut d, &mut p, command.clone());
    assert!(matches!(
        cancellation["result"]["state"].as_str(),
        Some("cancellation_requested" | "finished")
    ));
    assert_eq!(raw(&mut d, &mut p, command), cancellation);
    let completed = call(&mut d, &mut p, json!({"method":"job_status","handle":job}));
    if cancellation["result"]["state"] == "cancellation_requested" {
        assert_eq!(completed["error"]["code"], "cancelled");
    } else {
        assert!(completed["result"]["provenance"].is_object());
    }
    assert_eq!(p.snapshot().revision(), revision);
}

#[test]
fn observation_preparation_retains_native_failure_codes_and_ids() {
    use fold_motion::model::Reference;
    for code in ["cycle", "missing_resource", "unsupported", "type"] {
        let (mut dispatcher, mut project, document, output) = setup();
        let snapshot = project.snapshot();
        let mut value =
            Construction::from_document(&snapshot.state().documents[&document]).unwrap();
        let node = &mut value.definitions[0].nodes[0];
        let implicated = if code == "missing_resource" {
            ObjectId::new()
        } else {
            node.id
        };
        match code {
            "cycle" | "missing_resource" => {
                node.parameters.get_mut("height").unwrap().value =
                    serde_json::to_value(Binding::Link {
                        reference: Reference::Node { node: implicated }.into(),
                    })
                    .unwrap()
            }
            "unsupported" => node.version = u32::MAX,
            "type" => {
                node.parameters.get_mut("height").unwrap().value =
                    serde_json::to_value(Binding::Literal {
                        value: Literal::Bool { value: false },
                    })
                    .unwrap()
            }
            _ => unreachable!(),
        }
        // Opaque project persistence may retain unresolved native work. Observing
        // it must report the same semantic cause as proposing that construction.
        project
            .commit(EditBatch {
                base: snapshot.revision(),
                mutations: vec![Mutation::PutDocument(value.document(document).unwrap())],
            })
            .unwrap();
        let before = project.snapshot();
        let epoch = dispatcher.epoch();
        let result = call(
            &mut dispatcher,
            &mut project,
            json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":before.revision().0,"document":document},"output":output,"times":[Time::ZERO]}),
        );
        assert_eq!(result["error"]["code"], code, "{result}");
        assert!(
            result["error"]["details"]["native_ids"]
                .as_array()
                .unwrap()
                .contains(&json!(implicated)),
            "{result}"
        );
        assert_eq!(project.snapshot().revision(), before.revision());
        assert_eq!(
            project.snapshot().state().documents[&document],
            before.state().documents[&document]
        );
    }
}

#[test]
fn runtime_failure_identifies_innermost_operation_and_exact_local_time() {
    let (mut dispatcher, mut project, document, output) = setup();
    let snapshot = project.snapshot();
    let mut value = Construction::from_document(&snapshot.state().documents[&document]).unwrap();
    let shape = value.definitions[0].nodes[0].id;
    let definition = value.root.definition;
    let instance = value.root.id;
    value.definitions[0].inputs[0].default = Literal::Distance {
        value: -1.,
        unit: Unit::Canvas,
    };
    value.root.clock.offset = Time::new(1, 4).unwrap();
    value.root.clock.rate = fold_motion::types::Ratio::new(-2, 1).unwrap();
    let child = value.root.clone();
    let parent = ObjectId::new();
    let child_output = output;
    let output = ObjectId::new();
    value.root.id = ObjectId::new();
    value.root.definition = parent;
    value.root.clock = Default::default();
    let root_instance = value.root.id;
    value.definitions.push(fold_motion::model::Definition {
        id: parent,
        name: "Parent".into(),
        inputs: vec![],
        nodes: vec![],
        instances: vec![child],
        outputs: vec![fold_motion::model::Output {
            id: output,
            name: "main".into(),
            ty: fold_motion::types::Type::Scene,
            value: fold_motion::model::Reference::Instance {
                instance,
                output: child_output,
            }
            .into(),
            extensions: Default::default(),
        }],
        extensions: Default::default(),
    });
    project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(value.document(document).unwrap())],
        })
        .unwrap();
    let before = project.snapshot();
    let epoch = dispatcher.epoch();
    let result = call(
        &mut dispatcher,
        &mut project,
        json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":before.revision().0,"document":document},"output":output,"times":[Time::new(3,4).unwrap()]}),
    );
    assert_eq!(result["error"]["code"], "type", "{result}");
    assert_eq!(
        result["error"]["details"],
        json!({"native_ids":[shape], "definition":definition, "instance_path":[root_instance, instance], "asset_time":Time::new(-1,1).unwrap()})
    );
    assert_eq!(project.snapshot().revision(), before.revision());
    assert_eq!(
        project.snapshot().state().documents[&document],
        before.state().documents[&document]
    );
}

#[test]
fn spatial_observations_publish_active_camera_and_directional_light_controls() {
    use fold_motion::{
        model::{Parameter, Reference},
        registry,
    };
    let (mut d, mut p, id, output) = setup();
    let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let camera = registry::resolve("fold.motion.camera", 1, "perspective")
        .unwrap()
        .node("Camera", Default::default())
        .unwrap();
    let light = registry::resolve("fold.motion.light", 1, "directional")
        .unwrap()
        .node("Sun", Default::default())
        .unwrap();
    let link = |node| Binding::Link {
        reference: Reference::Node { node }.into(),
    };
    let scene = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    scene.mode = "spatial".into();
    scene
        .parameters
        .insert("camera".into(), Parameter::new(link(camera.id)).unwrap());
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.extend([link(camera.id), link(light.id)]);
    scene.parameters.get_mut("children").unwrap().value =
        serde_json::to_value(Binding::List { values }).unwrap();
    c.definitions[0].nodes.extend([camera, light]);
    p.commit(EditBatch {
        base: p.snapshot().revision(),
        mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
    })
    .unwrap();
    let epoch = d.epoch();
    let revision = p.snapshot().revision();
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":revision.0,"document":id},"output":output,"times":[Time::ZERO]}),
    );
    assert!(result.get("error").is_none(), "{result}");
    let scene = &result["result"]["samples"][0]["value"];
    assert_eq!(scene["camera"]["position"], json!([0., 1., 5.]));
    assert_eq!(scene["camera"]["near"], 0.01);
    assert_eq!(
        scene["camera"]["projection"]["vertical_fov"],
        std::f64::consts::FRAC_PI_4
    );
    assert_eq!(scene["camera"]["id"], scene["cameras"][0]["camera"]["id"]);
    assert_eq!(scene["lights"][0]["light"]["illuminance"], 1000.);
    assert_eq!(scene["lights"][0]["light"]["color"], json!([1., 1., 1.]));
    assert_eq!(result["result"]["provenance"]["frames_available"], false);
    assert_eq!(p.snapshot().revision(), revision);

    let expected = fold_motion::source::canonical(&c).unwrap();
    let camera = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.camera")
        .unwrap();
    camera.mode = "orthographic".into();
    camera.parameters.remove("vertical_fov");
    camera.parameters.insert(
        "vertical_size".into(),
        Parameter::new(Binding::Literal {
            value: Literal::Distance {
                value: 3.,
                unit: Unit::Meters,
            },
        })
        .unwrap(),
    );
    let proposed = call(
        &mut d,
        &mut p,
        json!({
            "method":"propose",
            "guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":revision.0,"scope":{"document":id,"definition":null}},
            "edit":{"kind":"source","expected":expected,"text":fold_motion::source::canonical(&c).unwrap(),"mapping":{}}
        }),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    let observed = call(
        &mut d,
        &mut p,
        json!({
            "method":"observe","target":{"kind":"candidate","handle":proposed["result"]["handle"]},
            "output":output,"times":[Time::ZERO]
        }),
    );
    assert!(observed.get("error").is_none(), "{observed}");
    let camera = &observed["result"]["samples"][0]["value"]["camera"];
    assert_eq!(camera["id"], scene["camera"]["id"]);
    assert_eq!(
        camera["projection"],
        json!({"kind":"orthographic","vertical_size":3.})
    );
    assert_eq!(
        p.snapshot().revision(),
        revision,
        "candidate observation is not Apply"
    );
}

#[test]
fn observations_keep_styling_per_placement_and_share_geometry_measurements() {
    use fold_motion::{
        model::{Parameter, Reference},
        registry,
        types::{Alpha, Encoding},
    };
    let (mut d, mut p, id, output) = setup();
    let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let definition = &mut c.definitions[0];
    let shape = definition
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.shape")
        .unwrap()
        .id;
    let link = |node| Binding::Link {
        reference: Reference::Node { node }.into(),
    };
    let fill = registry::resolve("fold.motion.fill", 1, "solid")
        .unwrap()
        .node(
            "Fill",
            [
                ("geometry".into(), link(shape)),
                (
                    "color".into(),
                    Binding::Literal {
                        value: Literal::Color {
                            value: [0.8, 0.1, 0.05, 0.5],
                            encoding: Encoding::LinearRec709,
                            alpha: Alpha::Straight,
                        },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let stroke = registry::resolve("fold.motion.stroke", 1, "solid_centered_butt_miter")
        .unwrap()
        .node("Stroke", [("geometry".into(), link(fill.id))].into())
        .unwrap();
    let place = definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap();
    place.parameters.get_mut("geometry").unwrap().value =
        serde_json::to_value(link(stroke.id)).unwrap();
    let other = registry::resolve("fold.motion.place", 1, "screen")
        .unwrap()
        .node("Fill only", [("geometry".into(), link(fill.id))].into())
        .unwrap();
    let scene = definition
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.push(link(other.id));
    scene.parameters.insert(
        "children".into(),
        Parameter::new(Binding::List { values }).unwrap(),
    );
    definition.nodes.extend([fill, stroke, other]);
    p.commit(EditBatch {
        base: p.snapshot().revision(),
        mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
    })
    .unwrap();
    let epoch = d.epoch();
    let revision = p.snapshot().revision();
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":revision.0,"document":id},"output":output,"times":[Time::ZERO]}),
    );
    assert!(result.get("error").is_none(), "{result}");
    let sample = &result["result"]["samples"][0];
    assert_eq!(sample["counts"]["unique_geometries"], 1);
    assert_eq!(sample["counts"]["unique_points"], 4);
    assert_eq!(sample["counts"]["placements"], 2);
    let stroked = &sample["value"]["entries"][0]["placement"];
    let filled = &sample["value"]["entries"][1]["placement"];
    assert_eq!(stroked["geometry"], filled["geometry"]);
    assert_eq!(stroked["appearance"]["fill"], filled["appearance"]["fill"]);
    assert_eq!(
        filled["appearance"]["fill"]["color"]["value"],
        json!([0.8, 0.1, 0.05, 0.5])
    );
    assert!(filled["appearance"]["stroke"].is_null());
    assert_eq!(
        stroked["appearance"]["stroke"]["mode"],
        "solid_centered_butt_miter"
    );
    assert_eq!(stroked["appearance"]["stroke"]["width"], 2.);
    assert_eq!(stroked["bounds_kind"], "geometry");
    assert_eq!(result["result"]["provenance"]["frames_available"], false);
}

#[test]
fn native_text_observations_report_font_identity_logical_mappings_and_layout_bounds() {
    use fold_motion::{model::Reference, registry};
    let (mut d, mut p, id, output) = setup();
    let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let expected = fold_motion::source::canonical(&c).unwrap();
    let text = registry::resolve("fold.motion.text", 1, "packaged")
        .unwrap()
        .node("Text", Default::default())
        .unwrap();
    let placement = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap();
    placement.parameters.get_mut("geometry").unwrap().value = serde_json::to_value(Binding::Link {
        reference: Reference::Node { node: text.id }.into(),
    })
    .unwrap();
    c.definitions[0].nodes.push(text);
    let epoch = d.epoch();
    let revision = p.snapshot().revision();
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":revision.0,"scope":{"document":id,"definition":null}},"edit":{"kind":"source","expected":expected,"text":fold_motion::source::canonical(&c).unwrap(),"mapping":{}}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    let handle = proposed["result"]["handle"].clone();
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"candidate","handle":handle},"output":output,"times":[Time::ZERO,Time::new(1,1).unwrap()]}),
    );
    assert!(result.get("error").is_none(), "{result}");
    let data = &result["result"];
    assert_eq!(data["provenance"]["candidate"], handle);
    let fonts = data["provenance"]["resource_inputs"]["fonts"]
        .as_array()
        .unwrap();
    assert_eq!(fonts.len(), 1);
    assert_eq!(fonts[0]["source"], "packaged");
    assert_eq!(
        fonts[0]["fingerprint"],
        "sha256:b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5"
    );
    assert_eq!(fonts[0]["family"], "Noto Sans");
    assert_eq!(data["provenance"]["frames_available"], false);
    let a = &data["samples"][0];
    let b = &data["samples"][1];
    assert_eq!(a["counts"]["glyphs"], 12);
    assert_eq!(a["counts"]["unique_points"], 0);
    let geometry = a["geometries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(geometry["bounds_kind"], "layout");
    assert_eq!(
        a["value"]["entries"][0]["placement"]["bounds_kind"],
        "layout"
    );
    let text = &geometry["text"];
    assert_eq!(text["text"], "MAKE IT MOVE");
    assert_eq!(text["line_count"], 1);
    assert_eq!(text["glyphs"].as_array().unwrap().len(), 12);
    let words = text["words"].as_array().unwrap();
    assert_eq!(words.len(), 3);
    assert_eq!(words[0]["text"], json!({"start":0,"end":4}));
    assert_eq!(words[0]["pivot_kind"], "advance_center");
    assert_eq!(words[0]["glyphs"].as_array().unwrap().len(), 4);
    for id in words[0]["glyphs"].as_array().unwrap() {
        assert!(
            text["glyphs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|glyph| &glyph["id"] == id)
        );
    }
    assert!(!text["clusters"].as_array().unwrap().is_empty());
    assert_eq!(text["runs"][0]["face_index"], 0);
    assert!(text["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(a["geometries"], b["geometries"]);
    assert_eq!(p.snapshot().revision(), revision);
}

#[test]
fn native_outline_candidate_observations_retain_curves_and_source_font_provenance() {
    use fold_motion::{model::Reference, registry};
    let (mut d, mut p, id, output) = setup();
    let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let expected = fold_motion::source::canonical(&c).unwrap();
    let text = registry::resolve("fold.motion.text", 1, "packaged")
        .unwrap()
        .node("Text", Default::default())
        .unwrap();
    let outline = registry::resolve("fold.motion.outline_text", 1, "curves")
        .unwrap()
        .node(
            "Outline Text",
            [(
                "text".into(),
                Binding::Link {
                    reference: Reference::Node { node: text.id }.into(),
                },
            )]
            .into(),
        )
        .unwrap();
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .get_mut("geometry")
        .unwrap()
        .value = serde_json::to_value(Binding::Link {
        reference: Reference::Node { node: outline.id }.into(),
    })
    .unwrap();
    c.definitions[0].nodes.extend([text, outline]);
    let epoch = d.epoch();
    let revision = p.snapshot().revision();
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":revision.0,"scope":{"document":id,"definition":null}},"edit":{"kind":"source","expected":expected,"text":fold_motion::source::canonical(&c).unwrap(),"mapping":{}}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"candidate","handle":proposed["result"]["handle"]},"output":output,"times":[Time::ZERO]}),
    );
    assert!(result.get("error").is_none(), "{result}");
    let data = &result["result"];
    assert_eq!(
        data["provenance"]["resource_inputs"]["fonts"][0]["fingerprint"],
        fold_motion::evaluation::text::FONT_FINGERPRINT
    );
    assert_eq!(data["provenance"]["frames_available"], false);
    let sample = &data["samples"][0];
    assert_eq!(sample["counts"]["glyphs"], 12);
    let geometry = sample["geometries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(geometry["bounds_kind"], "control_hull");
    assert_eq!(
        sample["value"]["entries"][0]["placement"]["bounds_kind"],
        "control_hull"
    );
    assert_eq!(geometry["source_text"]["text"], "MAKE IT MOVE");
    let glyphs = geometry["outlined_glyphs"].as_array().unwrap();
    assert_eq!(glyphs.len(), 12);
    for (index, glyph) in glyphs.iter().enumerate() {
        assert_eq!(
            glyph["source_glyph"],
            geometry["source_text"]["glyphs"][index]["id"]
        );
        assert_eq!(glyph["glyph_index"], index);
        assert_eq!(glyph["transform"].as_array().unwrap().len(), 6);
    }
    assert!(!glyphs[0]["commands"].as_array().unwrap().is_empty());
    assert!(glyphs[4]["commands"].as_array().unwrap().is_empty());
    assert_eq!(p.snapshot().revision(), revision);
}

#[test]
fn native_extrude_candidate_reports_metric_meshes_regions_and_shared_glyph_provenance() {
    use fold_motion::{model::Reference, registry, types::Type};
    for world in [false, true] {
        for mode in [None, Some("lit"), Some("unlit")] {
            let (mut d, mut p, id, output) = setup();
            let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
            let expected = fold_motion::source::canonical(&c).unwrap();
            let text = registry::resolve("fold.motion.text", 1, "packaged")
                .unwrap()
                .node(
                    "Text",
                    [(
                        "text".into(),
                        Binding::Literal {
                            value: Literal::Text {
                                value: "O O".into(),
                            },
                        },
                    )]
                    .into(),
                )
                .unwrap();
            let link = |node| Binding::Link {
                reference: Reference::Node { node }.into(),
            };
            let outline = registry::resolve("fold.motion.outline_text", 1, "curves")
                .unwrap()
                .node("Outline Text", [("text".into(), link(text.id))].into())
                .unwrap();
            let extrude = registry::resolve("fold.motion.extrude", 1, "straight_text")
                .unwrap()
                .node("Extrude", [("profile".into(), link(outline.id))].into())
                .unwrap();
            c.definitions[0].outputs[0].ty = Type::Geometry;
            c.definitions[0].outputs[0].value = Reference::Node { node: extrude.id }.into();
            c.definitions[0].nodes.extend([text, outline, extrude]);
            if let Some(mode) = mode {
                let source = c.definitions[0].nodes.last().unwrap().id;
                let material = registry::resolve("fold.motion.material", 1, mode)
                    .unwrap()
                    .node(
                        "Surface",
                        [
                            ("geometry".into(), link(source)),
                            (
                                "color".into(),
                                Binding::Literal {
                                    value: Literal::Color {
                                        value: [2., 0.5, 0.25, 0.5],
                                        encoding: fold_motion::types::Encoding::LinearRec709,
                                        alpha: fold_motion::types::Alpha::Straight,
                                    },
                                },
                            ),
                        ]
                        .into(),
                    )
                    .unwrap();
                c.definitions[0].outputs[0].value = Reference::Node { node: material.id }.into();
                c.definitions[0].nodes.push(material);
            }

            if world {
                let source = c.definitions[0].nodes.last().unwrap().id;
                let vector = |value: Vec<f64>, unit| Binding::Literal {
                    value: Literal::Vector {
                        value,
                        unit,
                        space: fold_motion::types::Space::World,
                    },
                };
                let q = std::f64::consts::FRAC_1_SQRT_2;
                let place = registry::resolve("fold.motion.place", 1, "world")
                    .unwrap()
                    .node(
                        "World",
                        [
                            ("geometry".into(), link(source)),
                            ("position".into(), vector(vec![3., 4., 5.], Unit::Meters)),
                            ("scale".into(), vector(vec![2., 3., 4.], Unit::Unitless)),
                            (
                                "rotation".into(),
                                Binding::Literal {
                                    value: Literal::Rotation {
                                        value: [0., 0., q, q],
                                        space: fold_motion::types::Space::World,
                                    },
                                },
                            ),
                            (
                                "opacity".into(),
                                Binding::Literal {
                                    value: Literal::Scalar { value: 0.25 },
                                },
                            ),
                        ]
                        .into(),
                    )
                    .unwrap();
                c.definitions[0].outputs[0].ty = Type::Placement;
                c.definitions[0].outputs[0].value = Reference::Node { node: place.id }.into();
                c.definitions[0].nodes.push(place);
            }
            let epoch = d.epoch();
            let revision = p.snapshot().revision();
            let proposed = call(
                &mut d,
                &mut p,
                json!({"method":"propose","guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":revision.0,"scope":{"document":id,"definition":null}},"edit":{"kind":"source","expected":expected,"text":fold_motion::source::canonical(&c).unwrap(),"mapping":{}}}),
            );
            assert!(proposed.get("error").is_none(), "{proposed}");
            let result = call(
                &mut d,
                &mut p,
                json!({"method":"observe","target":{"kind":"candidate","handle":proposed["result"]["handle"]},"output":output,"times":[Time::ZERO]}),
            );
            assert!(
                result.get("error").is_none(),
                "{}",
                result.get("error").unwrap_or(&Value::Null)
            );
            let data = &result["result"];
            assert_eq!(data["provenance"]["frames_available"], false);
            assert_eq!(
                data["provenance"]["resource_inputs"]["fonts"][0]["fingerprint"],
                fold_motion::evaluation::text::FONT_FINGERPRINT
            );
            let sample = &data["samples"][0];
            let geometry = sample["geometries"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap();
            assert_eq!(geometry["space"], "world");
            assert_eq!(geometry["unit"], "meters");
            assert_eq!(geometry["bounds_kind"], "mesh");
            assert_eq!(geometry["depth_meters"], 0.15);
            assert!((geometry["bounds"][0][2].as_f64().unwrap() + 0.15).abs() < 1e-7);
            assert_eq!(geometry["bounds"][1][2], 0.);
            let meshes = geometry["meshes"].as_object().unwrap();
            assert_eq!(meshes.len(), 1);
            let mesh = meshes.values().next().unwrap();
            assert_eq!(
                mesh["vertex_ids"].as_array().unwrap().len(),
                mesh["source_vertices"].as_array().unwrap().len()
            );
            assert_eq!(
                mesh["face_ids"].as_array().unwrap().len() * 3,
                mesh["indices"].as_array().unwrap().len()
            );
            assert_eq!(
                mesh["regions"]["sides"]["end"],
                mesh["indices"].as_array().unwrap().len()
            );
            let glyphs = &geometry["glyphs"];
            assert_eq!(glyphs[0]["mesh"], glyphs[2]["mesh"]);
            assert_ne!(glyphs[0]["id"], glyphs[2]["id"]);
            assert!(glyphs[1]["mesh"].is_null());
            assert_eq!(sample["counts"]["glyphs"], 3);
            let value = if world {
                &sample["value"]["placement"]
            } else {
                &sample["value"]
            };
            if world {
                assert_eq!(value["mode"], "world");
                assert_eq!(value["opacity"], 0.25);
                let bounds = &geometry["bounds"];
                let expected = [
                    [
                        3. - 3. * bounds[1][1].as_f64().unwrap(),
                        4. + 2. * bounds[0][0].as_f64().unwrap(),
                        5. + 4. * bounds[0][2].as_f64().unwrap(),
                    ],
                    [
                        3. - 3. * bounds[0][1].as_f64().unwrap(),
                        4. + 2. * bounds[1][0].as_f64().unwrap(),
                        5. + 4. * bounds[1][2].as_f64().unwrap(),
                    ],
                ];
                for row in 0..2 {
                    for axis in 0..3 {
                        assert!(
                            (value["bounds"][row][axis].as_f64().unwrap() - expected[row][axis])
                                .abs()
                                < 1e-10
                        );
                    }
                }
            }
            let material = &value["appearance"]["material"];
            if let Some(mode) = mode {
                assert_eq!(material["unlit"], mode == "unlit");
                assert_eq!(material["color"]["value"], json!([2., 0.5, 0.25, 0.5]));
                assert_eq!(material["roughness"], 0.5);
                assert_eq!(material["reflectance"], 0.5);
            } else {
                assert!(material.is_null());
            }

            assert_eq!(p.snapshot().revision(), revision);
        }
    }
}

#[test]
fn word_animation_observation_reports_evaluated_positions_and_conservative_bounds() {
    use fold_motion::{
        model::Reference,
        registry,
        types::{Literal, Space, Unit},
    };
    let (mut d, mut p, id, output) = setup();
    let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let expected = fold_motion::source::canonical(&c).unwrap();
    let text = registry::resolve("fold.motion.text", 1, "packaged")
        .unwrap()
        .node("Text", Default::default())
        .unwrap();
    let position = |y| Binding::Literal {
        value: Literal::Vector {
            value: vec![0., y],
            unit: Unit::Canvas,
            space: Space::Screen,
        },
    };
    let link = |node| Binding::Link {
        reference: Reference::Node { node }.into(),
    };
    let tween = registry::resolve("fold.motion.tween", 1, "cubic_out")
        .unwrap()
        .node(
            "Entrance",
            [
                ("from".into(), position(80.)),
                ("to".into(), position(0.)),
                (
                    "duration".into(),
                    Binding::Literal {
                        value: Literal::Duration {
                            value: Time::new(3, 5).unwrap(),
                        },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let animator = registry::resolve("fold.motion.text_animator", 1, "word_position")
        .unwrap()
        .node(
            "Words",
            [
                ("text".into(), link(text.id)),
                ("position".into(), link(tween.id)),
            ]
            .into(),
        )
        .unwrap();
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .get_mut("geometry")
        .unwrap()
        .value = serde_json::to_value(link(animator.id)).unwrap();
    c.definitions[0].nodes.extend([text, tween, animator]);
    let epoch = d.epoch();
    let revision = p.snapshot().revision();
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":revision.0,"scope":{"document":id,"definition":null}},"edit":{"kind":"source","expected":expected,"text":fold_motion::source::canonical(&c).unwrap(),"mapping":{}}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"candidate","handle":proposed["result"]["handle"]},"output":output,"times":[Time::ZERO,Time::new(1,1).unwrap()]}),
    );
    assert!(result.get("error").is_none(), "{result}");
    let geometry = |i: usize| {
        result["result"]["samples"][i]["geometries"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()
    };
    let a = geometry(0);
    let b = geometry(1);
    assert_eq!(a["bounds_kind"], "layout_envelope");
    assert_eq!(b["bounds_kind"], "layout");
    assert_eq!(a["text"]["identity"], b["text"]["identity"]);
    assert_eq!(a["text"]["glyphs"][0]["offset"], json!([0., 80.]));
    assert_eq!(b["text"]["glyphs"][0]["offset"], json!([0., 0.]));
    for i in 0..3 {
        assert_eq!(a["text"]["words"][i]["id"], b["text"]["words"][i]["id"]);
        assert!(
            (a["text"]["words"][i]["pivot"][1].as_f64().unwrap()
                - b["text"]["words"][i]["pivot"][1].as_f64().unwrap()
                - 80.)
                .abs()
                < 1e-10
        );
    }
    assert_eq!(p.snapshot().revision(), revision);
}

#[test]
fn box_observation_reports_mesh_uvs_and_transformed_bounds_without_text_provenance() {
    use fold_motion::{
        model::Reference,
        registry,
        types::{Space, Type},
    };
    let (mut d, mut p, id, output) = setup();
    let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let mesh = registry::resolve("fold.motion.mesh", 1, "box")
        .unwrap()
        .node("Box", Default::default())
        .unwrap();
    let world = registry::resolve("fold.motion.place", 1, "world")
        .unwrap()
        .node(
            "World",
            [
                (
                    "geometry".into(),
                    Binding::Link {
                        reference: Reference::Node { node: mesh.id }.into(),
                    },
                ),
                (
                    "position".into(),
                    Binding::Literal {
                        value: Literal::Vector {
                            value: vec![1., 2., 3.],
                            unit: Unit::Meters,
                            space: Space::World,
                        },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    c.definitions[0].outputs[0].ty = Type::Placement;
    c.definitions[0].outputs[0].value = Reference::Node { node: world.id }.into();
    c.definitions[0].nodes.extend([mesh, world]);
    p.commit(EditBatch {
        base: p.snapshot().revision(),
        mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
    })
    .unwrap();
    let revision = p.snapshot().revision();
    let epoch = d.epoch();
    let response = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":revision.0,"document":id},"output":output,"times":[Time::ZERO]}),
    );
    assert!(response.get("error").is_none(), "{response}");
    let sample = &response["result"]["samples"][0];
    assert_eq!(sample["counts"]["unique_points"], 24);
    assert_eq!(sample["counts"]["glyphs"], 0);
    let geometry = sample["geometries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(geometry["mesh"]["vertex_ids"].as_array().unwrap().len(), 24);
    assert_eq!(geometry["mesh"]["face_ids"].as_array().unwrap().len(), 12);
    assert_eq!(geometry["mesh"]["uv"].as_array().unwrap().len(), 24);
    let regions = geometry["mesh"]["regions"].as_object().unwrap();
    assert_eq!(regions.len(), 6);
    for (index, name) in [
        "positive_x",
        "negative_x",
        "positive_y",
        "negative_y",
        "positive_z",
        "negative_z",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            regions[name],
            serde_json::json!({"start": index * 6, "end": (index + 1) * 6})
        );
    }
    assert_eq!(geometry["mesh"]["source_vertices"], serde_json::json!([]));
    assert!(geometry["mesh"]["source_geometry"].is_null());
    assert_eq!(geometry["precision"], "f32");
    assert!(geometry.get("source_text").is_none());
    let bounds = &sample["value"]["placement"]["bounds"];
    let half = f64::from(0.06f32 * 0.5);
    assert_eq!(
        bounds,
        &json!([
            [1. - half, 2. - half, 3. - half],
            [1. + half, 2. + half, 3. + half]
        ])
    );
    assert_eq!(p.snapshot().revision(), revision);
}

#[test]
fn grid_observation_exposes_metric_cells_and_stable_ids_after_count_change() {
    use fold_motion::{
        model::{Parameter, Reference},
        registry,
        types::Type,
    };
    let (mut d, mut p, id, output) = setup();
    let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let grid = registry::resolve("fold.motion.distribute", 2, "grid")
        .unwrap()
        .node("Grid", Default::default())
        .unwrap();
    let grid_id = grid.id;
    c.definitions[0].outputs[0].ty = Type::Geometry;
    c.definitions[0].outputs[0].value = Reference::Node { node: grid_id }.into();
    c.definitions[0].nodes.push(grid);
    let mut ids = std::collections::BTreeMap::new();
    for columns in [20, 21, 0] {
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == grid_id)
            .unwrap()
            .parameters
            .insert(
                "columns".into(),
                Parameter::new(Binding::Literal {
                    value: Literal::Integer { value: columns },
                })
                .unwrap(),
            );
        p.commit(EditBatch {
            base: p.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
        let revision = p.snapshot().revision();
        let epoch = d.epoch();
        let response = call(
            &mut d,
            &mut p,
            json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":revision.0,"document":id},"output":output,"times":[Time::ZERO]}),
        );
        assert!(response.get("error").is_none(), "{response}");
        let sample = &response["result"]["samples"][0];
        assert_eq!(sample["counts"]["unique_points"], columns * 12);
        let stream = &sample["value"];
        assert_eq!(stream["kind"], "geometry");
        let mesh = &sample["geometries"][stream["id"].as_u64().unwrap().to_string()];
        assert_eq!(mesh["space"], "world");
        assert_eq!(mesh["unit"], "meters");
        for (index, id) in mesh["mesh"]["vertex_ids"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let cell = [
                (index % columns as usize) as u32,
                (index / columns as usize) as u32,
            ];
            if let Some(old) = ids.insert(cell, id.clone()) {
                assert_eq!(old, *id);
            }
        }
        if columns == 0 {
            assert!(stream["bounds"].is_null());
        }
        assert_eq!(p.snapshot().revision(), revision);
    }
}

#[test]
fn cloner_observation_reports_shared_prototype_instances_and_world_bounds() {
    use fold_motion::{model::Reference, registry, types::Type};
    for (factor, angle) in [(1., 0.), (0.5, std::f64::consts::FRAC_PI_4)] {
        let (mut d, mut p, id, output) = setup();
        let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
        let link = |node| Binding::Link {
            reference: Reference::Node { node }.into(),
        };
        let node = |key: &str, mode: &str, name: &str, bindings| {
            registry::resolve(
                &format!("fold.motion.{key}"),
                if matches!(key, "distribute" | "cloner") {
                    2
                } else {
                    1
                },
                mode,
            )
            .unwrap()
            .node(name, bindings)
            .unwrap()
        };
        let mesh = node("mesh", "box", "Box", std::collections::BTreeMap::new());
        let grid = node(
            "distribute",
            "grid",
            "Grid",
            std::collections::BTreeMap::new(),
        );
        let cloner = node(
            "cloner",
            "points",
            "Cloner",
            [
                ("prototype".into(), link(mesh.id)),
                ("points".into(), link(grid.id)),
            ]
            .into(),
        );
        let animator = node(
            "animator",
            "scale_multiply",
            "Scale",
            [
                ("geometry".into(), link(cloner.id)),
                (
                    "factor".into(),
                    Binding::Literal {
                        value: Literal::Scalar { value: factor },
                    },
                ),
            ]
            .into(),
        );
        let color = node(
            "animator",
            "color_replace",
            "Color",
            [
                ("geometry".into(), link(animator.id)),
                (
                    "color".into(),
                    Binding::Literal {
                        value: Literal::Color {
                            value: [2., 0., 1., 0.4],
                            encoding: fold_motion::types::Encoding::LinearRec709,
                            alpha: fold_motion::types::Alpha::Straight,
                        },
                    },
                ),
            ]
            .into(),
        );
        let rotation = node(
            "animator",
            "rotation_z_add",
            "Rotate",
            [
                ("geometry".into(), link(color.id)),
                (
                    "angle".into(),
                    Binding::Literal {
                        value: Literal::Scalar { value: angle },
                    },
                ),
            ]
            .into(),
        );
        let world = node(
            "place",
            "world",
            "World",
            [("geometry".into(), link(rotation.id))].into(),
        );
        c.definitions[0].outputs[0].ty = Type::Placement;
        c.definitions[0].outputs[0].value = Reference::Node { node: world.id }.into();
        c.definitions[0]
            .nodes
            .extend([mesh, grid, cloner, animator, color, rotation, world]);
        p.commit(EditBatch {
            base: p.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
        let revision = p.snapshot().revision();
        let epoch = d.epoch();
        let response = call(
            &mut d,
            &mut p,
            json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":revision.0,"document":id},"output":output,"times":[Time::ZERO]}),
        );
        assert!(response.get("error").is_none(), "{response}");
        let sample = &response["result"]["samples"][0];
        assert_eq!(sample["counts"]["unique_points"], 24);
        assert_eq!(sample["counts"]["unique_geometries"], 2);
        let geometries = sample["geometries"].as_object().unwrap();
        let collection = geometries
            .values()
            .find(|v| v.get("instances").is_some())
            .unwrap();
        let prototype = &geometries[&collection["prototype"].as_u64().unwrap().to_string()];
        assert_eq!(prototype["mesh"]["positions"].as_array().unwrap().len(), 24);
        let instances = collection["instances"].as_array().unwrap();
        assert_eq!(instances.len(), 240);
        assert!(
            instances
                .iter()
                .all(|v| v["color"]["value"] == json!([2., 0., 1., 0.4]))
        );
        assert!(
            instances
                .iter()
                .all(|v| v["scale"].as_f64() == Some(factor))
        );
        assert!(
            instances
                .iter()
                .all(|v| v["rotation_z"].as_f64() == Some(angle))
        );
        let identities: std::collections::BTreeSet<_> = instances
            .iter()
            .map(|v| v["id"].as_u64().unwrap())
            .collect();
        assert_eq!(identities.len(), 240);
        let bounds = &sample["value"]["placement"]["bounds"];
        let half = f64::from(0.03f32) * factor;
        let rotated_half = half * (angle.cos().abs() + angle.sin().abs());
        for (axis, extent) in [
            f64::from(1.14f32) + rotated_half,
            f64::from(0.66f32) + rotated_half,
            half,
        ]
        .into_iter()
        .enumerate()
        {
            assert!((bounds[0][axis].as_f64().unwrap() + extent).abs() < 1e-12);
            assert!((bounds[1][axis].as_f64().unwrap() - extent).abs() < 1e-12);
        }
        assert_eq!(p.snapshot().revision(), revision);
    }
}

#[test]
fn native_ramp_source_candidate_observes_clamped_scalar_without_committing() {
    use fold_motion::{model::Reference, registry, types::Type};
    let (mut d, mut p, id, output) = setup();
    let mut c = Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let expected = fold_motion::source::canonical(&c).unwrap();
    let tween = registry::resolve("fold.motion.tween", 1, "linear")
        .unwrap()
        .node(
            "Input",
            [
                (
                    "from".into(),
                    Binding::Literal {
                        value: Literal::Scalar { value: -1. },
                    },
                ),
                (
                    "to".into(),
                    Binding::Literal {
                        value: Literal::Scalar { value: 2. },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let ramp = registry::resolve("fold.motion.ramp", 1, "scalar_linear")
        .unwrap()
        .node(
            "Response",
            [
                (
                    "position".into(),
                    Binding::Link {
                        reference: Reference::Node { node: tween.id }.into(),
                    },
                ),
                (
                    "stops".into(),
                    Binding::Ramp {
                        ramp: fold_motion::ramp::Ramp::two(
                            Literal::Scalar { value: 0.2 },
                            Literal::Scalar { value: 1. },
                        )
                        .unwrap(),
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    c.definitions[0].outputs[0].ty = Type::Scalar;
    c.definitions[0].outputs[0].value = Reference::Node { node: ramp.id }.into();
    c.definitions[0].nodes.extend([tween, ramp]);
    let epoch = d.epoch();
    let revision = p.snapshot().revision();
    let proposed = call(
        &mut d,
        &mut p,
        json!({"method":"propose","guard":{"epoch":epoch,"request_id":ObjectId::new(),"base":revision.0,"scope":{"document":id,"definition":null}},"edit":{"kind":"source","expected":expected,"text":fold_motion::source::canonical(&c).unwrap(),"mapping":{}}}),
    );
    assert!(proposed.get("error").is_none(), "{proposed}");
    let result = call(
        &mut d,
        &mut p,
        json!({"method":"observe","target":{"kind":"candidate","handle":proposed["result"]["handle"]},"output":output,"times":[Time::ZERO,Time::new(1,2).unwrap(),Time::new(1,1).unwrap()]}),
    );
    assert!(result.get("error").is_none(), "{result}");
    for (sample, expected) in result["result"]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .zip([0.2, 0.6, 1.])
    {
        assert!((sample["value"]["value"]["value"].as_f64().unwrap() - expected).abs() < 1e-12);
    }
    assert_eq!(p.snapshot().revision(), revision);
}

#[test]
fn camera_plane_observation_distinguishes_canvas_bounds_from_projection() {
    use fold_motion::{
        model::{Parameter, Reference},
        registry,
    };
    let (mut dispatcher, mut project, document, output) = setup();
    let snapshot = project.snapshot();
    let mut c = Construction::from_document(&snapshot.state().documents[&document]).unwrap();
    let camera = registry::resolve("fold.motion.camera", 1, "perspective")
        .unwrap()
        .node("Attached Camera", Default::default())
        .unwrap();
    let placement = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap();
    placement.mode = "camera_plane_viewport".into();
    placement.parameters.insert(
        "distance".into(),
        Parameter::new(Binding::Literal {
            value: Literal::Distance {
                value: 2.,
                unit: Unit::Meters,
            },
        })
        .unwrap(),
    );
    placement.parameters.insert(
        "camera".into(),
        Parameter::new(Binding::Link {
            reference: Reference::Node { node: camera.id }.into(),
        })
        .unwrap(),
    );
    c.definitions[0].nodes.push(camera);
    project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(c.document(document).unwrap())],
        })
        .unwrap();
    let before = project.snapshot();
    let epoch = dispatcher.epoch();
    let observed = call(
        &mut dispatcher,
        &mut project,
        json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":before.revision().0,"document":document},"output":output,"times":[Time::ZERO]}),
    );
    assert!(observed.get("error").is_none(), "{observed}");
    let placement = &observed["result"]["samples"][0]["value"]["entries"][0]["placement"];
    assert_eq!(placement["mode"], "camera_plane", "{observed}");
    assert_eq!(placement["bounds_space"], "canvas_before_projection");
    assert_eq!(
        placement["attachment"]["sizing"],
        json!({"kind":"viewport"})
    );
    assert_eq!(placement["attachment"]["depth"], "scene");
    assert_eq!(placement["attachment"]["distance"], 2.);
    assert_eq!(
        placement["attachment"]["camera"]["position"],
        json!([0., 1., 5.])
    );
    assert_eq!(project.snapshot().revision(), before.revision());
}

#[test]
fn profile_mesh_observation_retains_source_curves_and_fonts_without_a_text_placement() {
    use fold_motion::{model::Reference, registry, types::Type};
    for (key, mode) in [("tessellate", "planar"), ("extrude", "closed_profile")] {
        let (mut dispatcher, mut project, document, output) = setup();
        let mut c =
            Construction::from_document(&project.snapshot().state().documents[&document]).unwrap();
        let text = registry::resolve("fold.motion.text", 1, "packaged")
            .unwrap()
            .node("Text", Default::default())
            .unwrap();
        let link = |id| Binding::Link {
            reference: Reference::Node { node: id }.into(),
        };
        let outlines = registry::resolve("fold.motion.outline_text", 1, "curves")
            .unwrap()
            .node("Outlines", [("text".into(), link(text.id))].into())
            .unwrap();
        let mesh = registry::resolve(&format!("fold.motion.{key}"), 1, mode)
            .unwrap()
            .node("Mesh", [("profile".into(), link(outlines.id))].into())
            .unwrap();
        c.definitions[0].outputs[0].ty = Type::Geometry;
        c.definitions[0].outputs[0].value = Reference::Node { node: mesh.id }.into();
        c.definitions[0].nodes = vec![text, outlines, mesh];
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(c.document(document).unwrap())],
            })
            .unwrap();
        let epoch = dispatcher.epoch();
        let revision = project.snapshot().revision();
        let response = call(
            &mut dispatcher,
            &mut project,
            json!({"method":"observe","target":{"kind":"committed","epoch":epoch,"revision":revision.0,"document":document},"output":output,"times":[Time::ZERO]}),
        );
        assert!(response.get("error").is_none(), "{response}");
        let fonts = response["result"]["provenance"]["resource_inputs"]["fonts"]
            .as_array()
            .unwrap();
        assert_eq!(fonts.len(), 1);
        assert_eq!(
            fonts[0]["fingerprint"],
            fold_motion::evaluation::text::FONT_FINGERPRINT
        );
        let geometries = response["result"]["samples"][0]["geometries"]
            .as_object()
            .unwrap();
        let mesh = geometries
            .values()
            .find(|g| g.get("mesh").is_some())
            .unwrap();
        let source = &geometries[&mesh["mesh"]["source_geometry"]
            .as_u64()
            .unwrap()
            .to_string()];
        assert_eq!(source["source_text"]["text"], "MAKE IT MOVE");
        assert!(!source["outlined_glyphs"].as_array().unwrap().is_empty());
        assert_eq!(project.snapshot().revision(), revision);
    }
}
