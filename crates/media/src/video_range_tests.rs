use super::*;

#[test]
fn opening_a_range_releases_an_idle_pin_before_a_still_active_source() {
    let directory = tempfile::tempdir().unwrap();
    let info = VideoInfo {
        width: 4,
        height: 4,
        rate: [30, 1],
        frames: 2,
    };
    let sources: Vec<_> = [64, 128, 192]
        .into_iter()
        .map(|value| {
            let path = directory.path().join(format!("{value}.mp4"));
            let mut encoder = crate::Encoder::new(&path, &info, 2, Cancel::default()).unwrap();
            for _ in 0..2 {
                encoder.write_rgb(&[value; 48]).unwrap();
            }
            encoder.finish().unwrap();
            inspect(&path, &Cancel::default()).unwrap()
        })
        .collect();
    let mut decoder = Decoder::default();
    decoder
        .decode_signal(&sources[0], Time::ZERO, [4, 4], &Cancel::default())
        .unwrap();
    let active = decoder.pinned.back().unwrap().file.path().to_owned();
    decoder
        .decode_signal(
            &sources[1],
            info.time(1).unwrap(),
            [4, 4],
            &Cancel::default(),
        )
        .unwrap();
    let idle = decoder.pinned.back().unwrap().file.path().to_owned();
    assert!(active.exists() && idle.exists());
    decoder
        .decode_signal(&sources[2], Time::ZERO, [4, 4], &Cancel::default())
        .unwrap();
    assert!(active.exists());
    assert!(
        !idle.exists(),
        "an ended range must not keep an unnecessary whole-file copy"
    );
    decoder
        .decode_signal(
            &sources[0],
            info.time(1).unwrap(),
            [4, 4],
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(decoder.statistics().launches, 3);
}

#[test]
fn missing_packet_durations_require_an_exact_timestamp_and_stream_end_proof() {
    let info = VideoInfo {
        width: 1920,
        height: 1080,
        rate: [25, 1],
        frames: 3,
    };
    let frames = vec![
        serde_json::json!({"best_effort_timestamp":0}),
        serde_json::json!({"best_effort_timestamp":3600}),
        serde_json::json!({"best_effort_timestamp":7200}),
    ];
    let stream = serde_json::json!({"start_pts":0,"duration_ts":10800});
    assert!(super::validate_timing(&info, [1, 90000], &frames, &stream).is_ok());
    assert!(super::validate_timing(&info, [1, 90000], &frames, &serde_json::json!({})).is_err());
    assert!(
        super::validate_timing(
            &info,
            [1, 90000],
            &frames,
            &serde_json::json!({"start_pts":0,"duration_ts":10801})
        )
        .is_err()
    );
    let mut vfr = frames.clone();
    vfr[1]["best_effort_timestamp"] = 3601.into();
    assert!(super::validate_timing(&info, [1, 90000], &vfr, &stream).is_err());
    let mut wrong_duration = frames;
    wrong_duration[0]["pkt_duration"] = 1.into();
    assert!(super::validate_timing(&info, [1, 90000], &wrong_duration, &stream).is_err());
}
