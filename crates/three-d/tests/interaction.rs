use fold_foundation::Time;
use fold_graph::{
    binding::Binding,
    types::{Literal, Space, Unit},
};
use fold_three_d::{
    editing,
    interaction::{View, world},
    model::{Content, Projection, Scene},
    tools,
};

#[test]
fn camera_picking_and_world_movement_match_evaluated_parented_geometry() {
    let mut scene = Scene::new(fold_media::VideoInfo {
        width: 640,
        height: 360,
        rate: [30, 1],
        frames: 150,
    });
    let object = scene.roots[0];
    let (near, edits) = tools::duplicate(&scene, object).unwrap();
    scene = editing::apply(&scene, &edits, &Default::default()).unwrap();
    scene = editing::apply(
        &scene,
        &tools::translate_world(&scene, near, Time::ZERO, [0., 0., 2.], false).unwrap(),
        &Default::default(),
    )
    .unwrap();
    for projection in [
        Projection::Perspective { vertical_fov: 1. },
        Projection::Orthographic { vertical_size: 4. },
    ] {
        if let Content::Camera {
            projection: value, ..
        } = &mut scene.objects.get_mut(&scene.active_camera).unwrap().content
        {
            *value = projection;
        }
        let view = View::new(&scene, Time::ZERO, [640, 360]).unwrap();
        assert_eq!(
            view.pick(&scene, Time::ZERO, [0.5, 0.5]).unwrap(),
            Some(near)
        );
        assert_eq!(view.pick(&scene, Time::ZERO, [0., 0.]).unwrap(), None);
        scene.objects.get_mut(&near).unwrap().visible = false;
        assert_eq!(
            view.pick(&scene, Time::ZERO, [0.5, 0.5]).unwrap(),
            Some(object)
        );
        scene.objects.get_mut(&near).unwrap().visible = true;
        let origin = [0., 0., 2.];
        let shifted = [0.25, 0., 2.];
        let pointer = view.project(shifted).unwrap();
        let amount = view.axis(pointer, origin, [1., 0., 0.]).unwrap();
        assert!((amount - 0.25).abs() < 1e-10);
        let point = view.plane(pointer, origin).unwrap();
        for (a, b) in point.into_iter().zip(shifted) {
            assert!((a - b).abs() < 1e-10);
        }
        assert!(view.axis([0.5, 0.5], origin, [0., 0., 1.]).is_none());
    }
    let (group, edits) = tools::group(&scene, &[object]).unwrap();
    scene = editing::apply(&scene, &edits, &Default::default()).unwrap();
    let parent = scene.objects.get_mut(&group).unwrap();
    parent.transform.scale.value = serde_json::to_value(Binding::Literal {
        value: Literal::Vector {
            value: vec![2., 3., 4.],
            unit: Unit::Unitless,
            space: Space::World,
        },
    })
    .unwrap();
    parent.transform.rotation.value = serde_json::to_value(Binding::Literal {
        value: Literal::euler(
            [0., 0., 0.7],
            fold_graph::types::EulerOrder::Xyz,
            Space::World,
        )
        .unwrap(),
    })
    .unwrap();
    let time = Time::new(1, 2).unwrap();
    let position = world(&scene, object, time).unwrap().point([0.; 3]);
    let edits = tools::translate_world(&scene, object, time, [1., 2., 3.], true).unwrap();
    let moved = editing::apply(&scene, &edits, &Default::default()).unwrap();
    let after = world(&moved, object, time).unwrap().point([0.; 3]);
    for i in 0..3 {
        assert!((after[i] - position[i] - (i + 1) as f64).abs() < 1e-10);
    }
    assert_eq!(
        world(&moved, object, Time::ZERO).unwrap(),
        world(&scene, object, Time::ZERO).unwrap()
    );
    assert_eq!(
        moved.objects[&object].transform.position.id,
        scene.objects[&object].transform.position.id
    );
    let mut locked = scene.clone();
    locked
        .objects
        .get_mut(&object)
        .unwrap()
        .transform
        .position
        .protected = true;
    assert!(tools::translate_world(&locked, object, time, [1., 0., 0.], false).is_err());
}

#[test]
fn local_rotation_scale_and_ring_projection_preserve_parent_and_keys() {
    let mut scene = Scene::new(fold_media::VideoInfo {
        width: 640,
        height: 360,
        rate: [30, 1],
        frames: 150,
    });
    let object = scene.roots[0];
    let (group, edits) = tools::group(&scene, &[object]).unwrap();
    scene = editing::apply(&scene, &edits, &Default::default()).unwrap();
    let edits = tools::scale_local(&scene, group, Time::ZERO, [2., 3., 1.], false).unwrap();
    scene = editing::apply(&scene, &edits, &Default::default()).unwrap();
    let time = Time::new(1, 2).unwrap();
    let matrix = world(&scene, object, time).unwrap();
    let view = View::new(&scene, time, [640, 360]).unwrap();
    let local = [0.3, 0.4, 0.];
    let point = view
        .local_plane(view.project(matrix.point(local)).unwrap(), matrix, 2)
        .unwrap();
    for (a, b) in point.into_iter().zip(local) {
        assert!((a - b).abs() < 1e-10);
    }
    assert!(view.local_plane([0.5, 0.5], matrix, 3).is_none());
    let edits =
        tools::rotate_local(&scene, object, time, 2, std::f64::consts::FRAC_PI_2, true).unwrap();
    let rotated = editing::apply(&scene, &edits, &Default::default()).unwrap();
    let point = world(&rotated, object, time).unwrap().point([1., 0., 0.]);
    assert!(point[0].abs() < 1e-10 && (point[1] - 3.).abs() < 1e-10);
    assert_eq!(world(&rotated, object, Time::ZERO).unwrap(), matrix);
    assert_eq!(rotated.objects[&group], scene.objects[&group]);
    let edits = tools::scale_local(&rotated, object, time, [2., 1., 1.], true).unwrap();
    let scaled = editing::apply(&rotated, &edits, &Default::default()).unwrap();
    let point = world(&scaled, object, time).unwrap().point([1., 0., 0.]);
    assert!(point[0].abs() < 1e-10 && (point[1] - 6.).abs() < 1e-10);
    assert_eq!(world(&scaled, object, Time::ZERO).unwrap(), matrix);
    assert_eq!(
        scaled.objects[&object].transform.rotation.id,
        scene.objects[&object].transform.rotation.id
    );
    assert!(tools::scale_local(&scene, object, time, [0., 1., 1.], false).is_err());
    scene
        .objects
        .get_mut(&object)
        .unwrap()
        .transform
        .rotation
        .protected = true;
    assert!(tools::rotate_local(&scene, object, time, 2, 1., false).is_err());
}
