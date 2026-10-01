//! Real codec contract tests. FFmpeg/ffprobe + libx264 + zscale are prerequisites;
//! absence is a failure, not a silently skipped media gate.
use fold_app::media_workflow as workflow;
use fold_foundation::Time;
use fold_media::{Cancel, Decoder, Encoder, VideoInfo};
use fold_platform::desktop::{DesktopClient, DesktopCommand, PreviewKey};
use fold_project::{Project, load, save};
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn fixture(path: &Path, values: &[u8]) {
    let info = VideoInfo {
        width: 64,
        height: 32,
        rate: [24000, 1001],
        frames: values.len() as u32,
    };
    let mut encoder = Encoder::new(path, &info, info.frames, Cancel::default()).unwrap();
    for value in values {
        encoder.write_rgb(&vec![*value; 64 * 32 * 3]).unwrap();
    }
    encoder.finish().unwrap();
}

#[test]
fn codec_timing_color_import_composition_export_and_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.mp4");
    let b = dir.path().join("b.mp4");
    fixture(&a, &[0, 64, 128, 192, 255]);
    fixture(&b, &[255, 192, 128, 64, 0]);
    let cancel = Cancel::default();
    let source = fold_media::inspect(&a, &cancel).unwrap();
    assert_eq!(source.info.rate, [24000, 1001]);
    assert_eq!(source.info.frames, 5);
    let mut decoder = Decoder::default();
    for i in [4, 0, 3, 1, 2] {
        let image = decoder
            .decode(&source, source.info.time(i).unwrap(), [64, 32], &cancel)
            .unwrap();
        let graph = fold_render::RenderGraph {
            width: 64,
            height: 32,
            nodes: vec![fold_render::ImageOp::Media(image)],
            output: 0,
        };
        let display = fold_render::render(graph).unwrap().to_display().unwrap();
        let expected = [0, 64, 128, 192, 255][i as usize];
        assert!(
            display
                .rgba()
                .chunks_exact(4)
                .all(|p| p[..3].iter().all(|v| v.abs_diff(expected) <= 3)),
            "frame {i}: {:?}",
            &display.rgba()[..4]
        );
    }
    assert!(
        decoder
            .decode(&source, Time::new(-1, 1).unwrap(), [64, 32], &cancel)
            .is_err()
    );
    assert!(
        decoder
            .decode(&source, source.info.time(5).unwrap(), [64, 32], &cancel)
            .is_err()
    );
    let mut project = Project::new(8);
    project
        .commit(workflow::import(&project.snapshot(), &[a.clone(), b], &cancel).unwrap())
        .unwrap();
    let pinned = project.snapshot();
    let content = workflow::content(&pinned).unwrap();
    project.undo().unwrap();
    assert!(workflow::content(&project.snapshot()).is_err());
    project.redo().unwrap();
    assert_eq!(content, workflow::content(&project.snapshot()).unwrap());
    let path = dir.path().join("video.fold");
    save(&pinned, &path).unwrap();
    let reopened = load(&path, 8).unwrap();
    assert_eq!(content, workflow::content(&reopened.snapshot()).unwrap());
    let key = PreviewKey {
        target: None,
        content,
        frame: 0,
        dimensions: [64, 32],
        view: 1,
    };
    let frame = workflow::evaluate(&pinned, &key, &mut decoder, &cancel).unwrap();
    let display = frame.to_display().unwrap();
    assert!(
        display.rgba()[0].abs_diff(188) <= 3,
        "{:?}",
        &display.rgba()[..4]
    ); // Linear-light blend, not encoded midpoint 128.
    let mut ppm = Vec::new();
    frame.write_ppm(&mut ppm).unwrap();
    assert_eq!(
        &ppm[b"P6\n64 32\n255\n".len()..],
        display
            .rgba()
            .chunks_exact(4)
            .flat_map(|p| p[..3].iter().copied())
            .collect::<Vec<_>>()
    );
    let output = dir.path().join("output.mp4");
    workflow::export(&pinned, &output, 1, 4, &cancel).unwrap();
    let result = fold_media::inspect(&output, &cancel).unwrap();
    assert_eq!(result.info.frames, 3);
    assert_eq!(result.info.rate, [24000, 1001]);
    for output_index in 0..3 {
        let mut key = key.clone();
        key.frame = output_index + 1;
        let expected = workflow::evaluate(&pinned, &key, &mut decoder, &cancel)
            .unwrap()
            .to_display()
            .unwrap();
        let encoded = decoder
            .decode(
                &result,
                result.info.time(output_index).unwrap(),
                [64, 32],
                &cancel,
            )
            .unwrap();
        let graph = fold_render::RenderGraph {
            width: 64,
            height: 32,
            nodes: vec![fold_render::ImageOp::Media(encoded)],
            output: 0,
        };
        let actual = fold_render::render(graph).unwrap().to_display().unwrap();
        assert!(
            actual
                .rgba()
                .iter()
                .zip(expected.rgba())
                .all(|(a, b)| a.abs_diff(*b) <= 4)
        );
    }
    assert!(workflow::export(&pinned, &output, 0, 1, &cancel).is_err());
    decoder
        .decode(&source, Time::ZERO, [64, 32], &cancel)
        .unwrap();
    std::fs::write(&a, b"replaced").unwrap();
    let mut fresh = Decoder::default();
    assert!(
        workflow::evaluate(&pinned, &key, &mut fresh, &cancel)
            .unwrap_err()
            .contains("fingerprint")
    );
    // The retained source remains immutable even after replacement on disk.
    assert!(
        decoder
            .decode(&source, Time::ZERO, [64, 32], &cancel)
            .is_ok()
    );
}

#[test]
fn seeks_cross_fractional_second_boundaries_and_cancel_active_encoder() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("long-gop.mp4");
    let values: Vec<u8> = (0..70).map(|i| i * 3).collect();
    fixture(&input, &values);
    let source = fold_media::inspect(&input, &Cancel::default()).unwrap();
    let mut decoder = Decoder::default();
    for i in [69, 23, 24, 25, 47, 48, 49, 0] {
        let image = decoder
            .decode(
                &source,
                source.info.time(i).unwrap(),
                [64, 32],
                &Cancel::default(),
            )
            .unwrap();
        let graph = fold_render::RenderGraph {
            width: 64,
            height: 32,
            nodes: vec![fold_render::ImageOp::Media(image)],
            output: 0,
        };
        let display = fold_render::render(graph).unwrap().to_display().unwrap();
        assert!(
            display.rgba()[0].abs_diff(values[i as usize]) <= 3,
            "frame {i}: got {} expected {}",
            display.rgba()[0],
            values[i as usize]
        );
    }
    let output = dir.path().join("cancel-active.mp4");
    let cancel = Cancel::default();
    let mut encoder = Encoder::new(&output, &source.info, 70, cancel.clone()).unwrap();
    encoder.write_rgb(&vec![128; 64 * 32 * 3]).unwrap();
    cancel.cancel();
    assert!(encoder.write_rgb(&vec![128; 64 * 32 * 3]).is_err());
    assert!(encoder.finish().is_err());
    assert!(!output.exists());
    // No orphaned output temporary remains after the encoder is dropped.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn saturated_colors_and_explicit_profile_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("colors.mp4");
    let info = VideoInfo {
        width: 64,
        height: 32,
        frames: 4,
        rate: [24, 1],
    };
    let colors = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [128, 64, 192]];
    let mut encoder = Encoder::new(&path, &info, 4, Cancel::default()).unwrap();
    for color in colors {
        encoder.write_rgb(&color.repeat(64 * 32)).unwrap();
    }
    encoder.finish().unwrap();
    let source = fold_media::inspect(&path, &Cancel::default()).unwrap();
    let mut decoder = Decoder::default();
    for (index, color) in colors.into_iter().enumerate() {
        let image = decoder
            .decode(
                &source,
                info.time(index as u32).unwrap(),
                [64, 32],
                &Cancel::default(),
            )
            .unwrap();
        let graph = fold_render::RenderGraph {
            width: 64,
            height: 32,
            nodes: vec![fold_render::ImageOp::Media(image)],
            output: 0,
        };
        let actual = fold_render::render(graph).unwrap().to_display().unwrap();
        assert!(
            actual.rgba()[..3]
                .iter()
                .zip(color)
                .all(|(a, b)| a.abs_diff(b) <= 5),
            "{color:?} -> {:?}",
            &actual.rgba()[..3]
        );
    }
    let unsupported = dir.path().join("untagged.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:size=64x32:rate=24",
            "-frames:v",
            "1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&unsupported)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(
        fold_media::inspect(&unsupported, &Cancel::default())
            .unwrap_err()
            .contains("unsupported")
    );
    let cancelled = Cancel::default();
    cancelled.cancel();
    assert!(
        fold_media::inspect(&path, &cancelled)
            .unwrap_err()
            .contains("cancel")
    );
}

#[test]
fn future_documents_and_extensions_survive_import_and_opacity_edits() {
    use fold_foundation::DocumentId;
    use fold_project::{Document, EditBatch, Mutation, Revision};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.mp4");
    fixture(&input, &[128]);
    let future = Document {
        id: DocumentId::new(),
        package_id: fold_timeline::PACKAGE.into(),
        type_id: fold_timeline::VIDEO_LAYERS.into(),
        schema_version: 99,
        revision: Revision::default(),
        dependencies: vec![],
        assets: vec![],
        payload: b"opaque future payload".to_vec(),
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(future.clone())],
        })
        .unwrap();
    project
        .commit(
            workflow::import(
                &project.snapshot(),
                std::slice::from_ref(&input),
                &Cancel::default(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        project.snapshot().state().documents[&future.id].payload,
        future.payload
    );
    let (reference, mut layers) = workflow::active(&project.snapshot()).unwrap();
    layers.extensions.insert("vendor-extra".into(), true.into());
    let mut document = layers.document(reference.document).unwrap();
    document
        .extensions
        .insert("envelope-extra".into(), "retained".into());
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let mut session = fold_app::session::Session::new(project);
    session.command(DesktopCommand::Opacity(0.25));
    let path = dir.path().join("saved.fold");
    session.command(DesktopCommand::Save(path.clone()));
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.state().busy {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut reopened = load(&path, 8).unwrap();
    assert_eq!(
        workflow::active(&reopened.snapshot())
            .unwrap()
            .1
            .foreground_opacity,
        0.25
    );
    reopened
        .commit(workflow::import(&reopened.snapshot(), &[input], &Cancel::default()).unwrap())
        .unwrap();
    let snapshot = reopened.snapshot();
    let (reference, layers) = workflow::active(&snapshot).unwrap();
    assert_eq!(layers.extensions["vendor-extra"], true);
    assert_eq!(
        snapshot.state().documents[&reference.document].extensions["envelope-extra"],
        "retained"
    );
    assert_eq!(
        snapshot.state().documents[&future.id].payload,
        future.payload
    );
}

#[test]
fn latest_requests_cancel_export_and_nonblocking_commands() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.mp4");
    fixture(&input, &[0, 64, 128, 192, 255]);
    let mut session = fold_app::session::Session::default();
    let start = Instant::now();
    session.command(DesktopCommand::Import(vec![input]));
    assert!(start.elapsed() < Duration::from_millis(100));
    let deadline = Instant::now() + Duration::from_secs(30);
    while session.state().busy {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        session.state().content.is_some(),
        "{}",
        session.state().status
    );
    for frame in [0, 1, 2, 3, 4] {
        session.request_preview(session.state().preview_key(frame, 2).unwrap());
    }
    let final_key = session.state().preview_key(4, 2).unwrap();
    loop {
        assert!(Instant::now() < deadline);
        if let Some(result) = session.take_preview() {
            assert_eq!(result.key, final_key);
            assert_eq!(result.frame.unwrap().dimensions(), [32, 16]);
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let successful = dir.path().join("concurrent.mp4");
    session.command(DesktopCommand::Export {
        path: successful.clone(),
        start: 0,
        end: 5,
    });
    assert!(session.state().busy);
    session.request_preview(session.state().preview_key(1, 4).unwrap());
    let mut saw_preview = false;
    while session.state().busy || !saw_preview {
        assert!(Instant::now() < deadline);
        let poll_start = Instant::now();
        session.poll();
        assert!(poll_start.elapsed() < Duration::from_millis(100));
        if let Some(result) = session.take_preview() {
            assert_eq!(result.key.frame, 1);
            assert_eq!(result.frame.unwrap().dimensions(), [16, 8]);
            saw_preview = true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(successful.exists(), "{}", session.state().status);
    let output = dir.path().join("cancelled.mp4");
    session.command(DesktopCommand::Export {
        path: output.clone(),
        start: 0,
        end: 5,
    });
    session.command(DesktopCommand::Cancel);
    while session.state().busy {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!output.exists());
    assert!(session.state().status.contains("cancel"));
}
