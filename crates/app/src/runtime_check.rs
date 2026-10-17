//! Executable acceptance fixtures using the same discovery and rendering services as the app.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_motion_canvas::{model::Canvas, motion, tools};
use fold_motion_context::runtime::{
    Inputs, Runtime,
    gpu::{GpuRuntime, Snapshot},
};
use fold_render::{
    ImageOp, RenderGraph,
    gpu::{Host, Renderer},
    shader::Program,
};
use std::sync::Arc;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    fold_platform::color::validate_runtime()?;
    println!("ACES default view: passed");
    let cancel = Cancel::default();
    let (host, adapter) = pollster::block_on(Host::headless_automatic())?;
    println!("GPU: {adapter:?}");
    let config = crate::motion_runtime::configuration();
    let mut runtime = GpuRuntime::new(config.clone(), host.clone(), &cancel)?;
    let id = ObjectId::new();
    let mut object = tools::rectangle("Runtime qualification", [8., 16.], [32., 40.]);
    object.paint.fill = Some([1., 0., 0., 1.].into());
    let mut canvas = Canvas {
        width: 96,
        height: 64,
        duration: Time::new(1, 1)?,
        roots: vec![id],
        objects: [(id, object)].into(),
        ..Default::default()
    };
    let document = DocumentId::new();
    let mut render =
        |canvas: &Canvas, time, generation| -> Result<f64, Box<dyn std::error::Error>> {
            let snapshot = Snapshot::from_document(
                ObjectId::new(),
                generation,
                &canvas.document(document)?,
                time,
                [96, 64],
            )?;
            let mut frame = runtime
                .render(&snapshot, &[], &cancel)?
                .into_current(snapshot.intent())?;
            let pixels = frame.readback(&cancel)?;
            centroid(pixels.pixels(), 96)
        };
    let initial = render(&canvas, Time::ZERO, 1)?;
    println!("Native Motion rendering: passed");
    // A unique source forces compilation; this cannot pass by reusing a warm program.
    canvas.objects.get_mut(&id).unwrap().motion = Some(motion::Motion {
        spec: motion::Spec {
            source: format!(
                "// Fresh source {id}\npub fn animate(input: MotionContext<'_>) -> Result<MotionPose, String> {{ let mut pose = MotionPose::default(); pose.position[0] = 24.0 * input.seconds(); Ok(pose) }}"
            ),
            controls: Default::default(),
            sample_rate: 4,
        },
        enabled: true,
        curves: [0., 0., 0., 1., 1., 1.].map(|value| {
            let mut curve = fold_animation::Curve::default();
            curve.insert(Time::ZERO, value);
            curve
        }),
    });
    let mut compiler = Runtime::new(config, &cancel)?;
    let curves = compiler.animate(
        &canvas,
        id,
        Inputs {
            fonts: &[],
            assets: &[],
        },
        &cancel,
    )?;
    canvas
        .objects
        .get_mut(&id)
        .unwrap()
        .motion
        .as_mut()
        .unwrap()
        .curves = curves;
    let moved = render(&canvas, Time::new(1, 2)?, 2)?;
    if (moved - initial - 12.).abs() > 1. {
        return Err(format!(
            "Fresh animation did not move the rendered object: {initial} -> {moved}"
        )
        .into());
    }
    println!("Fresh Motion script compilation and animated pixels: passed");
    drop(runtime);
    drop(compiler);
    let mut renderer = Renderer::new(host)?;
    let mut decoder = fold_media::Decoder::default();
    let source = |x| {
        ImageOp::Shader { inputs: vec![], program: Arc::new(Program {
        source: "fn effect(uv:vec2<f32>)->vec4<f32>{let d=abs(uv-vec2<f32>(fold_params.values[0].x,0.5));let a=(1.-smoothstep(0.10,0.11,d.x))*(1.-smoothstep(0.15,0.16,d.y))*0.6;return vec4<f32>(vec3<f32>(3.,0.5,0.1)*a,a);}".into(),
        images: vec![], values: vec![[x, 0., 0., 0.]], logical_size: [320., 180.], time: 0., progress: 0.,
    }) }
    };
    for fraction in [0.25, 0.5, 0.75] {
        let graph = RenderGraph {
            width: 320,
            height: 180,
            nodes: vec![
                source(0.35),
                source(0.55),
                ImageOp::Rife {
                    first: 0,
                    second: 1,
                    fraction,
                    motion_scale: 1,
                    cut_threshold: 0.,
                },
            ],
            output: 2,
        };
        let mut scene = renderer.evaluate(graph, None, &mut decoder, &cancel)?;
        if scene.statistics.cpu_adapter_nodes != 0
            || scene.statistics.rife_nanoseconds == 0
            || scene.statistics.rife_cut_holds != 0
        {
            return Err("RIFE did not execute GPU inference".into());
        }
        let pixels = scene.readback(&cancel)?;
        let center = centroid(pixels.pixels(), 320)? / 320.;
        let expected = 0.35 + 0.2 * f64::from(fraction);
        if (center - expected).abs() > 0.025 || !pixels.pixels().iter().any(|p| p[0] > 1.5) {
            return Err(format!("RIFE motion/HDR check failed at {fraction}: {center}").into());
        }
    }
    println!("RIFE fractional interpolation, HDR and GPU transport: passed");
    Ok(())
}

fn centroid(pixels: &[[f32; 4]], width: usize) -> Result<f64, Box<dyn std::error::Error>> {
    let mut mass = 0.;
    let mut moment = 0.;
    for (i, pixel) in pixels.iter().enumerate() {
        if pixel.iter().any(|v| !v.is_finite()) || !(0.0..=1.0001).contains(&pixel[3]) {
            return Err("Invalid rendered color/alpha".into());
        }
        mass += f64::from(pixel[3]);
        moment += f64::from(pixel[3]) * ((i % width) as f64 + 0.5);
    }
    if mass <= 0. {
        return Err("Renderer produced an empty image".into());
    }
    Ok(moment / mass)
}
