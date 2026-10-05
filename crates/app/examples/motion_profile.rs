//! Release Chroma Parade GPU benchmark: PROJECT [FRAMES] [WIDTH] [HEIGHT].
//! Samples the 24 fps demo every other frame; excludes viewer caching and UI latency.
//! FOLD_PROFILE_MAX_MS enforces a warm-frame mean; FOLD_PROFILE_VERIFY checks CPU parity.
use fold_foundation::Time;
use fold_project::DocumentRef;
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let project = fold_project::load(args.first().ok_or("PROJECT [FRAMES] [WIDTH] [HEIGHT]")?, 1)?;
    let count: u32 = args.get(1).map(|v| v.parse()).transpose()?.unwrap_or(12);
    if !(2..=120).contains(&count) {
        return Err("frame count must be 2..=120 for the ten-second demo".into());
    }
    let size = [
        args.get(2).map(|v| v.parse()).transpose()?.unwrap_or(960),
        args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(540),
    ];
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .organization
        .items
        .iter()
        .find_map(|item| {
            if item.name.contains("Composite") {
                if let fold_project::ItemId::Document(id) = item.id {
                    return Some(id);
                }
            }
            None
        })
        .ok_or("no Composite document")?;
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
    let mut decoder = fold_media::Decoder::default();
    let mut total = 0.;
    for frame in 0..count {
        let begin = Instant::now();
        let graph = packages.video_with(
            &snapshot,
            &source,
            Time::new(i64::from(frame * 2), 24)?,
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
        }
        println!(
            "{}",
            serde_json::json!({"frame":frame,"nodes":nodes,"wall_ms":ms,"ready_ms":ready_ms,"output_cpu_ms":output.statistics.output_cpu_nanoseconds as f64/1e6,"cpu_adapter_nodes":stats.cpu_adapter_nodes,"compile_ms":compile_ms,"gpu_ms":scene.gpu_nanoseconds()?.map(|n|n/1e6),"vector_prepare_ms":stats.vector_prepare_nanoseconds as f64/1e6,"evaluate_ms":stats.evaluate_cpu_nanoseconds as f64/1e6,"passes":stats.compute_passes,"vector_upload_bytes":stats.vector_upload_bytes,"upload_bytes":stats.upload_bytes,"peak_bytes":host.memory().peak,"allocated_bytes":host.memory().allocated})
        );
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
            let max = image
                .rgba()
                .iter()
                .zip(expected.rgba())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            eprintln!("frame={frame} max_code_error={max}");
            if max > 1 {
                return Err("CPU/GPU differ by more than one code".into());
            }
        }
    }
    let mean = total / f64::from(count - 1);
    eprintln!("warm_mean_ms={mean:.3}");
    if let Ok(limit) = std::env::var("FOLD_PROFILE_MAX_MS") {
        if mean > limit.parse::<f64>()? {
            return Err("warm frame performance target exceeded".into());
        }
    }
    Ok(())
}
