//! Saved-project EXR/compositor probe: exr_profile PROJECT [FRAMES=3] [DOCUMENT].
//! FOLD_PROFILE_VERIFY=1 compares display pixels with CPU execution.
//! FOLD_PROFILE_PIPELINE=1 prepares the next frame on a separate source worker.
//! FOLD_PROFILE_MAX_MS sets an optional distinct-frame mean acceptance threshold.
//! Reports display-ready headless times, not physical desktop playback cadence.
use fold_media::{Cancel, Decoder};
use fold_project::DocumentRef;
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args.len() > 3 {
        return Err("expected PROJECT [FRAMES] [DOCUMENT]".into());
    }
    let count: u32 = args.get(1).map_or(Ok(3), |s| s.parse())?;
    if count < 2 {
        return Err("measure at least two distinct frames".into());
    }
    let verify = std::env::var_os("FOLD_PROFILE_VERIFY").is_some();
    let pipeline = std::env::var_os("FOLD_PROFILE_PIPELINE").is_some();
    let project = fold_project::load(Path::new(&args[0]), 0)?;
    let snapshot = project.snapshot();
    let packages = fold_app::packages::builtins();
    let cancel = Cancel::default();
    let mut decoder = Decoder::default();
    let (host, adapter) = pollster::block_on(fold_render::gpu::Host::headless(
        fold_render::gpu::Host::DEFAULT_BUDGET,
    ))?;
    eprintln!("adapter={adapter:?}");
    let mut renderer = fold_render::gpu::Renderer::new(host.clone())?;
    fold_app::color::with_config(&snapshot, |config| {
        let display = config
            .map(|c| c.display(fold_color::WORKING_SPACE, &Default::default()))
            .transpose()?;
        for doc in snapshot.state().documents.values() {
            let document_id = serde_json::to_value(doc.id)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned();
            if args.get(2).is_some_and(|id| *id != document_id) {
                continue;
            }
            let source = DocumentRef {
                document: doc.id,
                output: "video".into(),
                extensions: Default::default(),
            };
            let info = packages.output_ref(&snapshot, &source)?;
            let dimensions = [info.width, info.height];
            let count = count.min(info.frames);
            if count < 2 {
                return Err("document has fewer than two frames".into());
            }
            let compile = |frame| {
                packages.video_with(
                    &snapshot,
                    &source,
                    info.time(frame).map_err(|e| e.to_string())?,
                    dimensions,
                    &cancel,
                )
            };
            let mut elapsed = Vec::new();
            let mut preparation: Option<
                std::thread::JoinHandle<
                    Result<fold_render::PreparedSources, fold_foundation::Failure>,
                >,
            > = None;
            for (iteration, frame) in (0..count).chain(std::iter::once(0)).enumerate() {
                let start = Instant::now();
                let graph = compile(frame)?;
                let nodes = graph.nodes.len();
                let compile_ms = start.elapsed().as_secs_f64() * 1000.;
                // Demand may join each source as it becomes ready; waiting for
                // the entire speculative graph here would serialize uploads.
                let scene = renderer.evaluate(graph.clone(), config, &mut decoder, &cancel)?;
                let prepared = preparation
                    .take()
                    .map(|task| task.join().expect("source preparation panic"));
                if let Some(Err(error)) = &prepared {
                    eprintln!("source preparation skipped: {error}");
                }
                drop(prepared);
                if pipeline && iteration + 1 < count as usize {
                    let next = compile(frame + 1)?;
                    let token = cancel.clone();
                    preparation = Some(std::thread::spawn(move || {
                        fold_render::prepare_sources(&next, &token)
                    }));
                }
                let stats = scene.statistics;
                let mut output = renderer.output(&scene, display.as_ref(), &cancel)?;
                while !output.is_ready()? {
                    host.poll()?;
                    if start.elapsed() > Duration::from_secs(30) {
                        return Err("frame timeout".into());
                    }
                    std::thread::sleep(Duration::from_micros(100));
                }
                let wall_ms = start.elapsed().as_secs_f64() * 1000.;
                if frame > 0 {
                    elapsed.push(wall_ms);
                }
                println!(
                    "{}",
                    serde_json::json!({"doc":doc.id,"frame":frame,"nodes":nodes,"pipeline":pipeline,"wall_ms":wall_ms,"compile_ms":compile_ms,"gpu_ms":scene.gpu_nanoseconds()?.map(|n|n/1e6),"adapter_ms":stats.cpu_adapter_nanoseconds as f64/1e6,"prepare_ms":stats.graph_prepare_nanoseconds as f64/1e6,"upload_bytes":stats.upload_bytes,"upload_allocations":stats.upload_allocations,"upload_reuses":stats.upload_reuses,"resident_stills":stats.resident_still_nodes,"gpu_allocated":host.memory().allocated,"gpu_peak":host.memory().peak,"decoded":fold_media::budget::decoded_usage().bytes,"working":fold_media::budget::working_usage().bytes})
                );
                if verify && (frame == 0 || frame == count - 1) {
                    let reference = match config {
                        Some(c) => fold_render::render_aces_with(
                            graph,
                            c,
                            &mut Decoder::default(),
                            &cancel,
                        ),
                        None => fold_render::render_with(
                            graph.clone(),
                            &mut Decoder::default(),
                            &cancel,
                        ),
                    }?
                    .over_black();
                    let expected = match display.as_ref() {
                        Some(processor) => reference.to_output(processor)?,
                        None => reference.to_display()?,
                    };
                    let actual = output.readback(&cancel)?;
                    let error = actual
                        .rgba()
                        .iter()
                        .zip(expected.rgba())
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap_or(0);
                    if error > 1 {
                        return Err(format!("CPU/GPU mismatch: {error} codes").into());
                    }
                    eprintln!("verified {document_id} frame {frame}: max_code_error={error}");
                }
            }
            let mean = elapsed.iter().sum::<f64>() / elapsed.len() as f64;
            elapsed.sort_by(f64::total_cmp);
            let p95 = elapsed[((elapsed.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)];
            eprintln!(
                "{} distinct={} mean_ms={mean:.3} p95_ms={p95:.3}",
                document_id,
                elapsed.len()
            );
            if let Ok(limit) = std::env::var("FOLD_PROFILE_MAX_MS") {
                let limit: f64 = limit.parse().map_err(|_| "invalid FOLD_PROFILE_MAX_MS")?;
                if mean > limit {
                    return Err(format!("frame budget exceeded: {mean:.3} > {limit:.3} ms").into());
                }
            }
        }
        Ok(())
    })?;
    Ok(())
}
