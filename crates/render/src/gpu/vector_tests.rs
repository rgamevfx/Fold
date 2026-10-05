use super::*;
use crate::{
    gpu::GpuFrame,
    vector::{self, Cap, Join, Segment, Stroke},
};

#[test]
#[ignore = "requires native Vulkan"]
fn native_float_coverage_matches_reference_without_full_image_upload() {
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut pipeline = VectorPipeline::new(&host);
    let cancel = fold_media::Cancel::default();
    let path = Arc::new(vec![
        Segment::Move([2., 3.]),
        Segment::Cubic([60., 1.], [1., 45.], [58., 43.]),
        Segment::Line([5., 40.]),
        Segment::Close,
    ]);
    for dimensions in [[64, 48], [17, 11]] {
        for cap in [Cap::Butt, Cap::Round, Cap::Square] {
            let drawings = vec![
                Drawing {
                    path: path.clone(),
                    transform: [1., 0.1, -0.2, 0.8, 0.25, 0.75],
                    fill: Some([2.5, -0.2, 0.7, 0.4]),
                    even_odd: true,
                    stroke: Some(Stroke {
                        color: [0.2, 1.5, 0.6, 0.6],
                        width: 2.3,
                        cap,
                        join: Join::Miter,
                    }),
                },
                Drawing {
                    path: path.clone(),
                    transform: [-0.5, 0., 0., 0.5, 30., 10.],
                    fill: Some([0.7, 0.3, 0.1, 0.8]),
                    even_odd: false,
                    stroke: None,
                },
            ];
            let expected =
                vector::rasterize(&drawings, dimensions[0], dimensions[1], &cancel, true).unwrap();
            let output = host.image(dimensions).unwrap();
            let status = Status::new(&host).unwrap();
            let mut encoder = host.device().create_command_encoder(&Default::default());
            status.start(&mut encoder);
            let mut reservations = vec![];
            let (bytes, _) = pipeline
                .encode(
                    &mut encoder,
                    &drawings,
                    &output,
                    &status,
                    &mut reservations,
                    &cancel,
                )
                .unwrap();
            // Geometric payload can exceed RGBA for tiny targets; it is not a
            // raster image. Repeated evaluation must not upload it again.
            assert!(bytes < 64 * 1024);
            assert_eq!(
                pipeline.packet(&drawings, dimensions, &cancel).unwrap().1,
                0
            );
            status.encode(&mut encoder);
            host.submit(encoder, vec![output.clone()], reservations);
            let mut frame = GpuFrame {
                dependencies: vec![],
                image: output,
                host: host.clone(),
                ready: status.submitted(),
                working_space: crate::WorkingSpace::AcesCg,
                config_identity: None,
                timing: None,
                lease_id: 1,
                statistics: Default::default(),
            };
            let actual = frame.readback(&cancel).unwrap();
            let max = actual
                .pixels()
                .iter()
                .flatten()
                .zip(expected.iter().flatten())
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            // Analytic f32 intersections versus the f64 area oracle; this is
            // independent of the former identical 8-bit CPU coverage masks.
            assert!(max < 0.0001, "{dimensions:?}/{cap:?}: {max}");
        }
    }
}
