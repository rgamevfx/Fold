#![cfg(feature = "spatial")]
use fold_app::{
    agent::dispatch::Dispatcher,
    output::{SceneRequest, evaluate_scene_gpu},
};
use fold_foundation::{ObjectId, Time};
use fold_project::{EditBatch, Mutation, Project};
use fold_render::gpu::{Host, Renderer};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn request(dispatcher: &mut Dispatcher, project: &mut Project, command: Value) -> Value {
    serde_json::from_slice(&dispatcher.dispatch(
        project,
        &serde_json::to_vec(&json!({"version":2,"command":command})).unwrap(),
        Instant::now(),
    ))
    .unwrap()
}
fn finish(dispatcher: &mut Dispatcher, project: &mut Project, mut response: Value) -> Value {
    let handle = response["result"]["job"].clone();
    let deadline = Instant::now() + Duration::from_secs(90);
    while response["result"]["state"] == "running" {
        assert!(Instant::now() < deadline, "frame job timed out");
        response = request(
            dispatcher,
            project,
            json!({"method":"job_status","handle":handle}),
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(response.get("error").is_none(), "{response}");
    response
}
fn decode(response: &Value) -> Vec<u8> {
    let text = response["result"]["image"]["data"].as_str().unwrap();
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
#[ignore = "requires a native Vulkan adapter"]
fn public_frame_jobs_match_registered_pixels_and_retain_candidate_context() {
    let mut project = fold_project::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/rewrite/evidence/m5b-controls-baseline.fold"),
        32,
    )
    .unwrap();
    let document_id = *project.snapshot().state().documents.keys().next().unwrap();
    let mut shifted = fold_motion::model::Construction::from_document(
        &project.snapshot().state().documents[&document_id],
    )
    .unwrap();
    shifted.canvas.start = Time::new(2, 1).unwrap();
    shifted.canvas.end = Time::new(6, 1).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                shifted.document(document_id).unwrap(),
            )],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let document = *snapshot.state().documents.keys().next().unwrap();
    let source = fold_app::packages::builtins()
        .default_output(&snapshot, document)
        .unwrap();
    let output: ObjectId = serde_json::from_value(json!(source.output)).unwrap();
    let mut dispatcher = Dispatcher::default();
    let epoch = dispatcher.epoch();
    let frame_request = |target| json!({"method":"observe_frame","target":target,"output":output,"time":Time::new(1,1).unwrap(),"dimensions":[480,270]});
    let committed = json!({"kind":"committed","epoch":epoch,"revision":snapshot.revision(),"document":document});
    let unavailable = request(
        &mut dispatcher,
        &mut project,
        frame_request(committed.clone()),
    );
    assert_eq!(unavailable["error"]["code"], "unsupported");
    let (host, info) = pollster::block_on(Host::headless(512 * 1024 * 1024)).unwrap();
    eprintln!("Agent frame adapter: {info:?}");
    dispatcher.set_render_host(Some(host.clone()));
    let discovery = request(&mut dispatcher, &mut project, json!({"method":"discover"}));
    assert_eq!(discovery["result"]["frame_observation"]["available"], true);
    let mut too_large = frame_request(committed.clone());
    too_large["dimensions"] = json!([4096, 4096]);
    assert_eq!(
        request(&mut dispatcher, &mut project, too_large)["error"]["code"],
        "budget"
    );
    let mut renderer = Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let frame = evaluate_scene_gpu(
        &snapshot,
        &SceneRequest {
            source,
            time: Time::new(1, 1).unwrap(),
            dimensions: [480, 270],
            region: None,
            preview: None,
        },
        &mut renderer,
        &mut fold_media::Decoder::default(),
        &cancel,
    )
    .unwrap();
    let reference = renderer
        .output(&frame, None, &cancel)
        .unwrap()
        .readback(&cancel)
        .unwrap();
    let submitted = request(
        &mut dispatcher,
        &mut project,
        frame_request(committed.clone()),
    );
    let response = finish(&mut dispatcher, &mut project, submitted);
    assert_eq!(decode(&response), reference.rgba());
    assert_eq!(
        response["result"]["provenance"]["scene_time"],
        json!(Time::new(3, 1).unwrap())
    );
    assert_eq!(
        response["result"]["provenance"]["asset_time"],
        json!(Time::new(3, 1).unwrap())
    );
    assert_eq!(
        response["result"]["provenance"]["revision"],
        json!(snapshot.revision())
    );
    assert_eq!(
        response["result"]["provenance"]["time"],
        json!(Time::new(1, 1).unwrap())
    );
    assert_eq!(
        response["result"]["provenance"]["output_size"],
        json!([480, 270])
    );
    assert!(response["result"]["provenance"]["camera"].is_object());
    assert!(
        !response["result"]["provenance"]["resource_inputs"]["fonts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(response["result"]["transfers"]["cpu_adapter_nodes"], 0);
    assert_eq!(
        response["result"]["transfers"]["evaluation_readback_bytes"],
        0
    );
    for [x, y, width, height] in [[37, 43, 361, 192], [135, 62, 180, 170], [479, 269, 1, 1]] {
        let region = json!({"x":x,"y":y,"width":width,"height":height});
        let mut crop_request = frame_request(committed.clone());
        crop_request["region"] = region.clone();
        let submitted = request(&mut dispatcher, &mut project, crop_request);
        let cropped = finish(&mut dispatcher, &mut project, submitted);
        assert_eq!(
            cropped["result"]["image"]["dimensions"],
            json!([width, height])
        );
        assert_eq!(
            cropped["result"]["provenance"]["canvas_size"],
            json!([480, 270])
        );
        assert_eq!(cropped["result"]["provenance"]["region"], region);
        assert_eq!(
            cropped["result"]["provenance"]["camera"],
            response["result"]["provenance"]["camera"]
        );
        let expected: Vec<u8> = (y..y + height)
            .flat_map(|row| {
                let start = (row * 480 + x) as usize * 4;
                reference.rgba()[start..start + width as usize * 4]
                    .iter()
                    .copied()
            })
            .collect();
        assert_eq!(
            decode(&cropped),
            expected,
            "region must preserve full-frame projection and display pixels"
        );
    }
    for region in [
        json!({"x":0,"y":0,"width":0,"height":1}),
        json!({"x":479,"y":269,"width":2,"height":1}),
        json!({"x":4294967295_u32,"y":0,"width":1,"height":1}),
    ] {
        let mut invalid = frame_request(committed.clone());
        invalid["region"] = region;
        assert_eq!(
            request(&mut dispatcher, &mut project, invalid)["error"]["code"],
            "type"
        );
        assert!(project.snapshot().same_content(&snapshot));
    }
    let mut full = frame_request(committed.clone());
    full["dimensions"] = json!([1920, 1080]);
    let submitted = request(&mut dispatcher, &mut project, full);
    let full = finish(&mut dispatcher, &mut project, submitted);
    assert_eq!(decode(&full).len(), 1920 * 1080 * 4);
    assert!(serde_json::to_vec(&full).unwrap().len() <= 16 * 1024 * 1024);
    let admission = fold_render::scheduling::Scheduler::shared()
        .enter(fold_render::scheduling::Class::Prepare, &cancel)
        .unwrap();
    let submitted = request(
        &mut dispatcher,
        &mut project,
        frame_request(committed.clone()),
    );
    let cancel_guard = json!({"epoch":epoch,"request_id":ObjectId::new(),"base":snapshot.revision(),"scope":{"document":document,"definition":null}});
    let cancellation = request(
        &mut dispatcher,
        &mut project,
        json!({"method":"cancel_job","guard":cancel_guard,"handle":submitted["result"]["job"]}),
    );
    assert!(cancellation.get("error").is_none());
    drop(admission);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = request(
            &mut dispatcher,
            &mut project,
            json!({"method":"job_status","handle":submitted["result"]["job"]}),
        );
        if status["result"]["state"] != "running" {
            assert_eq!(status["error"]["code"], "cancelled");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let original =
        fold_motion::model::Construction::from_document(&snapshot.state().documents[&document])
            .unwrap();
    let mut proposed = original.clone();
    proposed
        .presentation
        .annotations
        .insert(proposed.root.definition, "Frame candidate".into());
    let guard = json!({"epoch":epoch,"request_id":ObjectId::new(),"base":snapshot.revision(),"scope":{"document":document,"definition":null}});
    let submitted = request(
        &mut dispatcher,
        &mut project,
        json!({"method":"propose","guard":guard,"edit":{"kind":"source","expected":fold_motion::source::canonical(&original).unwrap(),"text":fold_motion::source::canonical(&proposed).unwrap(),"mapping":{}}}),
    );
    let candidate = finish(&mut dispatcher, &mut project, submitted)["result"]["handle"].clone();
    let submitted = request(
        &mut dispatcher,
        &mut project,
        frame_request(json!({"kind":"candidate","handle":candidate})),
    );
    let mut changed = original.clone();
    changed.seed += 1;
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(changed.document(document).unwrap())],
        })
        .unwrap();
    let observed = finish(&mut dispatcher, &mut project, submitted);
    assert_eq!(
        decode(&observed),
        reference.rgba(),
        "candidate frame must retain the original seed/context"
    );
    assert_eq!(observed["result"]["provenance"]["candidate"], candidate);
    assert_eq!(
        observed["result"]["provenance"]["revision"],
        json!(snapshot.revision())
    );
    assert_eq!(
        request(&mut dispatcher, &mut project, frame_request(committed))["error"]["code"],
        "conflict"
    );
    assert_eq!(
        fold_motion::model::Construction::from_document(
            &project.snapshot().state().documents[&document]
        )
        .unwrap(),
        changed
    );
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn public_mixed_document_candidate_frames_match_committed_composition() {
    let mut project = Project::new(16);
    let mut dispatcher = Dispatcher::default();
    let epoch = dispatcher.epoch();
    let document = fold_foundation::DocumentId::new();
    let scope = json!({"document":document,"definition":null,"object":null});
    let guard = |p: &Project| json!({"epoch":epoch,"request_id":ObjectId::new(),"base":p.snapshot().revision().0,"scope":scope});
    let g = guard(&project);
    let submitted = request(
        &mut dispatcher,
        &mut project,
        json!({"method":"propose","guard":g,"edit":{"kind":"project","edits":[
            {"op":"create","document":document,"type_id":fold_composition::COMPOSITION,"name":"Mixed"},
            {"op":"add_layer","composition":document,"kind":"scene"},
            {"op":"add_layer","composition":document,"kind":"image"}
        ]}}),
    );
    let prepared = finish(&mut dispatcher, &mut project, submitted);
    let handle = prepared["result"]["handle"].clone();
    let (host, _) = pollster::block_on(Host::headless(512 * 1024 * 1024)).unwrap();
    dispatcher.set_render_host(Some(host));
    let submitted = request(
        &mut dispatcher,
        &mut project,
        json!({"method":"observe_frame","target":{"kind":"candidate","handle":handle},"output":"video","time":Time::ZERO,"dimensions":[160,90]}),
    );
    let candidate = finish(&mut dispatcher, &mut project, submitted);
    assert_eq!(candidate["result"]["provenance"]["candidate"], handle);
    assert!(project.snapshot().state().documents.is_empty());
    let submitted = request(
        &mut dispatcher,
        &mut project,
        json!({"method":"inspect_candidate","handle":handle}),
    );
    let inspection = finish(&mut dispatcher, &mut project, submitted);
    let g = guard(&project);
    let applied = request(
        &mut dispatcher,
        &mut project,
        json!({"method":"apply","guard":g,"handle":handle,"receipt":inspection["result"]["receipt"]}),
    );
    assert!(applied.get("error").is_none(), "{applied}");
    let target = json!({"kind":"committed","epoch":epoch,"revision":project.snapshot().revision().0,"document":document});
    let submitted = request(
        &mut dispatcher,
        &mut project,
        json!({"method":"observe_frame","target":target,"output":"video","time":Time::ZERO,"dimensions":[160,90]}),
    );
    let committed = finish(&mut dispatcher, &mut project, submitted);
    assert_eq!(decode(&candidate), decode(&committed));
    assert!(committed["result"]["provenance"]["fingerprints"]["project"].is_string());
    let info = fold_app::packages::builtins()
        .output(&project.snapshot(), document)
        .unwrap();
    let mut invalid = request(
        &mut dispatcher,
        &mut project,
        json!({"method":"observe_frame","target":target,"output":"video","time":info.time(info.frames).unwrap(),"dimensions":[160,90]}),
    );
    let job = invalid["result"]["job"].clone();
    let deadline = Instant::now() + Duration::from_secs(30);
    while invalid["result"]["state"] == "running" {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
        invalid = request(
            &mut dispatcher,
            &mut project,
            json!({"method":"job_status","handle":job}),
        );
    }
    assert_eq!(invalid["error"]["code"], "type", "{invalid}");
}

#[test]
#[ignore = "requires native Vulkan and the packaged color runtime"]
fn mixed_scene_first_frame_retains_static_spheres_when_title_is_transparent() {
    check_scene_observations("Make-it-move.fold");
}
#[test]
#[ignore = "requires a native Vulkan adapter"]
fn mixed_compositor_fixture_matches_preview_after_public_source_edits() {
    check_scene_observations("m5a-mixed-scene.fold");
}
fn check_scene_observations(fixture: &str) {
    let mut project = fold_project::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../demos/{fixture}")),
        32,
    )
    .unwrap();
    let document = project
        .snapshot()
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_composition::COMPOSITION)
        .unwrap()
        .id;
    let mut dispatcher = Dispatcher::default();
    let (host, _) = pollster::block_on(Host::headless(512 * 1024 * 1024)).unwrap();
    // Simulate a visible full-size Viewer sharing its GPU host with the
    // observation renderer, which uses a different output raster.
    let mut viewer = Renderer::new(host.clone()).unwrap();
    let snapshot = project.snapshot();
    let cancel = fold_media::Cancel::default();
    let mut viewer_frame = |snapshot: &fold_project::Snapshot, time: Time, dimensions: [u32; 2]| {
        let scene = evaluate_scene_gpu(
            snapshot,
            &SceneRequest {
                source: fold_project::DocumentRef {
                    document,
                    output: "video".into(),
                    extensions: Default::default(),
                },
                time,
                dimensions,
                region: None,
                preview: None,
            },
            &mut viewer,
            &mut fold_media::Decoder::default(),
            &cancel,
        )
        .unwrap();
        fold_app::color::with_config(snapshot, |config| {
            let processor = config
                .map(|c| c.display(fold_color::WORKING_SPACE, &Default::default()))
                .transpose()?;
            viewer
                .output(&scene, processor.as_ref(), &cancel)?
                .readback(&cancel)
        })
        .unwrap()
        .rgba()
        .to_vec()
    };
    viewer_frame(&snapshot, Time::ZERO, [1920, 1080]);
    dispatcher.set_render_host(Some(host));
    for pass in 0..6 {
        // The original failure followed a public title-source edit, not merely
        // reopening a static project. Keep both Viewer and observation renderers
        // alive across the edit and render the newly committed snapshot.
        let snapshot = project.snapshot();
        let scene = snapshot
            .state()
            .documents
            .values()
            .find_map(|d| {
                let scene = fold_motion::scene::Scene::from_document(d).ok()?;
                scene
                    .objects
                    .iter()
                    .find(|o| o.name == "Title reveal")
                    .map(|o| {
                        (
                            d.id,
                            o.id,
                            fold_motion::source::canonical(&o.graph).unwrap(),
                        )
                    })
            })
            .unwrap();
        let next = if scene.2.contains("[-2.65,1.45,0.0]") {
            scene.2.replace("[-2.65,1.45,0.0]", "[-2.65,1.4,0.0]")
        } else {
            scene.2.replace("[-2.65,1.4,0.0]", "[-2.65,1.45,0.0]")
        };
        assert_ne!(next, scene.2, "Reproduction must change the title graph");
        let guard = json!({"epoch":dispatcher.epoch(),"request_id":ObjectId::new(),"base":snapshot.revision(),"scope":{"document":scene.0,"definition":null}});
        let submitted = request(
            &mut dispatcher,
            &mut project,
            json!({"method":"propose","guard":guard,"edit":{"kind":"project","edits":[{"op":"object_source","document":scene.0,"object":scene.1,"expected":scene.2,"source":next}]}}),
        );
        let proposal = finish(&mut dispatcher, &mut project, submitted);
        let handle = proposal["result"]["handle"].clone();
        let submitted = request(
            &mut dispatcher,
            &mut project,
            json!({"method":"inspect_candidate","handle":handle}),
        );
        let inspected = finish(&mut dispatcher, &mut project, submitted);
        let mut apply_guard = guard;
        apply_guard["request_id"] = json!(ObjectId::new());
        let applied = request(
            &mut dispatcher,
            &mut project,
            json!({"method":"apply","guard":apply_guard,"handle":handle,"receipt":inspected["result"]["receipt"]}),
        );
        finish(&mut dispatcher, &mut project, applied);
        let snapshot = project.snapshot();
        let target = json!({"kind":"committed","epoch":dispatcher.epoch(),"revision":snapshot.revision(),"document":document});
        let mut frames = vec![];
        for time in [Time::ZERO, Time::new(1, 1).unwrap(), Time::ZERO] {
            let submitted = request(
                &mut dispatcher,
                &mut project,
                json!({"method":"observe_frame","target":target,"output":"video","time":time,"dimensions":[960,540]}),
            );
            let handle = submitted["result"]["job"].clone();
            let mut observed = submitted;
            let deadline = Instant::now() + Duration::from_secs(90);
            while observed["result"]["state"] == "running" {
                assert!(Instant::now() < deadline, "Concurrent frame job timed out");
                // Continue actual scene evaluation and GPU submission, not just
                // presentation of a previously rendered Viewer texture.
                viewer_frame(&snapshot, Time::ZERO, [1920, 1080]);
                observed = request(
                    &mut dispatcher,
                    &mut project,
                    json!({"method":"job_status","handle":handle}),
                );
            }
            assert!(observed.get("error").is_none(), "{observed}");
            let pixels = decode(&observed);
            assert_eq!(
                pixels,
                viewer_frame(&snapshot, time, [960, 540]),
                "Public observation differs from native preview"
            );
            frames.push(pixels);
        }
        let crop = |image: &Vec<u8>| image[960 * 300 * 4..].to_vec();
        let reference = crop(&frames[1]);
        let colors: std::collections::BTreeSet<_> = reference.chunks_exact(4).collect();
        assert!(
            colors.len() > 100,
            "Held frame must contain shaded colored spheres"
        );
        assert_eq!(
            crop(&frames[0]),
            reference,
            "First zero-opacity frame lost static spheres"
        );
        assert_eq!(
            crop(&frames[2]),
            reference,
            "Reverse seek lost static spheres"
        );
        eprintln!("Source-edit observation pass {pass}: static sphere regions agree");
    }
}
