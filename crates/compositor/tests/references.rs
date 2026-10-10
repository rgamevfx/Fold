use fold_compositor::{Composite, Node, OperationConfig as P, package};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::packages::PackageRegistry;
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use serde_json::json;
fn fixture() -> Composite {
    let mut source = Node::new(
        P::Solid {
            rgba: [0.2, 1., 0., 1.],
        },
        vec![],
    );
    source.name = "Source".into();
    let grade = Node::new(
        P::ColorGrade {
            settings: Default::default(),
        },
        vec![source.id],
    );
    let output = Node::new(P::Output, vec![grade.id]);
    Composite {
        info: fold_media::VideoInfo {
            width: 4,
            height: 4,
            rate: [24, 1],
            frames: 72,
        },
        output: output.id,
        nodes: vec![source, grade, output],
        extensions: Default::default(),
    }
}
#[test]
fn public_links_and_expressions_render_animated_sources_and_survive_rename_save_and_undo() {
    let mut graph = fixture();
    let source = graph.nodes[0].id;
    let grade = graph.nodes[1].id;
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.2);
    curve.insert(Time::new(2, 1).unwrap(), 0.6);
    graph.nodes[0]
        .animation
        .insert("color.0".into(), curve)
        .unwrap();
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let id = DocumentId::new();
    let original = graph.document(id).unwrap();
    let (bound,_)=registry.author_cancellable(&original,&[
        json!({"op":"expression","node":grade,"property":"gain","component":0,"source":"ch(\"../Source/color/r\") * 2"}),
        json!({"op":"reference","node":grade,"property":"gain","component":1,"source_node":source,"source_property":"color","source_component":0}),
    ],&Default::default()).unwrap();
    let mut graph = Composite::from_document(&bound).unwrap();
    graph.node_mut(source).unwrap().name = "Renamed source".into();
    let bound = graph.document(id).unwrap();
    let catalog = registry.parameters(&bound).unwrap();
    assert!(
        catalog
            .iter()
            .any(|entry| entry.path == ["Grade", "gain", "r"])
    );
    let bytes = serde_json::to_vec(&bound).unwrap();
    let reopened: fold_project::Document = serde_json::from_slice(&bytes).unwrap();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(original)],
        })
        .unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(reopened)],
        })
        .unwrap();
    let target = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    for (n, value) in [(2, 0.6), (0, 0.2), (1, 0.4)] {
        let image = fold_render::render(
            registry
                .video(&project.snapshot(), &target, Time::new(n, 1).unwrap(), 4, 4)
                .unwrap(),
        )
        .unwrap();
        let pixel = image.pixels()[0];
        assert!((pixel[0] - 2. * value * value).abs() < 1e-6);
        assert!((pixel[1] - value).abs() < 1e-6);
    }
    project.undo().unwrap();
    let image = fold_render::render(
        registry
            .video(&project.snapshot(), &target, Time::ZERO, 4, 4)
            .unwrap(),
    )
    .unwrap();
    assert!((image.pixels()[0][0] - 0.2).abs() < 1e-6);
}
#[test]
fn references_reject_cycles_missing_targets_and_protected_edits_and_duplicate_self_links() {
    let graph = fixture();
    let id = DocumentId::new();
    let source = graph.nodes[0].id;
    let grade = graph.nodes[1].id;
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let doc = graph.document(id).unwrap();
    let link = |node,
                property: &str,
                component,
                source_node,
                source_property: &str,
                source_component| json!({"op":"reference","node":node,"property":property,"component":component,"source_node":source_node,"source_property":source_property,"source_component":source_component});
    let (bound, _) = registry
        .author_cancellable(
            &doc,
            &[link(grade, "gain", 0, source, "color", 0)],
            &Default::default(),
        )
        .unwrap();
    assert!(
        registry
            .author_cancellable(
                &bound,
                &[link(source, "color", 0, grade, "gain", 0)],
                &Default::default()
            )
            .unwrap_err()
            .contains("cycle")
    );
    assert!(
        registry
            .author_cancellable(
                &bound,
                &[json!({"op":"remove","nodes":[source]})],
                &Default::default()
            )
            .is_err()
    );
    let mut locked = graph.clone();
    locked.nodes[1]
        .operation
        .parameters
        .get_mut("settings")
        .unwrap()
        .protected = true;
    assert!(
        registry
            .author_cancellable(
                &locked.document(id).unwrap(),
                &[link(grade, "gain", 0, source, "color", 0)],
                &Default::default()
            )
            .is_err()
    );
    let (self_link, _) = registry
        .author_cancellable(
            &doc,
            &[link(grade, "gain", 0, grade, "gamma", 0)],
            &Default::default(),
        )
        .unwrap();
    let duplicate = ObjectId::new();
    let (copy, _) = registry
        .author_cancellable(
            &self_link,
            &[json!({"op":"duplicate","node":grade,"id":duplicate})],
            &Default::default(),
        )
        .unwrap();
    let mut copy = Composite::from_document(&copy).unwrap();
    copy.node_mut(duplicate)
        .unwrap()
        .set_property("gamma", &[2., 1., 1.])
        .unwrap();
    let prepared = copy.prepare().unwrap();
    let P::ColorGrade { settings } = prepared
        .evaluated_parameters(duplicate, Time::ZERO)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(settings.gain[0], 2.);
    let P::ColorGrade { settings } = prepared.evaluated_parameters(grade, Time::ZERO).unwrap()
    else {
        panic!()
    };
    assert_eq!(settings.gain[0], 1.);
    let (invalid,_)=registry.author_cancellable(&doc,&[json!({"op":"expression","node":grade,"property":"gain","component":0,"source":"1 / 0"})],&Default::default()).unwrap();
    assert!(
        Composite::from_document(&invalid)
            .unwrap()
            .prepare()
            .unwrap()
            .evaluated_parameters(grade, Time::ZERO)
            .is_err()
    );
}

#[test]
fn shader_interface_changes_preserve_driven_control_targets() {
    use fold_platform::effects::Control;
    let mut graph = fixture();
    let grade = graph.nodes[1].id;
    let control = |id: &str, value| Control {
        id: id.into(),
        label: id.into(),
        default: vec![value],
        range: None,
        extensions: Default::default(),
    };
    let mut settings = fold_compositor::shader::Shader {
        source: "fn effect(uv:vec2<f32>)->vec4<f32>{return source(uv);}".into(),
        images: vec!["source".into()],
        controls: vec![control("first", 0.2), control("second", 0.8)],
    };
    let shader = Node::new(
        P::Shader {
            settings: settings.clone(),
        },
        vec![graph.nodes[0].id],
    );
    let sid = shader.id;
    graph.nodes.push(shader);
    let target = graph
        .prepare()
        .unwrap()
        .node(sid)
        .unwrap()
        .property_reference("second", 0)
        .unwrap();
    graph
        .node_mut(grade)
        .unwrap()
        .animation
        .set_binding(
            "gain.0".into(),
            fold_graph::binding::Binding::Link {
                reference: target.into(),
            },
        )
        .unwrap();
    settings.controls.reverse();
    settings.controls[0].id = "renamed".into();
    graph
        .migrate_shader(
            sid,
            settings.clone(),
            &std::collections::BTreeMap::from([("second".into(), "renamed".into())]),
            &[],
        )
        .unwrap();
    let P::ColorGrade { settings: grade } = graph
        .prepare()
        .unwrap()
        .evaluated_parameters(grade, Time::ZERO)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(grade.gain[0], 0.8);
    let before = graph.clone();
    settings.controls.remove(0);
    assert!(
        graph
            .migrate_shader(sid, settings, &Default::default(), &["renamed".into()])
            .is_err()
    );
    assert_eq!(graph, before);
}
