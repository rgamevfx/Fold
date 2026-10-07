//! Release GPU motion/compositor benchmark: PROJECT [FRAMES] [WIDTH] [HEIGHT].
//! Excludes viewer caching and UI latency; defaults to every other frame at 24 fps.
//! FOLD_PROFILE_MAX_MS enforces a warm-frame mean; FOLD_PROFILE_VERIFY checks CPU parity.
//! FOLD_PROFILE_SOURCE=motion selects the uncomposited document. FOLD_PROFILE_START,
//! FOLD_PROFILE_STEP and FOLD_PROFILE_RATE select sample times (defaults 0, 2, 24).
use fold_foundation::Time;
use fold_project::DocumentRef;
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let project = fold_project::load(args.first().ok_or("PROJECT [FRAMES] [WIDTH] [HEIGHT]")?, 1)?;
    let count: u32 = args.get(1).map(|v| v.parse()).transpose()?.unwrap_or(12);
    if !(2..=1200).contains(&count) {
        return Err("frame count must be 2..=1200".into());
    }
    let size = [
        args.get(2).map(|v| v.parse()).transpose()?.unwrap_or(960),
        args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(540),
    ];
    let snapshot = project.snapshot();
    let source_type = match std::env::var("FOLD_PROFILE_SOURCE").as_deref() {
        Ok("motion") => "fold.motion.document",
        Ok("composition") => "fold.composition.document",
        _ => "fold.compositor.composite",
    };
    let requested_document: Option<fold_foundation::DocumentId> =
        std::env::var("FOLD_PROFILE_DOCUMENT")
            .ok()
            .map(|id| serde_json::from_value(serde_json::Value::String(id)))
            .transpose()?;
    let document = snapshot
        .state()
        .documents
        .iter()
        .find_map(|(id, document)| {
            (document.type_id == source_type
                && requested_document.is_none_or(|wanted| wanted == *id))
            .then_some(*id)
        })
        .ok_or("no matching source document")?;
    let setting = |name, fallback| -> Result<i64, Box<dyn std::error::Error>> {
        Ok(std::env::var(name)
            .ok()
            .map(|v| v.parse())
            .transpose()?
            .unwrap_or(fallback))
    };
    let start = setting("FOLD_PROFILE_START", 0)?;
    let step = setting("FOLD_PROFILE_STEP", 2)?;
    let rate = u32::try_from(setting("FOLD_PROFILE_RATE", 24)?)?;
    let cycle = setting("FOLD_PROFILE_CYCLE", 0)?;
    if cycle < 0 {
        return Err("profile cycle must be nonnegative".into());
    }
    let source = DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    let cancel = fold_media::Cancel::default();
    let packages = fold_app::packages::builtins();
    let (host, adapter) = pollster::block_on(fold_render::gpu::Host::headless(
        fold_render::gpu::Host::DEFAULT_BUDGET,
    ))?;
    eprintln!("{adapter:?}");
    let mut renderer = fold_render::gpu::Renderer::new(host.clone())?;
    let mut decoder = fold_media::Decoder::from_environment()?;
    let mut total = 0.;
    let mut ready_total = 0.;
    for frame in 0..count {
        let begin = Instant::now();
        let graph = packages.video_with(
            &snapshot,
            &source,
            Time::new(
                start
                    + if cycle > 0 {
                        (i64::from(frame) * step).rem_euclid(cycle)
                    } else {
                        i64::from(frame) * step
                    },
                rate,
            )?,
            size,
            &cancel,
        )?;
        let compile_ms = begin.elapsed().as_secs_f64() * 1000.;
        let nodes = graph.nodes.len();
        let reference = std::env::var_os("FOLD_PROFILE_VERIFY")
            .is_some()
            .then(|| graph.clone());
        let scene = fold_app::color::with_config(&snapshot, |config| {
            renderer.evaluate(graph, config, &mut decoder, &cancel)
        })?;
        let stats = scene.statistics;
        let mut output = fold_app::color::with_config(&snapshot, |config| {
            let display = config
                .map(|c| {
                    c.display(
                        fold_color::WORKING_SPACE,
                        &fold_platform::color::output(&snapshot, document)?.display_transform(),
                    )
                })
                .transpose()?;
            renderer.output(&scene, display.as_ref(), &cancel)
        })?;
        while !output.is_ready()? {
            host.poll()?;
            if begin.elapsed().as_secs() > 30 {
                return Err("GPU completion timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
        let ready_ms = begin.elapsed().as_secs_f64() * 1000.;
        let image = output.readback(&cancel)?;
        let ms = begin.elapsed().as_secs_f64() * 1000.;
        if frame > 0 {
            total += ms;
            ready_total += ready_ms;
        }
        println!(
            "{}",
            serde_json::json!({"frame":frame,"nodes":nodes,"wall_ms":ms,"ready_ms":ready_ms,"output_cpu_ms":output.statistics.output_cpu_nanoseconds as f64/1e6,"cpu_adapter_nodes":stats.cpu_adapter_nodes,"native_video_nodes":stats.native_video_nodes,"resident_video_nodes":stats.resident_video_nodes,"native_decode_ms":stats.native_decode_nanoseconds as f64/1e6,"native_codec_call_ms":stats.native_codec_call_nanoseconds as f64/1e6,"native_copy_ready_ms":stats.native_copy_ready_nanoseconds as f64/1e6,"native_allocate_ms":stats.native_allocate_nanoseconds as f64/1e6,"native_seeks":stats.native_seeks,"native_forward_reuses":stats.native_forward_reuses,"native_gpu_local_copy_bytes":stats.native_gpu_local_copy_bytes,"native_pipe_bytes":stats.native_pipe_bytes,"graph_prepare_ms":stats.graph_prepare_nanoseconds as f64/1e6,"queue_submit_ms":stats.queue_submit_nanoseconds as f64/1e6,"resident_still_nodes":stats.resident_still_nodes,"scene_readback_bytes":stats.readback_bytes,"compile_ms":compile_ms,"gpu_ms":scene.gpu_nanoseconds()?.map(|n|n/1e6),"vector_prepare_ms":stats.vector_prepare_nanoseconds as f64/1e6,"evaluate_ms":stats.evaluate_cpu_nanoseconds as f64/1e6,"passes":stats.compute_passes,"vector_upload_bytes":stats.vector_upload_bytes,"vector_geometry_upload_bytes":stats.vector_geometry_upload_bytes,"vector_instance_upload_bytes":stats.vector_instance_upload_bytes,"vector_instances":stats.vector_instances,"vector_unique_triangles":stats.vector_unique_triangles,"vector_execution_triangles":stats.vector_execution_triangles,"vector_cpu_transform_fallbacks":stats.vector_cpu_transform_fallbacks,"upload_bytes":stats.upload_bytes,"peak_bytes":host.memory().peak,"allocated_bytes":host.memory().allocated})
        );
        if let Some(directory) = std::env::var_os("FOLD_PROFILE_IMAGES") {
            use std::io::Write;
            let path = std::path::Path::new(&directory).join(format!("frame-{frame}.ppm"));
            let mut file = std::io::BufWriter::new(std::fs::File::create_new(path)?);
            writeln!(file, "P6\n{} {}\n255", size[0], size[1])?;
            for pixel in image.rgba().chunks_exact(4) {
                file.write_all(&pixel[..3])?;
            }
            file.flush()?;
        }
        if let Some(graph) = reference {
            let expected_frame = fold_app::color::with_config(&snapshot, |config| {
                let frame = match config {
                    Some(config) => {
                        fold_render::render_aces_with(graph, config, &mut decoder, &cancel)
                    }
                    None => fold_render::render_with(graph, &mut decoder, &cancel),
                }?
                .over_black();
                Ok(frame)
            })?;
            let expected = fold_app::color::delivery(&snapshot, document, &expected_frame)?;
            if let Some(directory) = std::env::var_os("FOLD_PROFILE_IMAGES") {
                use std::io::Write;
                let path = std::path::Path::new(&directory).join(format!("reference-{frame}.ppm"));
                let mut file = std::io::BufWriter::new(std::fs::File::create_new(path)?);
                writeln!(file, "P6\n{} {}\n255", size[0], size[1])?;
                for pixel in expected.rgba().chunks_exact(4) {
                    file.write_all(&pixel[..3])?;
                }
                file.flush()?;
            }
            let max = image
                .rgba()
                .iter()
                .zip(expected.rgba())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            eprintln!("frame={frame} max_code_error={max}");
            let tolerance = std::env::var("FOLD_PROFILE_CODE_TOLERANCE")
                .ok()
                .map(|v| v.parse::<u8>())
                .transpose()?
                .unwrap_or(1);
            if max > tolerance {
                return Err(
                    format!("CPU/GPU differ by more than {tolerance} display codes").into(),
                );
            }
        }
    }
    let mean = total / f64::from(count - 1);
    let ready_mean = ready_total / f64::from(count - 1);
    eprintln!("warm_mean_ms={mean:.3} warm_ready_mean_ms={ready_mean:.3}");
    if let Ok(limit) = std::env::var("FOLD_PROFILE_READY_MAX_MS") {
        if ready_mean > limit.parse::<f64>()? {
            return Err("display-ready frame performance target exceeded".into());
        }
    }
    if let Ok(limit) = std::env::var("FOLD_PROFILE_MAX_MS") {
        if mean > limit.parse::<f64>()? {
            return Err("warm frame performance target exceeded".into());
        }
    }
    Ok(())
}
