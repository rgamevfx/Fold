//! Actual retained GPU runtime. Reference readback occurs only in this example.
use fold_foundation::{DocumentId, ObjectId};
use fold_media::process::Cancel;
use fold_motion_canvas::{
    model::{Content, ControlValue},
    protocol::{Alpha, Encoding, Request, Response},
};
use fold_motion_context::runtime::{
    Config, Profile,
    gpu::{GpuRuntime, Intent, Snapshot},
};
use fold_project::Revision;
use fold_render::gpu::Host;
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !(4..=5).contains(&args.len()) {
        return Err("Expected scene directory, SDK directory, Rust compiler path and optional warm-frame count".into());
    }
    let warm_count: usize = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(0);
    if warm_count != 0 && !(20..=1000).contains(&warm_count) {
        return Err("Warm-frame count must be 20 through 1000".into());
    }
    let directory = PathBuf::from(&args[1]);
    let cancel = Cancel::default();
    let (host, adapter) = pollster::block_on(Host::headless(256 * 1024 * 1024))?;
    let mut runtime = GpuRuntime::new(
        Config {
            bridge: PathBuf::from(&args[2]).join("crates/motion-fframes"),
            cache: directory.join("cache"),
            rustc: (&args[3]).into(),
            profile: Profile::Release,
            compile_timeout: Duration::from_secs(600),
            frame_timeout: Duration::from_secs(30),
        },
        host.clone(),
        &cancel,
    )?;
    let document = DocumentId::new();
    let epoch = ObjectId::new();
    let mut fonts = vec![];
    let mut output = std::io::BufWriter::new(std::fs::File::create(
        directory.join("retained-gpu-frames.bin"),
    )?);
    let input = std::io::BufReader::new(std::fs::File::open(directory.join("requests.jsonl"))?);
    let mut timings = vec![];
    let mut warm_source = None;
    for line in input.lines() {
        let mut request: Request = serde_json::from_str(&line?)?;
        if let Some(bytes) = request.fonts {
            fonts = bytes;
        }
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
        let mut source = request.canvas.document(document)?;
        source.revision = Revision(request.id);
        if request.id == 3 {
            warm_source = Some(source.clone());
        }
        let snapshot =
            Snapshot::from_document(epoch, request.id, &source, request.time, request.dimensions)?;
        let intent = snapshot.intent();
        let start = Instant::now();
        let rendered = runtime.render(&snapshot, &fonts, &cancel)?;
        timings.push(start.elapsed().as_secs_f64() * 1000.);
        let mut frame = rendered.into_current(intent)?;
        assert_eq!(frame.descriptor()?.timing.unwrap().timestamp, intent.time);
        assert_eq!(frame.statistics.upload_bytes, 0);
        assert_eq!(frame.statistics.readback_bytes, 0);
        let pixels = frame.readback(&cancel)?;
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
        drop(frame);
        host.poll()?;
        assert_eq!(host.memory().allocated, host.memory().scratch);
        assert_eq!(
            host.memory().scratch,
            (host.memory().budget / 8).min(64 * 1024 * 1024)
        );
        // A completed old revision must still be refused by publication.
        if request.id == 3 {
            let old = runtime.render(&snapshot, &fonts, &cancel)?;
            assert!(
                old.into_current(Intent {
                    revision: Revision(intent.revision.0 + 1),
                    ..intent
                })
                .is_err()
            );
            host.poll()?;
            assert_eq!(host.memory().allocated, host.memory().scratch);
            assert_eq!(
                host.memory().scratch,
                (host.memory().budget / 8).min(64 * 1024 * 1024)
            );
            for current in [
                Intent {
                    epoch: ObjectId::new(),
                    ..intent
                },
                Intent {
                    generation: intent.generation + 1,
                    ..intent
                },
                Intent {
                    time: fold_foundation::Time::ZERO,
                    ..intent
                },
                Intent {
                    dimensions: [320, 180],
                    ..intent
                },
            ] {
                assert!(
                    runtime
                        .render(&snapshot, &fonts, &cancel)?
                        .into_current(current)
                        .is_err()
                );
            }
            // A changed program must not compile/start when its requested image
            // exceeds the host allowance. Admission precedes all worker work.
            let mut too_large = fold_motion_canvas::model::Canvas::from_document(&source)?;
            for object in too_large.objects.values_mut() {
                if let Content::Script { source, .. } = &mut object.content {
                    source.push_str("\n// rejected demand must not compile this source change\n");
                }
            }
            let changed = too_large.replace_document(&source)?;
            let limit = host.device().limits().max_texture_dimension_2d.min(16_384);
            let rejected = Snapshot::from_document(
                epoch,
                intent.generation + 1,
                &changed,
                intent.time,
                [limit, limit],
            )?;
            let error = runtime
                .render(&rejected, &fonts, &cancel)
                .err()
                .ok_or("Oversized demand was accepted")?;
            assert_eq!(error.kind, fold_foundation::FailureKind::RequestTooLarge);
            assert_eq!(host.memory().allocated, host.memory().scratch);
            assert_eq!(
                host.memory().scratch,
                (host.memory().budget / 8).min(64 * 1024 * 1024)
            );
        }
    }
    output.flush()?;
    let mut warm_timings = vec![];
    let mut warm_phases = vec![];
    if let Some(source) = warm_source {
        for index in 0..warm_count {
            let start = Instant::now();
            let snapshot = Snapshot::from_document(
                epoch,
                index as u64 + 100,
                &source,
                fold_foundation::Time::new((index % 120) as i64, 60)?,
                [640, 360],
            )?;
            let prepared = Instant::now();
            let rendered = runtime.render(&snapshot, &fonts, &cancel)?;
            let produced = Instant::now();
            let frame = rendered.into_current(snapshot.intent())?;
            assert_eq!(frame.statistics.upload_bytes, 0);
            assert_eq!(frame.statistics.readback_bytes, 0);
            drop(frame);
            let released = Instant::now();
            host.poll()?;
            assert_eq!(host.memory().allocated, host.memory().scratch);
            assert_eq!(
                host.memory().scratch,
                (host.memory().budget / 8).min(64 * 1024 * 1024)
            );
            if index >= 10 {
                warm_timings.push(start.elapsed().as_secs_f64() * 1000.);
                warm_phases.push(serde_json::json!({
                    "snapshot_ms":prepared.duration_since(start).as_secs_f64()*1000.,
                    "render_ms":produced.duration_since(prepared).as_secs_f64()*1000.,
                    "publication_release_ms":released.duration_since(produced).as_secs_f64()*1000.,
                    "poll_ms":released.elapsed().as_secs_f64()*1000.
                }));
            }
        }
    }
    warm_timings.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        warm_timings
            .get(((warm_timings.len().saturating_sub(1)) as f64 * fraction).ceil() as usize)
            .copied()
    };
    let statistics = runtime.statistics();
    std::fs::write(
        directory.join("retained-gpu-diagnostics.txt"),
        runtime.diagnostics(),
    )?;
    let retained_cache_allowance = host.memory().scratch;
    assert!(retained_cache_allowance > 0);
    drop(runtime);
    host.poll()?;
    assert_eq!(host.memory().allocated, 0);
    assert_eq!(statistics.workers_started, 1);
    assert_eq!(statistics.frames_completed, 8 + warm_count as u64);
    std::fs::write(
        directory.join("retained-gpu-evidence.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "device":adapter.name,"frame_request_ms_excluding_reference_readback":timings,"cpu_pixel_transport":false,
            "warm_frames":warm_count,"warmup_frames_discarded":warm_count.min(10),"warm_measured_frames":warm_timings.len(),
            "warm_host_phases":warm_phases,
            "warm_frame_ms_p50":percentile(0.5),"warm_frame_ms_p95":percentile(0.95),"warm_frame_ms_max":warm_timings.last(),
            "warm_measurement":"snapshot preparation, render, publication, release and host poll; no reference readback",
            "host_profile":if cfg!(debug_assertions) { "debug" } else { "release" },"worker_profile":"release",
        "stale_epoch_revision_generation_time_dimensions_rejected":true,"memory_released":true,
        "retained_cache_allowance":retained_cache_allowance,"cache_released_after_worker_exit":true,
        "runtime_statistics":statistics,"app_integration_qualified":false,"performance_qualified":false
        ,"producer_memory_reserved_before_demand":true,"oversized_changed_program_rejected_before_start":true,
        "producer_release_confirmed_before_reservation_drop":true
        }))?,
    )?;
    Ok(())
}
