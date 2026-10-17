use fold_media::{Cancel, Encoder, VideoInfo};
use std::process::Command;
#[test]
fn prores4444_roundtrip_retains_fractional_alpha_timing_and_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("alpha.mov");
    let info = VideoInfo {
        width: 32,
        height: 16,
        rate: [24000, 1001],
        frames: 3,
    };
    let mut encoder = Encoder::new_prores4444(&path, &info, 3, Cancel::default(), false).unwrap();
    for alpha in [0u16, 24576, 65535] {
        let bytes: Vec<u8> = (0..512)
            .flat_map(|_| {
                [32000u16, 32000, 32000, alpha]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
            })
            .collect();
        encoder.write_rgba16(&bytes).unwrap();
    }
    encoder.finish().unwrap();
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(&path)
        .output()
        .unwrap();
    let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let stream = &probe["streams"][0];
    assert_eq!(stream["codec_name"], "prores");
    assert_eq!(stream["profile"], "4444");
    assert_eq!(stream["r_frame_rate"], "24000/1001");
    assert_eq!(stream["nb_frames"], "3");
    assert_eq!(stream["color_space"], "bt709");
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&path)
        .args([
            "-vf",
            "alphaextract,format=gray16le",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len(), 3 * 512 * 2);
    for (frame, expected) in [0i32, 24576, 65535].into_iter().enumerate() {
        for pixel in decoded.stdout[frame * 1024..(frame + 1) * 1024].chunks_exact(2) {
            let value = u16::from_le_bytes([pixel[0], pixel[1]]) as i32;
            assert!(
                (value - expected).abs() <= 128,
                "alpha {value}, expected {expected}"
            );
        }
    }
    let pcm = dir.path().join("audio.f32");
    std::fs::write(&pcm, vec![0u8; 6006 * 2 * 4]).unwrap();
    let with_audio = dir.path().join("audio.mov");
    fold_media::audio::mux_audio(&path, &pcm, &with_audio, &Cancel::default()).unwrap();
    assert!(fold_media::audio::has_audio(&with_audio, &Cancel::default()).unwrap());
    let mux_alpha = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&with_audio)
        .args([
            "-vf",
            "alphaextract,format=gray16le",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(mux_alpha.status.success());
    assert_eq!(mux_alpha.stdout, decoded.stdout);
    assert!(Encoder::new_prores4444(&path, &info, 3, Cancel::default(), false).is_err());
    let incomplete = dir.path().join("incomplete.mov");
    assert!(
        Encoder::new_prores4444(&incomplete, &info, 3, Cancel::default(), false)
            .unwrap()
            .finish()
            .is_err()
    );
    assert!(!incomplete.exists());
    let cancel = Cancel::default();
    let cancelled = dir.path().join("cancelled.mov");
    let mut encoder = Encoder::new_prores4444(&cancelled, &info, 3, cancel.clone(), false).unwrap();
    cancel.cancel();
    assert!(encoder.write_rgba16(&vec![0; 512 * 8]).is_err());
    drop(encoder);
    assert!(!cancelled.exists());
}

#[test]
fn all_movie_profiles_encode_the_selected_codec_and_alpha_mode() {
    use fold_media::encoding::{H264, H264Preset, Movie, ProRes, ProResProfile};
    let dir = tempfile::tempdir().unwrap();
    let info = VideoInfo {
        width: 32,
        height: 16,
        rate: [24000, 1001],
        frames: 2,
    };
    let variants = ProResProfile::ALL
        .into_iter()
        .map(|profile| {
            Movie::ProRes(ProRes {
                profile,
                alpha: true,
            })
        })
        .chain([
            Movie::ProRes(ProRes {
                alpha: false,
                ..Default::default()
            }),
            Movie::H264(H264 {
                crf: 23,
                preset: H264Preset::Fast,
            }),
        ]);
    for (i, encoding) in variants.enumerate() {
        let path = dir
            .path()
            .join(format!("profile-{i}.{}", encoding.extension()));
        let mut encoder =
            Encoder::new_with_signal(&path, &info, 2, Cancel::default(), false, encoding).unwrap();
        for _ in 0..2 {
            match encoding {
                Movie::ProRes(_) => {
                    let bytes: Vec<u8> = (0..512)
                        .flat_map(|_| {
                            [16000u16, 8000, 0, 32768]
                                .into_iter()
                                .flat_map(u16::to_le_bytes)
                        })
                        .collect();
                    encoder.write_rgba16(&bytes).unwrap();
                }
                Movie::H264(_) => encoder.write_rgb(&[128, 64, 0].repeat(512)).unwrap(),
            }
        }
        encoder.finish().unwrap();
        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-show_streams", "-of", "json"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(probe.status.success());
        let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let stream = &probe["streams"][0];
        assert_eq!(stream["nb_frames"], "2");
        assert_eq!(stream["r_frame_rate"], "24000/1001");
        match encoding {
            Movie::ProRes(settings) => {
                assert_eq!(stream["codec_name"], "prores");
                let profile = match settings.profile {
                    ProResProfile::Proxy => "Proxy",
                    ProResProfile::Lt => "LT",
                    ProResProfile::Standard => "Standard",
                    ProResProfile::Hq => "HQ",
                    ProResProfile::FourFourFourFour => "4444",
                    ProResProfile::Xq => "XQ",
                };
                assert_eq!(stream["profile"], profile);
                assert_eq!(
                    stream["pix_fmt"].as_str().unwrap().starts_with("yuva"),
                    encoding.preserves_alpha()
                );
            }
            Movie::H264(_) => {
                assert_eq!(stream["codec_name"], "h264");
                assert_eq!(stream["pix_fmt"], "yuv420p");
            }
        }
    }
}
