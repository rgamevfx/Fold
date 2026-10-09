use fold_composition::{Composition, Layer};
use fold_foundation::{DocumentId, Time};
use fold_platform::{
    packages::PackageRegistry,
    parameters::{self, Literal, Unit},
};

#[test]
fn layer_properties_share_native_animation_and_survive_rename_reparent_and_reopen() {
    let mut registry = PackageRegistry::default();
    fold_composition::package::register(&mut registry).unwrap();
    let mut composition = Composition::default();
    let parent = Layer::new("Group", None, composition.info.clone()).unwrap();
    let mut layer = Layer::new("Layer / One", None, composition.info.clone()).unwrap();
    layer.parent = Some(parent.id);
    layer.offset[4] = 7.;
    layer.animation.insert(
        "transform.4".into(),
        fold_animation::Curve {
            keys: vec![
                fold_animation::Key::new(Time::ZERO, 10.),
                fold_animation::Key::new(Time::new(1, 1).unwrap(), 30.),
            ],
        },
    );
    layer.animation.insert(
        "scene.opacity.0".into(),
        fold_animation::Curve {
            keys: vec![
                fold_animation::Key::new(Time::ZERO, 0.25),
                fold_animation::Key::new(Time::new(1, 1).unwrap(), 0.75),
            ],
        },
    );
    let id = layer.id;
    composition.layers = vec![parent, layer];
    let document = composition.document(DocumentId::new()).unwrap();
    let entries = registry.parameters(&document).unwrap();
    let entry = entries
        .iter()
        .find(|e| {
            e.target.object == Some(id) && e.target.parameter.property() == Some("transform.tx")
        })
        .unwrap();
    assert_eq!(
        parameters::resolve(&entries, &[], &parameters::absolute_path(entry)).unwrap(),
        entry
    );
    let half = Time::new(1, 2).unwrap();
    assert_eq!(
        registry
            .sample_parameter(&document, &entry.target, half, &Default::default())
            .unwrap(),
        Literal::Distance {
            value: composition.layers[1].matrix(half)[4],
            unit: Unit::Canvas
        }
    );
    let opacity = entries
        .iter()
        .find(|e| e.target.object == Some(id) && e.target.parameter.property() == Some("opacity"))
        .unwrap();
    assert_eq!(
        registry
            .sample_parameter(&document, &opacity.target, half, &Default::default())
            .unwrap(),
        Literal::Scalar { value: 0.5 }
    );
    composition.layers[1].name = "Renamed".into();
    composition.layers[1].parent = None;
    composition.layers[1].locked = true;
    let changed = composition.document(document.id).unwrap();
    let reopened = serde_json::from_slice(&serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_eq!(
        registry
            .sample_parameter(&reopened, &entry.target, half, &Default::default())
            .unwrap(),
        Literal::Distance {
            value: 20.,
            unit: Unit::Canvas
        }
    );
    assert!(
        !registry
            .parameters(&reopened)
            .unwrap()
            .iter()
            .find(|e| e.target == entry.target)
            .unwrap()
            .editable
    );
    let mut bad = entry.target.clone();
    bad.parameter = parameters::Key::Property {
        name: "extensions.secret".into(),
    };
    assert!(
        registry
            .sample_parameter(&reopened, &bad, half, &Default::default())
            .is_err()
    );
    bad = entry.target.clone();
    bad.component = Some(0);
    assert!(
        registry
            .sample_parameter(&reopened, &bad, half, &Default::default())
            .is_err()
    );
}
