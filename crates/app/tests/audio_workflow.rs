use fold_app::{media_workflow, timeline_workflow};
use fold_foundation::{ObjectId, Time};
use fold_media::{
    Cancel,
    audio::{AUDIO_RATE, AudioDecoder},
};
use fold_project::Project;
use fold_timeline::{ClipEdit, edit_clip};
use std::{path::Path, process::Command};

fn fixture(path: &Path, rate: &str, duration: &str, tone: &str) {
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size=64x32:rate={rate}"),
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={tone}:sample_rate=44100"),
            "-t",
            duration,
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
            "aac",
        ])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}
#[test]
fn linked_edits_offline_audio_export_and_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.mp4");
    let b = dir.path().join("b.mp4");
    fixture(&a, "24", "2", "440");
    fixture(&b, "30", "1", "880");
    let cancel = Cancel::default();
    let mut project = Project::new(32);
    project
        .commit(timeline_workflow::import(&project.snapshot(), &[a, b], &cancel).unwrap())
        .unwrap();
    let before = project.snapshot();
    let (reference, seq) = timeline_workflow::active(&before).unwrap();
    assert_eq!(seq.info().unwrap().frames, 72);
    let mut decoder = AudioDecoder::default();
    let plan = seq.audio_plan(&before).unwrap();
    let expected = plan
        .evaluate(20_000, 48_000, &mut decoder, &cancel)
        .unwrap();
    assert!(expected.iter().any(|f| f[0].abs() > 0.01));
    let right = ObjectId::new();
    project
        .commit(
            edit_clip(
                &before,
                reference.document,
                seq.clips[0].id,
                ClipEdit::Split {
                    at: Time::new(1, 1).unwrap(),
                    right_id: right,
                },
            )
            .unwrap(),
        )
        .unwrap();
    let split = project.snapshot();
    let (_, split_seq) = timeline_workflow::active(&split).unwrap();
    assert_eq!(
        split_seq
            .audio_plan(&split)
            .unwrap()
            .evaluate(20_000, 48_000, &mut decoder, &cancel)
            .unwrap(),
        expected
    );
    project
        .commit(
            edit_clip(
                &split,
                reference.document,
                right,
                ClipEdit::Trim {
                    start: Time::new(5, 4).unwrap(),
                    end: Time::new(7, 4).unwrap(),
                },
            )
            .unwrap(),
        )
        .unwrap();
    let trimmed = project.snapshot();
    let (_, seq) = timeline_workflow::active(&trimmed).unwrap();
    let plan = seq.audio_plan(&trimmed).unwrap();
    assert!(
        plan.evaluate(48_000, 12_000, &mut decoder, &cancel)
            .unwrap()
            .iter()
            .all(|f| *f == [0.0; 2])
    );
    assert_eq!(
        plan.evaluate(60_000, 4096, &mut decoder, &cancel).unwrap(),
        split_seq
            .audio_plan(&split)
            .unwrap()
            .evaluate(60_000, 4096, &mut decoder, &cancel)
            .unwrap()
    );
    project
        .commit(
            edit_clip(
                &trimmed,
                reference.document,
                right,
                ClipEdit::Move {
                    start: Time::new(1, 1).unwrap(),
                },
            )
            .unwrap(),
        )
        .unwrap();
    let moved = project.snapshot();
    let (_, seq) = timeline_workflow::active(&moved).unwrap();
    assert_eq!(
        seq.audio_plan(&moved)
            .unwrap()
            .evaluate(48_000, 4096, &mut decoder, &cancel)
            .unwrap(),
        plan.evaluate(60_000, 4096, &mut decoder, &cancel).unwrap()
    );
    assert_eq!(
        project.undo().unwrap().state().documents,
        trimmed.state().documents
    );
    assert_eq!(
        project.redo().unwrap().state().documents,
        moved.state().documents
    );
    let saved = dir.path().join("edited.fold");
    fold_project::save(&moved, &saved).unwrap();
    let reopened = fold_project::load(&saved, 8).unwrap();
    assert_eq!(
        media_workflow::content(&reopened.snapshot()).unwrap(),
        media_workflow::content(&moved).unwrap()
    );
    let output = dir.path().join("range.mp4");
    media_workflow::export(&moved, &output, 12, 60, &cancel).unwrap();
    let source = fold_media::inspect(&output, &cancel).unwrap();
    assert_eq!(source.info.frames, 48);
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate,channels,duration",
            "-of",
            "default=nw=1",
        ])
        .arg(&output)
        .output()
        .unwrap();
    let text = String::from_utf8(probe.stdout).unwrap();
    assert!(text.contains("sample_rate=48000"), "{text}");
    assert!(text.contains("channels=2"), "{text}");
    assert!(text.contains("duration=2.000000"), "{text}");
    let pcm = AudioDecoder::default()
        .read(&source.clone().into(), 0, AUDIO_RATE as usize, &cancel)
        .unwrap();
    let expected = seq
        .audio_plan(&moved)
        .unwrap()
        .evaluate(24_000, AUDIO_RATE as usize, &mut decoder, &cancel)
        .unwrap();
    let mse = pcm
        .iter()
        .zip(expected)
        .map(|(a, b)| (a[0] - b[0]).powi(2))
        .sum::<f32>()
        / AUDIO_RATE as f32;
    assert!(mse < 0.0001, "AAC range sample alignment MSE {mse}");
    assert!(media_workflow::export(&moved, &output, 0, 1, &cancel).is_err());
    let cancelled = Cancel::default();
    cancelled.cancel();
    let output = dir.path().join("cancel.mp4");
    assert!(media_workflow::export(&moved, &output, 0, 24, &cancelled).is_err());
    assert!(!output.exists());
}

#[test]
fn missing_audio_is_silent_and_pinned_pcm_rejects_metadata_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("silent.mp4");
    let info = fold_media::VideoInfo {
        width: 2,
        height: 2,
        rate: [24, 1],
        frames: 2,
    };
    let cancel = Cancel::default();
    let mut encoder = fold_media::Encoder::new(&path, &info, 2, cancel.clone()).unwrap();
    for _ in 0..2 {
        encoder.write_rgb(&[0; 12]).unwrap();
    }
    encoder.finish().unwrap();
    let source = fold_media::inspect(&path, &cancel).unwrap();
    let mut decoder = AudioDecoder::default();
    assert_eq!(
        decoder
            .read(&source.clone().into(), -10, 48, &cancel)
            .unwrap(),
        vec![[0.0; 2]; 48]
    );
    let mut invalid = source.clone();
    invalid.info.frames = 3;
    assert!(decoder.read(&invalid.into(), 0, 48, &cancel).is_err());
    std::fs::write(&path, b"changed media").unwrap();
    assert_eq!(
        decoder
            .read(&source.clone().into(), 0, 48, &cancel)
            .unwrap(),
        vec![[0.0; 2]; 48]
    );
    assert!(
        AudioDecoder::default()
            .read(&source.clone().into(), 0, 48, &cancel)
            .is_err()
    );
    cancel.cancel();
    assert!(decoder.preflight(&source.into(), &cancel).is_err());
}

#[test]
fn sequence_desktop_commands_are_transactional_and_nonblocking() {
    use fold_platform::desktop::{DesktopClient, DesktopCommand};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.mp4");
    fixture(&path, "24000/1001", "2", "440");
    let mut project = Project::new(8);
    project
        .commit(
            timeline_workflow::import(&project.snapshot(), &[path], &Cancel::default()).unwrap(),
        )
        .unwrap();
    let mut session = fold_app::session::Session::new(project);
    let (reference, sequence) = timeline_workflow::active(&session.snapshot().unwrap()).unwrap();
    let clip = sequence.clips[0].id;
    let count = |s: &fold_app::session::Session| {
        timeline_workflow::active(&s.snapshot().unwrap())
            .unwrap()
            .1
            .clips
            .len()
    };
    let command = |s: &fold_app::session::Session, edit| {
        DesktopCommand::Extension(
            fold_timeline::package::request(
                &s.snapshot().unwrap(),
                fold_timeline::package::EDIT,
                &fold_timeline::package::EditArgs {
                    document: reference.document,
                    edit: fold_timeline::SequenceEdit::Clip {
                        id: clip,
                        linked: true,
                        edit,
                    },
                },
            )
            .unwrap(),
        )
    };
    let start = std::time::Instant::now();
    session.command(command(
        &session,
        ClipEdit::Split {
            at: sequence.info().unwrap().time(24).unwrap(),
            right_id: ObjectId::new(),
        },
    ));
    assert!(start.elapsed() < std::time::Duration::from_millis(100));
    assert_eq!(count(&session), 4);
    let content = session.state().content.clone();
    session.command(command(
        &session,
        ClipEdit::Move {
            start: sequence.info().unwrap().time(1).unwrap(),
        },
    ));
    assert_eq!(session.state().content, content);
    session.command(DesktopCommand::Undo);
    assert_eq!(count(&session), 2);
    session.command(DesktopCommand::Redo);
    assert_eq!(count(&session), 4);
    session.command(DesktopCommand::Seek(999));
    assert_eq!(session.state().frame, session.state().frames - 1);
}
