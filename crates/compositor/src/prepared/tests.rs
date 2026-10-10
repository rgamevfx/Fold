use super::*;
use crate::OperationConfig as P;
use fold_foundation::DocumentId;
use serde_json::json;

fn fixture() -> Composite {
    let source = Node::new(
        P::Solid {
            rgba: [0.2, 0.4, 0.6, 1.],
        },
        vec![],
    );
    let output = Node::new(P::Output, vec![source.id]);
    Composite {
        info: fold_media::VideoInfo {
            width: 2,
            height: 2,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![source, output],
        extensions: Default::default(),
    }
}

#[test]
fn consumers_share_one_decode_per_record_and_stable_indexes() {
    let mut native = fixture();
    for op in crate::OPERATORS {
        native
            .nodes
            .push(Node::disconnected(op.defaults(&native.info).unwrap()));
    }
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.2);
    curve.insert(Time::new(1, 1).unwrap(), 0.8);
    native.nodes[0]
        .animation
        .insert("color.0".into(), curve)
        .unwrap();
    let before = serde_json::to_vec(&native).unwrap();
    crate::operation::DECODES.with(|count| count.set(0));
    let prepared = native.prepare().unwrap();
    let counters = prepared.counters();
    assert_eq!(counters.decoded_records, native.nodes.len());
    assert_eq!(counters.decoded_channel_bindings, 1);
    assert_eq!(prepared.render_order().unwrap().len(), 2);
    for node in prepared.nodes() {
        assert_eq!(prepared.node(node.id).unwrap().config(), node.config());
        for (name, parameter) in &node.operation.parameters {
            assert_eq!(node.parameter_name(parameter.id).unwrap(), name);
        }
        assert!(node.parameter_name(ObjectId::new()).is_err());
        prepared.channel_names(node.id).unwrap();
        for time in [
            Time::ZERO,
            Time::new(1, 2).unwrap(),
            Time::new(1, 4).unwrap(),
        ] {
            prepared.evaluated_parameters(node.id, time).unwrap();
            prepared.evaluated_effect(node.id, time).unwrap();
            node.animated_properties();
        }
    }
    let P::Solid { rgba } = prepared
        .evaluated_parameters(prepared.nodes()[0].id, Time::new(1, 2).unwrap())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(rgba[0], 0.5);
    assert_eq!(prepared.counters(), counters);
    assert_eq!(
        crate::operation::DECODES.with(|count| count.get()),
        native.nodes.len()
    );
    assert_eq!(serde_json::to_vec(&native).unwrap(), before);
}

#[test]
fn invalid_native_records_return_diagnostics_at_every_execution_entry() {
    let mutations: Vec<Box<dyn Fn(&mut Composite)>> = vec![
        Box::new(|g| {
            g.nodes[0].operation.parameters.remove("rgba");
        }),
        Box::new(|g| {
            g.nodes[0]
                .operation
                .parameters
                .get_mut("rgba")
                .unwrap()
                .value = json!({"kind":"configuration","value":[0,0,0,2]});
        }),
        Box::new(|g| {
            g.nodes[0].operation.mode = "unsupported".into();
        }),
        Box::new(|g| {
            g.nodes[0].operation.version = 999;
        }),
        Box::new(|g| {
            g.nodes[0].inputs.push(None);
        }),
        Box::new(|g| {
            g.nodes[1].inputs.clear();
        }),
        Box::new(|g| {
            g.nodes[1].set_input(0, ObjectId::new()).unwrap();
        }),
        Box::new(|g| {
            g.nodes[1].operation.id = g.nodes[0].id;
        }),
        Box::new(|g| {
            g.nodes[0].effect.mix = f32::NAN;
        }),
        Box::new(|g| {
            g.nodes[0]
                .operation
                .extensions
                .insert("parameters".into(), json!({}));
        }),
        Box::new(|g| {
            g.nodes[0].animation = serde_json::from_value(
                json!({"color.0":{"id":ObjectId::new(),"value":{"kind":"bad"},"protected":false}}),
            )
            .unwrap();
        }),
    ];
    for mutate in mutations {
        let mut native = fixture();
        mutate(&mut native);
        assert!(native.prepare().is_err());
        assert!(native.render_order().is_err());
        assert!(native.channel_names(native.output).is_err());
        assert!(native.document(DocumentId::new()).is_err());
    }
}

#[test]
fn invocation_shadows_tracks_without_redecoding_or_changing_native_protection() {
    let mut native = fixture();
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.7);
    native.nodes[0]
        .animation
        .insert("color.0".into(), curve)
        .unwrap();
    native.extensions.insert(crate::reusable::KEY.into(), json!({
        "interface": {"version":1,"images":[],"controls":[{"id":"color","label":"Color","default":[0.2,0.4,0.6,1.],"range":null,"extensions":{}}],"extensions":{}},
        "bindings":[{"control":"color","node":native.nodes[0].id,"property":"color"}]
    }));
    let graph = fold_render::RenderGraph {
        width: 2,
        height: 2,
        nodes: vec![],
        output: 0,
    };
    let invocation = fold_platform::effects::Invocation {
        graph: &graph,
        images: Default::default(),
        values: BTreeMap::from([("color".into(), vec![0.1, 0.3, 0.5, 1.])]),
        version: 1,
        timing: fold_animation::timing::Range {
            start: Time::ZERO,
            duration: Time::new(1, 1).unwrap(),
        },
        format: fold_platform::format::Format::square([2, 2]),
    };
    let before = serde_json::to_vec(&native).unwrap();
    crate::operation::DECODES.with(|count| count.set(0));
    let prepared = native
        .prepare()
        .unwrap()
        .with_invocation(&invocation)
        .unwrap();
    assert_eq!(
        prepared
            .evaluated_parameters(prepared.nodes()[0].id, Time::ZERO)
            .unwrap(),
        P::Solid {
            rgba: [0.1, 0.3, 0.5, 1.]
        }
    );
    assert_eq!(crate::operation::DECODES.with(|count| count.get()), 2);
    assert_eq!(serde_json::to_vec(&native).unwrap(), before);
    drop(prepared);
    native.nodes[0]
        .operation
        .parameters
        .get_mut("rgba")
        .unwrap()
        .protected = true;
    assert!(
        native
            .prepare()
            .unwrap()
            .with_invocation(&invocation)
            .is_err()
    );
}
