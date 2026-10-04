//! Full-resolution compositor probe. SOURCE FRAMES BLUR [stage: source|blur|full].
//! Run a release native-video build with FOLD_COLOR_ROOT and decoder configuration.
//! Reports uncached evaluation, not presentation-cache or physical UI latency.
//! FOLD_PROFILE_VERIFY=1 compares the first frame with CPU and delivery output.
use fold_compositor::{Composite, Node, Parameters as P, Source};
use fold_foundation::{DocumentId, Time};
use fold_project::{DocumentRef, EditBatch, Mutation};
use fold_render::operations::{Edges, Grade};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("SOURCE FRAMES BLUR [source|blur|full]".into());
    }
    let count: u32 = args[1].parse()?;
    if count == 0 {
        return Err("frame count must be positive".into());
    }
    let verify = std::env::var_os("FOLD_PROFILE_VERIFY").is_some();
    let blur: f32 = args[2].parse()?;
    let stage = args.get(3).map(String::as_str).unwrap_or("full");
    let cancel = fold_media::Cancel::default();
    let info = fold_media::VideoInfo {
        width: 1920,
        height: 1080,
        rate: [25, 1],
        frames: count,
    };
    let read = Node::new(
        P::Read {
            source: Source::Unassigned { info: info.clone() },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: info.time(info.frames)?,
        },
        vec![],
    );
    let read_id = read.id;
    let softened = Node::new(
        P::GaussianBlur {
            size: [blur; 2],
            edges: Edges::Transparent,
        },
        vec![read.id],
    );
    let transform = Node::new(
        P::TransformImage {
            settings: fold_compositor::Transform {
                translate: [12., 8.],
                rotate: 3.,
                pivot: [960., 540.],
                ..Default::default()
            },
        },
        vec![softened.id],
    );
    let mask = Node::new(
        P::Shape {
            shape: fold_compositor::Shape::Ellipse,
            bounds: [320., 180., 1600., 900.],
        },
        vec![],
    );
    let mut grade = Node::new(
        P::ColorGrade {
            settings: Grade {
                multiply: [1.1, 1.3, 1.],
                ..Default::default()
            },
        },
        vec![transform.id],
    );
    grade.effect.mask = Some(mask.id);
    let output = Node::new(
        P::Output,
        vec![match stage {
            "source" => read.id,
            "blur" => softened.id,
            "full" => grade.id,
            _ => return Err("unknown stage".into()),
        }],
    );
    let composite = Composite {
        info: info.clone(),
        output: output.id,
        nodes: vec![read, softened, transform, mask, grade, output],
        extensions: Default::default(),
    };
    let id = DocumentId::new();
    let mut project = fold_app::color::new_project(8);
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![Mutation::PutDocument(composite.document(id)?)],
    })?;
    let packages = fold_app::packages::builtins();
    project.commit(packages.stage(
        &project.snapshot(),
        &fold_platform::packages::CommandRequest {
            id: "fold.app.read-source".into(),
            base: project.snapshot().revision(),
            arguments: serde_json::to_vec(
                &serde_json::json!({"document":id,"node":read_id,"path":args[0]}),
            )?,
        },
        &cancel,
    )?)?;
    let snapshot = project.snapshot();
    let (host, adapter) = pollster::block_on(fold_render::gpu::Host::headless(
        fold_render::gpu::Host::DEFAULT_BUDGET,
    ))?;
    eprintln!("{adapter:?}");
    let mut renderer = fold_render::gpu::Renderer::new(host.clone())?;
    let mut decoder = fold_media::Decoder::from_environment()?;
    fold_app::color::with_config(&snapshot, |config| {
        let config = config.unwrap();
        let display = config.display(fold_color::WORKING_SPACE, &Default::default())?;
        for frame in 0..count {
            let start = Instant::now();
            let graph = packages.video_with(
                &snapshot,
                &DocumentRef {
                    document: id,
                    output: "video".into(),
                    extensions: Default::default(),
                },
                info.time(frame)?,
                [1920, 1080],
                &cancel,
            )?;
            let compile_ms = start.elapsed().as_secs_f64() * 1000.;
            let nodes = graph.nodes.len();
            let reference = (verify && frame == 0).then(|| graph.clone());
            let scene = renderer.evaluate(graph, Some(config), &mut decoder, &cancel)?;
            let stats = scene.statistics;
            let output = renderer.output(&scene, Some(&display), &cancel)?;
            while !output.is_ready()? {
                host.poll()?;
                if start.elapsed() > Duration::from_secs(60) {
                    return Err("frame timeout".into());
                }
                std::thread::sleep(Duration::from_micros(100));
            }
            println!(
                "{}",
                serde_json::json!({"frame":frame,"stage":stage,"blur":blur,"nodes":nodes,"wall_ms":start.elapsed().as_secs_f64()*1000.,"compile_ms":compile_ms,"gpu_ms":scene.gpu_nanoseconds()?.map(|n|n/1e6),"adapter_ms":stats.cpu_adapter_nanoseconds as f64/1e6,"prepare_ms":stats.graph_prepare_nanoseconds as f64/1e6,"submit_cpu_ms":stats.evaluate_cpu_nanoseconds as f64/1e6,"decode_ms":stats.native_decode_nanoseconds as f64/1e6,"upload_bytes":stats.upload_bytes,"passes":stats.compute_passes,"resident_stills":stats.resident_still_nodes,"allocated_bytes":host.memory().allocated})
            );
            if let Some(graph) = reference {
                let reference = fold_render::render_aces_with(
                    graph,
                    config,
                    &mut fold_media::Decoder::default(),
                    &cancel,
                )?
                .over_black()
                .to_output(&display)?;
                let actual = {
                    let mut output = output;
                    output.readback(&cancel)?
                };
                let max = actual
                    .rgba()
                    .iter()
                    .zip(reference.rgba())
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                eprintln!("CPU/GPU output max_code_error={max}");
                if max > 1 {
                    return Err("CPU/GPU output differs by more than one code".into());
                }
                let mut delivery = renderer.output(&scene, Some(&display), &cancel)?;
                if delivery.readback(&cancel)?.rgba() != actual.rgba() {
                    return Err("matched preview/delivery differ".into());
                }
            }
        }
        Ok(())
    })?;
    Ok(())
}
