use fold_foundation::{DocumentId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{Limits, Prepared, Value},
    model::{Construction, Reference},
    ramp::Ramp,
    registry, source,
    types::{Alpha, Encoding, Literal},
};
fn scalar(value: f64) -> Literal {
    Literal::Scalar { value }
}
fn fixture(ramp: Ramp, position: f64) -> Construction {
    let mut c =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    let mode = if ramp.ty == fold_motion::types::Type::Scalar {
        "scalar_linear"
    } else {
        "color_linear"
    };
    let node = registry::resolve("fold.motion.ramp", 1, mode)
        .unwrap()
        .node(
            "Response",
            [
                ("stops".into(), Binding::Ramp { ramp: ramp.clone() }),
                (
                    "position".into(),
                    Binding::Literal {
                        value: scalar(position),
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    c.definitions[0].outputs[0].ty = ramp.ty;
    c.definitions[0].outputs[0].value = Reference::Node { node: node.id }.into();
    c.definitions[0].nodes = vec![node];
    c
}
fn edit(c: &mut Construction, f: impl FnOnce(&mut Ramp)) {
    let parameter = c.definitions[0].nodes[0]
        .parameters
        .get_mut("stops")
        .unwrap();
    let Binding::Ramp { mut ramp } = parameter.binding().unwrap() else {
        panic!()
    };
    f(&mut ramp);
    parameter.value = serde_json::to_value(Binding::Ramp { ramp }).unwrap();
}
#[test]
fn scalar_color_sampling_clamps_and_preserves_stop_identity_and_extensions() {
    let mut ramp = Ramp::two(scalar(0.2), scalar(1.)).unwrap();
    ramp.extensions
        .insert("vendor".into(), serde_json::json!({"future":true}));
    ramp.stops[0]
        .extensions
        .insert("note".into(), serde_json::json!("keep"));
    let original = ramp.stops.iter().map(|s| s.id).collect::<Vec<_>>();
    let inserted = ramp.insert(0.25).unwrap();
    assert_eq!(ramp.stops[1].id, inserted);
    assert_eq!(ramp.stops[0].id, original[0]);
    assert_eq!(ramp.stops[2].id, original[1]);
    for (position, expected) in [
        (-1., 0.2),
        (0., 0.2),
        (0.25, 0.4),
        (0.5, 0.6),
        (1., 1.),
        (2., 1.),
    ] {
        let Literal::Scalar { value } = ramp.sample(position).unwrap() else {
            panic!()
        };
        assert!((value - expected).abs() < 1e-12);
        let c = fixture(ramp.clone(), position);
        let parsed = source::parse(&source::canonical(&c).unwrap()).unwrap();
        assert_eq!(parsed.construction, c);
        let Value::Primitive(value) = Prepared::new(c.clone())
            .unwrap()
            .evaluate(
                c.definitions[0].outputs[0].id,
                Time::ZERO,
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(value, ramp.sample(position).unwrap());
    }
    let color = |value| Literal::Color {
        value,
        encoding: Encoding::LinearRec709,
        alpha: Alpha::Straight,
    };
    let colors = Ramp::two(color([2., 0., 0., 0.25]), color([0., 0., 4., 0.75])).unwrap();
    assert_eq!(colors.sample(0.5).unwrap(), color([1., 0., 2., 0.5]));
    let c = fixture(colors.clone(), 0.5);
    assert_eq!(
        source::parse(&source::canonical(&c).unwrap())
            .unwrap()
            .construction,
        c
    );
    let Value::Primitive(value) = Prepared::new(c.clone())
        .unwrap()
        .evaluate(
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(value, colors.sample(0.5).unwrap());
    assert!(ramp.insert(0.25).is_err());
    assert!(ramp.sample(f64::NAN).is_err());
    let mut bad = ramp.clone();
    bad.stops[1].position = bad.stops[0].position;
    assert!(bad.validate().is_err());
    let mut bad = ramp.clone();
    bad.stops[1].id = bad.stops[0].id;
    assert!(bad.validate().is_err());
    let mut bad = ramp;
    bad.stops[1].value = color([0.; 4]);
    assert!(bad.validate().is_err());
}
#[test]
fn ramp_stop_merge_conflicts_protection_and_reconciliation_follow_native_ids() {
    use fold_motion::{merge, protection, reconcile};
    let base = fixture(Ramp::two(scalar(0.2), scalar(1.)).unwrap(), 0.5);
    let mut artist = base.clone();
    edit(&mut artist, |r| r.stops[0].value = scalar(0.3));
    let mut agent = base.clone();
    edit(&mut agent, |r| r.stops[1].value = scalar(0.9));
    let merged = merge::construction(&base, &agent, &artist).unwrap();
    let Binding::Ramp { ramp } = merged.definitions[0].nodes[0].parameters["stops"]
        .binding()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(ramp.stops[0].value, scalar(0.3));
    assert_eq!(ramp.stops[1].value, scalar(0.9));
    let mut conflict = base.clone();
    edit(&mut conflict, |r| r.stops[0].value = scalar(0.4));
    assert!(merge::construction(&base, &conflict, &artist).is_err());
    let mut locked = base.clone();
    locked.definitions[0].nodes[0]
        .parameters
        .get_mut("stops")
        .unwrap()
        .protected = true;
    let mut changed = locked.clone();
    edit(&mut changed, |r| r.stops[0].value = scalar(0.5));
    assert!(protection::validate(&locked, &changed).is_err());
    let ids = reconcile::identities(&base.definitions[0]).unwrap();
    assert_eq!(
        ids.values()
            .filter(|v| **v == reconcile::Kind::Ramp)
            .count(),
        1
    );
    assert_eq!(
        ids.values()
            .filter(|v| **v == reconcile::Kind::Stop)
            .count(),
        2
    );
    let Binding::Ramp { ramp: original } = base.definitions[0].nodes[0].parameters["stops"]
        .binding()
        .unwrap()
    else {
        panic!()
    };
    let fresh = Ramp::two(scalar(0.2), scalar(1.)).unwrap();
    let mapping = [
        (fresh.id, original.id),
        (fresh.stops[0].id, original.stops[0].id),
        (fresh.stops[1].id, original.stops[1].id),
    ]
    .into();
    let mut proposed = base.clone();
    edit(&mut proposed, |r| *r = fresh);
    let remapped =
        reconcile::definition(&base.definitions[0], &proposed.definitions[0], &mapping).unwrap();
    assert_eq!(remapped, base.definitions[0]);
}

#[test]
fn ramp_edits_commit_undo_redo_and_reopen_without_losing_stop_ids() {
    use fold_motion::editing;
    use fold_project::{EditBatch, Mutation, Project};
    let id = DocumentId::new();
    let base = fixture(Ramp::two(scalar(0.2), scalar(1.)).unwrap(), 0.5);
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(base.document(id).unwrap())],
        })
        .unwrap();
    let mut changed = base.clone();
    edit(&mut changed, |r| {
        r.insert(0.375).unwrap();
        r.stops[1].value = scalar(0.9);
    });
    editing::prepare(
        &project.snapshot(),
        id,
        editing::Scope::Document,
        changed.clone(),
    )
    .unwrap()
    .apply(&mut project)
    .unwrap();
    let read =
        |p: &Project| Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    assert_eq!(read(&project), changed);
    project.undo().unwrap();
    assert_eq!(read(&project), base);
    project.redo().unwrap();
    assert_eq!(read(&project), changed);
    let reopened = Construction::from_document(
        &serde_json::from_str(&serde_json::to_string(&changed.document(id).unwrap()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        source::canonical(&reopened).unwrap(),
        source::canonical(&changed).unwrap()
    );
}
