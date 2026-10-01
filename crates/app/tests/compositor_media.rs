//! End-to-end audio preservation and MP4 export. Set FOLD_COMPOSITOR_ARTIFACT_DIR
//! to retain the editable project and exported movie outside the repository.
use fold_app::{composition, media_workflow, packages};
use fold_compositor::{Composite, Node, Parameters};
use fold_media::{Cancel, audio::AudioDecoder};
use fold_project::Project;
use fold_timeline::{ImportArgs, Sequence, SourceMedia, TrackKind, package};
use std::process::Command;
#[test]
fn create_composition_keeps_linked_audio_and_exports_the_same_video() {
    let temp = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("FOLD_COMPOSITOR_ARTIFACT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&directory).unwrap();
    let input = directory.join("Compositor source.mp4");
    assert!(
        Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=24",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000",
                "-t",
                "3",
                "-c:v",
                "libx264",
                "-threads",
                "1",
                "-pix_fmt",
                "yuv420p",
                "-color_range",
                "tv",
                "-colorspace",
                "bt709",
                "-color_trc",
                "bt709",
                "-color_primaries",
                "bt709",
                "-c:a",
                "aac"
            ])
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let mut project = Project::new(32);
    let snapshot = project.snapshot();
    let request = package::request(
        &snapshot,
        package::IMPORT,
        &ImportArgs {
            document: None,
            paths: vec![input],
            at: None,
            video_track: None,
            audio_track: None,
            audio_only: false,
        },
    )
    .unwrap();
    project
        .commit(
            packages::builtins()
                .stage(&snapshot, &request, &Cancel::default())
                .unwrap(),
        )
        .unwrap();
    let before = project.snapshot();
    let (reference, sequence) = fold_timeline::active(&before).unwrap();
    let selected: Vec<_> = sequence
        .clips
        .iter()
        .filter(|c| sequence.track(c.track).unwrap().kind == TrackKind::Video)
        .flat_map(|c| sequence.linked_ids(c.id, true).unwrap())
        .collect();
    let before_audio = packages::builtins()
        .audio(&before, reference.document)
        .unwrap();
    let before_pixels = fold_render::render(
        packages::builtins()
            .video(
                &before,
                &reference,
                fold_foundation::Time::new(1, 1).unwrap(),
                320,
                180,
            )
            .unwrap(),
    )
    .unwrap();
    let request = package::request(
        &before,
        composition::CREATE,
        &composition::CreateArgs {
            document: reference.document,
            clips: selected,
        },
    )
    .unwrap();
    project
        .commit(
            packages::builtins()
                .stage(&before, &request, &Cancel::default())
                .unwrap(),
        )
        .unwrap();
    let after = project.snapshot();
    let nested = Sequence::from_document(&after.state().documents[&reference.document]).unwrap();
    let audio = nested
        .clips
        .iter()
        .find(|c| nested.track(c.track).unwrap().kind == TrackKind::Audio)
        .unwrap();
    assert!(audio.link.is_none());
    let original_audio = sequence.clips.iter().find(|c| c.id == audio.id).unwrap();
    assert_eq!(
        (audio.start, audio.source_start, audio.duration, audio.level),
        (
            original_audio.start,
            original_audio.source_start,
            original_audio.duration,
            original_audio.level
        )
    );
    let after_audio = packages::builtins()
        .audio(&after, reference.document)
        .unwrap();
    assert_eq!(
        before_audio
            .evaluate(
                48000,
                2048,
                &mut AudioDecoder::default(),
                &Cancel::default()
            )
            .unwrap(),
        after_audio
            .evaluate(
                48000,
                2048,
                &mut AudioDecoder::default(),
                &Cancel::default()
            )
            .unwrap()
    );
    assert_eq!(
        before_pixels.pixels(),
        fold_render::render(
            packages::builtins()
                .video(
                    &after,
                    &reference,
                    fold_foundation::Time::new(1, 1).unwrap(),
                    320,
                    180
                )
                .unwrap()
        )
        .unwrap()
        .pixels()
    );
    // Give the retained inspection project a visible editable grade/blur chain.
    let SourceMedia::Document { source, .. } = &nested
        .clips
        .iter()
        .find(|c| nested.track(c.track).unwrap().kind == TrackKind::Video)
        .unwrap()
        .info
    else {
        panic!("expected nested clip")
    };
    let mut composite =
        Composite::from_document(&after.state().documents[&source.document]).unwrap();
    let index = composite.nodes.len() - 1;
    let previous = composite.nodes[index].inputs[0].unwrap();
    let grade = Node::new(
        Parameters::Grade {
            gain: [0.65, 1.0, 0.8],
        },
        vec![previous],
    );
    let blur = Node::new(Parameters::Blur { radius: 2 }, vec![grade.id]);
    composite.nodes[index].inputs[0] = Some(blur.id);
    composite.nodes.splice(index..index, [grade, blur]);
    let request = package::request(
        &after,
        fold_compositor::package::EDIT,
        &fold_compositor::package::EditArgs {
            document: source.document,
            composite,
        },
    )
    .unwrap();
    project
        .commit(
            packages::builtins()
                .stage(&after, &request, &Cancel::default())
                .unwrap(),
        )
        .unwrap();
    let path = directory.join("phase-8.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 32).unwrap();
    let output = directory.join("phase-8-export.mp4");
    media_workflow::export(&reopened.snapshot(), &output, 0, 48, &Cancel::default()).unwrap();
    let encoded = fold_media::inspect(&output, &Cancel::default()).unwrap();
    assert_eq!(encoded.info.frames, 48);
    assert!(fold_media::audio::has_audio(&output, &Cancel::default()).unwrap());
    // Undo restores the nested graph edit, then the entire create/replace batch.
    project.undo().unwrap();
    assert_eq!(
        media_workflow::content(&project.snapshot()).unwrap(),
        media_workflow::content(&after).unwrap()
    );
    project.undo().unwrap();
    assert_eq!(
        media_workflow::content(&project.snapshot()).unwrap(),
        media_workflow::content(&before).unwrap()
    );
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
    assert_eq!(project.snapshot().state().assets, before.state().assets);
    // Redo must restore both documents as one batch, then the graph edit.
    project.redo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        after.state().documents
    );
    assert_eq!(project.snapshot().state().assets, after.state().assets);
    project.redo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        reopened.snapshot().state().documents
    );
    assert_eq!(
        project.snapshot().state().assets,
        reopened.snapshot().state().assets
    );
}
