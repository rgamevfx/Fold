use fold_compositor::{
    Composite, Node, OperationConfig as P,
    roto::{Combine, Layer, Roto},
};
use fold_foundation::{DocumentId, Time};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
fn fixture(settings: Roto) -> (Composite, fold_foundation::ObjectId) {
    let roto = Node::disconnected(P::Roto { settings });
    let id = roto.id;
    let output = Node::new(P::Output, vec![id]);
    (
        Composite {
            info: fold_media::VideoInfo {
                width: 64,
                height: 64,
                rate: [24, 1],
                frames: 48,
            },
            output: output.id,
            nodes: vec![roto, output],
            extensions: Default::default(),
        },
        id,
    )
}
fn compile(graph: &Composite, time: Time, size: u32) -> fold_render::RenderGraph {
    let mut project = Project::new(8);
    let id = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(id).unwrap())],
        })
        .unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    registry
        .video(
            &project.snapshot(),
            &DocumentRef {
                document: id,
                output: "video".into(),
                extensions: Default::default(),
            },
            time,
            size,
            size,
        )
        .unwrap()
}
#[test]
fn roto_feather_hole_channels_and_exact_animation_survive_roundtrip() {
    let mut settings = Roto::default();
    let mut shape = Layer::rectangle([16., 16.], [48., 48.], false);
    let id = shape.id;
    shape.shape.as_mut().unwrap().feather = 8.;
    let mut hole = Layer::rectangle([24., 24.], [40., 40.], true);
    hole.combine = Combine::Subtract;
    settings.root.children = vec![shape, hole];
    let (mut graph, node) = fixture(settings);
    let path = format!("roto/{id}/translate.0");
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 4.);
    graph
        .node_mut(node)
        .unwrap()
        .animation
        .insert(path, curve)
        .unwrap();
    let doc = graph.document(DocumentId::new()).unwrap();
    let graph = Composite::from_document(&doc).unwrap();
    let pixel = fold_render::render(compile(&graph, Time::ZERO, 64)).unwrap();
    let alpha = |x: usize, y: usize| pixel.pixels()[y * 64 + x][3];
    assert_eq!(alpha(20, 20), 1.);
    assert_eq!(alpha(32, 32), 0.);
    assert_eq!(alpha(0, 0), 0.);
    assert!(alpha(13, 32) > 0. && alpha(13, 32) < 1.);
    assert_eq!(
        pixel.pixels()[20 * 64 + 20][0],
        0.,
        "alpha output must leave RGB unchanged"
    );
    let sampled = graph
        .prepare()
        .unwrap()
        .evaluated_parameters(node, Time::new(1, 2).unwrap())
        .unwrap();
    let P::Roto { settings } = sampled else {
        unreachable!()
    };
    assert_eq!(settings.root.find(id).unwrap().translate, [2., 0.]);
    let small = fold_render::render(compile(&graph, Time::ZERO, 32)).unwrap();
    assert_eq!(small.pixels()[10 * 32 + 10][3], 1.);
    let mut inward = Roto::default();
    let mut shape = Layer::rectangle([16., 16.], [48., 48.], false);
    shape.shape.as_mut().unwrap().feather = -8.;
    inward.root.children.push(shape);
    let (inward, _) = fixture(inward);
    let image = fold_render::render(compile(&inward, Time::ZERO, 64)).unwrap();
    assert_eq!(image.pixels()[32 * 64 + 15][3], 0.);
    assert!(image.pixels()[32 * 64 + 17][3] > 0. && image.pixels()[32 * 64 + 17][3] < 1.);
    assert_eq!(image.pixels()[32 * 64 + 32][3], 1.);
}
#[test]
fn roto_samples_deforming_points_and_group_opacity_once() {
    let mut settings = Roto::default();
    let mut group = Layer::group("Holdout");
    group.opacity = 0.5;
    group
        .children
        .push(Layer::rectangle([8., 8.], [24., 24.], false));
    group
        .children
        .push(Layer::rectangle([12., 8.], [28., 24.], false));
    let point = group.children[0].shape.as_ref().unwrap().points[0].id;
    let shape = group.children[0].id;
    settings.root.children.push(group);
    settings.motion_samples = 4;
    let (mut graph, node) = fixture(settings);
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 8.);
    curve.insert(Time::new(1, 1).unwrap(), 16.);
    graph
        .node_mut(node)
        .unwrap()
        .animation
        .insert(format!("roto/{shape}/point/{point}/position.0"), curve)
        .unwrap();
    let pixels = fold_render::render(compile(&graph, Time::new(1, 2).unwrap(), 64)).unwrap();
    assert!(
        (pixels.pixels()[16 * 64 + 16][3] - 0.5).abs() < 1e-6,
        "isolated group opacity applies after overlap"
    );
    let operations = compile(&graph, Time::new(1, 2).unwrap(), 64);
    assert!(
        operations
            .nodes
            .iter()
            .filter_map(|n| if let fold_render::ImageOp::Coverage(p) = n {
                Some(p)
            } else {
                None
            })
            .flat_map(|p| p.nodes.iter())
            .filter(|n| matches!(n, fold_render::coverage::Op::Mix { .. }))
            .count()
            == 3
    );
}
#[cfg(feature = "ui")]
#[test]
#[ignore = "native GPU qualification and timing"]
fn roto_gpu_matches_cpu_and_reuses_geometry() {
    use fold_render::gpu::{Host, Renderer};
    let (host, info) = pollster::block_on(Host::headless(512 * 1024 * 1024)).unwrap();
    eprintln!("Roto GPU: {info:?}");
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let mut settings = Roto::default();
    let mut shape = Layer::rectangle([16., 16.], [48., 48.], true);
    shape.shape.as_mut().unwrap().feather = 4.;
    settings.root.children.push(shape);
    let mut inward = Layer::rectangle([2., 2.], [14., 14.], false);
    inward.shape.as_mut().unwrap().feather = -2.;
    settings.root.children.push(inward);
    let (graph, _) = fixture(settings);
    let ir = compile(&graph, Time::ZERO, 128);
    let expected = fold_render::render(ir.clone()).unwrap();
    let mut gpu = renderer
        .evaluate(ir.clone(), None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(gpu.statistics.cpu_adapter_nodes, 0);
    assert_eq!(gpu.statistics.readback_bytes, 0);
    let actual = gpu.readback(&cancel).unwrap();
    let error = actual
        .pixels()
        .iter()
        .zip(expected.pixels())
        .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
        .fold(0f32, f32::max);
    assert!(error < 0.002, "error {error}");
    let mut warm = renderer.evaluate(ir, None, &mut decoder, &cancel).unwrap();
    warm.readback(&cancel).unwrap();
    assert_eq!(warm.statistics.vector_geometry_upload_bytes, 0);
    eprintln!("Roto error {error}, warm statistics {:?}", warm.statistics);
    for size in [512, 1080, 2160] {
        let ir = compile(&graph, Time::ZERO, size);
        let start = std::time::Instant::now();
        let mut frame = renderer
            .evaluate(ir.clone(), None, &mut decoder, &cancel)
            .unwrap();
        frame.readback(&cancel).unwrap();
        let cold = start.elapsed();
        let start = std::time::Instant::now();
        let mut frame = renderer.evaluate(ir, None, &mut decoder, &cancel).unwrap();
        while !frame.is_ready().unwrap() {
            host.poll().unwrap();
            assert!(start.elapsed().as_secs() < 30);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let ready = start.elapsed();
        frame.readback(&cancel).unwrap();
        eprintln!(
            "Roto {size}x{size}: cold incl. readback {cold:?}, warm ready {ready:?}, warm incl. readback {:?}, prepare {} ns, evaluate CPU {} ns",
            start.elapsed(),
            frame.statistics.vector_prepare_nanoseconds,
            frame.statistics.evaluate_cpu_nanoseconds
        );
    }
}

#[cfg(feature = "ui")]
#[test]
#[ignore = "native Roto playback cost regression"]
fn roto_playback_cost() {
    use fold_render::gpu::{Host, Renderer};
    use std::time::{Duration, Instant};
    let (host, adapter) = pollster::block_on(Host::headless(512 * 1024 * 1024)).unwrap();
    eprintln!("Playback adapter: {adapter:?}");
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let mut static_passes = 0;
    for mode in ["bypass", "hard", "feather", "animated", "deformed"] {
        let mut settings = Roto::default();
        let mut shape = Layer::rectangle([16., 16.], [48., 48.], true);
        if mode != "hard" {
            shape.shape.as_mut().unwrap().feather = 4.;
        }
        let shape_id = shape.id;
        let point_id = shape.shape.as_ref().unwrap().points[0].id;
        settings.root.children.push(shape);
        let (mut graph, roto) = fixture(settings);
        let background = Node::disconnected(P::Solid {
            rgba: [0.2, 0.3, 0.4, 1.],
        });
        graph
            .node_mut(roto)
            .unwrap()
            .set_input(0, background.id)
            .unwrap();
        if mode == "bypass" {
            graph
                .node_mut(graph.output)
                .unwrap()
                .set_input(0, background.id)
                .unwrap();
        }
        graph.nodes.insert(0, background);
        if mode == "animated" || mode == "deformed" {
            let mut curve = fold_animation::Curve::default();
            let base = if mode == "deformed" { 48. } else { 0. };
            curve.insert(Time::ZERO, base);
            curve.insert(Time::new(1, 1).unwrap(), base + 8.);
            graph
                .node_mut(roto)
                .unwrap()
                .animation
                .insert(
                    if mode == "deformed" {
                        format!("roto/{shape_id}/point/{point_id}/position.0")
                    } else {
                        format!("roto/{shape_id}/translate.0")
                    },
                    curve,
                )
                .unwrap();
        }
        let mut renderer = Renderer::new(host.clone()).unwrap();
        let mut samples = vec![];
        let mut passes = 0;
        let mut cpu = 0;
        let mut gpu_us = 0.;
        for frame in 0..8 {
            graph.nodes[0]
                .set_configuration(P::Solid {
                    rgba: [0.2 + frame as f32 * 0.01, 0.3, 0.4, 1.],
                })
                .unwrap();
            let start = Instant::now();
            let ir = compile(&graph, Time::new(frame, 24).unwrap(), 1080);
            let compiled = start.elapsed();
            let start = Instant::now();
            let result = renderer.evaluate(ir, None, &mut decoder, &cancel).unwrap();
            while !result.is_ready().unwrap() {
                host.poll().unwrap();
                assert!(start.elapsed() < Duration::from_secs(30));
                std::thread::sleep(Duration::from_micros(100));
            }
            assert_eq!(result.statistics.readback_bytes, 0);
            assert_eq!(result.statistics.cpu_adapter_nodes, 0);
            if frame > 1 {
                samples.push((start.elapsed().as_micros(), compiled.as_micros()));
                passes = result.statistics.compute_passes;
                cpu = result.statistics.evaluate_cpu_nanoseconds / 1000;
                gpu_us = result.gpu_nanoseconds().unwrap().unwrap_or(0.) / 1000.;
                if mode == "hard" || mode == "feather" {
                    assert_eq!(result.statistics.coverage_pixels, 0);
                    assert_eq!(result.statistics.resident_coverage_nodes, 1);
                }
            }
        }
        samples.sort();
        eprintln!(
            "{mode}: median ready/compile us {:?}, evaluate CPU us {cpu}, GPU us {gpu_us:.1}, passes {passes}",
            samples[samples.len() / 2]
        );
        if mode == "feather" {
            static_passes = passes;
        }
    }
    assert!(
        static_passes <= 2,
        "static Roto must reuse its matte while the background changes; got {static_passes} passes"
    );
}

#[cfg(feature = "ui")]
#[test]
#[ignore = "native scalar coverage parity and invalidation"]
fn roto_gpu_scalar_tiles_preserve_compositing_and_invalidation() {
    use fold_render::gpu::{Host, Renderer};
    let (host, _) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for mode in [
        Combine::Over,
        Combine::Add,
        Combine::Subtract,
        Combine::Intersect,
        Combine::Replace,
    ] {
        let mut settings = Roto::default();
        settings.root.invert = true;
        settings.root.opacity = 0.8;
        let mut a = Layer::rectangle([-8., 8.], [48., 48.], false);
        a.opacity = 0.7;
        a.shape.as_mut().unwrap().feather = 3.;
        let mut b = Layer::rectangle([16., 16.], [40., 40.], true);
        b.opacity = 0.4;
        b.combine = mode;
        b.shape.as_mut().unwrap().feather = -2.;
        settings.root.children = vec![a, b];
        let (graph, _) = fixture(settings);
        for size in [64, 96] {
            let ir = compile(&graph, Time::ZERO, size);
            let expected = fold_render::render(ir.clone()).unwrap();
            let mut actual = renderer
                .evaluate(ir.clone(), None, &mut decoder, &cancel)
                .unwrap();
            assert_eq!(
                actual.statistics.resident_coverage_nodes, 0,
                "configuration or size must invalidate"
            );
            let pixels = actual.readback(&cancel).unwrap();
            let error = expected
                .pixels()
                .iter()
                .zip(pixels.pixels())
                .map(|(a, b)| (a[3] - b[3]).abs())
                .fold(0f32, f32::max);
            assert!(error < 0.002, "{mode:?} {size}: {error}");
            assert!(
                (pixels.pixels()[0][3] - 0.8).abs() < 1e-6,
                "inverted area outside the cropped tiles"
            );
            let covered = match mode {
                Combine::Over => 0.82,
                Combine::Add => 1.,
                Combine::Subtract => 0.42,
                Combine::Intersect => 0.28,
                Combine::Replace => 0.4,
            };
            let center = (size / 2) as usize;
            assert!(
                (pixels.pixels()[center * size as usize + center][3] - (1. - covered) * 0.8).abs()
                    < 1e-5
            );
            let mut warm = renderer
                .evaluate(ir.clone(), None, &mut decoder, &cancel)
                .unwrap();
            warm.readback(&cancel).unwrap();
            assert_eq!(warm.statistics.resident_coverage_nodes, 1);
            assert_eq!(warm.statistics.coverage_pixels, 0);
            assert_eq!(warm.statistics.vector_prepare_nanoseconds, 0);
            if mode == Combine::Over && size == 64 {
                use fold_render::region::{Plan, Region, crop};
                let region = Region {
                    x: 19,
                    y: 7,
                    width: 24,
                    height: 29,
                };
                let plan = Plan::new(ir, region, false).unwrap();
                let mut tile = renderer
                    .evaluate(plan.graph, None, &mut decoder, &cancel)
                    .unwrap();
                let tile = crop(tile.readback(&cancel).unwrap(), plan.output).unwrap();
                let expected = crop(expected, region).unwrap();
                for (a, b) in tile
                    .pixels()
                    .iter()
                    .flatten()
                    .zip(expected.pixels().iter().flatten())
                {
                    assert!(
                        (a - b).abs() < 0.002,
                        "regional coverage must preserve global coordinates"
                    );
                }
            }
        }
    }
    let stopped = fold_media::Cancel::default();
    stopped.cancel();
    let (graph, _) = fixture(Roto::default());
    assert!(
        renderer
            .evaluate(
                compile(&graph, Time::ZERO, 64),
                None,
                &mut decoder,
                &stopped
            )
            .is_err()
    );
}
