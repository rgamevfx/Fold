//! Manual release benchmark of the saved Katie workload, independent of viewer caches.
use fold_foundation::Time;
use fold_motion::{Motion, evaluation::Evaluator};
use std::time::Instant;

#[test]
#[ignore = "release workload benchmark; run with --ignored --nocapture"]
fn katie_scene() {
    let project = fold_project::load("../../demos/katie-chromatic-bars.fold", 1).unwrap();
    let snapshot = project.snapshot();
    let document = &snapshot.state().documents[&serde_json::from_value(serde_json::json!(
        "0e7284c5-e13b-4244-b455-f4d71772c5ac"
    ))
    .unwrap()];
    let mut motion = Motion::from_document(document).unwrap();
    let mode = std::env::var("FOLD_PROFILE_STAGE").unwrap_or_default();
    if mode == "empty_wrangle" {
        for group in motion.groups.values_mut() {
            for node in &mut group.graph.nodes {
                if node.kind == "fold.motion.wrangle" {
                    node.settings["source"] = "".into();
                }
            }
        }
    }
    if mode == "no_mask" {
        for object in &mut motion.scene.as_mut().unwrap().objects {
            object.masks.clear();
        }
    }
    let mut times = Vec::new();
    for frame in 0..8 {
        let start = Instant::now();
        let mut evaluator = Evaluator::new(&motion, Time::new(frame, 30).unwrap()).unwrap();
        let value = evaluator.scene().unwrap();
        std::hint::black_box(value);
        let ms = start.elapsed().as_secs_f64() * 1000.;
        println!("frame {frame}: scene {ms:.3} ms");
        if frame > 0 {
            times.push(ms);
        }
    }
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!("Katie warm scene mean {mean:.3} ms");
    if let Ok(limit) = std::env::var("FOLD_PROFILE_MAX_MS") {
        assert!(
            mean <= limit.parse::<f64>().unwrap(),
            "Katie scene exceeded frame budget: {mean:.3} ms"
        );
    }
}
