//! Full-resolution compositor probe. SOURCE FRAMES BLUR [source|shared|blur|full]
//! [forward|reverse|random|held|pingpong]. All times use the source's exact frame rate.
//! Run a release native-video build with FOLD_COLOR_ROOT and decoder configuration.
//! Reports uncached evaluation, not presentation-cache or physical UI latency.
//! FOLD_PROFILE_VERIFY=1 compares the first frame with CPU and delivery output.
//! FOLD_PROFILE_GPU_MIB selects a budget (0 = automatic); FOLD_PROFILE_SECOND
//! composites a second real clip over the first. Pingpong revisits decoded frames.
use fold_compositor::{Composite, Node, OperationConfig as P, Source};
use fold_foundation::{DocumentId, Time};
use fold_project::{DocumentRef, EditBatch, Mutation};
use fold_render::operations::{Edges, Grade};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err(
            "SOURCE FRAMES BLUR [source|shared|blur|full] [forward|reverse|random|held|pingpong]"
                .into(),
        );
    }
    let count: u32 = args[1].parse()?;
    if count == 0 {
        return Err("frame count must be positive".into());
    }
    let verify = std::env::var_os("FOLD_PROFILE_VERIFY").is_some();
    let blur: f32 = args[2].parse()?;
    let stage = args.get(3).map(String::as_str).unwrap_or("full");
    let order = args.get(4).map(String::as_str).unwrap_or("forward");
    let mut frames: Vec<_> = (0..count).collect();
    match order {
        "forward" => {}
        "pingpong" => frames.extend((0..count).rev()),
        "reverse" => frames.reverse(),
        "held" => frames.fill(0),
        "random" => {
            let mut seed = 0x464f4c44_u64;
            for index in (1..frames.len()).rev() {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                frames.swap(index, (seed % (index as u64 + 1)) as usize);
            }
        }
        _ => return Err("unknown frame order".into()),
    }
    let cancel = fold_media::Cancel::default();
    let source = fold_media::inspect(std::path::Path::new(&args[0]), &cancel)?;
    let second = std::env::var_os("FOLD_PROFILE_SECOND")
        .map(|path| fold_media::inspect(std::path::Path::new(&path), &cancel))
        .transpose()?;
    let info = source.info;
    if count > info.frames || second.as_ref().is_some_and(|s| count > s.info.frames) {
        return Err("frame count exceeds source duration".into());
    }
    let dimensions = [info.width, info.height];
    eprintln!("source={} info={info:?}", args[0]);
    let read = Node::new(
        P::Read {
            options: Default::default(),
            source: Source::Unassigned { info: info.clone() },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: info.time(info.frames)?,
        },
        vec![],
    );
    let read_id = read.id;
    let softened = Node::new(
        P::Blur {
            filter: Default::default(),
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
                pivot: [f64::from(info.width) / 2., f64::from(info.height) / 2.],
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
    grade.effect.set_mask(mask.id);
    let output = Node::new(
        P::Output,
        vec![match stage {
            "source" | "shared" => read.id,
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
    let budget = std::env::var("FOLD_PROFILE_GPU_MIB")
        .ok()
        .map(|s| s.parse::<u64>())
        .transpose()?
        .unwrap_or(1024)
        * 1024
        * 1024;
    let (host, adapter) = if budget == 0 {
        pollster::block_on(fold_render::gpu::Host::headless_automatic())?
    } else {
        pollster::block_on(fold_render::gpu::Host::headless(budget))?
    };
    eprintln!("budget={}", host.memory().budget);
    eprintln!("{adapter:?}");
    let mut renderer = fold_render::gpu::Renderer::new(host.clone())?;
    let mut decoder = fold_media::Decoder::from_environment()?;
    fold_app::color::with_config(&snapshot, |config| {
        let config = config.unwrap();
        let display = config.display(fold_color::WORKING_SPACE, &Default::default())?;
        for (sample, &frame) in frames.iter().enumerate() {
            let start = Instant::now();
            let mut graph = packages.video_with(
                &snapshot,
                &DocumentRef {
                    document: id,
                    output: "video".into(),
                    extensions: Default::default(),
                },
                info.time(frame)?,
                dimensions,
                &cancel,
            )?;
            if stage == "shared" {
                // Model two provider fragments reading the same source frame.
                let first = graph.output;
                let second = graph.append(graph.clone())?;
                graph.output = graph.nodes.len();
                graph.nodes.push(fold_render::ImageOp::Over {
                    foreground: first,
                    background: second,
                });
            }
            if let Some(second) = &second {
                let background = graph.output;
                let foreground = graph.nodes.len();
                graph.nodes.push(fold_render::ImageOp::VideoInput {
                    source: second.clone(),
                    time: second.info.time(frame)?,
                    space: fold_platform::color::VIDEO_INPUT.into(),
                });
                graph.output = graph.nodes.len();
                graph.nodes.push(fold_render::ImageOp::Over {
                    foreground,
                    background,
                });
            }
            let compile_ms = start.elapsed().as_secs_f64() * 1000.;
            let nodes = graph.nodes.len();
            let reference = (verify && sample == 0).then(|| graph.clone());
            let mut scene = renderer.evaluate(graph, Some(config), &mut decoder, &cancel)?;
            let output = renderer.output(&scene, Some(&display), &cancel)?;
            while !output.is_ready()? {
                host.poll()?;
                if start.elapsed() > Duration::from_secs(60) {
                    return Err("frame timeout".into());
                }
                std::thread::sleep(Duration::from_micros(100));
            }
            let stats = output.statistics;
            println!(
                "{}",
                serde_json::json!({"frame":frame,"sample":sample,"order":order,"stage":stage,"blur":blur,"nodes":nodes,"wall_ms":start.elapsed().as_secs_f64()*1000.,"compile_ms":compile_ms,"gpu_ms":output.gpu_nanoseconds()?.map(|n|n/1e6),"scene_gpu_ms":scene.gpu_nanoseconds()?.map(|n|n/1e6),"adapter_ms":stats.cpu_adapter_nanoseconds as f64/1e6,"prepare_ms":stats.graph_prepare_nanoseconds as f64/1e6,"evaluate_cpu_ms":stats.evaluate_cpu_nanoseconds as f64/1e6,"output_cpu_ms":stats.output_cpu_nanoseconds as f64/1e6,"queue_submit_ms":stats.queue_submit_nanoseconds as f64/1e6,"decode_ms":stats.native_decode_nanoseconds as f64/1e6,"upload_bytes":stats.upload_bytes,"native_copy_bytes":stats.native_gpu_local_copy_bytes,"pipe_bytes":stats.native_pipe_bytes,"readback_bytes":stats.readback_bytes,"status_readback_bytes":stats.status_readback_bytes,"passes":stats.compute_passes,"fused_passes":stats.fused_pointwise_passes,"native_allocate_ms":stats.native_allocate_nanoseconds as f64/1e6,"resident_stills":stats.resident_still_nodes,"allocated_bytes":host.memory().allocated,"peak_bytes":host.memory().peak})
            );
            if let Some(graph) = reference {
                let alpha_reference = fold_render::render_aces_with(
                    graph.clone(),
                    config,
                    &mut fold_media::Decoder::default(),
                    &cancel,
                )?
                .into_rgba16(Some(&display))?;
                let alpha_actual = renderer.output_rgba16(&scene, Some(&display), &cancel)?;
                let gpu_scene = scene.readback(&cancel)?;
                let packed_reference = gpu_scene.into_rgba16(Some(&display))?;
                let packed_error = alpha_actual
                    .bytes()
                    .chunks_exact(2)
                    .zip(packed_reference.bytes().chunks_exact(2))
                    .map(|(a, b)| {
                        u16::from_le_bytes(a.try_into().unwrap())
                            .abs_diff(u16::from_le_bytes(b.try_into().unwrap()))
                    })
                    .max()
                    .unwrap();
                eprintln!("same-scene GPU/CPU packing max_code_error={packed_error}");
                if packed_error > 16 {
                    return Err("GPU packing differs on identical scene pixels".into());
                }
                let alpha_error = alpha_actual
                    .bytes()
                    .chunks_exact(2)
                    .zip(alpha_reference.bytes().chunks_exact(2))
                    .map(|(a, b)| {
                        u16::from_le_bytes(a.try_into().unwrap())
                            .abs_diff(u16::from_le_bytes(b.try_into().unwrap()))
                    })
                    .max()
                    .unwrap();
                let visible_error = alpha_actual
                    .bytes()
                    .chunks_exact(8)
                    .zip(alpha_reference.bytes().chunks_exact(8))
                    .map(|(a, b)| {
                        let alpha_a = u16::from_le_bytes(a[6..8].try_into().unwrap());
                        let alpha_b = u16::from_le_bytes(b[6..8].try_into().unwrap());
                        // Straight RGB has no observable meaning when both
                        // quantized alphas are zero. Still compare alpha itself.
                        let start = if alpha_a == 0 && alpha_b == 0 { 6 } else { 0 };
                        a[start..]
                            .chunks_exact(2)
                            .zip(b[start..].chunks_exact(2))
                            .map(|(a, b)| {
                                u16::from_le_bytes(a.try_into().unwrap())
                                    .abs_diff(u16::from_le_bytes(b.try_into().unwrap()))
                            })
                            .max()
                            .unwrap()
                    })
                    .max()
                    .unwrap();
                eprintln!(
                    "CPU/GPU RGBA16 raw_max_code_error={alpha_error} visible_max_code_error={visible_error}"
                );
                // OCIO CPU/GPU transforms are not bit-identical. The explicit
                // bound is 16/65535; hidden RGB differences remain reported.
                if visible_error > 16 {
                    return Err("GPU alpha delivery differs on visible pixels".into());
                }
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
