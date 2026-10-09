//! C0 read-only fixture benchmark. Emits JSON measurements and stderr stage events.
//! Offscreen panel drawing and command-to-refresh are not physical presentation latency.
use fold_app::session::Session;
use fold_foundation::{DocumentId, Time};
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_project::{EditBatch, Mutation};
use serde_json::{Value, json};
use std::{hint::black_box, path::Path, time::Instant};

fn measure(name: &str, mut work: impl FnMut() -> Result<(), String>) -> Result<Value, String> {
    let timestamp = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    };
    eprintln!(
        "{}",
        json!({"stage":name,"event":"begin","unix_ms":timestamp()})
    );
    let mut samples = Vec::new();
    for _ in 0..31 {
        let start = Instant::now();
        work()?;
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let cold = samples.remove(0);
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    eprintln!(
        "{}",
        json!({"stage":name,"event":"end","unix_ms":timestamp()})
    );
    Ok(
        json!({"name":name,"first_ms":cold,"warm_ms":samples,"median_ms":(sorted[14]+sorted[15])/2.,"p95_ms":sorted[28]}),
    )
}

fn main() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = fold_project::load(
        root.join("docs/rewrite/evidence/m5b-controls-baseline.fold"),
        32,
    )
    .map_err(|e| e.to_string())?;
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .next()
        .ok_or("fixture is empty")?;
    let construction = fold_motion::model::Construction::from_document(document)?;
    let output = construction
        .definition(construction.root.definition)?
        .outputs[0]
        .id;
    let prepared = fold_motion::evaluation::Prepared::new(construction.clone())?;
    let mut measurements = Vec::new();
    measurements.push(measure("motion_decode_structure", || {
        black_box(fold_motion::model::Construction::from_document(document)?);
        Ok(())
    })?);
    measurements.push(measure("motion_clone_validate_prepare", || {
        black_box(fold_motion::evaluation::Prepared::new(
            construction.clone(),
        )?);
        Ok(())
    })?);
    for time in [
        Time::ZERO,
        Time::new(1, 2).map_err(|e| e.to_string())?,
        Time::new(2, 1).map_err(|e| e.to_string())?,
    ] {
        measurements.push(measure(&format!("motion_evaluate_{time:?}"), || {
            black_box(prepared.evaluate(output, time, Default::default(), &|| Ok(()))?);
            Ok(())
        })?);
    }
    measurements.push(measure("motion_precancelled_evaluation", || {
        if prepared
            .evaluate(output, Time::ZERO, Default::default(), &|| {
                Err("C0 cancellation".into())
            })
            .is_ok()
        {
            return Err("pre-cancelled evaluation succeeded".into());
        }
        Ok(())
    })?);
    for count in [16, 64, 256] {
        use fold_compositor::{Composite, Node, OperationConfig as P};
        let mut nodes = vec![Node::new(
            P::Solid {
                rgba: [0.2, 0.3, 0.4, 1.],
            },
            vec![],
        )];
        for _ in 1..count - 1 {
            nodes.push(Node::new(
                P::ColorGrade {
                    settings: Default::default(),
                },
                vec![nodes.last().unwrap().id],
            ));
        }
        let end = Node::new(P::Output, vec![nodes.last().unwrap().id]);
        let graph = Composite {
            info: fold_media::VideoInfo {
                width: 480,
                height: 270,
                rate: [24, 1],
                frames: 96,
            },
            output: end.id,
            nodes: {
                nodes.push(end);
                nodes
            },
            extensions: Default::default(),
        };
        let record = graph.document(DocumentId::new())?;
        measurements.push(measure(
            &format!("compositor_decode_validate_{count}"),
            || {
                black_box(Composite::from_document(&record)?);
                Ok(())
            },
        )?);
        measurements.push(measure(
            &format!("compositor_configurations_{count}"),
            || {
                for node in &graph.nodes {
                    black_box(node.try_configuration()?);
                }
                Ok(())
            },
        )?);
    }
    let mut scene_project = fold_project::load(root.join("demos/m5b-background-spheres.fold"), 32)
        .map_err(|e| e.to_string())?;
    let original = scene_project.snapshot();
    let scene_document = original
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_composition::COMPOSITION)
        .ok_or("missing Composition")?;
    let scene_id = scene_document.id;
    let mut changed = scene_document.as_ref().clone();
    changed
        .extensions
        .insert("c0-benchmark".into(), json!(true));
    scene_project
        .commit(EditBatch {
            base: original.revision(),
            mutations: vec![Mutation::PutDocument(changed)],
        })
        .map_err(|e| e.to_string())?;
    let retention = scene_project.retention();
    let mut session = Session::new(scene_project);
    measurements.push(measure("session_undo_redo_with_refresh", || {
        session.command(DesktopCommand::Undo);
        if session.state().dirty && !session.state().can_redo {
            return Err(session.state().status.clone());
        }
        session.command(DesktopCommand::Redo);
        if !session.state().can_undo {
            return Err(session.state().status.clone());
        }
        Ok(())
    })?);
    let mut panels = fold_ui::sdk::PanelRegistry::new(&fold_app::packages::builtins());
    fold_app::project_panel::register(&mut panels)?;
    fold_timeline::ui::register(&mut panels)?;
    fold_compositor::ui::register(&mut panels)?;
    fold_composition::ui::register(&mut panels)?;
    fold_motion::ui::register(&mut panels)?;
    let mut probe = fold_ui::WorkspaceProbe::new(panels);
    probe.bind(
        fold_composition::COMPOSITION,
        scene_id,
        fold_platform::workspace::LinkGroup::A,
    );
    for width in [1600., 580.] {
        measurements.push(measure(&format!("offscreen_shell_frame_{width}"), || {
            probe.frame(&mut session, width);
            Ok(())
        })?);
    }
    println!(
        "{}",
        json!({
            "source_sha256":env!("FOLD_SOURCE_SHA256"),"profile":env!("FOLD_BUILD_PROFILE"),
            "motion_nodes":construction.definitions.iter().map(|d|d.nodes.len()).sum::<usize>(),
            "motion_payload_bytes":document.payload.len(),"measurements":measurements,
            "retention":{"roots":retention.roots,"payload_bytes":retention.payload_bytes,"external_handles":retention.external_handles},
            "limitations":"31 sequential in-process samples, first separated from 30 warm samples; preparation includes clone; shell uses real ImGui panels offscreen without GPU/scanout; undo/redo includes two refreshes, not edit-to-present; logical retained payload bytes exclude allocator overhead; no CPU allocation-count claim"
        })
    );
    Ok(())
}
