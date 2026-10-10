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
            .filter(|n| matches!(n, fold_render::ImageOp::Vector(_)))
            .count()
            >= 8
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
