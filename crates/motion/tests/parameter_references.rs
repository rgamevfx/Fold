use fold_foundation::{DocumentId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{Limits, Prepared},
    model::{Construction, Reference},
    types::{Literal, Type},
};

fn fixture() -> Construction {
    Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap()).unwrap()
}
fn set(c: &mut Construction, name: &str, binding: Binding) {
    c.definitions[0].nodes[0]
        .parameters
        .get_mut(name)
        .unwrap()
        .value = serde_json::to_value(binding).unwrap();
}
fn reference(c: &Construction, name: &str) -> Reference {
    Reference::Parameter {
        parameter: c.definitions[0].nodes[0].parameters[name].id,
        component: None,
    }
}
fn sample(c: &Construction, target: Reference, time: Time) -> Literal {
    let mut c = c.clone();
    c.definitions[0].outputs[0].value.target = target;
    c.definitions[0].outputs[0].ty = Type::Distance {
        unit: fold_motion::types::Unit::Canvas,
    };
    Prepared::new(c.clone())
        .unwrap()
        .evaluate(
            c.definitions[0].outputs[0].id,
            time,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap()
        .primitive()
        .unwrap()
        .clone()
}
#[test]
fn reference_expression_retains_identity_through_rename_source_and_reopen() {
    let mut c = fixture();
    let scope = &c.definitions[0];
    let source = format!(
        "ch(\"../{}/height\") * .5",
        fold_graph::channel::escape(&scope.nodes[0].name)
    );
    let parsed = fold_motion::expression_source::parse(&source, &mut |path| {
        fold_graph::channel::resolve_definition(scope, Some(&scope.nodes[0].name), path)
    })
    .unwrap();
    set(
        &mut c,
        "width",
        Binding::Expression {
            expression: parsed.expression,
        },
    );
    let target = reference(&c, "width");
    let expected = sample(&c, target, Time::ZERO);
    c.definitions[0].nodes[0].name = "renamed / rectangle".into();
    c.validate_semantics().unwrap();
    let source = fold_motion::source::canonical(&c).unwrap();
    assert!(source.contains("ch("));
    let restored = fold_motion::source::parse(&source).unwrap().construction;
    assert_eq!(c, restored);
    assert_eq!(sample(&restored, target, Time::ZERO), expected);
    let bytes = serde_json::to_vec(&c).unwrap();
    let restored: Construction = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(sample(&restored, target, Time::ZERO), expected);
}
#[test]
fn cycles_fail_and_parameter_tracks_do_not_add_nodes() {
    let mut c = fixture();
    let count = c.definitions[0].nodes.len();
    let height = c.definitions[0].nodes[0].parameters["height"].id;
    c = fold_motion::keyed::animate(&c, height, Time::ZERO).unwrap();
    assert_eq!(count, c.definitions[0].nodes.len());
    let width = reference(&c, "width");
    set(
        &mut c,
        "width",
        Binding::Link {
            reference: Reference::Parameter {
                parameter: height,
                component: None,
            }
            .into(),
        },
    );
    c.validate_semantics().unwrap();
    assert_eq!(
        sample(&c, width, Time::new(2, 1).unwrap()),
        sample(&c, width, Time::ZERO)
    );
    set(
        &mut c,
        "height",
        Binding::Link {
            reference: width.into(),
        },
    );
    assert!(c.validate_semantics().unwrap_err().contains("Cyclic"));
}

#[test]
fn referenced_tracks_sample_each_nested_occurrence_clock_without_cross_talk() {
    use fold_foundation::ObjectId;
    use fold_motion::{
        model::{Definition, Instance, Output},
        types::{Clock, Ratio, Unit},
    };
    let mut c = fixture();
    let height = c.definitions[0].nodes[0].parameters["height"].id;
    c = fold_motion::keyed::animate(&c, height, Time::ZERO).unwrap();
    let Binding::Track { mut track } = c.definitions[0].nodes[0].parameters["height"]
        .binding()
        .unwrap()
    else {
        panic!()
    };
    let mut end = track.keys[0].clone();
    end.id = ObjectId::new();
    end.time = Time::new(2, 1).unwrap();
    end.value = Literal::Distance {
        value: 200.,
        unit: Unit::Canvas,
    };
    track.keys.push(end);
    set(&mut c, "height", Binding::Track { track });
    let target = reference(&c, "height");
    set(
        &mut c,
        "width",
        Binding::Link {
            reference: target.into(),
        },
    );
    let width = c.definitions[0].nodes[0].parameters["width"].id;
    let child = c.root.clone();
    let mut forward = Instance {
        id: ObjectId::new(),
        name: "Forward".into(),
        ..child.clone()
    };
    forward.clock = Clock::default();
    let mut reverse = Instance {
        id: ObjectId::new(),
        name: "Reverse".into(),
        ..child
    };
    reverse.clock = Clock {
        offset: Time::new(2, 1).unwrap(),
        rate: Ratio::new(-1, 1).unwrap(),
    };
    let parent = Definition {
        id: ObjectId::new(),
        name: "Parent".into(),
        inputs: vec![],
        nodes: vec![],
        instances: vec![forward.clone(), reverse.clone()],
        outputs: vec![Output {
            id: ObjectId::new(),
            name: "main".into(),
            ty: c.definitions[0].outputs[0].ty.clone(),
            value: Reference::Instance {
                instance: forward.id,
                output: c.definitions[0].outputs[0].id,
            }
            .into(),
            extensions: Default::default(),
        }],
        extensions: Default::default(),
    };
    c.root.definition = parent.id;
    c.root.overrides.clear();
    c.definitions.push(parent);
    let document = c.document(DocumentId::new()).unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    let catalog = registry.parameters(&document).unwrap();
    let forward_target = catalog
        .iter()
        .find(|entry| {
            entry.target.parameter.native() == Some(width)
                && entry.target.occurrence == [forward.id]
        })
        .unwrap()
        .target
        .clone();
    let reverse_target = catalog
        .iter()
        .find(|entry| {
            entry.target.parameter.native() == Some(width)
                && entry.target.occurrence == [reverse.id]
        })
        .unwrap()
        .target
        .clone();
    assert_ne!(forward_target, reverse_target);
    let prepared = Prepared::new(c).unwrap();
    for time in [
        Time::ZERO,
        Time::new(2, 1).unwrap(),
        Time::new(1, 2).unwrap(),
        Time::ZERO,
    ] {
        let a = prepared
            .sample_parameters(&[forward.id], &[width], time, &|| Ok(()))
            .unwrap();
        let b = prepared
            .sample_parameters(&[reverse.id], &[width], time, &|| Ok(()))
            .unwrap();
        assert_eq!(
            registry
                .sample_parameter(&document, &forward_target, time, &Default::default())
                .unwrap(),
            a[&width]
        );
        assert_eq!(
            registry
                .sample_parameter(&document, &reverse_target, time, &Default::default())
                .unwrap(),
            b[&width]
        );
        let Literal::Distance { value: a, .. } = a[&width] else {
            panic!()
        };
        let Literal::Distance { value: b, .. } = b[&width] else {
            panic!()
        };
        assert!((a + b - 300.).abs() < 1e-10);
    }
}
