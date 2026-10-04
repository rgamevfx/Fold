//! Reproducible headless execution probe, not a desktop real-time benchmark.
//! Usage: execution_probe <background.mp4> <overlay.mp4> <new-project.fold> <frames>
//! Inputs must share the verified 1080p30 profile and contain at least `frames`.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::{Cancel, Decoder};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use fold_timeline::{Clip, ImportArgs, SourceMedia, Track, TrackKind};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 {
        return Err("expected background overlay new-project frames".into());
    }
    let destination = PathBuf::from(&args[2]);
    if destination.exists() {
        return Err("project already exists".into());
    }
    let frames: u32 = args[3].to_str().ok_or("invalid frame count")?.parse()?;
    let cancel = Cancel::default();
    let gpu_requested = std::env::var("FOLD_RENDER_BACKEND").as_deref() == Ok("gpu");
    #[cfg(not(feature = "gpu"))]
    if gpu_requested {
        return Err("GPU probe requires --features gpu".into());
    }
    let mut project = if gpu_requested {
        fold_app::color::new_project(8)
    } else {
        Project::new(8)
    };
    let import = |project: &mut Project,
                  path,
                  document,
                  video_track,
                  audio_track|
     -> Result<(), Box<dyn std::error::Error>> {
        let batch = fold_timeline::import_media(
            &project.snapshot(),
            &ImportArgs {
                document,
                paths: vec![path],
                at: Some(Time::ZERO),
                video_track,
                audio_track,
                audio_only: false,
            },
            &cancel,
        )?;
        project.commit(batch)?;
        Ok(())
    };
    import(&mut project, PathBuf::from(&args[0]), None, None, None)?;
    let (source, sequence) = fold_timeline::active(&project.snapshot())?;
    import(
        &mut project,
        PathBuf::from(&args[1]),
        Some(source.document),
        Some(sequence.tracks[1].id),
        Some(sequence.tracks[3].id),
    )?;
    let (_, mut sequence) = fold_timeline::active(&project.snapshot())?;
    let info = sequence.info()?;
    if info.width != 1920
        || info.height != 1080
        || info.rate != [30, 1]
        || frames == 0
        || frames > info.frames
    {
        return Err("probe requires 1920x1080 at 30/1 fps and a valid frame count".into());
    }
    for clip in &mut sequence.clips {
        if clip.track == sequence.tracks[1].id {
            clip.level = 0.5;
        }
    }
    let mut motion = fold_motion::Motion::empty();
    motion.info = info.clone();
    let text = fold_motion::authoring::add_content(&mut motion, "fold.motion.text")?;
    fold_motion::authoring::wave_position(
        &mut motion,
        text,
        fold_motion::geometry::Domain::Glyphs,
    )?;
    let title = DocumentId::new();
    let track = Track::new(TrackKind::Video, "Procedural title");
    sequence.clips.push(Clip {
        id: ObjectId::new(),
        track: track.id,
        link: None,
        asset: Default::default(),
        info: SourceMedia::Document {
            source: DocumentRef {
                document: title,
                output: "video".into(),
                extensions: Default::default(),
            },
            info: info.clone(),
        },
        start: Time::ZERO,
        source_start: Time::ZERO,
        duration: info.time(info.frames)?,
        level: 1.,
        extensions: Default::default(),
    });
    sequence.tracks.push(track);
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![
            Mutation::PutDocument(motion.document(title)?),
            Mutation::PutDocument(sequence.document(source.document)?),
        ],
    })?;
    project.commit(fold_app::media_workflow::select_output(
        &project.snapshot(),
        source.document,
    )?)?;
    if gpu_requested {
        project.commit(fold_app::color::set_output(
            &project.snapshot(),
            source.document,
            fold_color::settings::OutputTransform {
                display: "sRGB - Display".into(),
                ..Default::default()
            },
        )?)?;
    }
    let snapshot = project.snapshot();
    fold_project::save(&snapshot, &destination)?;
    let packages = fold_app::packages::builtins();
    let audio = packages.audio(&snapshot, source.document)?;
    if audio.regions.is_empty() {
        return Err("probe requires stereo audio content".into());
    }
    let mut audio_decoder = fold_media::audio::AudioDecoder::default();
    let begin = Instant::now();
    audio.preflight(&mut audio_decoder, &cancel)?;
    let cold_audio_ms = begin.elapsed().as_secs_f64() * 1000.;
    let begin = Instant::now();
    audio.preflight(&mut audio_decoder, &cancel)?;
    let warm_audio_ms = begin.elapsed().as_secs_f64() * 1000.;
    audio.evaluate(0, 4096, &mut audio_decoder, &cancel)?;
    println!(
        "audio {}",
        serde_json::json!({"cold_prepare_ms":cold_audio_ms,"warm_prepare_ms":warm_audio_ms,"pcm_usage":audio_decoder.prepared_usage()})
    );
    let mut decoder = Decoder::from_environment()?;
    // The oracle is deliberately software/CPU even when production evaluation
    // uses GPU-only decoder surfaces. Never force a hidden hardware download.
    let mut reference_decoder = Decoder::default();
    #[cfg(feature = "gpu")]
    let mut gpu = if gpu_requested {
        let (host, adapter) =
            pollster::block_on(fold_render::gpu::Host::headless(512 * 1024 * 1024))?;
        println!("GPU adapter: {adapter:?}");
        Some(fold_render::gpu::Renderer::new(host)?)
    } else {
        None
    };
    for pass in ["initial", "repeat"] {
        let mut compile_ms = Vec::new();
        let mut render_ms = Vec::new();
        let mut display_ms = Vec::new();
        for frame in 0..frames {
            #[cfg(feature = "gpu")]
            if let Some(renderer) = gpu.as_mut() {
                let begin = Instant::now();
                let request = fold_app::media_workflow::SceneRequest {
                    preview: None,
                    source: source.clone(),
                    time: info.time(frame)?,
                    dimensions: [info.width, info.height],
                };
                let scene = fold_app::media_workflow::evaluate_scene_gpu(
                    &snapshot,
                    &request,
                    renderer,
                    &mut decoder,
                    &cancel,
                )?;
                let mut display = fold_app::color::with_config(&snapshot, |config| {
                    let processor = config
                        .unwrap()
                        .display(fold_color::WORKING_SPACE, &Default::default())?;
                    renderer.output(&scene, Some(&processor), &cancel)
                })?;
                while !display.is_ready()? {
                    renderer.host().poll()?;
                    if begin.elapsed().as_secs() > 30 {
                        return Err("GPU frame timeout".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                println!(
                    "gpu frame={frame} pass={pass} wall_ms={:.3} gpu_ms={:?} transfers={:?} memory={:?}",
                    begin.elapsed().as_secs_f64() * 1000.,
                    display.gpu_nanoseconds()?.map(|v| v / 1e6),
                    display.statistics,
                    renderer.host().memory()
                );
                if frame == 0 {
                    let actual = display.readback(&cancel)?;
                    let reference = fold_app::media_workflow::evaluate_scene(
                        &snapshot,
                        &request,
                        &mut reference_decoder,
                        &cancel,
                    )?
                    .over_black();
                    let expected = fold_app::color::preview(&snapshot, &reference)?;
                    let error = actual
                        .rgba()
                        .iter()
                        .zip(expected.rgba())
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    println!("CPU/GPU output max_code_error={error}");
                    if error > 1 {
                        return Err("CPU/GPU pre-encode error exceeds one SDR code".into());
                    }
                    let delivery = fold_app::color::with_config(&snapshot, |config| {
                        let p = config.unwrap().display(
                            fold_color::WORKING_SPACE,
                            &fold_platform::color::output(&snapshot, source.document)?
                                .display_transform(),
                        )?;
                        renderer.output(&scene, Some(&p), &cancel)
                    })?
                    .readback(&cancel)?;
                    if actual.rgba() != delivery.rgba() {
                        return Err("matched GPU preview/delivery differ".into());
                    }
                }
                continue;
            }
            let begin = Instant::now();
            let graph = packages.video_with(
                &snapshot,
                &source,
                info.time(frame)?,
                [info.width, info.height],
                &cancel,
            )?;
            compile_ms.push(begin.elapsed().as_secs_f64() * 1000.);
            let begin = Instant::now();
            let result = fold_render::render_with(graph, &mut decoder, &cancel)?.over_black();
            render_ms.push(begin.elapsed().as_secs_f64() * 1000.);
            let begin = Instant::now();
            let display = result.to_display()?;
            std::hint::black_box(display.rgba());
            display_ms.push(begin.elapsed().as_secs_f64() * 1000.);
        }
        println!(
            "video {}",
            serde_json::json!({"pass":pass,"frames":frames,"compile_ms":compile_ms,"decode_render_ms":render_ms,"display_ms":display_ms})
        );
    }
    if gpu_requested {
        let path = destination.with_extension("mp4");
        fold_app::media_workflow::export(&project.snapshot(), &path, 0, frames.min(4), &cancel)?;
        println!("GPU explicit-output export: {}", path.display());
    }
    Ok(())
}
