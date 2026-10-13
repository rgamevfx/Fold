use fold_foundation::{DocumentId, ObjectId, Time};
use fold_graph::{
    binding::{Binding, Track},
    types::{Literal, Space, Unit},
};
use fold_media::VideoInfo;
use fold_project::Revision;
use fold_three_d::model::{Content, Object, Scene, Transform};

#[test]
fn native_scene_retains_shared_resources_exact_animation_hierarchy_and_extensions() {
    let id = DocumentId::new();
    let mut scene = Scene::new(VideoInfo {
        width: 640,
        height: 360,
        rate: [24000, 1001],
        frames: 240,
    });
    let instance = scene.roots[0];
    let group = ObjectId::new();
    let mut transform = Transform::default();
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
        value: vec![2., 0., 0.],
        unit: Unit::Meters,
        space: Space::World,
    };
    track.keys.push(key);
    transform.position.value = serde_json::to_value(Binding::Track { track }).unwrap();
    scene.roots[0] = group;
    scene.objects.insert(
        group,
        Object {
            name: "Group".into(),
            visible: true,
            protected: false,
            transform,
            content: Content::Group {
                children: vec![instance],
                extensions: Default::default(),
            },
            extensions: Default::default(),
        },
    );
    scene
        .extensions
        .insert("vendor.scene".into(), serde_json::json!({"future":true}));
    let document = scene.document(id).unwrap();
    let scene = Scene::from_document(&document).unwrap();
    assert_eq!(scene.document(id).unwrap().payload, document.payload);
    let time = Time::new(1, 2).unwrap();
    let (frame, binding) = scene
        .evaluate(id, Revision(9), time, [640, 360], &|| Ok(()))
        .unwrap();
    assert_eq!(frame.meshes.len(), 1);
    assert_eq!(frame.instances[0].id, instance);
    assert_eq!(
        frame.instances[0].transform.affine.unwrap().point([0.; 3]),
        [1., 0., 0.]
    );
    assert_eq!(binding.time, time);
    assert_eq!(binding.duration, Time::new(1001, 24000).unwrap());
    assert_eq!(binding.revision, 9);
    assert!(
        scene
            .evaluate(id, Revision(9), time, [640, 360], &|| Err(
                "Cancelled".into()
            ))
            .is_err()
    );
    let mut duplicate = scene.clone();
    duplicate.roots.push(instance);
    assert!(duplicate.validate().is_err());
    let mut hidden_invalid = scene.clone();
    let light = hidden_invalid
        .objects
        .values_mut()
        .find(|object| matches!(object.content, Content::DirectionalLight { .. }))
        .unwrap();
    light.visible = false;
    let Content::DirectionalLight { illuminance, .. } = &mut light.content else {
        unreachable!()
    };
    *illuminance = -1.;
    assert!(hidden_invalid.validate().is_err());
    let mut inactive_invalid = scene.clone();
    let mut camera = inactive_invalid.objects[&inactive_invalid.active_camera].clone();
    let new_camera = ObjectId::new();
    camera.visible = false;
    camera.transform = Transform::default();
    let Content::Camera { near, .. } = &mut camera.content else {
        unreachable!()
    };
    *near = -1.;
    inactive_invalid.roots.push(new_camera);
    inactive_invalid.objects.insert(new_camera, camera);
    assert!(inactive_invalid.validate().is_err());
    let mut cycle = scene;
    let Content::Group { children, .. } = &mut cycle.objects.get_mut(&group).unwrap().content
    else {
        unreachable!()
    };
    children.push(group);
    assert!(cycle.validate().is_err());
}
