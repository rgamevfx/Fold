//! Measure a saved project's uncached graph-to-GPU frame latency.
//! Usage: PROJECT DOCUMENT FRAMES WIDTH HEIGHT [original|flat]
//! FOLD_PROFILE_MAX_MS optionally rejects warm frame medians above a budget.
use fold_foundation::DocumentId;
use fold_project::DocumentRef;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 5 {
        return Err("PROJECT DOCUMENT FRAMES WIDTH HEIGHT [original|flat]".into());
    }
    let document: DocumentId = serde_json::from_value(serde_json::Value::String(args[1].clone()))?;
    let count: u32 = args[2].parse()?;
    if count < 2 {
        return Err("at least two frames required".into());
    }
    let dimensions = [args[3].parse()?, args[4].parse()?];
    let mode = args.get(5).map(String::as_str).unwrap_or("original");
    if !matches!(mode, "original" | "flat") {
        return Err("unknown mode".into());
    }
    let cancel = fold_media::Cancel::default();
    let project = fold_project::load(&args[0], 8)?;
    let snapshot = project.snapshot();
    let packages = fold_app::packages::builtins();
    let output = DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    let info = packages.output_ref(&snapshot, &output)?;
    let (host, adapter) = pollster::block_on(fold_render::gpu::Host::headless_automatic())?;
    eprintln!("{adapter:?}");
    let mut renderer = fold_render::gpu::Renderer::new(host.clone())?;
    let mut decoder = fold_media::Decoder::default();
    let mut timings = Vec::new();
    fold_app::color::with_config(&snapshot, |config| {
        let config = config.ok_or("project needs color configuration")?;
        let display = config.display(fold_color::WORKING_SPACE, &Default::default())?;
        for frame in 0..count {
            let start = Instant::now();
            let mut graph =
                packages.video_with(&snapshot, &output, info.time(frame)?, dimensions, &cancel)?;
            if mode == "flat" {
                for op in &mut graph.nodes {
                    if let fold_render::ImageOp::Shader { program, .. } = op {
                        std::sync::Arc::make_mut(program).source =
                            "fn effect(uv:vec2<f32>)->vec4<f32>{return vec4<f32>(uv,0.,1.);}"
                                .into();
                    }
                }
            }
            let compile_ms = start.elapsed().as_secs_f64() * 1000.;
            let scene = renderer.evaluate(graph, Some(config), &mut decoder, &cancel)?;
            let result = renderer.output(&scene, Some(&display), &cancel)?;
            while !result.is_ready()? {
                host.poll()?;
                if start.elapsed() > Duration::from_secs(60) {
                    return Err("frame timeout".into());
                }
                std::thread::sleep(Duration::from_micros(100));
            }
            let wall_ms = start.elapsed().as_secs_f64() * 1000.;
            if frame > 0 {
                timings.push(wall_ms);
            }
            let s = result.statistics;
            println!(
                "{}",
                serde_json::json!({"frame":frame,"mode":mode,"dimensions":dimensions,"wall_ms":wall_ms,"compile_ms":compile_ms,"scene_gpu_ms":scene.gpu_nanoseconds()?.map(|n|n/1e6),"display_gpu_ms":result.gpu_nanoseconds()?.map(|n|n/1e6),"adapter_ms":s.cpu_adapter_nanoseconds as f64/1e6,"prepare_ms":s.graph_prepare_nanoseconds as f64/1e6,"evaluate_cpu_ms":s.evaluate_cpu_nanoseconds as f64/1e6,"output_cpu_ms":s.output_cpu_nanoseconds as f64/1e6,"queue_submit_ms":s.queue_submit_nanoseconds as f64/1e6,"upload_bytes":s.upload_bytes,"readback_bytes":s.readback_bytes,"passes":s.compute_passes,"resident_stills":s.resident_still_nodes,"shader_compilations":s.shader_compilations,"peak_bytes":host.memory().peak})
            );
        }
        Ok(())
    })?;
    timings.sort_by(f64::total_cmp);
    let median = timings[timings.len() / 2];
    eprintln!("warm median {median:.3} ms");
    if let Ok(limit) = std::env::var("FOLD_PROFILE_MAX_MS") {
        if median > limit.parse::<f64>()? {
            return Err("warm frame latency exceeds budget".into());
        }
    }
    Ok(())
}
