//! Uncached Phase 57 image-effect and Motion workloads. Run with --release --features gpu.
//! Prints per-frame preparation, GPU readiness, transfer and memory measurements as JSONL.
use fold_compositor::{
    Composite, Node, Parameters as P,
    presets::{self, Preset},
};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::effects::Instance;
use fold_project::{Document, DocumentRef, EditBatch, Mutation, Project};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
fn reference(document: DocumentId) -> DocumentRef {
    DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count: u32 = std::env::args()
        .nth(1)
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(60);
    if !(2..=600).contains(&count) {
        return Err("Use 2..600 frames".into());
    }
    let info = fold_media::VideoInfo {
        width: 1920,
        height: 1080,
        rate: [30, 1],
        frames: 600,
    };
    let (host, adapter) = pollster::block_on(fold_render::gpu::Host::headless(
        fold_render::gpu::Host::DEFAULT_BUDGET,
    ))?;
    println!(
        "{}",
        serde_json::json!({"adapter":format!("{adapter:?}"),"frames":count,"dimensions":[1920,1080],"cache":"No frame-output cache; same renderer/pipeline cache; changing sample time and controls"})
    );
    for case in ["glow", "stack8", "motion1024_glow"] {
        let mut documents: Vec<Document> = vec![];
        let glow = DocumentId::new();
        let film = DocumentId::new();
        let grain = DocumentId::new();
        for (preset, id) in [
            (Preset::Glow, glow),
            (Preset::FilmLook, film),
            (Preset::Grain, grain),
        ] {
            documents.push(presets::create(preset, id)?);
        }
        let source = if case.starts_with("motion") {
            use fold_motion::{
                authoring::{add_node, node},
                fields::Datum,
            };
            let mut m = fold_motion::Motion::empty();
            m.info = info.clone();
            m.retain_canvas();
            let shape = add_node(&mut m, "fold.motion.ellipse")?;
            node(&mut m, shape)?.set("width", Datum::Scalar(16.));
            node(&mut m, shape)?.set("height", Datum::Scalar(16.));
            let fill = add_node(&mut m, "fold.motion.fill")?;
            node(&mut m, fill)?.connect("content", shape, "content");
            node(&mut m, fill)?.set("color", Datum::Color([1., 0.75, 0.5, 1.]));
            let points = add_node(&mut m, "fold.motion.grid_points")?;
            node(&mut m, points)?.set("columns", Datum::Scalar(32.));
            node(&mut m, points)?.set("rows", Datum::Scalar(32.));
            node(&mut m, points)?.set("spacing", Datum::Vector([60., 32.]));
            node(&mut m, points)?.set("origin", Datum::Vector([20., 20.]));
            let copies = add_node(&mut m, "fold.motion.instance_on_points")?;
            node(&mut m, copies)?.connect("source", fill, "content");
            node(&mut m, copies)?.connect("points", points, "points");
            let output = m.graph.output.node;
            node(&mut m, output)?.connect("content", copies, "content");
            let id = DocumentId::new();
            documents.push(m.document(id)?);
            Node::new(
                P::Read {
                    source: fold_compositor::Source::Document {
                        source: reference(id),
                        info: info.clone(),
                    },
                    start: Time::ZERO,
                    source_start: Time::ZERO,
                    duration: info.time(info.frames)?,
                },
                vec![],
            )
        } else {
            Node::new(
                P::Solid {
                    rgba: [1., 0.75, 0.5, 1.],
                },
                vec![],
            )
        };
        let mut nodes = vec![source];
        let definitions = if case == "stack8" {
            vec![film, grain, film, grain, film, grain, film, grain]
        } else {
            vec![glow]
        };
        for id in definitions {
            let mut effect = Instance {
                id: ObjectId::new(),
                definition: reference(id),
                version: 1,
                values: BTreeMap::new(),
                animation: BTreeMap::new(),
                enabled: true,
                extensions: Default::default(),
            };
            if id == glow {
                effect.values.insert("threshold".into(), vec![0.5]);
                let mut curve = fold_animation::Curve::default();
                curve.insert(Time::ZERO, 0.5);
                curve.insert(Time::new(20, 1)?, 1.5);
                effect.animation.insert("strength".into(), vec![curve]);
            }
            let node = Node::new(
                P::Instance {
                    instance: effect,
                    images: vec!["source".into()],
                },
                vec![nodes.last().unwrap().id],
            );
            nodes.push(node);
        }
        let output = Node::new(P::Output, vec![nodes.last().unwrap().id]);
        let output_id = output.id;
        nodes.push(output);
        let id = DocumentId::new();
        documents.push(
            Composite {
                info: info.clone(),
                nodes,
                output: output_id,
                extensions: Default::default(),
            }
            .document(id)?,
        );
        let mut project = Project::new(1);
        project.commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: documents.into_iter().map(Mutation::PutDocument).collect(),
        })?;
        let snapshot = project.snapshot();
        let packages = fold_app::packages::builtins();
        let cancel = fold_media::Cancel::default();
        let mut decoder = fold_media::Decoder::default();
        let mut renderer = fold_render::gpu::Renderer::new(host.clone())?;
        for frame in 0..count {
            let begin = Instant::now();
            let graph = packages.video_with(
                &snapshot,
                &reference(id),
                Time::new(i64::from(frame), 30)?,
                [1920, 1080],
                &cancel,
            )?;
            let prepare = begin.elapsed().as_secs_f64() * 1000.;
            let scene = renderer.evaluate(graph, None, &mut decoder, &cancel)?;
            while !scene.is_ready()? {
                host.poll()?;
                if begin.elapsed() > Duration::from_secs(30) {
                    return Err("frame timeout".into());
                }
                std::thread::sleep(Duration::from_micros(100));
            }
            let stats = scene.statistics;
            println!(
                "{}",
                serde_json::json!({"case":case,"frame":frame,"prepare_ms":prepare,"ready_ms":begin.elapsed().as_secs_f64()*1000.,"gpu_ms":scene.gpu_nanoseconds()?.map(|v|v/1e6),"shader_compilations":stats.shader_compilations,"passes":stats.compute_passes,"upload_bytes":stats.upload_bytes,"readback_bytes":stats.readback_bytes,"status_bytes":stats.status_readback_bytes,"vector_instances":stats.vector_instances,"vector_prepare_ms":stats.vector_prepare_nanoseconds as f64/1e6,"vector_upload_bytes":stats.vector_upload_bytes,"allocated_bytes":host.memory().allocated,"peak_bytes":host.memory().peak})
            );
        }
    }
    Ok(())
}
