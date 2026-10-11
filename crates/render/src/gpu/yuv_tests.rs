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
        let mut uploads = vec![];
        let bytes = pipeline
            .encode(
                &host,
                &mut encoder,
                &native,
                &output,
                &mut reservations,
                &mut uploads,
            )
            .unwrap();
        assert_eq!(
            bytes,
            if dimensions == [64, 32] { 3072 } else { 0 },
            "resize reuses immutable GPU input"
        );
        status.encode(&mut encoder);
        host.submit(encoder, vec![output.clone()], reservations);
        for upload in &uploads {
            upload.submitted();
        }
        drop(uploads);
        let mut frame = GpuFrame {
            dependencies: vec![],
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

#[test]
#[ignore = "requires Vulkan"]
fn high_precision_chroma_and_alpha_match_cpu() {
    let cancel = fold_media::Cancel::default();
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut pipeline = YuvPipeline::new(&host);
    for depth in [8, 10, 12] {
        for subsampling in [[2, 2], [2, 1], [1, 1]] {
            for alpha in [false, true] {
                for full_range in [false, true] {
                    for rgb in [false, true] {
                        if rgb && (subsampling != [1, 1] || !full_range) {
                            continue;
                        }
                        let format = fold_media::VideoFormat {
                            depth,
                            rgb,
                            subsampling,
                            alpha,
                            full_range,
                            nvdec_compatible: false,
                            ..Default::default()
                        };
                        let mut bytes = Vec::new();
                        let max = (1u32 << depth) - 1;
                        for i in 0..format.bytes([6, 4]) / u64::from(format.sample_bytes()) {
                            let sample = ((i * 71) as u32 % max) as u16;
                            if depth == 8 {
                                bytes.push(sample as u8);
                            } else {
                                bytes.extend(sample.to_le_bytes());
                            }
                        }
                        let native = fold_media::YuvFrame::from_planes(
                            format,
                            [6, 4],
                            bytes,
                            fold_foundation::Time::ZERO,
                            fold_foundation::Time::new(1, 24).unwrap(),
                        )
                        .unwrap();
                        for dimensions in [[6, 4], [3, 3], [12, 8]] {
                            let reference = native.reconstruct_signal(dimensions, &cancel).unwrap();
                            let output = host.image(dimensions).unwrap();
                            let status = Status::new(&host).unwrap();
                            let mut encoder =
                                host.device().create_command_encoder(&Default::default());
                            status.start(&mut encoder);
                            let mut reservations = vec![];
                            let mut uploads = vec![];
                            pipeline
                                .encode(
                                    &host,
                                    &mut encoder,
                                    &native,
                                    &output,
                                    &mut reservations,
                                    &mut uploads,
                                )
                                .unwrap();
                            status.encode(&mut encoder);
                            host.submit(encoder, vec![output.clone()], reservations);
                            for upload in &uploads {
                                upload.submitted();
                            }
                            drop(uploads);
                            let mut frame = GpuFrame {
                                dependencies: vec![],
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
                            let error = actual
                                .pixels()
                                .iter()
                                .zip(reference.encoded_pixels())
                                .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
                                .fold(0f32, f32::max);
                            assert!(
                                error < 1e-5,
                                "depth={depth} sub={subsampling:?} alpha={alpha} full={full_range} size={dimensions:?}: {error}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native Vulkan"]
fn plane_recycling_preserves_unsubmitted_consumers() {
    let (host, _) = pollster::block_on(Host::headless(512 * 1024)).unwrap();
    let mut pipeline = YuvPipeline::new(&host);
    let cancel = fold_media::Cancel::default();
    let mut pending = Vec::new();
    let format = fold_media::VideoFormat {
        alpha: true,
        rgb: true,
        full_range: true,
        subsampling: [1, 1],
        ..Default::default()
    };
    // Each plane set fits the cache; two sets exceed its allowance. Keep the
    // first commands unsubmitted while newer frames evict its source storage.
    for value in [32u8, 96, 192] {
        let native = fold_media::YuvFrame::from_planes(
            format,
            [128, 64],
            vec![value; format.bytes([128, 64]) as usize],
            fold_foundation::Time::ZERO,
            fold_foundation::Time::new(1, 24).unwrap(),
        )
        .unwrap();
        let expected = native.reconstruct_signal([1, 1], &cancel).unwrap();
        let output = host.image([1, 1]).unwrap();
        let status = Status::new(&host).unwrap();
        let mut encoder = host.device().create_command_encoder(&Default::default());
        status.start(&mut encoder);
        let mut reservations = vec![];
        let mut uploads = vec![];
        pipeline
            .encode(
                &host,
                &mut encoder,
                &native,
                &output,
                &mut reservations,
                &mut uploads,
            )
            .unwrap();
        status.encode(&mut encoder);
        pending.push((encoder, output, status, reservations, uploads, expected));
    }
    for (encoder, output, status, reservations, uploads, expected) in pending {
        host.submit(encoder, vec![output.clone()], reservations);
        for upload in &uploads {
            upload.submitted();
        }
        drop(uploads);
        let mut frame = GpuFrame {
            dependencies: vec![],
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
        assert!(
            actual
                .pixels()
                .iter()
                .zip(expected.encoded_pixels())
                .all(|(a, b)| a.iter().zip(b).all(|(a, b)| (*a - b).abs() < 1e-5))
        );
    }
    let retired = Arc::downgrade(&pipeline.cache.back().unwrap().reservation);
    let native = fold_media::YuvFrame::from_planes(
        format,
        [128, 64],
        vec![64; format.bytes([128, 64]) as usize],
        fold_foundation::Time::ZERO,
        fold_foundation::Time::new(1, 24).unwrap(),
    )
    .unwrap();
    let output = host.image([1, 1]).unwrap();
    let mut encoder = host.device().create_command_encoder(&Default::default());
    let mut reservations = vec![];
    let mut uploads = vec![];
    pipeline
        .encode(
            &host,
            &mut encoder,
            &native,
            &output,
            &mut reservations,
            &mut uploads,
        )
        .unwrap();
    assert!(
        std::sync::Weak::ptr_eq(
            &retired,
            &Arc::downgrade(&pipeline.cache.back().unwrap().reservation)
        ),
        "completed storage should be recycled"
    );
    drop(encoder);
    drop(reservations);
    drop(uploads);
    pipeline.clear();
}

#[test]
#[ignore = "requires native Vulkan"]
fn abandoned_plane_upload_is_not_a_cache_hit() {
    let (host, _) = pollster::block_on(Host::headless(512 * 1024)).unwrap();
    let mut pipeline = YuvPipeline::new(&host);
    let format = fold_media::VideoFormat::default();
    let native = fold_media::YuvFrame::from_planes(
        format,
        [16, 16],
        vec![96; format.bytes([16, 16]) as usize],
        fold_foundation::Time::ZERO,
        fold_foundation::Time::new(1, 24).unwrap(),
    )
    .unwrap();
    let output = host.image([16, 16]).unwrap();
    for _ in 0..2 {
        let mut encoder = host.device().create_command_encoder(&Default::default());
        let mut reservations = vec![];
        let mut uploads = vec![];
        let bytes = pipeline
            .encode(
                &host,
                &mut encoder,
                &native,
                &output,
                &mut reservations,
                &mut uploads,
            )
            .unwrap();
        assert_eq!(
            bytes,
            native.storage_bytes(),
            "abandoned commands did not initialize the cache"
        );
        drop(encoder);
        drop(reservations);
        drop(uploads);
        host.poll().unwrap();
    }
}
