use fold_foundation::{ObjectId, Time};
use fold_motion::{
    Motion,
    evaluation::{Evaluator, Value},
    fields::Datum,
    graph::{Input, Link, Node},
    program,
};
use serde_json::json;
const SOURCE: &str = r#"
let mut a = Asset::new("Moving circles");
let radius = a.scalar("radius", "Radius", 10., "px");
let speed = a.scalar("speed", "Speed", 20., "px/s");
let shape = a.circle("circle", radius);
let paint = a.fill("paint", &shape, [0.4, 0.1, 0.8, 1.]);
let moved = a.translate("move", &paint, vec2(time() * speed, 0.));
a.output(&moved);
a
"#;
fn make() -> (Motion, ObjectId, ObjectId) {
    let mut m = Motion::empty();
    let definition = ObjectId::new();
    let instance = ObjectId::new();
    program::define(&mut m, definition, SOURCE, &Default::default()).unwrap();
    program::instantiate(&mut m, definition, instance).unwrap();
    (m, definition, instance)
}
fn bounds(m: &Motion, instance: ObjectId, time: Time) -> [f64; 4] {
    let mut evaluator = Evaluator::new(m, time).unwrap();
    let Value::Content(c) = evaluator
        .resolve(&Link {
            node: instance,
            socket: "content".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    fold_motion::geometry::bounds::content(&c, &Default::default())
        .unwrap()
        .unwrap()
}
#[test]
fn real_rust_builds_native_animation_and_preserves_artist_edits_on_revision_and_reload() {
    let (mut m, definition, instance) = make();
    assert_eq!(bounds(&m, instance, Time::ZERO), [-10., -10., 10., 10.]);
    assert_eq!(
        bounds(&m, instance, Time::new(1, 2).unwrap()),
        [0., -10., 20., 10.]
    );
    // An artist-supplied driver remains connected when source is replaced.
    let mut driver = Node::new("fold.motion.expression").unwrap();
    driver.settings = json!({"expression":{"op":"literal","value":{"Scalar":20.}}});
    let driver_id = driver.id;
    m.graph.nodes.push(driver);
    let internal = &mut m.groups.get_mut(&definition).unwrap().graph.nodes[0];
    internal.position = [811., 409.];
    let layout_id = internal.id;
    let definition_key = serde_json::to_value(definition)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let metadata = m.extensions.get_mut(program::METADATA).unwrap();
    metadata[&definition_key]["future.metadata"] = json!({"preserve":true});
    let n = m.graph.nodes.iter_mut().find(|n| n.id == instance).unwrap();
    n.connect("speed", driver_id, "value");
    n.set("radius", Datum::Scalar(16.));
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 16.);
    curve.insert(Time::new(1, 1).unwrap(), 24.);
    n.animation.insert("radius.0".into(), curve);
    n.extensions
        .insert("future.artist".into(), json!({"keep":true}));
    n.position = [50., 80.];
    let artist = n.clone();
    let ids = m.groups[&definition]
        .graph
        .nodes
        .iter()
        .map(|n| n.id)
        .collect::<Vec<_>>();
    program::define(
        &mut m,
        definition,
        &SOURCE.replace("time() * speed", "time() * speed * 2."),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(m.graph.node(instance).unwrap(), &artist);
    assert_eq!(
        m.groups[&definition]
            .graph
            .node(layout_id)
            .unwrap()
            .position,
        [811., 409.]
    );
    assert_eq!(
        m.extensions[program::METADATA][&definition_key]["future.metadata"],
        json!({"preserve":true})
    );
    assert_eq!(
        ids,
        m.groups[&definition]
            .graph
            .nodes
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>()
    );
    let expected = bounds(&m, instance, Time::new(1, 1).unwrap());
    assert_eq!(expected, [16., -24., 64., 24.]);
    let restored = Motion::from_document(&m.document(Default::default()).unwrap()).unwrap();
    assert_eq!(
        bounds(&restored, instance, Time::new(1, 1).unwrap()),
        expected
    );
    assert!(program::record(&restored, definition).unwrap().is_some());
}
#[test]
fn interface_internal_edits_and_compiler_failures_do_not_mutate_artwork() {
    let (mut m, definition, _) = make();
    for source in [
        SOURCE.replace("\"radius\"", "\"diameter\""),
        SOURCE.replace("10.", "unknown"),
        "loop {}".into(),
        "let secret=include_str!(\"/etc/passwd\");".into(),
    ] {
        let before = m.clone();
        assert!(program::define(&mut m, definition, &source, &Default::default()).is_err());
        assert_eq!(m, before);
    }
    m.groups
        .get_mut(&definition)
        .unwrap()
        .graph
        .nodes
        .last_mut()
        .unwrap()
        .extensions
        .insert("artist".into(), json!(true));
    let before = m.clone();
    assert!(
        program::define(&mut m, definition, SOURCE, &Default::default())
            .unwrap_err()
            .contains("internals")
    );
    assert_eq!(m, before);
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(program::compiler::compile(SOURCE, &cancel).is_err());
}
#[test]
fn public_operation_is_atomic_and_runtime_wrangle_composes_with_text_and_controls() {
    let source = r#"
let mut a=Asset::new("Text cells");
let label=a.text_control("text","Text","A");
let amount=a.scalar("amount","Amount",12.,"px");
let text=a.text("text",label,100.);
let centered=a.align("center",&text,[0.,0.]);
let points=a.shape_points("points",&centered,12.);
let points=a.wrangle("offset",&points,"Points","position[1] += chf(\"amount\");",&[("amount",amount)]);
let dot=a.circle("dot",3.);
let copies=a.copies("copies",&dot,&points);
a.output(&copies);a
"#;
    let d = Motion::empty().document(Default::default()).unwrap();
    let definition = ObjectId::new();
    let instance = ObjectId::new();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    let next = registry.author(
        &d,
        &[json!({"op":"rust_asset","definition":definition,"instance":instance,"source":source})],
    )
    .unwrap();
    let mut m = Motion::from_document(&next).unwrap();
    let first = bounds(&m, instance, Time::ZERO);
    m.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == instance)
        .unwrap()
        .set("amount", Datum::Scalar(22.));
    let second = bounds(&m, instance, Time::ZERO);
    assert!((second[1] - first[1] - 10.).abs() < 1e-9);
    assert!(matches!(
        m.graph.node(instance).unwrap().inputs["text"],
        Input::Value(Datum::Text(_))
    ));
}

#[test]
fn source_defaults_apply_only_to_new_instances_and_new_controls() {
    let (mut m, definition, instance) = make();
    let revised = SOURCE.replace("10.,", "15.,").replace(
        "let shape =",
        "let _extra = a.scalar(\"extra\", \"Extra\", 7., \"\"); let shape =",
    );
    program::define(&mut m, definition, &revised, &Default::default()).unwrap();
    let new_instance = ObjectId::new();
    program::instantiate(&mut m, definition, new_instance).unwrap();
    assert_eq!(
        m.graph.node(instance).unwrap().inputs["radius"],
        Input::Value(Datum::Scalar(10.))
    );
    assert_eq!(
        m.graph.node(new_instance).unwrap().inputs["radius"],
        Input::Value(Datum::Scalar(15.))
    );
    for id in [instance, new_instance] {
        assert_eq!(
            m.graph.node(id).unwrap().inputs["extra"],
            Input::Value(Datum::Scalar(7.))
        );
    }
}

#[test]
fn retained_timing_fit_and_control_metadata_follow_edits() {
    let source = r#"let mut a=Asset::new("Fit");
let r=a.scalar_range("radius","Radius",20.,"px",[1.,100.]);
a.describe("radius","Geometry",false,"Circle radius before fitting");
let c=a.circle("circle",r); let c=a.fit("fit",&c,[20.,30.],[100.,200.]);
let c=a.translate("move",&c,vec2(progress(0.,1.).smooth()*100.,0.)); a.output(&c); a"#;
    let mut m = Motion::empty();
    let definition = ObjectId::new();
    let instance = ObjectId::new();
    program::define(&mut m, definition, source, &Default::default()).unwrap();
    program::instantiate(&mut m, definition, instance).unwrap();
    assert_eq!(m.groups[&definition].inputs[0].range, Some([1., 100.]));
    assert_eq!(bounds(&m, instance, Time::ZERO), [20., 80., 120., 180.]);
    assert_eq!(
        bounds(&m, instance, Time::new(1, 2).unwrap()),
        [70., 80., 170., 180.]
    );
    m.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == instance)
        .unwrap()
        .set("radius", Datum::Scalar(90.));
    assert_eq!(bounds(&m, instance, Time::ZERO), [20., 80., 120., 180.]);
    let looped = source.replace("progress(0.,1.).smooth()", "phase(2.)");
    program::define(&mut m, definition, &looped, &Default::default()).unwrap();
    assert_eq!(
        bounds(&m, instance, Time::ZERO),
        bounds(&m, instance, Time::new(2, 1).unwrap())
    );
    let before = m.clone();
    assert!(
        program::define(
            &mut m,
            definition,
            &source.replace("[1.,100.]", "[100.,1.]"),
            &Default::default()
        )
        .is_err()
    );
    assert_eq!(m, before);
}

#[test]
fn explicit_interface_migration_preserves_keys_and_rejects_ambiguous_changes() {
    let (mut m, def, instance) = make();
    let mut curve = fold_animation::Curve::default();
    let key = curve.insert(Time::ZERO, 42.);
    let n = m.graph.nodes.iter_mut().find(|n| n.id == instance).unwrap();
    n.set("speed", Datum::Scalar(42.));
    n.animation.insert("speed.0".into(), curve);
    let new = SOURCE.replace("\"speed\"", "\"velocity\"");
    let before = m.clone();
    assert!(program::define(&mut m, def, &new, &Default::default()).is_err());
    assert_eq!(m, before);
    program::define_with_migrations(
        &mut m,
        def,
        &new,
        &[program::Migration::Rename {
            from: "speed".into(),
            to: "velocity".into(),
        }],
        &Default::default(),
    )
    .unwrap();
    let n = m.graph.node(instance).unwrap();
    assert_eq!(n.inputs["velocity"], Input::Value(Datum::Scalar(42.)));
    assert_eq!(n.animation["velocity.0"].keys[0].id, key);
    assert!(!n.inputs.contains_key("speed"));
    assert_eq!(
        bounds(&m, instance, Time::new(1, 1).unwrap()),
        [32., -10., 52., 10.]
    );
    let removed = new.replace(
        "let radius = a.scalar(\"radius\", \"Radius\", 10., \"px\");",
        "let radius=10.;",
    );
    program::define_with_migrations(
        &mut m,
        def,
        &removed,
        &[program::Migration::Remove {
            input: "radius".into(),
        }],
        &Default::default(),
    )
    .unwrap();
    assert!(
        !m.graph
            .node(instance)
            .unwrap()
            .inputs
            .contains_key("radius")
    );
    let typed = removed
        .replace(
            "a.scalar(\"velocity\", \"Speed\", 20., \"px/s\")",
            "a.vector(\"velocity\", \"Offset\", [20.,0.], \"px\")",
        )
        .replace("vec2(time() * speed, 0.)", "speed");
    program::define_with_migrations(
        &mut m,
        def,
        &typed,
        &[program::Migration::Replace {
            from: "velocity".into(),
            to: "velocity".into(),
            value: Datum::Vector([8., 9.]),
        }],
        &Default::default(),
    )
    .unwrap();
    assert!(m.graph.node(instance).unwrap().animation.is_empty());
    assert_eq!(bounds(&m, instance, Time::ZERO), [-2., -1., 18., 19.]);
}

#[test]
fn portable_asset_preserves_source_fonts_and_conflicts_atomically() {
    let mut m = Motion::empty();
    let def = ObjectId::new();
    let instance = ObjectId::new();
    let source = "let mut a=Asset::new(\"Portable\"); let c=a.text(\"text\",\"Portable\",40.); a.output(&c); a";
    program::define(&mut m, def, source, &Default::default()).unwrap();
    program::instantiate(&mut m, def, instance).unwrap();
    let original = bounds(&m, instance, Time::ZERO);
    let mut bundle = program::bundle::export(&m, def).unwrap();
    bundle
        .extensions
        .insert("future.bundle".into(), json!({"keep":true}));
    assert!(!bundle.fonts.is_empty());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("portable.foldasset");
    program::bundle::write(&bundle, &path).unwrap();
    assert!(program::bundle::write(&bundle, &path).is_err());
    let decoded = program::bundle::read(&path).unwrap();
    let mut imported = Motion::empty();
    assert_eq!(
        program::bundle::import(&mut imported, &decoded).unwrap(),
        def
    );
    assert_eq!(
        program::bundle::export(&imported, def).unwrap().extensions,
        bundle.extensions
    );
    program::instantiate(&mut imported, def, instance).unwrap();
    assert_eq!(bounds(&imported, instance, Time::ZERO), original);
    assert_eq!(
        program::record(&imported, def).unwrap().unwrap().source,
        source
    );
    program::define(
        &mut imported,
        def,
        &source.replace("40.", "80."),
        &Default::default(),
    )
    .unwrap();
    let before = imported.clone();
    assert!(program::bundle::import(&mut imported, &decoded).is_err());
    assert_eq!(imported, before);
}

#[test]
fn large_native_assets_have_complete_bounded_control_evidence() {
    let mut m = Motion::empty();
    let def = ObjectId::new();
    let instance = ObjectId::new();
    let source = "let mut a=Asset::new(\"Copies\");let r=a.scalar(\"radius\",\"Radius\",3.,\"px\");let c=a.circle(\"c\",r);let p=a.grid(\"grid\",25.,20.,[10.,10.],[0.,0.]);let copies=a.copies(\"copies\",&c,&p);a.output(&copies);a";
    program::define(&mut m, def, source, &Default::default()).unwrap();
    program::instantiate(&mut m, def, instance).unwrap();
    let d = m.document(Default::default()).unwrap();
    let q = json!({"mode":"verify","check":"control_response","control":instance,"input":"radius","values":[{"Scalar":3.},{"Scalar":4.}],"target":{"node":instance,"output":"content"},"times":[{"numerator":0,"denominator":1},{"numerator":1,"denominator":1}]});
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    let r = registry.query(&d, &q).unwrap();
    assert_eq!(r["status"], "pass", "{r}");
    assert_eq!(r["samples"][0]["variants"][0]["truncated"], true);
    assert_eq!(
        r["samples"][0]["variants"][0]["fingerprint"]["complete"],
        true
    );
    assert_eq!(
        m.graph.node(instance).unwrap().inputs["radius"],
        Input::Value(Datum::Scalar(3.))
    );
}

#[test]
fn sdk_keyframes_use_exact_time_and_stagger_samples_instance_index() {
    let source = r#"let mut a=Asset::new("Timing");
let keys=a.keyframes("keys",&[(0,1,20.),(1,3,60.)],"Linear");
let c=a.add(Node::new("circle","fold.motion.ellipse").link("width",&keys).link("height",&keys));
let c=a.align("center",&c,[0.,0.]);
let p=a.grid("grid",2.,1.,[100.,0.],[0.,0.]);
let copies=a.copies("copies",&c,&p);
let moved=a.translate_each("move",&copies,vec2(stagger(0.,1.,0.5)*100.,0.));
a.output(&moved);a"#;
    let mut m = Motion::empty();
    let def = ObjectId::new();
    let instance = ObjectId::new();
    program::define(&mut m, def, source, &Default::default()).unwrap();
    program::instantiate(&mut m, def, instance).unwrap();
    let b = bounds(&m, instance, Time::new(1, 6).unwrap());
    assert!(
        (b[0] - (100. / 6. - 20.)).abs() < 1e-9 && (b[2] - 120.).abs() < 1e-9,
        "{b:?}"
    );
    assert!(
        (b[1] + 20.).abs() < 1e-9 && (b[3] - 20.).abs() < 1e-9,
        "{b:?}"
    );
    let bad = source.replace("(1,3,60.)", "(1,0,60.)");
    let before = m.clone();
    assert!(program::define(&mut m, def, &bad, &Default::default()).is_err());
    assert_eq!(m, before);
}
