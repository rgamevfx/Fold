//! Provider and headless requests must consume the same immutable solver result.
use fold_foundation::{DocumentId, Time};
use fold_motion::{Motion, authoring as a, fields::Datum};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
#[test]
fn solver_preview_export_overrides_and_undo_share_evaluation() {
    let mut m = Motion::empty();
    m.info.width = 64;
    m.info.height = 64;
    let source = a::add_node(&mut m, "fold.motion.rectangle").unwrap();
    a::node(&mut m, source)
        .unwrap()
        .set("width", Datum::Scalar(4.));
    a::node(&mut m, source)
        .unwrap()
        .set("height", Datum::Scalar(4.));
    let drift = a::drift::create(&mut m).unwrap();
    let n = a::node(&mut m, drift).unwrap();
    n.connect("source", source, "content");
    n.set("count", Datum::Scalar(2.));
    n.set("bounds", Datum::Vector([64., 64.]));
    n.set("speed", Datum::Scalar(12.));
    let control = a::expose_input(&mut m, drift, "speed").unwrap();
    let control_id = m
        .graph
        .node(control)
        .unwrap()
        .settings::<fold_motion::nodes::interface::ControlSettings>()
        .unwrap()
        .id;
    let output = m.graph.output.node;
    a::node(&mut m, output)
        .unwrap()
        .connect("content", drift, "content");
    let id = DocumentId::new();
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(m.document(id).unwrap())],
        })
        .unwrap();
    let original = project.snapshot();
    let packages = fold_app::packages::builtins();
    for (frame, speed) in [(24, 12.), (6, 24.), (18, 0.), (24, 12.)] {
        let mut reference = DocumentRef {
            document: id,
            output: "video".into(),
            extensions: Default::default(),
        };
        let overrides = std::collections::BTreeMap::from([(control_id, Datum::Scalar(speed))]);
        reference.extensions.insert(
            "fold.controls".into(),
            serde_json::to_value(&overrides).unwrap(),
        );
        let t = Time::new(frame, 24).unwrap();
        let from_provider = packages
            .video_with(&original, &reference, t, [64, 64], &Default::default())
            .unwrap();
        let mut expected = m.clone();
        expected.controls = overrides;
        let headless = fold_motion::evaluation::compile(&expected, t, 64, 64).unwrap();
        assert_eq!(
            fold_render::render(from_provider).unwrap().pixels(),
            fold_render::render(headless).unwrap().pixels()
        );
    }
    let mut edited = m.clone();
    a::node(&mut edited, source)
        .unwrap()
        .set("width", Datum::Scalar(8.));
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(edited.document(id).unwrap())],
        })
        .unwrap();
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        original.state().documents
    );
    project.redo().unwrap();
    assert_ne!(
        project.snapshot().state().documents,
        original.state().documents
    );
}
