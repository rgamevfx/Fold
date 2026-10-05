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
