//! Explicit opt-in native device test. Runs against the default output device,
//! not a fake clock. FOLD_NATIVE_SECONDS=600 exercises the ten-minute gate.
#![cfg(feature = "desktop")]
use fold_app::{media_workflow, playback::Playback, timeline_workflow};
use fold_foundation::{Rounding, Time};
use fold_media::{Cancel, Decoder, audio::AUDIO_RATE};
use fold_platform::desktop::PreviewKey;
use fold_project::Project;
use std::{
    process::Command,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires a native stereo 48 kHz float output device; plays a quiet test tone"]
fn native_device_clock_seek_and_drift() {
    let seconds: u32 = std::env::var("FOLD_NATIVE_SECONDS")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(6);
    assert!((4..=600).contains(&seconds) && seconds.is_multiple_of(2));
    let directory = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for (index, rate) in [30, 24].into_iter().enumerate() {
        let path = directory.path().join(format!("source{index}.mp4"));
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
                "sine=frequency=440:sample_rate=44100",
                "-af",
                "volume=0.05",
                "-t",
                &(seconds / 2).to_string(),
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
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        paths.push(path);
    }
    let mut project = Project::new(8);
    project
        .commit(timeline_workflow::import(&project.snapshot(), &paths, &Cancel::default()).unwrap())
        .unwrap();
    let snapshot = project.snapshot();
    let (source, info) = media_workflow::output(&snapshot).unwrap();
    let mut playback = Playback::default();
    let content = media_workflow::content(&snapshot).unwrap();
    let mut decoder = Decoder::default();
    let mut key = PreviewKey {
        output: "video".into(),
        target: None,
        content,
        frame: 0,
        dimensions: [64, 32],
        view: 1,
        channels: Default::default(),
    };
    media_workflow::evaluate(&snapshot, &key, &mut decoder, &Cancel::default()).unwrap();
    playback.play(snapshot.clone(), source.document, 0, info.frames);
    let deadline = Instant::now() + Duration::from_secs(u64::from(seconds) + 120);
    let mut anchor = None;
    let mut last_sample = 0;
    let mut max_drift = 0.0f64;
    let mut max_render_latency = Duration::ZERO;
    let mut video_demands = 0;
    while playback.playing() {
        assert!(Instant::now() < deadline, "playback timeout");
        if let Some(error) = playback.take_error() {
            panic!("{error}");
        }
        if let Some(sample) = playback.sample() {
            assert!(
                sample >= last_sample,
                "clock moved backwards: {sample} < {last_sample}"
            );
            last_sample = sample;
            if sample > 4800 && sample < u64::from(seconds) * u64::from(AUDIO_RATE) - 4800 {
                let (first, time) = *anchor.get_or_insert((sample, Instant::now()));
                let drift = ((sample - first) as f64 / f64::from(AUDIO_RATE)
                    - time.elapsed().as_secs_f64())
                .abs();
                max_drift = max_drift.max(drift);
            }
            let frame = Time::new(sample as i64, AUDIO_RATE)
                .unwrap()
                .to_ticks(info.rate[0], info.rate[1], Rounding::Floor)
                .unwrap() as u32;
            if frame < info.frames && frame != key.frame {
                key.frame = frame;
                let before = Instant::now();
                media_workflow::evaluate(&snapshot, &key, &mut decoder, &Cancel::default())
                    .unwrap();
                max_render_latency = max_render_latency.max(before.elapsed());
                video_demands += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(playback.take_error(), None);
    assert_eq!(
        playback.sample(),
        Some(u64::from(seconds) * u64::from(AUDIO_RATE))
    );
    println!(
        "native duration={seconds}s output=30fps source_rates=30,24 PCM=48000 stereo; demands={video_demands}; max clock-vs-monotonic drift={max_drift:.6}s; max video decode={max_render_latency:?}; underrun callbacks={}",
        playback.underruns()
    );
    assert_eq!(playback.underruns(), 0);
    assert!(
        max_drift < 1.0 / 30.0,
        "accumulating device-clock drift exceeds one output frame"
    );
    // A seek replaces a live stream, with a fresh primed ring and clock origin.
    playback.play(snapshot.clone(), source.document, 0, info.frames);
    playback.play(
        snapshot.clone(),
        source.document,
        info.frames - 30,
        info.frames,
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(Instant::now() < deadline);
        if let Some(error) = playback.take_error() {
            panic!("seek: {error}");
        }
        if let Some(sample) = playback.sample() {
            assert!(sample >= u64::from(seconds - 1) * u64::from(AUDIO_RATE));
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    playback.pause();
    assert!(!playback.playing());
    assert!(playback.sample().is_none());
    // A marked review end must clip longer audio regions without invalidating
    // their original source mapping or leaving the device clock at full length.
    playback.play(snapshot.clone(), source.document, 5, 8);
    let deadline = Instant::now() + Duration::from_secs(15);
    while playback.playing() {
        assert!(Instant::now() < deadline, "short review playback timeout");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(playback.take_error(), None);
    let short_end = info
        .time(8)
        .unwrap()
        .to_ticks(AUDIO_RATE, 1, Rounding::Ceil)
        .unwrap() as u64;
    assert_eq!(playback.sample(), Some(short_end));
    playback.pause();
    // Loop a short marked range without resetting the device clock or leaving
    // priming gaps. Multiple wraps must still expose a monotonic absolute clock.
    playback.play_looping(snapshot.clone(), source.document, 5, 5, 8);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = 0;
    let mut running = false;
    loop {
        assert!(Instant::now() < deadline, "loop clock timeout");
        assert_eq!(playback.take_error(), None);
        if let Some(sample) = playback.sample() {
            assert!(sample >= last);
            last = sample;
            running = true;
            if sample >= short_end + u64::from(AUDIO_RATE) {
                break;
            }
        } else {
            assert!(!running, "loop must not re-prime the device stream");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(playback.underruns(), 0);
    playback.pause();
    drop(playback);

    // Exercise the application monitor service, not just the device wrapper.
    use fold_platform::{
        desktop::{DesktopClient, DesktopCommand as C, ViewerTransport},
        workspace::PanelInstanceId,
    };
    let mut session = fold_app::session::Session::new(project);
    let a = PanelInstanceId(1);
    let b = PanelInstanceId(2);
    let intent = |time| ViewerTransport {
        mode: Default::default(),
        output: source.clone(),
        time,
        range: Default::default(),
        looping: true,
        playing: true,
    };
    session.command(C::ViewerTransport {
        viewer: a,
        transport: intent(Time::ZERO),
    });
    session.command(C::ViewerTransport {
        viewer: b,
        transport: intent(Time::new(1, 1).unwrap()),
    });
    session.command(C::MonitorViewer(Some(a)));
    for id in [a, b] {
        let before_a = session.viewer_transport(a).unwrap();
        let before_b = session.viewer_transport(b).unwrap();
        session.command(C::MonitorViewer(Some(id)));
        assert_eq!(session.viewer_transport(a).unwrap(), before_a);
        assert_eq!(session.viewer_transport(b).unwrap(), before_b);
        let initial = session.viewer_transport(id).unwrap().time;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            session.poll();
            assert!(
                session.viewer_audio_error(id).is_none(),
                "{:?}",
                session.viewer_audio_error(id)
            );
            if session.viewer_transport(id).unwrap().time > initial {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "monitor did not advance after handover"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(session.viewer_transport(a).unwrap().playing);
        assert!(session.viewer_transport(b).unwrap().playing);
    }
    // Switching the monitored transport to Every-frame retires the audio clock;
    // readiness, not elapsed/device time, now controls this viewer alone.
    let mut every = session.viewer_transport(b).unwrap();
    every.mode = fold_platform::desktop::PlaybackMode::EveryFrame;
    session.command(C::ViewerTransport {
        viewer: b,
        transport: every.clone(),
    });
    let (generation, requested) = session.viewer_request(b).unwrap();
    let state = session.preview_state(&source, requested);
    let image = state.preview_key(state.frame, 4).unwrap();
    assert!(session.present_viewer(b, generation, &image));
    let held_time = session.viewer_transport(b).unwrap().time;
    let until = Instant::now() + Duration::from_millis(150);
    while Instant::now() < until {
        session.poll();
        assert_eq!(session.viewer_transport(b).unwrap().time, held_time);
        assert!(session.viewer_audio_error(b).is_none());
        std::thread::sleep(Duration::from_millis(2));
    }
    every.time = held_time;
    every.mode = fold_platform::desktop::PlaybackMode::RealTime;
    session.command(C::ViewerTransport {
        viewer: b,
        transport: every,
    });
    assert!(!session.present_viewer(b, generation, &image));
    let deadline = Instant::now() + Duration::from_secs(15);
    while session.viewer_transport(b).unwrap().time == held_time {
        assert!(
            Instant::now() < deadline,
            "Real-time audio did not resume after mode switch"
        );
        session.poll();
        assert!(session.viewer_audio_error(b).is_none());
        std::thread::sleep(Duration::from_millis(2));
    }
    let outgoing = session.viewer_transport(a).unwrap();
    let closed = Instant::now();
    session.command(C::CloseViewer(b));
    assert!(
        closed.elapsed() < Duration::from_millis(100),
        "close must not join device workers on UI"
    );
    assert!(session.viewer_transport(b).is_none());
    assert_eq!(session.viewer_transport(a).unwrap(), outgoing);
    let teardown = Instant::now();
    drop(session);
    assert!(
        teardown.elapsed() < Duration::from_secs(2),
        "bounded worker teardown"
    );
}
