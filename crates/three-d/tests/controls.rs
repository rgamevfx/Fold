use fold_foundation::{DocumentId, ObjectId, Time};
use fold_graph::{
    binding::{Binding, Track},
    types::{Literal, Space, Unit},
};
use fold_media::{Cancel, VideoInfo};
use fold_platform::packages::PackageRegistry;
use fold_three_d::{model::Scene, package};

#[test]
fn shared_control_channels_edit_coupled_tracks_and_capture_static_pose() {
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let mut scene = Scene::new(VideoInfo {
        width: 640,
        height: 360,
        rate: [24000, 1001],
        frames: 240,
    });
    let object = scene.roots[0];
    let parameter = scene.objects[&object].transform.position.id;
    let mut track = Track::constant(
        Literal::Vector {
            value: vec![0.; 3],
            unit: Unit::Meters,
            space: Space::World,
        },
        Time::ZERO,
    )
    .unwrap();
    let mut key = track.keys[0].clone();
    key.id = ObjectId::new();
    key.time = Time::new(1, 1).unwrap();
    key.value = Literal::Vector {
        value: vec![2., 4., 6.],
        unit: Unit::Meters,
        space: Space::World,
    };
    track.keys.push(key);
    let key_ids = track.keys.iter().map(|key| key.id).collect::<Vec<_>>();
    scene
        .objects
        .get_mut(&object)
        .unwrap()
        .transform
        .position
        .value = serde_json::to_value(Binding::Track { track }).unwrap();
    let id = DocumentId::new();
    let mut document = scene.document(id).unwrap();
    document
        .extensions
        .insert("vendor.control-document".into(), serde_json::json!(true));
    let catalog = registry.parameters(&document).unwrap();
    let x = catalog
        .iter()
        .find(|entry| {
            entry.target.object == Some(object)
                && entry.target.parameter.native() == Some(parameter)
                && entry.target.component == Some(0)
        })
        .unwrap();
    assert!(x.editable);
    assert_eq!(x.path, vec!["Box", "Position", "x"]);
    let time = Time::new(1, 2).unwrap();
    assert_eq!(
        registry
            .sample_parameter(&document, &x.target, time, &Cancel::default())
            .unwrap(),
        Literal::Distance {
            value: 1.,
            unit: Unit::Meters
        }
    );
    let mut channels = registry
        .animation_channels(&document, Some(object))
        .unwrap();
    assert_eq!(channels.len(), 3);
    for channel in &channels {
        assert_eq!(
            channel
                .curve
                .keys
                .iter()
                .map(|key| key.id)
                .collect::<Vec<_>>(),
            key_ids
        );
    }
    let channel = channels
        .iter_mut()
        .find(|channel| channel.component == "X")
        .unwrap();
    channel.curve.keys[0].value = 1.;
    let edited = registry
        .edit_animation(&document, &[channel.clone()], time)
        .unwrap();
    assert_eq!(edited.extensions, document.extensions);
    assert_eq!(
        registry
            .sample_parameter(&edited, &x.target, time, &Cancel::default())
            .unwrap(),
        Literal::Distance {
            value: 1.5,
            unit: Unit::Meters
        }
    );
    let after = Scene::from_document(&edited).unwrap();
    let Binding::Track { track } = after.objects[&object].transform.position.binding().unwrap()
    else {
        unreachable!()
    };
    assert_eq!(
        track.keys.iter().map(|key| key.id).collect::<Vec<_>>(),
        key_ids
    );
    assert_eq!(
        track.sample(time).unwrap(),
        Literal::Vector {
            value: vec![1.5, 2., 3.],
            unit: Unit::Meters,
            space: Space::World
        }
    );
    let mut protected = after.clone();
    protected.objects.get_mut(&object).unwrap().protected = true;
    let protected = protected.document(id).unwrap();
    assert!(
        !registry
            .parameters(&protected)
            .unwrap()
            .iter()
            .find(|entry| entry.target == x.target)
            .unwrap()
            .editable
    );
    let mut next = channel.clone();
    next.curve.keys[0].value = 3.;
    assert!(
        registry
            .edit_animation(&protected, &[next.clone()], time)
            .is_err()
    );
    assert!(
        registry
            .edit_animation(&edited, &[next.clone(), next], time)
            .is_err()
    );
    let mut cleared = registry.animation_channels(&edited, Some(object)).unwrap();
    for channel in &mut cleared {
        channel.curve.keys.clear();
    }
    let static_document = registry.edit_animation(&edited, &cleared, time).unwrap();
    assert!(
        registry
            .animation_channels(&static_document, Some(object))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        registry
            .sample_parameter(
                &static_document,
                &x.target,
                Time::new(1, 1).unwrap(),
                &Cancel::default()
            )
            .unwrap(),
        Literal::Distance {
            value: 1.5,
            unit: Unit::Meters
        }
    );
}
