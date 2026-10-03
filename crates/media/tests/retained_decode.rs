use fold_media::{Cancel, DecodeBackend, Decoder, Encoder, VideoInfo};

fn fixture(path: &std::path::Path) -> fold_media::VideoSource {
    let info = VideoInfo {
        width: 64,
        height: 32,
        rate: [24000, 1001],
        frames: 80,
    };
    let mut encoder = Encoder::new(path, &info, info.frames, Cancel::default()).unwrap();
    for i in 0..info.frames {
        encoder
            .write_rgb(&vec![(i * 3) as u8; 64 * 32 * 3])
            .unwrap();
    }
    encoder.finish().unwrap();
    fold_media::inspect(path, &Cancel::default()).unwrap()
}

fn exercise(backend: DecodeBackend) {
    let directory = tempfile::tempdir().unwrap();
    let source = fixture(&directory.path().join("source.mp4"));
    let mut decoder = Decoder::with_backend(backend);
    let mut reference = Decoder::default();
    // Independent consumers can retain different ranges of the same source.
    let mut ranges = Decoder::with_backend(backend);
    for frame in [0, 60, 1, 61] {
        ranges
            .decode_signal(
                &source,
                source.info.time(frame).unwrap(),
                [64, 32],
                &Cancel::default(),
            )
            .unwrap();
    }
    assert_eq!(ranges.statistics().launches, 2);
    assert_eq!(ranges.statistics().decoded_frames, 4);
    drop(ranges);
    let first_token = Cancel::default();
    for frame in 0..80 {
        let cancel = if frame == 0 {
            first_token.clone()
        } else {
            Cancel::default()
        };
        let image = decoder
            .decode_signal(&source, source.info.time(frame).unwrap(), [64, 32], &cancel)
            .unwrap();
        let expected = reference
            .decode_signal(&source, source.info.time(frame).unwrap(), [64, 32], &cancel)
            .unwrap();
        assert_eq!(
            image.encoded_pixels().collect::<Vec<_>>(),
            expected.encoded_pixels().collect::<Vec<_>>()
        );
        // Cancelling a completed demand must not kill a retained stream used by
        // subsequent independent demands.
        first_token.cancel();
    }
    assert_eq!(decoder.statistics().launches, 1);
    assert_eq!(decoder.statistics().decoded_frames, 80);
    assert_eq!(decoder.statistics().pipe_bytes, 80 * 64 * 32 * 3 / 2);
    assert!(decoder.retained_bytes() <= 64 * 1024 * 1024);
    for frame in [79, 0, 49, 23, 24, 48, 1] {
        decoder
            .decode_signal(
                &source,
                source.info.time(frame).unwrap(),
                [64, 32],
                &Cancel::default(),
            )
            .unwrap();
    }
    assert_eq!(
        decoder.statistics().launches,
        1,
        "reverse cache hits do not reopen a codec"
    );
    assert_eq!(decoder.statistics().cache_hits, 7);
    let odd = decoder
        .decode_signal(
            &source,
            source.info.time(3).unwrap(),
            [31, 17],
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(odd.dimensions(), [31, 17]);
    // Different preview sizes reuse the same native samples, not another decode.
    for frame in [79, 0, 49, 23, 24, 48, 1] {
        decoder
            .decode_signal(
                &source,
                source.info.time(frame).unwrap(),
                [32, 16],
                &Cancel::default(),
            )
            .unwrap();
    }
    assert_eq!(
        decoder.statistics().launches,
        1,
        "resize must not reopen a native decoder"
    );
    let cancel = Cancel::default();
    cancel.cancel();
    assert!(
        decoder
            .decode_signal(&source, source.info.time(0).unwrap(), [64, 32], &cancel)
            .unwrap_err()
            .contains("cancel")
    );
    std::fs::remove_file(&source.path).unwrap();
    assert!(
        Decoder::default()
            .decode(
                &source,
                source.info.time(0).unwrap(),
                [64, 32],
                &Cancel::default()
            )
            .is_err()
    );
    // Pinned sources survive unlink, including uncached requests.
    decoder
        .decode_signal(
            &source,
            source.info.time(50).unwrap(),
            [16, 16],
            &Cancel::default(),
        )
        .unwrap();
}
#[test]
fn unsupported_nvdec_geometry_fails_before_launch() {
    let source = fold_media::VideoSource {
        path: "not-opened.mp4".into(),
        fingerprint: "not-read".into(),
        info: VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 1,
        },
    };
    let mut decoder = Decoder::with_backend(DecodeBackend::Cuda);
    let error = decoder
        .decode_signal(
            &source,
            fold_foundation::Time::ZERO,
            [16, 16],
            &Cancel::default(),
        )
        .unwrap_err();
    assert!(error.contains("select the software decoder"), "{error}");
    assert_eq!(decoder.statistics().launches, 0);
}

#[test]
fn timestamp_gaps_and_forged_metadata_are_not_treated_as_cfr() {
    let directory = tempfile::tempdir().unwrap();
    let source = fixture(&directory.path().join("source.mp4"));
    let mut forged = source.clone();
    forged.info.frames -= 1;
    let error = Decoder::default()
        .decode_signal(
            &forged,
            fold_foundation::Time::ZERO,
            [64, 32],
            &Cancel::default(),
        )
        .unwrap_err();
    assert!(error.contains("metadata mismatch"), "{error}");
    let vfr = directory.path().join("gap.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&source.path)
        .args([
            "-vf",
            "setpts=PTS+gte(N\\,40)*1001/(24000*TB)",
            "-fps_mode",
            "passthrough",
            "-c:v",
            "libx264",
            "-threads",
            "1",
        ])
        .arg(&vfr)
        .status()
        .unwrap();
    assert!(status.success());
    let error = fold_media::inspect(&vfr, &Cancel::default()).unwrap_err();
    assert!(error.contains("constant-frame-rate"), "{error}");
}

#[test]
fn retained_software_ranges_and_request_lifetimes() {
    exercise(DecodeBackend::Software);
}
#[test]
#[ignore = "requires NVIDIA CUDA/NVDEC and FFmpeg CUDA support"]
fn retained_nvdec_matches_software() {
    exercise(DecodeBackend::Cuda);
}
