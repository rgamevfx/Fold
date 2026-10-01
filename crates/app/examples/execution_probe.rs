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
    let mut project = Project::new(8);
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
    let mut decoder = Decoder::default();
    for pass in ["initial", "repeat"] {
        let mut compile_ms = Vec::new();
        let mut render_ms = Vec::new();
        let mut display_ms = Vec::new();
        for frame in 0..frames {
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
    Ok(())
}
