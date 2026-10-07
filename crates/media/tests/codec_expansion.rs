use fold_media::{Cancel, Decoder};
use std::{path::Path, process::Command};
fn ffmpeg(args: &[&str], path: &Path) {
    let result = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-threads",
            "1",
            "-filter_threads",
            "1",
        ])
        .args(args)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
fn video_profiles_preserve_native_samples_and_exact_seeks() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = Cancel::default();
    for (codec, profile, pix, ext) in [
        ("libx265", "main", "yuv420p", "mp4"),
        ("libx265", "main10", "yuv420p10le", "mp4"),
        ("prores_ks", "0", "yuv422p10le", "mov"),
        ("prores_ks", "1", "yuv422p10le", "mov"),
        ("prores_ks", "2", "yuv422p10le", "mov"),
        ("prores_ks", "3", "yuv422p10le", "mov"),
        ("prores_ks", "4", "yuva444p10le", "mov"),
        ("prores_ks", "5", "yuva444p10le", "mov"),
        ("dnxhd", "dnxhr_lb", "yuv422p", "mov"),
        ("dnxhd", "dnxhr_sq", "yuv422p", "mov"),
        ("dnxhd", "dnxhr_hq", "yuv422p", "mov"),
        ("dnxhd", "dnxhr_hqx", "yuv422p10le", "mov"),
        ("dnxhd", "dnxhr_444", "yuv444p10le", "mov"),
        ("dnxhd", "dnxhr_444", "gbrp10le", "mov"),
    ] {
        let path = dir.path().join(format!("{codec}-{profile}-{pix}.{ext}"));
        let mut args = vec![
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=256x128:rate=24",
            "-vf",
            "format=yuva444p10le,geq=lum='p(X,Y)':cb='p(X,Y)':cr='p(X,Y)':a='X/W*1023'",
            "-frames:v",
            "4",
            "-c:v",
            codec,
            "-profile:v",
            profile,
            "-pix_fmt",
            pix,
            "-threads",
            "1",
            "-color_range",
            "tv",
            "-colorspace",
            "bt709",
            "-color_trc",
            "bt709",
            "-color_primaries",
            "bt709",
        ];
        if pix.starts_with("gbr") {
            args.extend(["-color_range", "pc"]);
        }
        if codec == "libx265" {
            args.extend(["-x265-params", "pools=none:frame-threads=1:log-level=error"]);
        }
        ffmpeg(&args, &path);
        let linked = fold_media::ingest::inspect_linked(&path, &cancel)
            .unwrap_or_else(|e| panic!("{profile}: {e}"));
        assert!(matches!(
            linked.metadata.profile,
            fold_media::ingest::SourceProfile::Video(_)
        ));
        let source = fold_media::inspect(&path, &cancel).unwrap();
        if profile == "5" {
            let thumb = fold_media::thumbnail::thumbnail(
                &path,
                &source.fingerprint,
                &linked.metadata.profile,
                &cancel,
            )
            .unwrap();
            assert!(thumb[(36 * 128 + 4) * 4 + 3] < 16);
            assert!(thumb[(36 * 128 + 124) * 4 + 3] > 230);
        }
        let mut decoder = Decoder::default();
        for i in [0, 1, 3, 0, 2] {
            let native = decoder
                .decode_native(&source, source.info.time(i).unwrap(), &cancel)
                .unwrap();
            let out = Command::new("ffmpeg")
                .args(["-v", "error", "-threads", "1", "-i"])
                .arg(&path)
                .args([
                    "-vf",
                    &format!("select=eq(n\\,{i})"),
                    "-frames:v",
                    "1",
                    "-threads:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    native.format().pixel_format,
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(out.status.success());
            assert_eq!(native.bytes(), out.stdout, "{profile} frame {i}");
            let image = native.reconstruct_signal([256, 128], &cancel).unwrap();
            let pixels: Vec<_> = image.encoded_pixels().collect();
            if i == 0 {
                let oracle = decoder
                    .decode_signal_reference(
                        &source,
                        source.info.time(i).unwrap(),
                        [256, 128],
                        &cancel,
                    )
                    .unwrap();
                let error = pixels
                    .iter()
                    .zip(oracle.encoded_pixels())
                    .filter(|(a, _)| a[3] > 0.01)
                    .flat_map(|(a, b)| (0..3).map(move |c| (a[c] / a[3] - b[c]).abs()))
                    .fold(0f32, f32::max);
                assert!(
                    error < 0.0001,
                    "{profile} independent color/alpha oracle: {error}"
                );
            }
            if native.format().alpha {
                assert!(pixels.iter().any(|p| p[3] < 0.1), "{profile} lost alpha");
                assert!(pixels.iter().any(|p| p[3] > 0.9), "{profile} lost alpha");
            }
        }
        assert!(decoder.statistics().launches <= 3);
    }
}
#[test]
fn audio_formats_have_exact_pcm_reads_and_float_headroom() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = Cancel::default();
    for (codec, ext) in [
        ("pcm_s16le", "wav"),
        ("pcm_s24le", "wav"),
        ("pcm_s32le", "wav"),
        ("pcm_f32le", "wav"),
        ("libmp3lame", "mp3"),
        ("libmp3lame-vbr", "mp3"),
        ("aac", "m4a"),
        ("aac", "aac"),
        ("flac", "flac"),
    ] {
        let path = dir.path().join(format!("{codec}.{ext}"));
        let encoder = if codec == "libmp3lame-vbr" {
            "libmp3lame"
        } else {
            codec
        };
        let mut args = vec![
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=2.0*sin(2*PI*440*t):s=44100:d=0.3",
            "-c:a",
            encoder,
        ];
        if codec == "libmp3lame-vbr" {
            args.extend(["-q:a", "2"]);
        }
        ffmpeg(&args, &path);
        let source =
            fold_media::inspect_wave(&path, &cancel).unwrap_or_else(|e| panic!("{codec}: {e}"));
        if ext != "aac" {
            assert_eq!(source.info.frames, 13230, "gapless {codec}/{ext} duration");
        }
        let linked = fold_media::ingest::inspect_linked(&path, &cancel).unwrap();
        assert!(matches!(
            linked.metadata.profile,
            fold_media::ingest::SourceProfile::Wave(_)
        ));
        let reference = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&path)
            .args([
                "-af",
                "aresample=48000:async=1:first_pts=0",
                "-ac",
                "2",
                "-ar",
                "48000",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(reference.status.success());
        let mut decoder = fold_media::audio::AudioDecoder::default();
        let source = fold_media::audio::AudioSource::Wave(source);
        for start in [0, 8000, 4000] {
            let actual = decoder.read(&source, start, 1024, &cancel).unwrap();
            for (i, p) in actual.iter().enumerate() {
                for ch in 0..2 {
                    let offset = ((start as usize + i) * 2 + ch) * 4;
                    let expected = f32::from_le_bytes(
                        reference.stdout[offset..offset + 4].try_into().unwrap(),
                    );
                    assert_eq!(p[ch], expected, "{codec} sample {i}");
                }
            }
            if codec == "pcm_f32le" {
                assert!(actual.iter().any(|p| p[0].abs() > 1.));
            }
        }
        if ext != "aac" {
            let end = decoder.read(&source, 14400, 32, &cancel).unwrap();
            assert!(
                end.iter().all(|p| *p == [0., 0.]),
                "padding leaked into {codec}/{ext}"
            );
        }
    }
}

#[test]
fn dnxhd_and_mxf_are_importable() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = Cancel::default();
    for ext in ["mov", "mxf"] {
        let path = dir.path().join(format!("dnxhd.{ext}"));
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1920x1080:rate=24",
                "-frames:v",
                "2",
                "-c:v",
                "dnxhd",
                "-b:v",
                "115M",
                "-pix_fmt",
                "yuv422p",
                "-threads",
                "1",
            ],
            &path,
        );
        let source = fold_media::inspect(&path, &cancel).unwrap();
        let linked = fold_media::ingest::inspect_linked(&path, &cancel).unwrap();
        let frame = Decoder::default()
            .decode_native(&source, source.info.time(1).unwrap(), &cancel)
            .unwrap();
        assert_eq!(frame.format().subsampling, [2, 1]);
        let thumb = fold_media::thumbnail::thumbnail(
            &path,
            &source.fingerprint,
            &linked.metadata.profile,
            &cancel,
        )
        .unwrap();
        assert_eq!(thumb.len(), 128 * 72 * 4);
    }
}

#[test]
fn incremental_audio_retains_blocks_across_cursor_eviction_and_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = Cancel::default();
    let mut sources = vec![];
    for frequency in [220, 440, 880] {
        let path = dir.path().join(format!("{frequency}.flac"));
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency={frequency}:sample_rate=48000:duration=1"),
                "-c:a",
                "flac",
            ],
            &path,
        );
        sources.push(fold_media::audio::AudioSource::Wave(
            fold_media::inspect_wave(&path, &cancel).unwrap(),
        ));
    }
    let mut decoder = fold_media::audio::AudioDecoder::default();
    for source in &sources {
        decoder.preflight(source, &cancel).unwrap();
    }
    assert_eq!(
        decoder.prepared_usage(),
        (3, 3 * 4096 * 8),
        "preflight must not decode complete sources"
    );
    let resumed = decoder.read(&sources[0], 9000, 1024, &cancel).unwrap();
    let expected = fold_media::audio::AudioDecoder::default()
        .read(&sources[0], 9000, 1024, &cancel)
        .unwrap();
    assert_eq!(resumed, expected);
    let stopped = Cancel::default();
    stopped.cancel();
    assert!(decoder.read(&sources[0], 16000, 1024, &stopped).is_err());
    assert_eq!(
        decoder.read(&sources[0], 9000, 1024, &cancel).unwrap(),
        expected
    );
}
