use fold_foundation::{DocumentId, Time};
use fold_motion::{
    Motion, authoring as a,
    canvas::{Canvas, Framing},
    evaluation::compile,
    fields::Datum,
};
#[test]
fn resolution_changes_preserve_geometry_and_scale_rasterization() {
    let mut m = Motion::empty();
    m.info.width = 128;
    m.info.height = 96;
    let source = a::add_content(&mut m, "fold.motion.rectangle").unwrap();
    a::node(&mut m, source)
        .unwrap()
        .set("width", Datum::Scalar(20.));
    a::node(&mut m, source)
        .unwrap()
        .set("height", Datum::Scalar(20.));
    m.retain_canvas();
    let original = m.graph.clone();
    let small = fold_render::render(compile(&m, Time::ZERO, 128, 96).unwrap()).unwrap();
    m.info.width = 256;
    m.info.height = 192;
    let large = fold_render::render(compile(&m, Time::ZERO, 256, 192).unwrap()).unwrap();
    assert_eq!(m.graph, original);
    assert_eq!(small.pixels()[45 * 128 + 40], large.pixels()[90 * 256 + 80]);
    assert_eq!(large.pixels()[90 * 256 + 80], [1.; 4]);
    let restored = Motion::from_document(&m.document(DocumentId::new()).unwrap()).unwrap();
    assert_eq!(restored.canvas, m.canvas);
    let preview = fold_render::render(compile(&restored, Time::ZERO, 128, 96).unwrap()).unwrap();
    assert_eq!(small.pixels(), preview.pixels());
}
#[test]
fn portrait_framing_and_viewer_picking_use_identical_mapping() {
    let mut m = Motion::empty();
    m.info.width = 1080;
    m.info.height = 1920;
    m.canvas = Some(Canvas {
        size: [1920., 1080.],
        framing: Framing::Fit,
    });
    let matrix = m.canvas_matrix(m.output_format(), [1080, 1920]).unwrap();
    let (scale, offset) = m.viewport_mapping([540., 960.]).unwrap();
    assert_eq!(scale, [matrix.a / 2., matrix.d / 2.]);
    assert_eq!(offset, [matrix.tx / 2., matrix.ty / 2.]);
    assert_eq!(matrix.a, matrix.d);
    assert!(matrix.ty > 0.);
    m.canvas.as_mut().unwrap().framing = Framing::Responsive;
    assert_eq!(m.canvas_size(), [607.5, 1080.]);
}

#[test]
fn pixel_distances_use_full_output_resolution_and_reference_sizes_remain_logical() {
    use fold_motion::{
        evaluation::{Evaluator, Value},
        fields::{Budget, Sample},
        graph::Link,
    };
    let mut m = Motion::empty();
    m.info.width = 1920;
    m.info.height = 1080;
    m.retain_canvas();
    let node = a::add_node(&mut m, "fold.motion.format").unwrap();
    for (format, expected) in [([1920, 1080], [1., 1.]), ([3840, 2160], [0.5, 0.5])] {
        m.info.width = format[0];
        m.info.height = format[1];
        let mut evaluator = Evaluator::new(&m, Time::ZERO).unwrap();
        let Value::Field(field) = evaluator
            .resolve(&Link {
                node,
                socket: "pixel_size".into(),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            field
                .sample(&Sample::default(), &mut Budget::default())
                .unwrap()
                .vector()
                .unwrap(),
            expected
        );
    }
    let source = fold_project::DocumentRef {
        document: DocumentId::new(),
        output: "video".into(),
        extensions: Default::default(),
    };
    let info = fold_media::VideoInfo {
        width: 3840,
        height: 2160,
        rate: [24, 1],
        frames: 24,
    };
    let id = a::composition::place(
        &mut m,
        source,
        info,
        Time::ZERO,
        Time::ZERO,
        Time::new(1, 1).unwrap(),
        "Reference".into(),
    )
    .unwrap();
    let settings = a::node(&mut m, id)
        .unwrap()
        .settings::<fold_motion::nodes::composition::Settings>()
        .unwrap();
    assert_eq!(settings.size, Some([1920., 1080.]));
}
