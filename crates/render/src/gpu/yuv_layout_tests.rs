use super::*;
use crate::gpu::{GpuFrame, validation::Status};
#[test]
#[ignore = "requires native Vulkan"]
fn padded_nv12_is_equivalent_to_planar_and_rejects_out_of_bounds_layout() {
    let cancel = fold_media::Cancel::default();
    let bytes: Vec<u8> = (0..36).map(|i| (16 + i * 6) as u8).collect();
    let frame = fold_media::YuvFrame::from_420(
        [6, 4],
        bytes,
        fold_foundation::Time::ZERO,
        fold_foundation::Time::new(1, 30).unwrap(),
    )
    .unwrap();
    let mut nv12 = vec![0; 48];
    for y in 0..4 {
        nv12[y * 8..y * 8 + 6].copy_from_slice(&frame.bytes()[y * 6..y * 6 + 6]);
    }
    for y in 0..2 {
        for x in 0..3 {
            nv12[32 + y * 8 + x * 2] = frame.bytes()[24 + y * 3 + x];
            nv12[33 + y * 8 + x * 2] = frame.bytes()[30 + y * 3 + x];
        }
    }
    let (host, _) = pollster::block_on(Host::headless(2 * 1024 * 1024)).unwrap();
    let mut pipeline = YuvPipeline::new(&host);
    let buffer = host
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &nv12,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let layout = Layout {
        dimensions: [6, 4],
        luma_stride: 8,
        chroma_stride: 8,
        chroma_offsets: [32, 33],
        chroma_step: 2,
        format: Default::default(),
        sample_shift: 0,
    };
    assert!(
        Layout {
            chroma_offsets: [u32::MAX, 33],
            ..layout
        }
        .parameters(48)
        .is_err()
    );
    assert!(
        Layout {
            chroma_stride: 4,
            ..layout
        }
        .parameters(48)
        .is_err()
    );
    for dimensions in [[6, 4], [5, 3], [12, 8]] {
        let reference = frame.reconstruct_signal(dimensions, &cancel).unwrap();
        let output = host.image(dimensions).unwrap();
        let status = Status::new(&host).unwrap();
        let mut encoder = host.device().create_command_encoder(&Default::default());
        status.start(&mut encoder);
        let mut reservations = vec![];
        pipeline
            .encode_external(
                &host,
                &mut encoder,
                &buffer,
                layout,
                &output,
                &mut reservations,
            )
            .unwrap();
        status.encode(&mut encoder);
        host.submit(encoder, vec![output.clone()], reservations);
        let mut gpu = GpuFrame {
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
        let actual = gpu.readback(&cancel).unwrap();
        let error = actual
            .pixels()
            .iter()
            .zip(reference.encoded_pixels())
            .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
            .fold(0f32, f32::max);
        assert!(error < 1e-5, "{dimensions:?}: {error}");
    }
    assert_eq!(pipeline.external_layouts.len(), 1);
}

#[test]
#[ignore = "requires Vulkan"]
fn p010_preserves_low_ten_bit_samples() {
    let format = fold_media::VideoFormat {
        depth: 10,
        pixel_format: "yuv420p10le",
        ..Default::default()
    };
    let samples = [64u16, 513, 901, 940, 81, 102, 1023, 643, 512, 257, 401, 903];
    let frame = fold_media::YuvFrame::from_planes(
        format,
        [4, 2],
        samples.iter().flat_map(|v| v.to_le_bytes()).collect(),
        fold_foundation::Time::ZERO,
        fold_foundation::Time::new(1, 24).unwrap(),
    )
    .unwrap();
    let mut p010: Vec<u8> = samples[..8]
        .iter()
        .flat_map(|v| (v << 6).to_le_bytes())
        .collect();
    for i in 0..2 {
        for v in [samples[8 + i], samples[10 + i]] {
            p010.extend((v << 6).to_le_bytes());
        }
    }
    let (host, _) = pollster::block_on(Host::headless(2 * 1024 * 1024)).unwrap();
    let mut pipeline = YuvPipeline::new(&host);
    let buffer = host
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &p010,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let output = host.image([4, 2]).unwrap();
    let status = Status::new(&host).unwrap();
    let mut encoder = host.device().create_command_encoder(&Default::default());
    status.start(&mut encoder);
    let mut reservations = vec![];
    pipeline
        .encode_external(
            &host,
            &mut encoder,
            &buffer,
            Layout {
                dimensions: [4, 2],
                luma_stride: 8,
                chroma_stride: 8,
                chroma_offsets: [16, 18],
                chroma_step: 4,
                format,
                sample_shift: 6,
            },
            &output,
            &mut reservations,
        )
        .unwrap();
    status.encode(&mut encoder);
    host.submit(encoder, vec![output.clone()], reservations);
    let mut gpu = GpuFrame {
        dependencies: vec![],
        image: output,
        host,
        ready: status.submitted(),
        working_space: crate::WorkingSpace::LegacyLinearSrgb,
        config_identity: None,
        timing: None,
        lease_id: 1,
        statistics: Default::default(),
    };
    let cancel = fold_media::Cancel::default();
    let actual = gpu.readback(&cancel).unwrap();
    let reference = frame.reconstruct_signal([4, 2], &cancel).unwrap();
    let error = actual
        .pixels()
        .iter()
        .zip(reference.encoded_pixels())
        .flat_map(|(a, b)| (0..4).map(move |c| (a[c] - b[c]).abs()))
        .fold(0f32, f32::max);
    assert!(error < 1e-5, "P010 error: {error}");
}
