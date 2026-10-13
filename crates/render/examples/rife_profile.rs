//! Focused GPU qualification for RIFE: moving HDR/alpha shape, fractional timing,
//! warm inference latency and GPU-only tensor transport. Requires FOLD_RIFE_ROOT.
//! cargo run -p fold-render --release --features shared-images --example rife_profile -- 320 180
#[cfg(all(feature = "gpu", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use fold_render::{
        ImageOp, RenderGraph,
        gpu::{Host, Renderer},
        shader::Program,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    let w = args.first().map_or(Ok(320), |s| s.parse())?;
    let h = args.get(1).map_or(Ok(180), |s| s.parse())?;
    let (host, adapter) = pollster::block_on(Host::headless_automatic())?;
    println!("{adapter:?}");
    let mut renderer = Renderer::new(host)?;
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let source = |x: f32| {
        ImageOp::Shader{inputs:vec![],program:std::sync::Arc::new(Program {source:"fn effect(uv:vec2<f32>)->vec4<f32>{let d=abs(uv-vec2<f32>(fold_params.values[0].x,0.5));let a=(1.-smoothstep(0.10,0.11,d.x))*(1.-smoothstep(0.15,0.16,d.y))*0.6;return vec4<f32>(vec3<f32>(3.,0.5,0.1)*a,a);}".into(),images:vec![],values:vec![[x,0.,0.,0.]],logical_size:[w as f32,h as f32],time:0.,progress:0.})}
    };
    for fraction in [0.25, 0.5, 0.75, 0.5] {
        let begin = std::time::Instant::now();
        let graph = RenderGraph {
            width: w,
            height: h,
            nodes: vec![
                source(0.35),
                source(0.55),
                ImageOp::Rife {
                    first: 0,
                    second: 1,
                    fraction,
                    motion_scale: 1,
                    cut_threshold: 0.35,
                },
            ],
            output: 2,
        };
        let mut scene = renderer.evaluate(graph, None, &mut decoder, &cancel)?;
        let stats = scene.statistics;
        let frame = scene.readback(&cancel)?;
        let mut mass = 0.;
        let mut moment = 0.;
        let mut peak = 0f32;
        for (i, p) in frame.pixels().iter().enumerate() {
            assert!(p.iter().all(|v| v.is_finite()));
            assert!((0.0..=0.6001).contains(&p[3]));
            mass += p[3] as f64;
            moment += p[3] as f64 * ((i % w as usize) as f64 + 0.5) / w as f64;
            peak = peak.max(p[0]);
        }
        let center = moment / mass;
        let expected = 0.35 + 0.2 * fraction as f64;
        println!(
            "t={fraction}: centroid={center:.4} expected={expected:.4} peak={peak:.3} wall_ms={:.2} inference_ms={:.2} cpu_adapters={} status_bytes={}",
            begin.elapsed().as_secs_f64() * 1000.,
            stats.rife_nanoseconds as f64 / 1e6,
            stats.cpu_adapter_nodes,
            stats.status_readback_bytes
        );
        assert!(
            (center - expected).abs() < 0.06,
            "interpolation did not follow the moving shape"
        );
        assert!(peak > 1.5, "HDR values were clipped");
        assert_eq!(stats.cpu_adapter_nodes, 0);
        let path = std::env::temp_dir().join(format!("fold-rife-{w}-{fraction}.ppm"));
        frame.over_black().write_ppm(std::fs::File::create(path)?)?;
    }
    // Two shutter samples of one source pair exercise retained GPU input reuse.
    let graph = RenderGraph {
        width: w,
        height: h,
        nodes: vec![
            source(0.35),
            source(0.55),
            ImageOp::Rife {
                first: 0,
                second: 1,
                fraction: 0.25,
                motion_scale: 1,
                cut_threshold: 0.35,
            },
            ImageOp::Rife {
                first: 0,
                second: 1,
                fraction: 0.75,
                motion_scale: 1,
                cut_threshold: 0.35,
            },
            ImageOp::Mix {
                original: 2,
                processed: 3,
                mask: None,
                mask_channel: 0,
                invert: false,
                amount: 0.5,
                channels: [true; 4],
            },
        ],
        output: 4,
    };
    let mut scene = renderer.evaluate(graph, None, &mut decoder, &cancel)?;
    assert_eq!(scene.statistics.rife_nodes, 2);
    let frame = scene.readback(&cancel)?;
    let (mass, moment) =
        frame
            .pixels()
            .iter()
            .enumerate()
            .fold((0f64, 0f64), |(mass, moment), (i, p)| {
                (
                    mass + p[3] as f64,
                    moment + p[3] as f64 * ((i % w as usize) as f64 + 0.5) / w as f64,
                )
            });
    assert!((moment / mass - 0.45).abs() < 0.03);
    // A hard cut holds the nearest source instead of synthesizing a transition.
    let graph = RenderGraph {
        width: w,
        height: h,
        nodes: vec![
            ImageOp::Solid {
                rgba: [0., 0., 0., 1.],
            },
            ImageOp::Solid { rgba: [1.; 4] },
            ImageOp::Rife {
                first: 0,
                second: 1,
                fraction: 0.75,
                motion_scale: 2,
                cut_threshold: 0.35,
            },
        ],
        output: 2,
    };
    let mut scene = renderer.evaluate(graph, None, &mut decoder, &cancel)?;
    assert_eq!(scene.statistics.rife_cut_holds, 1);
    assert!(
        scene
            .readback(&cancel)?
            .pixels()
            .iter()
            .all(|p| *p == [1.; 4])
    );
    println!("shared-pair shutter reuse and hard-cut hold: passed");
    Ok(())
}
#[cfg(not(all(feature = "gpu", target_os = "linux")))]
fn main() {
    eprintln!("RIFE qualification requires Linux and the gpu feature");
}
