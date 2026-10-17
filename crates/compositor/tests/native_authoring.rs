use fold_compositor::{Composite, Node, OperationConfig as C};
use fold_foundation::DocumentId;
use fold_platform::packages::PackageRegistry;
use serde_json::json;

fn fixture() -> Composite {
    let solid = Node::new(
        C::Solid {
            rgba: [0.2, 0.3, 0.4, 1.],
        },
        vec![],
    );
    let grade = Node::new(
        C::ColorGrade {
            settings: fold_render::operations::Grade {
                gain: [1.; 3],
                ..Default::default()
            },
        },
        vec![solid.id],
    );
    let output = Node::new(C::Output, vec![grade.id]);
    Composite {
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![solid, grade, output],
        extensions: Default::default(),
    }
}
fn registry() -> PackageRegistry {
    let mut registry = PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    registry
}
#[test]
fn native_records_are_authoritative_and_scoped_source_preserves_neighbors() {
    let mut graph = fixture();
    let source = graph.nodes[0].id;
    let parameter = graph.nodes[0].operation.parameters["rgba"].id;
    let neighbor = graph.nodes[1].clone();
    let expected = graph.source(Some(&[source])).unwrap();
    let mut fragment: serde_json::Value = serde_json::from_str(&expected).unwrap();
    fragment[0]["operation"]["parameters"]["rgba"]["value"]["values"][0]["value"]["value"] =
        json!(0.8);
    let source_text = serde_json::to_string_pretty(&fragment).unwrap();
    graph
        .replace_source(Some(&[source]), &expected, &source_text)
        .unwrap();
    assert_eq!(
        graph.nodes[0].prepare().unwrap().config().clone(),
        C::Solid {
            rgba: [0.8, 0.3, 0.4, 1.]
        }
    );
    assert_eq!(graph.nodes[0].operation.parameters["rgba"].id, parameter);
    assert_eq!(graph.nodes[1], neighbor);
    assert!(
        graph
            .replace_source(Some(&[source]), &expected, &source_text)
            .unwrap_err()
            .contains("conflict")
    );
    let current = graph.source(Some(&[source])).unwrap();
    let node = serde_json::from_str::<Vec<serde_json::Value>>(&current).unwrap()[0].clone();
    let duplicate = serde_json::to_string(&vec![node.clone(), node]).unwrap();
    assert!(
        graph
            .replace_source(Some(&[source]), &current, &duplicate)
            .unwrap_err()
            .contains("Duplicate")
    );
    assert_eq!(graph.source(Some(&[source])).unwrap(), current);
    let document = graph.document(DocumentId::new()).unwrap();
    assert_eq!(document.schema_version, 8);
    assert_eq!(Composite::from_document(&document).unwrap(), graph);
    let payload: serde_json::Value = serde_json::from_slice(&document.payload).unwrap();
    assert!(payload["nodes"][0].get("parameters").is_none());
    assert_eq!(payload["nodes"][0]["operation"]["operation"], "Solid");
}
#[test]
fn public_source_and_whole_document_edits_cannot_bypass_native_protection() {
    let mut graph = fixture();
    graph.nodes[0]
        .operation
        .parameters
        .get_mut("rgba")
        .unwrap()
        .protected = true;
    let id = graph.nodes[0].id;
    let document = graph.document(DocumentId::new()).unwrap();
    let expected = graph.source(Some(&[id])).unwrap();
    let mut source: serde_json::Value = serde_json::from_str(&expected).unwrap();
    source[0]["operation"]["parameters"]["rgba"]["protected"] = json!(false);
    let source = serde_json::to_string(&source).unwrap();
    let error = registry()
        .author_cancellable(
            &document,
            &[json!({"op":"source", "nodes":[id], "expected":expected, "source":source})],
            &Default::default(),
        )
        .unwrap_err();
    assert!(error.contains("Protected"), "{error}");
    assert!(
        graph.nodes[0]
            .set_configuration(C::Solid { rgba: [1.; 4] })
            .is_err()
    );
    assert_eq!(Composite::from_document(&document).unwrap(), graph);
}
#[test]
fn masks_preserve_edge_identity_and_protected_effects_reject_replacement() {
    let mut graph = fixture();
    let source = graph.nodes[0].id;
    let grade = graph.nodes[1].id;
    graph.connect(source, grade, "mask").unwrap();
    let edge = graph.nodes[1].effect.mask.as_mut().unwrap();
    let edge_id = edge.id;
    edge.extensions.insert("vendor.route".into(), json!(17));
    graph.connect(source, grade, "mask").unwrap();
    assert_eq!(graph.nodes[1].effect.mask.as_ref().unwrap().id, edge_id);
    assert_eq!(
        graph.nodes[1].effect.mask.as_ref().unwrap().extensions["vendor.route"],
        17
    );
    graph.nodes[1].effect.protected = true;
    let document = graph.document(DocumentId::new()).unwrap();
    assert!(
        registry()
            .author_cancellable(
                &document,
                &[json!({"op":"disconnect", "node":grade, "input":"mask"})],
                &Default::default()
            )
            .unwrap_err()
            .contains("protected")
    );
}
#[test]
fn retired_parameter_protocol_and_unknown_versions_never_execute() {
    let mut graph = fixture();
    let document = graph.document(DocumentId::new()).unwrap();
    assert!(registry().author_cancellable(&document, &[json!({"op":"create_node", "id":fold_foundation::ObjectId::new(), "parameters":{"Solid":{"rgba":[1,0,0,1]}}})], &Default::default()).is_err());
    graph.nodes[0].operation.version = 999;
    let error = graph.validate().unwrap_err();
    assert!(error.contains("Unresolved compositor operation"), "{error}");
    let mut document = document;
    document.payload = serde_json::to_vec(&graph).unwrap();
    let mut project = fold_project::Project::new(8);
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(document.clone())],
        })
        .unwrap();
    let path = std::env::temp_dir().join(format!("fold-unresolved-{}.fold", DocumentId::new()));
    fold_project::save(&project.snapshot(), &path).unwrap();
    let loaded = fold_project::load(&path, 8).unwrap();
    assert_eq!(
        loaded.snapshot().state().documents[&document.id].payload,
        document.payload
    );
    assert!(Composite::from_document(&document).is_err());
    std::fs::remove_file(path).unwrap();
}
#[test]
fn every_inventoried_operation_uses_native_records_without_losing_kernel_configuration() {
    let info = fixture().info;
    assert!(!fold_compositor::OPERATORS.is_empty());
    for operation in fold_compositor::OPERATORS {
        let config = operation.defaults(&info).unwrap();
        let node = Node::disconnected(config.clone());
        assert_eq!(node.operation.operation, operation.id);
        assert_eq!(node.operation.version, operation.version);
        assert_eq!(
            node.prepare().unwrap().config().clone(),
            config,
            "{} {}",
            operation.id,
            operation.version
        );
        let encoded = serde_json::to_vec(&node).unwrap();
        let decoded: Node = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(node, decoded);
        assert_eq!(decoded.prepare().unwrap().config().clone(), config);
    }
}

#[test]
fn instance_overrides_keep_native_control_and_key_ids_across_inspector_projection() {
    let mut config = fold_compositor::OPERATORS
        .iter()
        .find(|o| o.id == "Effect")
        .unwrap()
        .defaults(&fixture().info)
        .unwrap();
    let C::Instance { instance, .. } = &mut config else {
        panic!()
    };
    let mut curve = fold_animation::Curve::default();
    curve.insert(fold_foundation::Time::ZERO, 0.5);
    curve.insert(fold_foundation::Time::new(1, 1).unwrap(), 1.);
    instance.animation.insert("gain".into(), vec![curve]);
    let mut node = Node::disconnected(config);
    let native = &mut node.operation.parameters.get_mut("instance").unwrap().value["value"]["overrides"]
        ["gain"];
    native["protected"] = json!(true);
    native["vendor.control"] = json!({"keep":true});
    let before = node.clone();
    node.set_configuration(node.prepare().unwrap().config().clone())
        .unwrap();
    assert_eq!(node, before);
    let mut config = node.prepare().unwrap().config().clone();
    let C::Instance { instance, .. } = &mut config else {
        panic!()
    };
    instance.animation.get_mut("gain").unwrap()[0].keys[0].value = 0.1;
    assert!(
        node.set_configuration(config)
            .unwrap_err()
            .contains("Protected instance")
    );
    assert_eq!(node, before);
}

#[test]
fn shared_composition_curves_edit_native_instance_overrides_without_changing_the_definition() {
    let mut graph = fixture();
    let mut config = fold_compositor::OPERATORS
        .iter()
        .find(|o| o.id == "Effect")
        .unwrap()
        .defaults(&graph.info)
        .unwrap();
    let C::Instance { instance, .. } = &mut config else {
        panic!()
    };
    let mut curve = fold_animation::Curve::default();
    curve.insert(fold_foundation::Time::ZERO, 0.5);
    instance.animation.insert("gain".into(), vec![curve]);
    let effect = Node::new(config, vec![graph.nodes[0].id]);
    let id = effect.id;
    graph.nodes.push(effect);
    let document = graph.document(DocumentId::new()).unwrap();
    let registry = registry();
    let mut channels = registry.animation_channels(&document, None).unwrap();
    assert_eq!(channels.len(), 1);
    let key = channels[0].curve.keys[0].id;
    channels[0].curve.keys[0].value = 0.8;
    let changed = registry
        .edit_animation(&document, &channels, fold_foundation::Time::ZERO)
        .unwrap();
    let projected = registry.animation_channels(&changed, None).unwrap();
    assert_eq!(projected[0].curve.keys[0].id, key);
    assert_eq!(projected[0].curve.keys[0].value, 0.8);
    let updated = Composite::from_document(&changed).unwrap();
    let C::Instance { instance, .. } = updated
        .node(id)
        .unwrap()
        .prepare()
        .unwrap()
        .config()
        .clone()
    else {
        panic!()
    };
    assert_eq!(
        instance.values_at(fold_foundation::Time::ZERO).unwrap()["gain"],
        vec![0.8]
    );
    assert_eq!(updated.nodes[..3], graph.nodes[..3]);
    assert_eq!(changed.dependencies, document.dependencies);
}

#[test]
fn native_rebase_preserves_disjoint_edits_and_rejects_conflicts_and_new_protection() {
    let base = fixture();
    let mut proposed = base.clone();
    proposed.nodes[0]
        .set_configuration(C::Solid {
            rgba: [0.8, 0.3, 0.4, 1.],
        })
        .unwrap();
    let mut current = base.clone();
    current.nodes[0].name = "Artist plate".into();
    current.nodes[1]
        .set_configuration(C::ColorGrade {
            settings: fold_render::operations::Grade {
                gain: [0.9; 3],
                ..Default::default()
            },
        })
        .unwrap();
    let merged = Composite::rebase(&base, &proposed, &current).unwrap();
    assert_eq!(merged.nodes[0].name, "Artist plate");
    assert_eq!(
        merged.nodes[0].prepare().unwrap().config().clone(),
        proposed.nodes[0].prepare().unwrap().config().clone()
    );
    assert_eq!(merged.nodes[1], current.nodes[1]);
    assert_eq!(
        merged.nodes[0].operation.parameters["rgba"].id,
        base.nodes[0].operation.parameters["rgba"].id
    );
    current.nodes[0]
        .set_configuration(C::Solid { rgba: [0.1; 4] })
        .unwrap();
    assert!(Composite::rebase(&base, &proposed, &current).is_err());
    current = base.clone();
    current.nodes[0]
        .operation
        .parameters
        .get_mut("rgba")
        .unwrap()
        .protected = true;
    assert!(Composite::rebase(&base, &proposed, &current).is_err());
    current = base.clone();
    current.nodes.remove(0);
    assert!(Composite::rebase(&base, &proposed, &current).is_err());
}

#[test]
fn mixed_static_and_animated_override_projection_is_stable_and_preserves_protection() {
    let mut config = fold_compositor::OPERATORS
        .iter()
        .find(|o| o.id == "Effect")
        .unwrap()
        .defaults(&fixture().info)
        .unwrap();
    let C::Instance { instance, .. } = &mut config else {
        panic!()
    };
    let mut curve = fold_animation::Curve::default();
    curve.insert(fold_foundation::Time::ZERO, 0.5);
    instance
        .animation
        .insert("gain".into(), vec![curve.clone(), curve]);
    let mut node = Node::disconnected(config);
    let parameter = &mut node.operation.parameters.get_mut("instance").unwrap().value["value"]["overrides"]
        ["gain"];
    parameter["value"]["values"][1] =
        json!({"kind":"literal","value":{"kind":"scalar","value":0.75}});
    parameter["protected"] = json!(true);
    let before = node.clone();
    assert_eq!(
        node.prepare().unwrap().config().clone(),
        node.prepare().unwrap().config().clone()
    );
    node.set_configuration(node.prepare().unwrap().config().clone())
        .unwrap();
    assert_eq!(node, before);
    let mut edited = node.prepare().unwrap().config().clone();
    let C::Instance { instance, .. } = &mut edited else {
        panic!()
    };
    instance.animation.get_mut("gain").unwrap()[1].keys[0].value = 0.8;
    assert!(node.set_configuration(edited).is_err());
    assert_eq!(node, before);
}

#[test]
fn animation_cannot_bypass_protected_static_controls_or_effect_mix() {
    for (index, parameter, path) in [(0, Some("rgba"), "color.0"), (1, None, "mix.0")] {
        let mut graph = fixture();
        if let Some(parameter) = parameter {
            graph.nodes[index]
                .operation
                .parameters
                .get_mut(parameter)
                .unwrap()
                .protected = true;
        } else {
            graph.nodes[index].effect.protected = true;
        }
        let id = graph.nodes[index].id;
        let expected = graph.source(Some(&[id])).unwrap();
        let mut proposed = graph.clone();
        let mut curve = fold_animation::Curve::default();
        curve.insert(fold_foundation::Time::ZERO, 0.25);
        proposed.nodes[index]
            .animation
            .insert(path.into(), curve)
            .unwrap();
        assert!(
            graph
                .replace_source(
                    Some(&[id]),
                    &expected,
                    &proposed.source(Some(&[id])).unwrap()
                )
                .unwrap_err()
                .contains("Protected")
        );
        assert_eq!(graph.source(Some(&[id])).unwrap(), expected);
    }
}

#[test]
fn parameter_service_samples_the_render_configuration_and_keeps_native_identity() {
    use fold_foundation::Time;
    use fold_graph::types::Literal;
    let mut graph = fixture();
    let mut transform = Node::new(
        C::ColorGrade {
            settings: Default::default(),
        },
        vec![graph.nodes[1].id],
    );
    transform
        .animation
        .insert(
            "gain.0".into(),
            fold_animation::Curve {
                keys: vec![
                    fold_animation::Key::new(Time::ZERO, 0.25),
                    fold_animation::Key::new(Time::new(1, 1).unwrap(), 0.75),
                ],
            },
        )
        .unwrap();
    let id = transform.animation.bindings()["gain.0"].id;
    graph.nodes[2].set_input(0, transform.id).unwrap();
    graph.nodes.push(transform);
    let document = graph.document(DocumentId::new()).unwrap();
    let registry = registry();
    let entries = registry.parameters(&document).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.target.parameter.native() == Some(id))
        .unwrap();
    assert_eq!(
        registry
            .sample_parameter(
                &document,
                &entry.target,
                Time::new(1, 2).unwrap(),
                &Default::default()
            )
            .unwrap(),
        Literal::Scalar { value: 0.5 }
    );
    graph.nodes.last_mut().unwrap().name = "Moved / transform".into();
    let renamed = graph.document(document.id).unwrap();
    assert_eq!(
        registry
            .sample_parameter(
                &renamed,
                &entry.target,
                Time::new(1, 1).unwrap(),
                &Default::default()
            )
            .unwrap(),
        Literal::Scalar { value: 0.75 }
    );
}
