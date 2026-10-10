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
            let report = pipeline
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
            assert!(report.upload_bytes < 64 * 1024);
            assert_eq!(
                pipeline
                    .packet(&drawings, dimensions, &cancel)
                    .unwrap()
                    .1
                    .upload_bytes,
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

#[test]
#[ignore = "requires native Vulkan"]
fn unconfigured_vectors_stay_on_gpu_with_float_coverage() {
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut renderer = crate::gpu::Renderer::new(host).unwrap();
    let drawings = vec![Drawing {
        path: Arc::new(vec![
            Segment::Move([0.25, 0.25]),
            Segment::Line([20.75, 0.25]),
            Segment::Line([20.75, 20.75]),
            Segment::Line([0.25, 20.75]),
            Segment::Close,
        ]),
        transform: [1., 0., 0., 1., 0., 0.],
        fill: Some([0.12345, 0.54321, 0.8, 0.4321]),
        even_odd: false,
        stroke: None,
    }];
    let graph = crate::RenderGraph {
        width: 32,
        height: 32,
        nodes: vec![crate::ImageOp::Vector(Arc::new(drawings))],
        output: 0,
    };
    let cancel = fold_media::Cancel::default();
    let mut frame = renderer
        .evaluate(
            graph.clone(),
            None,
            &mut fold_media::Decoder::default(),
            &cancel,
        )
        .unwrap();
    assert_eq!(
        frame.statistics.cpu_adapter_nodes, 0,
        "vectors must not rasterize/upload CPU images"
    );
    assert!(frame.statistics.vector_upload_bytes > 0);
    let actual = frame.readback(&cancel).unwrap();
    let expected = crate::render(graph).unwrap();
    for (a, b) in actual
        .pixels()
        .iter()
        .flatten()
        .zip(expected.pixels().iter().flatten())
    {
        assert!((a - b).abs() < 0.0001, "{a} != {b}");
    }
    assert!((actual.pixels()[33][3] - 0.4321).abs() < 0.00001);
}

fn rectangle() -> Drawing {
    Drawing {
        path: Arc::new(vec![
            Segment::Move([0.2, 0.3]),
            Segment::Line([8.7, 0.3]),
            Segment::Line([8.7, 9.2]),
            Segment::Line([0.2, 9.2]),
            Segment::Close,
        ]),
        transform: [1., 0., 0., 1., 1., 1.],
        fill: Some([0.2, 0.4, 0.8, 0.3]),
        stroke: None,
        even_odd: false,
    }
}
fn render(
    pipeline: &mut VectorPipeline,
    host: &Host,
    drawings: &[Drawing],
    size: [u32; 2],
) -> (crate::Frame, Report) {
    let cancel = fold_media::Cancel::default();
    let output = host.image(size).unwrap();
    let status = Status::new(host).unwrap();
    let mut encoder = host.device().create_command_encoder(&Default::default());
    status.start(&mut encoder);
    let mut reservations = vec![];
    let report = pipeline
        .encode(
            &mut encoder,
            drawings,
            &output,
            &status,
            &mut reservations,
            &cancel,
        )
        .unwrap();
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
    (frame.readback(&cancel).unwrap(), report)
}
#[test]
#[ignore = "requires native Vulkan"]
fn native_instances_keep_order_across_tile_words_and_reuse_only_static_geometry() {
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut pipeline = VectorPipeline::new(&host);
    let mut drawings: Vec<_> = (0..97)
        .map(|i| {
            let mut d = rectangle();
            d.transform[4] = (i % 7) as f64 - 2.4;
            d.fill = Some([i as f64 / 96., 0.2, 0.8 - i as f64 / 120., 0.03]);
            d
        })
        .collect();
    let (first, report) = render(&mut pipeline, &host, &drawings, [16, 16]);
    assert_eq!(report.instances, 97);
    assert_eq!(report.unique_triangles, 2);
    assert!(report.geometry_upload_bytes > 0);
    let expected =
        vector::rasterize(&drawings, 16, 16, &fold_media::Cancel::default(), true).unwrap();
    let max = first
        .pixels()
        .iter()
        .flatten()
        .zip(expected.iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    assert!(max < 0.0001, "ordered alpha differs: {max}");
    for d in &mut drawings {
        d.transform[5] += 0.25;
        d.fill.as_mut().unwrap()[0] *= 0.5;
    }
    let (_, animated) = render(&mut pipeline, &host, &drawings, [16, 16]);
    assert_eq!(animated.geometry_upload_bytes, 0);
    assert!(animated.instance_upload_bytes > 0);
    assert_eq!(animated.upload_bytes, animated.instance_upload_bytes);
    let (_, cached) = render(&mut pipeline, &host, &drawings, [16, 16]);
    assert_eq!(cached.upload_bytes, 0);
    let (empty, _) = render(&mut pipeline, &host, &[], [16, 16]);
    assert!(empty.pixels().iter().all(|p| *p == [0.; 4]));
}
#[test]
#[ignore = "requires native Vulkan"]
fn native_repeated_geometry_exceeds_old_triangle_limit_without_repeated_uploads() {
    let (host, _) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    let mut pipeline = VectorPipeline::new(&host);
    let mut path = Vec::new();
    for i in 0..96 {
        let a = i as f64 * std::f64::consts::TAU / 96.;
        let p = [8. + 6. * a.cos(), 8. + 6. * a.sin()];
        path.push(if i == 0 {
            Segment::Move(p)
        } else {
            Segment::Line(p)
        });
    }
    path.push(Segment::Close);
    let d = Drawing {
        path: Arc::new(path),
        transform: [1., 0., 0., 1., 0., 0.],
        fill: Some([0.2, 0.4, 0.8, 1.]),
        even_odd: false,
        stroke: Some(Stroke {
            color: [0.8, 0.2, 0.4, 0.5],
            width: 0.7,
            cap: Cap::Butt,
            join: Join::Miter,
        }),
    };
    let (actual, report) = render(&mut pipeline, &host, &vec![d; 1000], [16, 16]);
    assert!(report.execution_triangles > crate::vector_geometry::MAX_TRIANGLES as u64);
    assert!(report.unique_triangles < 1000);
    assert!(report.geometry_upload_bytes < 32 * 1024);
    assert_eq!(actual.pixels()[8 * 16 + 8], [0.2, 0.4, 0.8, 1.]);
    assert_eq!(actual.pixels()[0], [0.; 4]);
}
#[test]
#[ignore = "requires native Vulkan"]
fn native_abandoned_encoder_and_budget_failure_leave_reusable_pipeline() {
    let (host, _) = pollster::block_on(Host::headless(64 * 1024)).unwrap();
    let mut pipeline = VectorPipeline::new(&host);
    let cancel = fold_media::Cancel::default();
    let drawings = vec![rectangle()];
    {
        let output = host.image([16, 16]).unwrap();
        let status = Status::new(&host).unwrap();
        let mut encoder = host.device().create_command_encoder(&Default::default());
        pipeline
            .encode(
                &mut encoder,
                &drawings,
                &output,
                &status,
                &mut vec![],
                &cancel,
            )
            .unwrap();
        // Drop without submission: a cached packet must not imply prepared GPU contents.
    }
    let (first, _) = render(&mut pipeline, &host, &drawings, [16, 16]);
    assert!(first.pixels()[5 * 16 + 5][3] > 0.29);
    assert!(
        pipeline
            .packet(&vec![rectangle(); 2000], [16, 16], &cancel)
            .is_err()
    );
    let (after, _) = render(&mut pipeline, &host, &drawings, [16, 16]);
    assert_eq!(first.pixels(), after.pixels());
    pipeline.clear();
    let (cleared, report) = render(&mut pipeline, &host, &drawings, [16, 16]);
    assert_eq!(first.pixels(), cleared.pixels());
    assert!(report.geometry_upload_bytes > 0);
}

#[test]
#[ignore = "requires native Vulkan"]
fn native_extreme_coordinates_use_clipped_preparation_and_remain_equivalent() {
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut pipeline = VectorPipeline::new(&host);
    // Every former dispatch class: fill rules, open/closed strokes, all caps
    // and joins, reflection/shear, huge translation cancellation, and mixed order.
    for transform in [
        [100000., 0., 0., 100000., -500000., -500000.],
        [100000., 0.2, 0., 1., -500000., 0.25],
        [-100000., 0.1, 30000., 1., 500000., 0.5],
        [200000., 0., 0., 1., -1000000., 0.],
        [10000., 0., 0., 1., -20000., 0.],
    ] {
        for even_odd in [false, true] {
            for cap in [Cap::Butt, Cap::Round, Cap::Square] {
                for join in [Join::Miter, Join::Round, Join::Bevel] {
                    let mut d = rectangle();
                    d.transform = transform;
                    d.even_odd = even_odd;
                    d.fill = Some([2.5, -0.2, 0.7, 0.4]);
                    d.stroke = Some(Stroke {
                        color: [0.2, 1.5, -0.1, 0.6],
                        width: 0.4,
                        cap,
                        join,
                    });
                    let mut open = d.clone();
                    Arc::make_mut(&mut open.path).pop();
                    open.fill = None;
                    let drawings = [rectangle(), d, open, rectangle()];
                    let (actual, report) = render(&mut pipeline, &host, &drawings, [16, 16]);
                    assert!(report.instances >= 3);
                    assert_eq!(report.passes, 3);
                    let expected =
                        vector::rasterize(&drawings, 16, 16, &fold_media::Cancel::default(), true)
                            .unwrap();
                    let max = actual
                        .pixels()
                        .iter()
                        .flatten()
                        .zip(expected.iter().flatten())
                        .map(|(a, b)| (a - b).abs())
                        .fold(0f32, f32::max);
                    assert!(
                        max < 0.0001,
                        "{transform:?} {even_odd} {cap:?} {join:?}: {max}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native Vulkan"]
fn native_extreme_curves_holes_and_invisible_paints_use_retained_coverage() {
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut pipeline = VectorPipeline::new(&host);
    let mut d = rectangle();
    d.transform = [1., 0., 0., 1., 0.25, 0.5];
    d.path = Arc::new(vec![
        Segment::Move([-100000., 2.]),
        Segment::Quad([0., 0.], [100000., 2.]),
        Segment::Line([100000., 14.]),
        Segment::Cubic([10000., 16.], [-10000., 16.], [-100000., 14.]),
        Segment::Close,
        Segment::Move([4., 4.]),
        Segment::Line([10., 4.]),
        Segment::Line([10., 10.]),
        Segment::Line([4., 10.]),
        Segment::Close,
    ]);
    for even_odd in [false, true] {
        d.even_odd = even_odd;
        let mut offscreen = d.clone();
        offscreen.transform[5] = 100000.;
        let mut singular = d.clone();
        singular.transform[0] = 0.;
        let mut transparent = d.clone();
        transparent.fill = Some([0.; 4]);
        let mut empty = d.clone();
        empty.path = Arc::new(vec![]);
        let drawings = [offscreen, d.clone(), singular, transparent, empty];
        let (actual, report) = render(&mut pipeline, &host, &drawings, [16, 16]);
        assert_eq!(report.instances, 1);
        assert_eq!(report.passes, 3);
        let expected =
            vector::rasterize(&drawings, 16, 16, &fold_media::Cancel::default(), true).unwrap();
        for (a, b) in actual
            .pixels()
            .iter()
            .flatten()
            .zip(expected.iter().flatten())
        {
            assert!((a - b).abs() < 0.0001, "{a} != {b}");
        }
    }
}

#[test]
#[ignore = "requires native Vulkan"]
fn native_retained_gpu_atlas_does_not_pin_evicted_cpu_geometry() {
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut pipeline = VectorPipeline::new(&host);
    let drawings = vec![rectangle()];
    let scene = pipeline
        .sources
        .prepare(&drawings, [16, 16], &fold_media::Cancel::default())
        .unwrap();
    let source = Arc::downgrade(&scene.sources[0]);
    drop(scene);
    let (first, _) = render(&mut pipeline, &host, &drawings, [16, 16]);
    pipeline.sources = Sources::default();
    assert!(
        source.upgrade().is_none(),
        "GPU retention must not bypass CPU cache eviction"
    );
    let (cached, report) = render(&mut pipeline, &host, &drawings, [16, 16]);
    assert_eq!(report.upload_bytes, 0);
    assert_eq!(first.pixels(), cached.pixels());
}
