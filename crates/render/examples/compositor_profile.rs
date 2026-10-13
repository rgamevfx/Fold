//! Synthetic GPU compositor latency, without media/OCIO setup or presentation cache.
//! cargo run -p fold-render --release --features gpu --example compositor_profile --
//!   1920 1080 60 16 0
//! Arguments: WIDTH HEIGHT SAMPLES LAYERS BLUR_SIZE. Ten warmup frames are excluded.
//! FOLD_PROFILE_MAX_MS optionally enforces the warm wall-time p95 budget.
//! This measures serial frame latency, not pipelined throughput or desktop FPS.
#[cfg(feature = "gpu")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use fold_render::{
        ImageOp, RenderGraph,
        gpu::{Host, Renderer},
    };
    use std::time::{Duration, Instant};

    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err("WIDTH HEIGHT SAMPLES LAYERS BLUR_SIZE".into());
    }
    let width: u32 = args[0].parse()?;
    let height: u32 = args[1].parse()?;
    let count: u32 = args[2].parse()?;
    let layers: usize = args[3].parse()?;
    let blur: f32 = args[4].parse()?;
    if count == 0 || count > 10000 || layers > 1024 || !blur.is_finite() || blur < 0. {
        return Err("invalid sample count, layer count or blur size".into());
    }
    let (host, adapter) = pollster::block_on(Host::headless_automatic())?;
    eprintln!("adapter={adapter:?}");
    let mut renderer = Renderer::new(host.clone())?;
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let mut walls = Vec::new();
    let mut gpu = Vec::new();
    for sample in 0..count + 10 {
        let start = Instant::now();
        let mut nodes = vec![ImageOp::Solid {
            rgba: [0.08, 0.12, 0.2, 1.],
        }];
        let mut output = 0;
        for layer in 0..layers {
            let foreground = nodes.len();
            nodes.push(ImageOp::Solid {
                rgba: [0.2, 0.1, 0.05, 0.5],
            });
            nodes.push(ImageOp::Opacity {
                input: foreground,
                opacity: 0.5 + ((sample as usize + layer) % 30) as f32 / 100.,
            });
            nodes.push(ImageOp::Over {
                foreground: foreground + 1,
                background: output,
            });
            output = nodes.len() - 1;
        }
        if blur > 0. {
            nodes.push(ImageOp::FilteredBlur {
                input: output,
                size: [blur; 2],
                filter: Default::default(),
                edges: fold_render::operations::Edges::Transparent,
            });
            output = nodes.len() - 1;
        }
        let graph = RenderGraph {
            width,
            height,
            nodes,
            output,
        };
        let build_ms = start.elapsed().as_secs_f64() * 1000.;
        let scene = renderer.evaluate(graph, None, &mut decoder, &cancel)?;
        let display = renderer.output(&scene, None, &cancel)?;
        while !display.is_ready()? {
            host.poll()?;
            if start.elapsed() > Duration::from_secs(30) {
                return Err("GPU frame timeout".into());
            }
            std::thread::sleep(Duration::from_micros(100));
        }
        let wall_ms = start.elapsed().as_secs_f64() * 1000.;
        let gpu_ms = display.gpu_nanoseconds()?.map(|n| n / 1e6);
        let s = display.statistics;
        if sample >= 10 {
            walls.push(wall_ms);
            gpu.extend(gpu_ms);
        }
        println!(
            "{}",
            serde_json::json!({
                "sample":sample,"warmup":sample<10,"dimensions":[width,height],
                "layers":layers,"blur":blur,"wall_ms":wall_ms,"build_ms":build_ms,
                "gpu_ms":gpu_ms,"prepare_ms":s.graph_prepare_nanoseconds as f64/1e6,
                "evaluate_cpu_ms":s.evaluate_cpu_nanoseconds as f64/1e6,
                "output_cpu_ms":s.output_cpu_nanoseconds as f64/1e6,
                "queue_submit_ms":s.queue_submit_nanoseconds as f64/1e6,
                "passes":s.compute_passes,"fused_opacity_passes":s.fused_opacity_passes,
                "upload_bytes":s.upload_bytes,"readback_bytes":s.readback_bytes,
                "status_readback_bytes":s.status_readback_bytes,
                "allocated_bytes":host.memory().allocated,"peak_bytes":host.memory().peak
            })
        );
    }
    walls.sort_by(f64::total_cmp);
    gpu.sort_by(f64::total_cmp);
    let p95 = walls[(walls.len() * 95).div_ceil(100) - 1];
    eprintln!(
        "{}",
        serde_json::json!({
            "samples":count,"dimensions":[width,height],"layers":layers,"blur":blur,
            "wall_median_ms":walls[walls.len()/2],"wall_p95_ms":p95,
            "gpu_median_ms":gpu.get(gpu.len()/2)
        })
    );
    if let Ok(limit) = std::env::var("FOLD_PROFILE_MAX_MS")
        && p95 > limit.parse::<f64>()?
    {
        return Err("warm frame p95 exceeds budget".into());
    }
    Ok(())
}

#[cfg(not(feature = "gpu"))]
fn main() {
    eprintln!("compositor_profile requires --features gpu");
    std::process::exit(1);
}
