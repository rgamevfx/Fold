use super::*;
use crate::gpu::{GpuFrame, validation::Status};
#[test]
#[ignore = "requires native Vulkan and FFmpeg"]
fn native_yuv_reconstruction_matches_float_reference() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("colors.mp4");
    let info = fold_media::VideoInfo {
        width: 64,
        height: 32,
        rate: [30, 1],
        frames: 2,
    };
    let cancel = fold_media::Cancel::default();
    let mut encoder = fold_media::Encoder::new(&path, &info, 2, cancel.clone()).unwrap();
    let rgb: Vec<_> = (0..32)
        .flat_map(|y| {
            (0..64).flat_map(move |x| [(x * 4) as u8, (y * 8) as u8, if x < 32 { 32 } else { 240 }])
        })
        .collect();
    encoder.write_rgb(&rgb).unwrap();
    encoder.write_rgb(&rgb).unwrap();
    encoder.finish().unwrap();
    let source = fold_media::inspect(&path, &cancel).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let native = decoder
        .decode_native(&source, fold_foundation::Time::ZERO, &cancel)
        .unwrap();
    assert_eq!(native.storage_bytes(), 64 * 32 * 3 / 2);
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut pipeline = YuvPipeline::new(&host);
    for dimensions in [[64, 32], [32, 16], [31, 17], [30, 14], [126, 66], [1, 1]] {
        let reference = decoder
            .decode_signal(&source, fold_foundation::Time::ZERO, dimensions, &cancel)
            .unwrap();
        if matches!(dimensions, [64, 32] | [32, 16]) {
            let oracle = decoder
                .decode_signal_reference(&source, fold_foundation::Time::ZERO, dimensions, &cancel)
                .unwrap();
            let error = reference
                .encoded_pixels()
                .zip(oracle.encoded_pixels())
                .flat_map(|(a, b)| (0..3).map(move |i| (a[i] - b[i]).abs()))
                .fold(0f32, f32::max);
            assert!(error < 0.00001, "independent FFmpeg oracle: {error}");
        }
        let output = host.image(dimensions).unwrap();
        let status = Status::new(&host).unwrap();
        let mut encoder = host.device().create_command_encoder(&Default::default());
        status.start(&mut encoder);
        let mut reservations = vec![];
        let bytes = pipeline
            .encode(&host, &mut encoder, &native, &output, &mut reservations)
            .unwrap();
        assert_eq!(
            bytes,
            if dimensions == [64, 32] { 3072 } else { 0 },
            "resize reuses immutable GPU input"
        );
        status.encode(&mut encoder);
        host.submit(encoder, vec![output.clone()], reservations);
        let mut frame = GpuFrame {
            image: output,
            host: host.clone(),
            ready: status.submitted(),
            working_space: crate::WorkingSpace::LegacyLinearSrgb,
            config_identity: None,
            timing: None,
            lease_id: 1,
            statistics: Default::default(),
        };
        let actual = frame.readback(&cancel).unwrap();
        let max = actual
            .pixels()
            .iter()
            .zip(reference.encoded_pixels())
            .flat_map(|(a, b)| (0..3).map(move |i| (a[i] - b[i]).abs()))
            .fold(0f32, f32::max);
        eprintln!("{dimensions:?} native YUV max error {max}");
        assert!(max < 0.00001, "{dimensions:?}: {max}");
    }
}
