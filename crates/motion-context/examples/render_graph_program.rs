//! Qualify the public video capability through Fold's normal image evaluator.
//! Pixel readback is an explicit qualification step after GPU evaluation.
use fold_foundation::DocumentId;
use fold_media::{Cancel, Decoder};
use fold_motion_canvas::{
    model::{Content, ControlValue},
    protocol::{Alpha, Encoding, Request, Response},
};
use fold_motion_context::{
    runtime::{Config, Profile},
    video::Provider,
};
use fold_platform::VideoRegistry;
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use fold_render::{
    ImageOp,
    gpu::{Host, Renderer},
};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Expected scene directory, SDK directory and Rust compiler path".into());
    }
    let directory = PathBuf::from(&args[1]);
    let requests: Vec<Request> =
        std::io::BufReader::new(std::fs::File::open(directory.join("requests.jsonl"))?)
            .lines()
            .map(|line| Ok(serde_json::from_str(&line?)?))
            .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    let fonts = requests
        .first()
        .and_then(|request| request.fonts.clone())
        .ok_or("Missing fonts")?;
    let mut registry = VideoRegistry::default();
    registry.register(Provider::new(
        Config {
            bridge: PathBuf::from(&args[2]).join("crates/motion-fframes"),
            cache: directory.join("cache"),
            rustc: (&args[3]).into(),
            profile: Profile::Release,
            compile_timeout: Duration::from_secs(600),
            frame_timeout: Duration::from_secs(30),
        },
        fonts,
    ))?;
    let cancel = Cancel::default();
    let (host, adapter) = pollster::block_on(Host::headless(256 * 1024 * 1024))?;
    let mut renderer = Renderer::new(host.clone())?;
    let mut decoder = Decoder::default();
    let mut project = Project::new(8);
    let document = DocumentId::new();
    let reference = DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    let mut output =
        std::io::BufWriter::new(std::fs::File::create(directory.join("graph-frames.bin"))?);
    let mut timings = vec![];
    for mut request in requests {
        if request.id == 2 {
            for object in request.canvas.objects.values_mut() {
                if let Content::Script { controls, .. } = &mut object.content {
                    for control in controls.values_mut() {
                        if control.name == "Count" {
                            if let ControlValue::Number(value) = &mut control.value {
                                value.value = 8000.;
                            }
                        }
                    }
                }
            }
        }
        project.commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(request.canvas.document(document)?)],
        })?;
        let snapshot = project.snapshot();
        let info = registry.outputs(&snapshot.state().documents[&document])[0]
            .info
            .clone()?;
        assert_eq!(
            [info.width, info.height],
            [request.canvas.width, request.canvas.height]
        );
        assert_eq!(info.rate, request.canvas.rate);
        let mut graph = registry.compile_with(
            &snapshot,
            &reference,
            request.time,
            request.dimensions,
            &cancel,
        )?;
        // Commit a later empty canvas after compilation. The retained source must
        // still render the captured scene, not consult mutable project state.
        let mut empty = request.canvas.clone();
        empty.objects.clear();
        empty.roots.clear();
        project.commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(empty.document(document)?)],
        })?;
        graph.nodes.push(ImageOp::Opacity {
            input: graph.output,
            opacity: 0.5,
        });
        graph.output = graph.nodes.len() - 1;
        let start = Instant::now();
        let mut frame = renderer.evaluate(graph, None, &mut decoder, &cancel)?;
        assert_eq!(frame.statistics.upload_bytes, 0);
        assert_eq!(frame.statistics.readback_bytes, 0);
        let pixels = frame.readback(&cancel)?;
        timings.push(start.elapsed().as_secs_f64() * 1000.);
        serde_json::to_writer(
            &mut output,
            &Response {
                id: request.id,
                dimensions: request.dimensions,
                bytes: pixels.pixels().len() * 16,
                encoding: Encoding::LinearSrgb,
                alpha: Alpha::Premultiplied,
                error: None,
            },
        )?;
        output.write_all(b"\n")?;
        for pixel in pixels.pixels() {
            for value in pixel {
                output.write_all(&value.to_le_bytes())?;
            }
        }
    }
    output.flush()?;
    std::fs::write(
        directory.join("graph-evidence.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "adapter": adapter.name, "video_registry": true, "immutable_snapshot": true,
            "gpu_opacity": 0.5, "cpu_upload_bytes": 0, "working_readback_before_qualification": 0,
            "evaluation_and_qualification_readback_ms": timings, "artist_qualified": false,
            "application_registered": false,
        }))?,
    )?;
    Ok(())
}
