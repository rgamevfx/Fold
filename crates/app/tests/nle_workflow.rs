//! Multi-track reference workflow through registered commands, not test-only
//! mutations. Optional FOLD_NLE_ARTIFACT_DIR retains a native-demo project.
use fold_app::{media_workflow, packages};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::{
    Cancel, Decoder,
    audio::{AUDIO_RATE, AudioDecoder},
};
use fold_platform::desktop::PreviewKey;
use fold_project::{Project, Snapshot};
use fold_timeline::{ClipEdit, ImportArgs, Sequence, SequenceEdit, TrackKind, package};
use std::{path::Path, process::Command};
fn t(n: i64) -> Time {
    Time::new(n, 1).unwrap()
}
fn ffmpeg(args: &[&str], path: &Path) {
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y"])
        .args(args)
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}
fn sequence(snapshot: &Snapshot) -> (DocumentId, Sequence) {
    let (reference, seq) = fold_timeline::active(snapshot).unwrap();
    (reference.document, seq)
}
fn apply(project: &mut Project, edit: SequenceEdit) {
    let snapshot = project.snapshot();
    let (document, _) = sequence(&snapshot);
    let request = package::request(
        &snapshot,
        package::EDIT,
        &package::EditArgs { document, edit },
    )
    .unwrap();
    project
        .commit(
            packages::builtins()
                .stage(&snapshot, &request, &Cancel::default())
                .unwrap(),
        )
        .unwrap();
}
fn import(project: &mut Project, args: ImportArgs) {
    let snapshot = project.snapshot();
    let request = package::request(&snapshot, package::IMPORT, &args).unwrap();
    project
        .commit(
            packages::builtins()
                .stage(&snapshot, &request, &Cancel::default())
                .unwrap(),
        )
        .unwrap();
}
fn clip(id: ObjectId, edit: ClipEdit) -> SequenceEdit {
    SequenceEdit::Clip {
        id,
        linked: true,
        edit,
    }
}
#[test]
fn multitrack_nle_edit_mix_render_save_reopen_and_export() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("FOLD_NLE_ARTIFACT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&directory).unwrap();
    let main = directory.join("Camera A.mp4");
    let overlay = directory.join("Cutaway.mp4");
    let music = directory.join("Music.wav");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100",
            "-t",
            "14",
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
        ],
        &main,
    );
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=red:size=320x180:rate=30",
            "-t",
            "4",
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
            "-an",
        ],
        &overlay,
    );
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:sample_rate=44100",
            "-t",
            "16",
            "-ac",
            "2",
            "-c:a",
            "pcm_s16le",
        ],
        &music,
    );
    let mut project = Project::new(32);
    import(
        &mut project,
        ImportArgs {
            document: None,
            paths: vec![main.clone()],
            at: None,
            video_track: None,
            audio_track: None,
            audio_only: false,
        },
    );
    let (document, seq) = sequence(&project.snapshot());
    let video = seq.clips[0].id;
    apply(
        &mut project,
        clip(
            video,
            ClipEdit::Trim {
                start: t(0),
                end: t(6),
            },
        ),
    );
    import(
        &mut project,
        ImportArgs {
            document: Some(document),
            paths: vec![main],
            at: Some(t(9)),
            video_track: Some(seq.tracks[0].id),
            audio_track: Some(seq.tracks[2].id),
            audio_only: false,
        },
    );
    let (_, seq) = sequence(&project.snapshot());
    let second = seq
        .clips
        .iter()
        .find(|c| c.start == t(9) && seq.track(c.track).unwrap().kind == TrackKind::Video)
        .unwrap()
        .id;
    apply(
        &mut project,
        clip(
            second,
            ClipEdit::Trim {
                start: t(9),
                end: t(14),
            },
        ),
    );
    import(
        &mut project,
        ImportArgs {
            document: Some(document),
            paths: vec![overlay],
            at: Some(t(3)),
            video_track: Some(seq.tracks[1].id),
            audio_track: None,
            audio_only: false,
        },
    );
    let (_, seq) = sequence(&project.snapshot());
    let cutaway = seq
        .clips
        .iter()
        .find(|c| c.track == seq.tracks[1].id)
        .unwrap()
        .id;
    apply(&mut project, clip(cutaway, ClipEdit::Level(0.5)));
    import(
        &mut project,
        ImportArgs {
            document: Some(document),
            paths: vec![music],
            at: Some(t(0)),
            video_track: None,
            audio_track: Some(seq.tracks[3].id),
            audio_only: true,
        },
    );
    let (_, seq) = sequence(&project.snapshot());
    let music_clip = seq
        .clips
        .iter()
        .find(|c| c.track == seq.tracks[3].id)
        .unwrap()
        .id;
    apply(&mut project, clip(music_clip, ClipEdit::Level(0.35)));
    let before = project.snapshot();
    apply(
        &mut project,
        clip(
            video,
            ClipEdit::Split {
                at: t(2),
                right_id: ObjectId::new(),
            },
        ),
    );
    let split = project.snapshot();
    assert_eq!(sequence(&split).1.clips.len(), 8);
    assert_eq!(
        project.undo().unwrap().state().documents,
        before.state().documents
    );
    assert_eq!(
        project.redo().unwrap().state().documents,
        split.state().documents
    );
    let audio = packages::builtins().audio(&split, document).unwrap();
    let mut decoder = AudioDecoder::default();
    audio.preflight(&mut decoder, &Cancel::default()).unwrap();
    let mixed = audio
        .evaluate(
            3 * u64::from(AUDIO_RATE),
            4096,
            &mut decoder,
            &Cancel::default(),
        )
        .unwrap();
    let mut muted_seq = sequence(&split).1;
    muted_seq.tracks[3].enabled = false;
    let without_music = muted_seq
        .audio_plan(&split)
        .unwrap()
        .evaluate(
            3 * u64::from(AUDIO_RATE),
            4096,
            &mut decoder,
            &Cancel::default(),
        )
        .unwrap();
    assert!(
        mixed
            .iter()
            .zip(&without_music)
            .any(|(a, b)| (a[0] - b[0]).abs() > 0.001)
    );
    muted_seq.tracks[3].enabled = true;
    muted_seq.tracks[3].solo = true;
    let solo = muted_seq
        .audio_plan(&split)
        .unwrap()
        .evaluate(
            3 * u64::from(AUDIO_RATE),
            4096,
            &mut decoder,
            &Cancel::default(),
        )
        .unwrap();
    for ((sum, voice), music) in mixed.iter().zip(without_music).zip(solo) {
        assert!((sum[0] - voice[0] - music[0]).abs() < 0.000001);
    }
    let saved = directory.join("Multi-track.fold");
    fold_project::save(&split, &saved).unwrap();
    let reopened = fold_project::load(&saved, 32).unwrap().snapshot();
    assert_eq!(split.state().documents, reopened.state().documents);
    let content = media_workflow::content(&reopened).unwrap();
    let key = PreviewKey {
        content,
        frame: 72,
        dimensions: [320, 180],
        view: 1,
    };
    let mut video_decoder = Decoder::default();
    let preview = media_workflow::evaluate(&reopened, &key, &mut video_decoder, &Cancel::default())
        .unwrap()
        .to_display()
        .unwrap();
    let output = directory.join("Selected range.mp4");
    if output.exists() && std::env::var_os("FOLD_NLE_ARTIFACT_DIR").is_some() {
        std::fs::remove_file(&output).unwrap();
    }
    media_workflow::export(&reopened, &output, 72, 96, &Cancel::default()).unwrap();
    let encoded = fold_media::inspect(&output, &Cancel::default()).unwrap();
    assert_eq!(encoded.info.frames, 24);
    let image = video_decoder
        .decode(&encoded, Time::ZERO, [320, 180], &Cancel::default())
        .unwrap();
    let displayed = fold_render::render(fold_render::RenderGraph {
        width: 320,
        height: 180,
        nodes: vec![fold_render::ImageOp::Media(image)],
        output: 0,
    })
    .unwrap()
    .to_display()
    .unwrap();
    let mean_error = displayed
        .rgba()
        .iter()
        .zip(preview.rgba())
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / displayed.rgba().len() as f64;
    assert!(
        mean_error < 3.0,
        "preview/export mean byte error: {mean_error}"
    );
    let actual = AudioDecoder::default()
        .read(&encoded.into(), 0, 4096, &Cancel::default())
        .unwrap();
    let mse = actual
        .iter()
        .zip(mixed)
        .map(|(a, b)| (a[0] - b[0]).powi(2))
        .sum::<f32>()
        / 4096.0;
    assert!(mse < 0.0001, "mixed range AAC alignment MSE: {mse}");
}
