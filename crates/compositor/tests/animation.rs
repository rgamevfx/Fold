use fold_animation::Curve;
use fold_compositor::{Composite, Node, Parameters as P};
use fold_foundation::{DocumentId, Time};
use fold_platform::packages::{CommandRequest, PackageRegistry};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
fn fixture() -> Composite {
    let source = Node::new(
        P::Solid {
            rgba: [1., 0., 0., 1.],
        },
        vec![],
    );
    let transform = Node::new(
        P::Transform {
            translate: [0., 0.],
            scale: [1., 1.],
            opacity: 1.,
        },
        vec![source.id],
    );
    let output = Node::new(P::Output, vec![transform.id]);
    Composite {
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 96,
        },
        output: output.id,
        nodes: vec![source, transform, output],
        extensions: Default::default(),
    }
}
#[test]
fn animation_compiles_from_snapshots_roundtrips_extensions_and_undoes_atomically() {
    let id = DocumentId::new();
    let mut graph = fixture();
    graph.nodes[1]
        .extensions
        .insert("future".into(), serde_json::json!({"data":[1,2,3]}));
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(id).unwrap())],
        })
        .unwrap();
    let before = project.snapshot();
    let mut curve = Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(2, 1).unwrap(), 1.);
    graph.nodes[1].animation.insert("opacity.0".into(), curve);
    let mut registry = PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    let request = CommandRequest {
        id: fold_compositor::package::EDIT.into(),
        base: before.revision(),
        arguments: serde_json::to_vec(&fold_compositor::package::EditArgs {
            document: id,
            composite: graph.clone(),
        })
        .unwrap(),
    };
    project
        .commit(
            registry
                .stage(&before, &request, &Default::default())
                .unwrap(),
        )
        .unwrap();
    let edited = project.snapshot();
    let reference = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    for (n, expected) in [(2, 1.), (0, 0.), (1, 0.5), (1, 0.5)] {
        let frame = fold_render::render(
            registry
                .video(&edited, &reference, Time::new(n, 1).unwrap(), 16, 16)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(frame.pixels()[0], [expected, 0., 0., expected]);
    }
    // Old immutable snapshots are unaffected by subsequent animation edits.
    let frame = fold_render::render(
        registry
            .video(&before, &reference, Time::ZERO, 16, 16)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(frame.pixels()[0][3], 1.);
    let bytes = serde_json::to_vec(&graph.document(id).unwrap()).unwrap();
    let reopened = Composite::from_document(&serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(reopened, graph);
    project.undo().unwrap();
    assert!(
        Composite::from_document(&project.snapshot().state().documents[&id])
            .unwrap()
            .nodes[1]
            .animation
            .is_empty()
    );
    project.redo().unwrap();
    assert_eq!(
        Composite::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        graph
    );
}
#[test]
fn static_schema_migrates_and_component_curves_do_not_change_other_values() {
    let mut graph = fixture();
    let id = DocumentId::new();
    let mut legacy = graph.document(id).unwrap();
    legacy.schema_version = 2;
    assert_eq!(Composite::from_document(&legacy).unwrap(), graph);
    let mut curve = Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 100.);
    graph.nodes[1]
        .animation
        .insert("position.0".into(), curve.clone());
    let P::Transform {
        translate,
        scale,
        opacity,
    } = graph.nodes[1]
        .evaluated_parameters(Time::new(1, 2).unwrap())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(translate, [50., 0.]);
    assert_eq!(scale, [1., 1.]);
    assert_eq!(opacity, 1.);
    graph.nodes[1]
        .animation
        .insert("unavailable.0".into(), curve);
    assert!(graph.validate().is_err());
}

#[test]
fn removing_the_last_channel_holds_its_current_value_and_static_parameters_are_exact() {
    let mut node = Node::new(
        P::Crop {
            rect: [0, 0, 100_000, 100_000],
        },
        vec![],
    );
    assert_eq!(
        node.evaluated_parameters(Time::ZERO).unwrap(),
        node.parameters
    );
    let mut curve = Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 100.);
    node.animation.insert("bounds.0".into(), curve);
    let time = Time::new(1, 2).unwrap();
    let value = node.evaluated_parameters(time).unwrap();
    assert_eq!(
        value,
        P::Crop {
            rect: [50, 0, 100_000, 100_000]
        }
    );
    node.remove_animation_channel("bounds.0", time);
    assert!(node.animation.is_empty());
    assert_eq!(node.parameters, value);
}
