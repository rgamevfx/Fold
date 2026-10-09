use fold_compositor::{
    Composite, Node, OperationConfig as P,
    reusable::{Binding, Definition, KEY},
};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::effects::{Control, Instance, Interface};
use fold_project::{Document, DocumentRef, EditBatch, Mutation, Project};
use std::collections::BTreeMap;
fn info() -> fold_media::VideoInfo {
    fold_media::VideoInfo {
        width: 2,
        height: 2,
        rate: [24, 1],
        frames: 24,
    }
}
fn graph(mut nodes: Vec<Node>) -> Composite {
    let output = Node::new(P::Output, vec![nodes.last().unwrap().id]);
    let id = output.id;
    nodes.push(output);
    Composite {
        info: info(),
        nodes,
        output: id,
        extensions: Default::default(),
    }
}
fn reference(id: DocumentId) -> DocumentRef {
    DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    }
}
fn instance(id: DocumentId, gain: f64) -> Instance {
    Instance {
        id: ObjectId::new(),
        definition: reference(id),
        version: 1,
        values: BTreeMap::from([("gain".into(), vec![gain; 3])]),
        animation: Default::default(),
        enabled: true,
        extensions: Default::default(),
    }
}
fn render(
    documents: Vec<Document>,
    root: DocumentId,
    time: Time,
) -> Result<fold_render::RenderGraph, String> {
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: documents.into_iter().map(Mutation::PutDocument).collect(),
        })
        .unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    Ok(registry.video(&project.snapshot(), &reference(root), time, 2, 2)?)
}
fn definition() -> Composite {
    let source = Node::new(
        P::Input {
            id: "source".into(),
        },
        vec![],
    );
    let grade = Node::new(
        P::ColorGrade {
            settings: fold_render::operations::Grade {
                gain: [1.; 3],
                ..Default::default()
            },
        },
        vec![source.id],
    );
    let mut g = graph(vec![source, grade.clone()]);
    let definition = Definition {
        extensions: Default::default(),
        interface: Interface {
            version: 1,
            images: vec!["source".into()],
            controls: vec![Control {
                id: "gain".into(),
                label: "Gain".into(),
                default: vec![1.; 3],
                range: Some([0., 10.]),
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        },
        bindings: vec![Binding {
            extensions: Default::default(),
            control: "gain".into(),
            node: grade.id,
            property: "gain".into(),
        }],
    };
    g.extensions
        .insert(KEY.into(), serde_json::to_value(definition).unwrap());
    g
}
#[test]
fn independent_instances_share_source_work_and_preserve_definition() {
    let id = DocumentId::new();
    let root = DocumentId::new();
    let mut authored = definition();
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 3.);
    authored.nodes[1]
        .animation
        .insert("gain.0".into(), curve)
        .unwrap();
    let mut native = serde_json::to_value(&authored.nodes[1].animation).unwrap();
    native["gain.0"]["protected"] = serde_json::json!(true);
    authored.nodes[1].animation = serde_json::from_value(native).unwrap();
    let definition = authored.document(id).unwrap();
    let source = Node::new(
        P::Solid {
            rgba: [0.1, 0.2, 0.3, 1.],
        },
        vec![],
    );
    let first = Node::new(
        P::Instance {
            instance: instance(id, 2.),
            images: vec!["source".into()],
        },
        vec![source.id],
    );
    let second = Node::new(
        P::Instance {
            instance: instance(id, 0.5),
            images: vec!["source".into()],
        },
        vec![first.id],
    );
    let host = graph(vec![source, first, second]);
    let result = render(
        vec![definition.clone(), host.document(root).unwrap()],
        root,
        Time::ZERO,
    )
    .unwrap();
    assert_eq!(
        result
            .nodes
            .iter()
            .filter(|n| matches!(n, fold_render::ImageOp::Solid { .. }))
            .count(),
        1
    );
    for pixel in fold_render::render(result).unwrap().pixels() {
        assert_eq!(*pixel, [0.1, 0.2, 0.3, 1.]);
    }
    let saved = Composite::from_document(&definition).unwrap();
    assert_eq!(saved, authored);
    assert!(
        matches!(saved.nodes[1].prepare().unwrap().config().clone(),P::ColorGrade {settings} if settings.gain == [1.;3])
    );
}
#[test]
fn dissolve_interpolates_alpha_instead_of_over_compositing() {
    let a = Node::new(
        P::Solid {
            rgba: [0.5, 0., 0., 0.5],
        },
        vec![],
    );
    let b = Node::new(
        P::Solid {
            rgba: [0., 0., 0.25, 0.25],
        },
        vec![],
    );
    let mix = Node::new(
        P::Dissolve {
            curve: Default::default(),
        },
        vec![a.id, b.id],
    );
    let g = graph(vec![a, b, mix]);
    let root = DocumentId::new();
    let image = fold_render::render(
        render(
            vec![g.document(root).unwrap()],
            root,
            Time::new(1, 2).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    for pixel in image.pixels() {
        assert_eq!(*pixel, [0.25, 0., 0.125, 0.375]);
    }
}
#[test]
fn incompatible_instance_versions_fail_instead_of_rebinding() {
    let id = DocumentId::new();
    let root = DocumentId::new();
    let source = Node::new(P::Solid { rgba: [1.; 4] }, vec![]);
    let mut i = instance(id, 1.);
    i.version = 2;
    let effect = Node::new(
        P::Instance {
            instance: i,
            images: vec!["source".into()],
        },
        vec![source.id],
    );
    let host = graph(vec![source, effect]);
    assert!(
        render(
            vec![
                definition().document(id).unwrap(),
                host.document(root).unwrap()
            ],
            root,
            Time::ZERO
        )
        .unwrap_err()
        .contains("migrate")
    );
}

#[test]
fn shader_interface_rename_preserves_connections_values_and_key_identity() {
    let source = Node::new(P::Solid { rgba: [1.; 4] }, vec![]);
    let settings = fold_compositor::shader::Shader {
        source: "fn effect(uv:vec2<f32>)->vec4<f32>{return source(uv)*gain();}".into(),
        images: vec!["source".into()],
        controls: vec![Control {
            id: "gain".into(),
            label: "Gain".into(),
            default: vec![0.5],
            range: Some([0., 1.]),
            extensions: Default::default(),
        }],
    };
    let mut node = Node::new(
        P::Shader {
            settings: settings.clone(),
        },
        vec![source.id],
    );
    let mut curve = fold_animation::Curve::default();
    let key = curve.insert(Time::ZERO, 0.25);
    node.animation.insert("gain.0".into(), curve).unwrap();
    let before_binding = node.animation.bindings()["gain.0"].clone();
    let mut next = settings.clone();
    next.images[0] = "image".into();
    next.controls[0].id = "amount".into();
    next.controls[0].default = vec![1.];
    fold_compositor::shader::migrate(
        &mut node,
        next,
        &BTreeMap::from([
            ("source".into(), "image".into()),
            ("gain".into(), "amount".into()),
        ]),
        &[],
    )
    .unwrap();
    assert_eq!(node.input_ids(), vec![Some(source.id)]);
    assert_eq!(node.animation.bindings()["amount.0"], before_binding);
    assert_eq!(
        node.animation.curve("amount.0").unwrap().unwrap().keys[0].id,
        key
    );
    let P::Shader { settings } = &node.prepare().unwrap().config().clone() else {
        panic!()
    };
    assert_eq!(settings.controls[0].default, vec![0.5]);
    let mut invalid = settings.clone();
    invalid.controls.clear();
    assert!(fold_compositor::shader::migrate(&mut node, invalid, &BTreeMap::new(), &[]).is_err());
}

#[test]
fn library_presets_are_editable_graphs_with_valid_promoted_controls() {
    for preset in [
        fold_compositor::presets::Preset::Glow,
        fold_compositor::presets::Preset::Dissolve,
        fold_compositor::presets::Preset::CircularWipe,
        fold_compositor::presets::Preset::FilmLook,
        fold_compositor::presets::Preset::Grain,
    ] {
        let document = fold_compositor::presets::create(preset, DocumentId::new()).unwrap();
        let graph = Composite::from_document(&document).unwrap();
        graph.prepare().unwrap().validate_reusable().unwrap();
        assert!(graph.nodes.len() >= 3);
        for node in &graph.nodes {
            if let P::Shader { settings } = &node.prepare().unwrap().config().clone() {
                settings
                    .program(fold_platform::format::Format::square([1920, 1080]), 0., 0.)
                    .unwrap()
                    .validate_source()
                    .unwrap();
            }
        }
    }
}
