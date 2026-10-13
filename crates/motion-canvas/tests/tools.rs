use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::{
    editing,
    model::*,
    tools::{self, Part},
};

fn cubic(a: &Point, b: &Point, t: f64) -> [f64; 2] {
    let p = [
        a.position,
        [a.position[0] + a.outgoing[0], a.position[1] + a.outgoing[1]],
        [b.position[0] + b.incoming[0], b.position[1] + b.incoming[1]],
        b.position,
    ];
    let weights = [
        (1. - t).powi(3),
        3. * (1. - t).powi(2) * t,
        3. * (1. - t) * t * t,
        t.powi(3),
    ];
    std::array::from_fn(|axis| (0..4).map(|i| p[i][axis] * weights[i]).sum())
}

#[test]
fn captured_affine_transforms_preserve_shear_reflection_and_singular_geometry() {
    for matrix in [
        [1., 0.4, 0.8, 2., 12., -7.],
        [-2., 1., 3., 0.5, 12., -7.],
        [2., 3., 4., 6., 12., -7.],
        [0., 0., 3., 4., 12., -7.],
        [0., 0., 0., 0., 12., -7.],
    ] {
        let captured = fold_motion_canvas::model::Transform::from_affine(matrix).unwrap();
        let evaluated = tools::Transform(captured.affine(Time::ZERO).unwrap());
        let expected = tools::Transform(matrix);
        for point in [[0., 0.], [12., -3.], [-7., 20.]] {
            for (actual, expected) in evaluated
                .point(point)
                .into_iter()
                .zip(expected.point(point))
            {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        let object = Object {
            transform: captured,
            ..tools::new_object("rectangle").unwrap()
        };
        let id = ObjectId::new();
        let canvas = Canvas {
            roots: vec![id],
            objects: [(id, object)].into(),
            ..Default::default()
        };
        canvas.validate().unwrap();
        assert_eq!(
            tools::transforms(&canvas, Time::ZERO).unwrap()[&id],
            evaluated
        );
    }
    assert!(fold_motion_canvas::model::Transform::from_affine([f64::NAN; 6]).is_err());
}

#[test]
fn vector_tools_keep_curves_exact_and_use_animated_group_coordinates() {
    let a = Point {
        position_track: None,
        incoming_track: None,
        outgoing_track: None,
        id: ObjectId::new(),
        position: [0., 0.],
        incoming: [-8., 3.],
        outgoing: [20., 40.],
        extensions: [("vendor.anchor".into(), serde_json::json!({"keep":true}))].into(),
    };
    let b = Point {
        position_track: None,
        incoming_track: None,
        outgoing_track: None,
        id: ObjectId::new(),
        position: [100., 10.],
        incoming: [-20., 30.],
        outgoing: [8., -3.],
        extensions: Default::default(),
    };
    let contour = ObjectId::new();
    let path = ObjectId::new();
    let group = ObjectId::new();
    let object = Object::new(
        "Path",
        Content::Path {
            contours: vec![Contour {
                id: contour,
                points: vec![a.clone(), b.clone()],
                closed: true,
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        },
    );
    let mut parent = Object::new(
        "Group",
        Content::Group {
            children: vec![path],
            extensions: Default::default(),
        },
    );
    parent.transform.x.value = 120.;
    parent.transform.y.value = 50.;
    parent.transform.rotation.value = 90.;
    tools::set_scalar(&mut parent.transform.scale_x, Time::ZERO, 1., true);
    tools::set_scalar(
        &mut parent.transform.scale_x,
        Time::new(1, 1).unwrap(),
        3.,
        true,
    );
    let canvas = Canvas {
        roots: vec![group],
        objects: [(path, object), (group, parent)].into(),
        ..Default::default()
    };
    let time = Time::new(1, 2).unwrap();

    let (inserted, split) =
        tools::split_segment(&canvas, path, contour, a.id, 0.3, Time::ZERO).unwrap();
    let split_canvas = editing::apply(&canvas, &[split]).unwrap();
    let Content::Path { contours, .. } = &split_canvas.objects[&path].content else {
        unreachable!()
    };
    let points = &contours[0].points;
    assert_eq!(
        (points[0].id, points[1].id, points[2].id),
        (a.id, inserted, b.id)
    );
    assert_eq!(points[0].extensions, a.extensions);
    for i in 0..101 {
        let t = f64::from(i) / 100.;
        let original = cubic(&a, &b, t);
        let split = if t <= 0.3 {
            cubic(&points[0], &points[1], t / 0.3)
        } else {
            cubic(&points[1], &points[2], (t - 0.3) / 0.7)
        };
        for axis in 0..2 {
            assert!((original[axis] - split[axis]).abs() < 1e-10);
        }
    }
    // The closing edge is a real segment, including its handles on the first anchor.
    let (_, closing) =
        tools::split_segment(&split_canvas, path, contour, b.id, 0.5, Time::ZERO).unwrap();
    let closed = editing::apply(&split_canvas, &[closing]).unwrap();
    assert!(
        matches!(&closed.objects[&path].content, Content::Path { contours, .. } if contours[0].points.len() == 4 && contours[0].closed)
    );

    let world = tools::transforms(&split_canvas, time).unwrap();
    let target = world[&path].point([9., 11.]);
    let movement = tools::move_point(
        &split_canvas,
        path,
        a.id,
        Part::Anchor,
        target,
        time,
        true,
        false,
    )
    .unwrap();
    let moved = editing::apply(&split_canvas, &[movement]).unwrap();
    let Content::Path { contours, .. } = &moved.objects[&path].content else {
        unreachable!()
    };
    let moved_point = &contours[0].points[0];
    assert!(
        (moved_point.position[0] - 9.).abs() < 1e-10
            && (moved_point.position[1] - 11.).abs() < 1e-10
    );
    assert_eq!(moved_point.incoming, points[0].incoming);
    assert_eq!(moved_point.outgoing, points[0].outgoing);

    let handle_target = world[&path].point([15., 16.]);
    let handle = tools::move_point(
        &moved,
        path,
        a.id,
        Part::Outgoing,
        handle_target,
        time,
        true,
        false,
    )
    .unwrap();
    let handled = editing::apply(&moved, &[handle]).unwrap();
    let Content::Path { contours, .. } = &handled.objects[&path].content else {
        unreachable!()
    };
    assert!((contours[0].points[0].outgoing[0] - 6.).abs() < 1e-10);
    assert!((contours[0].points[0].incoming[0] + 6.).abs() < 1e-10);

    let edits = tools::translate(&canvas, &[path], time, [10., 20.], false).unwrap();
    let translated = editing::apply(&canvas, &edits).unwrap();
    let before = tools::transforms(&canvas, time).unwrap()[&path].point([0., 0.]);
    let after = tools::transforms(&translated, time).unwrap()[&path].point([0., 0.]);
    assert!(
        (after[0] - before[0] - 10.).abs() < 1e-10 && (after[1] - before[1] - 20.).abs() < 1e-10
    );
    let edits = tools::translate(&canvas, &[group, path], time, [10., 20.], false).unwrap();
    assert_eq!(edits.len(), 2);
    let translated = editing::apply(&canvas, &edits).unwrap();
    assert_eq!(translated.objects[&path], canvas.objects[&path]);

    let mut singular = canvas.clone();
    singular
        .objects
        .get_mut(&group)
        .unwrap()
        .transform
        .scale_y
        .value = 0.;
    assert!(
        tools::move_point(
            &singular,
            path,
            a.id,
            Part::Anchor,
            target,
            time,
            true,
            false
        )
        .is_err()
    );
    canvas.validate().unwrap();
}

#[test]
fn grouping_and_drawing_order_retain_animated_objects_and_one_undo() {
    use fold_foundation::DocumentId;
    use fold_motion_canvas::hierarchy;
    use fold_project::{EditBatch, Mutation, Project};
    let parent = ObjectId::new();
    let children: Vec<_> = (0..4).map(|_| ObjectId::new()).collect();
    let mut canvas = Canvas::default();
    let mut group = tools::new_object("group").unwrap();
    group.content = Content::Group {
        children: children.clone(),
        extensions: Default::default(),
    };
    tools::set_scalar(&mut group.transform.x, Time::ZERO, 50., true);
    tools::set_scalar(&mut group.transform.x, Time::new(1, 1).unwrap(), 80., true);
    group.transform.rotation.value = 30.;
    canvas.roots.push(parent);
    canvas.objects.insert(parent, group);
    for &id in &children {
        let mut object = tools::new_object("rectangle").unwrap();
        tools::set_scalar(&mut object.transform.y, Time::ZERO, 10., true);
        tools::set_scalar(&mut object.transform.y, Time::new(1, 1).unwrap(), 30., true);
        object
            .extensions
            .insert("vendor.keep".into(), serde_json::json!([1, 2, 3]));
        canvas.objects.insert(id, object);
    }
    let (id, edits) = hierarchy::group(&canvas, &[children[2], children[0]]).unwrap();
    let after = editing::apply(&canvas, &edits).unwrap();
    assert_eq!(
        hierarchy::children(&after, Some(parent)).unwrap(),
        &[children[1], id, children[3]]
    );
    assert_eq!(
        hierarchy::children(&after, Some(id)).unwrap(),
        &[children[0], children[2]]
    );
    for time in [
        Time::ZERO,
        Time::new(1, 2).unwrap(),
        Time::new(1, 1).unwrap(),
    ] {
        let before_transforms = tools::transforms(&canvas, time).unwrap();
        let after_transforms = tools::transforms(&after, time).unwrap();
        for &child in &children {
            assert_eq!(before_transforms[&child], after_transforms[&child]);
            assert_eq!(canvas.objects[&child], after.objects[&child]);
        }
    }
    let reorder = hierarchy::arrange(&after, &[children[0]], hierarchy::Order::Front).unwrap();
    let reordered = editing::apply(&after, &reorder).unwrap();
    assert_eq!(
        hierarchy::children(&reordered, Some(id)).unwrap(),
        &[children[2], children[0]]
    );
    assert!(hierarchy::group(&after, &[children[0], children[1]]).is_err());
    let mut locked = canvas.clone();
    locked.objects.get_mut(&children[0]).unwrap().protected = true;
    assert!(hierarchy::group(&locked, &[children[0], children[2]]).is_err());
    assert!(hierarchy::arrange(&locked, &[children[0]], hierarchy::Order::Front).is_err());
    for (order, expected) in [
        (
            hierarchy::Order::Forward,
            vec![children[1], children[0], children[3], children[2]],
        ),
        (
            hierarchy::Order::Front,
            vec![children[1], children[3], children[0], children[2]],
        ),
        (
            hierarchy::Order::Backward,
            vec![children[0], children[2], children[1], children[3]],
        ),
        (
            hierarchy::Order::Back,
            vec![children[0], children[2], children[1], children[3]],
        ),
    ] {
        let mut arranged = canvas.clone();
        arranged.objects.get_mut(&children[1]).unwrap().protected = true;
        let edits = hierarchy::arrange(&arranged, &[children[2], children[0]], order).unwrap();
        let arranged = editing::apply(&arranged, &edits).unwrap();
        assert_eq!(
            hierarchy::children(&arranged, Some(parent)).unwrap(),
            expected
        );
    }
    let document = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(canvas.document(document).unwrap())],
        })
        .unwrap();
    project
        .commit(editing::stage(&project.snapshot(), document, &edits).unwrap())
        .unwrap();
    assert_eq!(
        Canvas::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        after
    );
    project.undo().unwrap();
    assert_eq!(
        Canvas::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        canvas
    );
}

#[test]
fn animated_anchor_edits_share_sampling_and_retain_key_identity() {
    use fold_graph::{
        binding::{Interpolation, Key, Track},
        types::{Literal, Space, Unit},
    };
    let vector = |p: [f64; 2]| Literal::Vector {
        value: p.to_vec(),
        unit: Unit::Canvas,
        space: Space::Screen,
    };
    let id = ObjectId::new();
    let point_id = ObjectId::new();
    let mut track = Track::constant(vector([0., 0.]), Time::ZERO).unwrap();
    track
        .extensions
        .insert("vendor.track".into(), serde_json::json!(true));
    track.keys.push(Key {
        id: ObjectId::new(),
        time: Time::new(1, 1).unwrap(),
        value: vector([100., 40.]),
        interpolation: Interpolation::Linear,
        handles: vec![],
        extensions: Default::default(),
    });
    let point = Point {
        id: point_id,
        position: [3., 4.],
        position_track: Some(track.clone()),
        incoming_track: None,
        outgoing_track: None,
        incoming: [-2., -3.],
        outgoing: [2., 3.],
        extensions: Default::default(),
    };
    let contour = Contour {
        id: ObjectId::new(),
        closed: false,
        points: vec![point.clone()],
        extensions: Default::default(),
    };
    let time = Time::new(1, 2).unwrap();
    assert_eq!(point.position_at(time).unwrap(), [50., 20.]);
    assert_eq!(contour.at(time).unwrap().points[0].position, [50., 20.]);
    let canvas = Canvas {
        roots: vec![id],
        objects: [(
            id,
            Object::new(
                "Animated path",
                Content::Path {
                    contours: vec![contour],
                    extensions: Default::default(),
                },
            ),
        )]
        .into(),
        ..Default::default()
    };
    let mut static_canvas = canvas.clone();
    let Content::Path { contours, .. } = &mut static_canvas.objects.get_mut(&id).unwrap().content
    else {
        unreachable!()
    };
    contours[0].points[0].position_track = None;
    let created = tools::move_point(
        &static_canvas,
        id,
        point_id,
        Part::Anchor,
        [7., 8.],
        time,
        false,
        true,
    )
    .unwrap();
    let keyed = editing::apply(&static_canvas, &[created]).unwrap();
    let Content::Path { contours, .. } = &keyed.objects[&id].content else {
        unreachable!()
    };
    assert_eq!(contours[0].points[0].position_at(time).unwrap(), [7., 8.]);
    assert!(contours[0].points[0].position_track.is_some());
    let edit = tools::move_point(
        &canvas,
        id,
        point_id,
        Part::Anchor,
        [80., 60.],
        time,
        false,
        false,
    )
    .unwrap();
    let moved = editing::apply(&canvas, &[edit]).unwrap();
    let command = editing::Edit::MovePoint {
        object: id,
        expected: Box::new(canvas.objects[&id].clone()),
        point: point_id,
        part: Part::Anchor,
        position: [80., 60.],
        time: Time::ZERO,
        linked: false,
        auto_key: false,
    };
    let encoded = serde_json::to_value(&command).unwrap();
    assert_eq!(encoded["op"], "move_point");
    let decoded = serde_json::from_value(encoded).unwrap();
    let direct = tools::move_point(
        &canvas,
        id,
        point_id,
        Part::Anchor,
        [80., 60.],
        Time::ZERO,
        false,
        false,
    )
    .unwrap();
    let semantic = editing::apply(&canvas, &[decoded]).unwrap();
    assert_eq!(semantic, editing::apply(&canvas, &[direct]).unwrap());
    assert!(editing::apply(&semantic, &[command]).is_err());
    let serialized = moved.document(fold_foundation::DocumentId::new()).unwrap();
    assert_eq!(Canvas::from_document(&serialized).unwrap(), moved);
    let Content::Path { contours, .. } = &moved.objects[&id].content else {
        unreachable!()
    };
    let updated = &contours[0].points[0];
    assert_eq!(updated.position_at(time).unwrap(), [80., 60.]);
    assert_eq!(updated.position_track.as_ref().unwrap().id, track.id);
    assert_eq!(
        updated.position_track.as_ref().unwrap().keys[0].id,
        track.keys[0].id
    );
    assert_eq!(
        updated.position_track.as_ref().unwrap().extensions,
        track.extensions
    );
    let edit = tools::move_point(
        &moved,
        id,
        point_id,
        Part::Outgoing,
        [90., 80.],
        time,
        false,
        false,
    )
    .unwrap();
    let handled = editing::apply(&moved, &[edit]).unwrap();
    let Content::Path { contours, .. } = &handled.objects[&id].content else {
        unreachable!()
    };
    assert_eq!(contours[0].points[0].outgoing, [10., 20.]);
    let auto = tools::move_point(
        &moved,
        id,
        point_id,
        Part::Outgoing,
        [90., 80.],
        time,
        true,
        true,
    )
    .unwrap();
    let keyed_handles = editing::apply(&moved, &[auto]).unwrap();
    let Content::Path { contours, .. } = &keyed_handles.objects[&id].content else {
        unreachable!()
    };
    let handle = &contours[0].points[0];
    assert_eq!(handle.incoming_at(time).unwrap(), [-10., -20.]);
    assert_eq!(handle.outgoing_at(time).unwrap(), [10., 20.]);
    let handle_id = handle.outgoing_track.as_ref().unwrap().id;
    let at_one = tools::move_point(
        &keyed_handles,
        id,
        point_id,
        Part::Outgoing,
        [120., 50.],
        Time::new(1, 1).unwrap(),
        false,
        false,
    )
    .unwrap();
    let later = editing::apply(&keyed_handles, &[at_one]).unwrap();
    let Content::Path { contours, .. } = &later.objects[&id].content else {
        unreachable!()
    };
    let handle = &contours[0].points[0];
    assert_eq!(
        handle.outgoing_at(Time::new(3, 4).unwrap()).unwrap(),
        [15., 15.]
    );
    assert_eq!(handle.outgoing_track.as_ref().unwrap().id, handle_id);
    let sampled = contours[0].at(Time::new(3, 4).unwrap()).unwrap();
    assert_eq!(sampled.points[0].outgoing, [15., 15.]);
    assert!(sampled.points[0].tracks().next().is_none());
    let mut reversed = later.objects[&id].clone();
    let Content::Path { contours, .. } = &mut reversed.content else {
        unreachable!()
    };
    contours[0].reverse();
    assert_eq!(
        contours[0].points[0].incoming_track.as_ref().unwrap().id,
        handle_id
    );
    editing::apply(
        &later,
        &[editing::Edit::Replace {
            id,
            expected: Box::new(later.objects[&id].clone()),
            object: Box::new(reversed),
        }],
    )
    .unwrap();
    let mut value = updated.clone();
    value.position_track.as_mut().unwrap().extensions.clear();
    assert!(
        editing::apply(
            &moved,
            &[editing::Edit::Point {
                object: id,
                expected: updated.clone(),
                value
            }]
        )
        .is_err()
    );
}

#[test]
fn animated_subdivision_preserves_curves_between_distinct_key_schedules() {
    use fold_graph::{
        binding::{Interpolation, Key, Track},
        types::{Literal, Space, Unit},
    };
    fn track(samples: &[(i64, [f64; 2])]) -> Track {
        let vector = |p: [f64; 2]| Literal::Vector {
            value: p.to_vec(),
            unit: Unit::Canvas,
            space: Space::Screen,
        };
        let mut track =
            Track::constant(vector(samples[0].1), Time::new(samples[0].0, 4).unwrap()).unwrap();
        for (time, value) in &samples[1..] {
            track.keys.push(Key {
                id: ObjectId::new(),
                time: Time::new(*time, 4).unwrap(),
                value: vector(*value),
                interpolation: Interpolation::Linear,
                handles: vec![],
                extensions: Default::default(),
            });
        }
        track
    }
    let mut a = Point {
        id: ObjectId::new(),
        position: [0., 0.],
        incoming: [0.; 2],
        outgoing: [20., 40.],
        position_track: Some(track(&[(0, [0., 0.]), (4, [30., -10.])])),
        incoming_track: None,
        outgoing_track: Some(track(&[(0, [20., 40.]), (2, [0.; 2]), (6, [40., -20.])])),
        extensions: Default::default(),
    };
    a.outgoing_track
        .as_mut()
        .unwrap()
        .extensions
        .insert("vendor.track".into(), serde_json::json!(true));
    a.outgoing_track.as_mut().unwrap().keys[0]
        .extensions
        .insert("vendor.key".into(), serde_json::json!(true));
    let b = Point {
        id: ObjectId::new(),
        position: [100., 10.],
        incoming: [-20., 30.],
        outgoing: [0.; 2],
        position_track: Some(track(&[(1, [100., 10.]), (5, [140., 50.])])),
        incoming_track: Some(track(&[(0, [-20., 30.]), (2, [0.; 2]), (7, [-10., -40.])])),
        outgoing_track: None,
        extensions: Default::default(),
    };
    let contour = Contour {
        id: ObjectId::new(),
        points: vec![a.clone(), b.clone()],
        closed: false,
        extensions: Default::default(),
    };
    let object = ObjectId::new();
    let canvas = Canvas {
        roots: vec![object],
        objects: [(
            object,
            Object::new(
                "Curve",
                Content::Path {
                    contours: vec![contour.clone()],
                    extensions: Default::default(),
                },
            ),
        )]
        .into(),
        ..Default::default()
    };
    // At 1/2 s both handles collapse: the picker uses a line fraction.
    for (time, click_fraction) in [(Time::ZERO, 0.35), (Time::new(1, 2).unwrap(), 0.35)] {
        let (_, edit) =
            tools::split_segment(&canvas, object, contour.id, a.id, click_fraction, time).unwrap();
        let after = editing::apply(&canvas, &[edit]).unwrap();
        let Content::Path { contours, .. } = &after.objects[&object].content else {
            unreachable!()
        };
        let split = &contours[0];
        let command = editing::Edit::SplitSegment {
            object,
            expected: Box::new(canvas.objects[&object].clone()),
            contour: contour.id,
            start: a.id,
            fraction: click_fraction,
            time,
        };
        let document = fold_foundation::DocumentId::new();
        let mut project = fold_project::Project::new(8);
        project
            .commit(fold_project::EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![fold_project::Mutation::PutDocument(
                    canvas.document(document).unwrap(),
                )],
            })
            .unwrap();
        project
            .commit(
                editing::stage(
                    &project.snapshot(),
                    document,
                    std::slice::from_ref(&command),
                )
                .unwrap(),
            )
            .unwrap();
        let committed = project.snapshot().state().documents[&document].clone();
        project.undo().unwrap();
        assert_eq!(
            Canvas::from_document(&project.snapshot().state().documents[&document]).unwrap(),
            canvas
        );
        project.redo().unwrap();
        assert_eq!(project.snapshot().state().documents[&document], committed);
        let encoded = serde_json::to_value(&command).unwrap();
        assert_eq!(encoded["op"], "split_segment");
        let decoded = serde_json::from_value(encoded).unwrap();
        let semantic = editing::apply(&canvas, &[decoded]).unwrap();
        let Content::Path {
            contours: semantic_contours,
            ..
        } = &semantic.objects[&object].content
        else {
            unreachable!()
        };
        let mut stale = canvas.clone();
        stale.objects.get_mut(&object).unwrap().name = "Changed".into();
        assert!(editing::apply(&stale, std::slice::from_ref(&command)).is_err());
        let mut locked = canvas.clone();
        locked.objects.get_mut(&object).unwrap().protected = true;
        let locked_command = editing::Edit::SplitSegment {
            object,
            expected: Box::new(locked.objects[&object].clone()),
            contour: contour.id,
            start: a.id,
            fraction: click_fraction,
            time,
        };
        assert!(editing::apply(&locked, &[locked_command]).is_err());
        let retained = split.points[0].outgoing_track.as_ref().unwrap();
        let source = a.outgoing_track.as_ref().unwrap();
        assert_eq!(retained.id, source.id);
        assert_eq!(retained.extensions, source.extensions);
        assert_eq!(retained.keys[0].id, source.keys[0].id);
        assert_eq!(retained.keys[0].extensions, source.keys[0].extensions);
        let pose = split.at(time).unwrap();
        if time != Time::ZERO {
            let before = contour.at(time).unwrap();
            for axis in 0..2 {
                let expected = before.points[0].position[axis] * (1. - click_fraction)
                    + before.points[1].position[axis] * click_fraction;
                assert!((pose.points[1].position[axis] - expected).abs() < 1e-9);
            }
        }
        // The retained first handle gives the fixed cubic subdivision parameter.
        let fraction = match &retained.keys[0].value {
            Literal::Vector { value, .. } => value[0] / 20.,
            _ => unreachable!(),
        };
        for tick in 0..=80 {
            let time = Time::new(tick, 40).unwrap();
            let original = contour.at(time).unwrap();
            let sampled = split.at(time).unwrap();
            let semantic_pose = semantic_contours[0].at(time).unwrap();
            for (artist, agent) in sampled.points.iter().zip(&semantic_pose.points) {
                assert_eq!(artist.position, agent.position);
                assert_eq!(artist.incoming, agent.incoming);
                assert_eq!(artist.outgoing, agent.outgoing);
            }
            for step in 0..=100 {
                let u = f64::from(step) / 100.;
                let expected = cubic(&original.points[0], &original.points[1], u);
                let actual = if u <= fraction {
                    cubic(&sampled.points[0], &sampled.points[1], u / fraction)
                } else {
                    cubic(
                        &sampled.points[1],
                        &sampled.points[2],
                        (u - fraction) / (1. - fraction),
                    )
                };
                for axis in 0..2 {
                    assert!((expected[axis] - actual[axis]).abs() < 1e-8);
                }
            }
        }
    }
    let mut held = canvas.clone();
    let Content::Path { contours, .. } = &mut held.objects.get_mut(&object).unwrap().content else {
        unreachable!()
    };
    for point in &mut contours[0].points {
        for track in [
            &mut point.position_track,
            &mut point.incoming_track,
            &mut point.outgoing_track,
        ]
        .into_iter()
        .flatten()
        {
            for key in &mut track.keys {
                key.interpolation = Interpolation::Hold;
            }
        }
    }
    let mut mixed = held.clone();
    let Content::Path { contours, .. } = &mut mixed.objects.get_mut(&object).unwrap().content
    else {
        unreachable!()
    };
    for point in &mut contours[0].points {
        for track in [
            &mut point.position_track,
            &mut point.incoming_track,
            &mut point.outgoing_track,
        ]
        .into_iter()
        .flatten()
        {
            let mut last = track.keys.last().unwrap().clone();
            last.time = Time::new(1, 1).unwrap();
            last.interpolation = Interpolation::Linear;
            track.keys.truncate(1);
            track.keys[0].time = Time::ZERO;
            track.keys.push(last.clone());
            last.id = ObjectId::new();
            last.time = Time::new(2, 1).unwrap();
            let Literal::Vector { value, .. } = &mut last.value else {
                unreachable!()
            };
            for component in value {
                *component += 5.;
            }
            track.keys.push(last);
        }
    }
    let mut held_next = held.clone();
    let Content::Path { contours, .. } = &mut held_next.objects.get_mut(&object).unwrap().content
    else {
        unreachable!()
    };
    for point in &mut contours[0].points {
        for track in [
            &mut point.position_track,
            &mut point.incoming_track,
            &mut point.outgoing_track,
        ]
        .into_iter()
        .flatten()
        {
            for key in &mut track.keys {
                key.interpolation = Interpolation::HoldNext;
            }
        }
    }
    let mut bezier = mixed.clone();
    let Content::Path { contours, .. } = &mut bezier.objects.get_mut(&object).unwrap().content
    else {
        unreachable!()
    };
    for point in &mut contours[0].points {
        for track in [
            &mut point.position_track,
            &mut point.incoming_track,
            &mut point.outgoing_track,
        ]
        .into_iter()
        .flatten()
        {
            for key in &mut track.keys {
                key.interpolation = Interpolation::Bezier;
                key.handles = (0..2)
                    .map(|axis| fold_graph::binding::Handles {
                        tangents: fold_animation::Tangents::Broken,
                        outgoing: fold_animation::Handle {
                            fraction: 0.2 + axis as f64 * 0.1,
                            value: 17. - axis as f64 * 31.,
                        },
                        incoming: fold_animation::Handle {
                            fraction: 0.4 - axis as f64 * 0.1,
                            value: -9. + axis as f64 * 25.,
                        },
                    })
                    .collect();
            }
        }
    }
    // Equal endpoint values do not imply a constant Bézier segment: temporal
    // handle offsets can still create an overshoot between the keys.
    let position = contours[0].points[0].position_track.as_mut().unwrap();
    position.keys[1].value = position.keys[0].value.clone();
    let mut automatic = bezier.clone();
    let Content::Path { contours, .. } = &mut automatic.objects.get_mut(&object).unwrap().content
    else {
        unreachable!()
    };
    for point in &mut contours[0].points {
        for track in [
            &mut point.position_track,
            &mut point.incoming_track,
            &mut point.outgoing_track,
        ]
        .into_iter()
        .flatten()
        {
            for key in &mut track.keys {
                for handle in &mut key.handles {
                    handle.tangents = fold_animation::Tangents::Auto;
                }
            }
        }
    }
    for source in [&held, &mixed, &held_next, &bezier, &automatic] {
        let Content::Path { contours, .. } = &source.objects[&object].content else {
            unreachable!()
        };
        let original = &contours[0];
        let (_, edit) =
            tools::split_segment(source, object, contour.id, a.id, 0.35, Time::ZERO).unwrap();
        let held_split = editing::apply(source, &[edit]).unwrap();
        let Content::Path { contours, .. } = &held_split.objects[&object].content else {
            unreachable!()
        };
        assert_eq!(
            contours[0].points[0].position_track,
            original.points[0].position_track
        );
        assert_eq!(
            contours[0].points[2].position_track,
            original.points[1].position_track
        );
        assert_eq!(
            serde_json::from_value::<Canvas>(serde_json::to_value(&held_split).unwrap()).unwrap(),
            held_split
        );
        for tick in 0..=160 {
            let time = Time::new(tick, 80).unwrap();
            let before = original.at(time).unwrap();
            let after = contours[0].at(time).unwrap();
            for step in 0..=100 {
                let u = f64::from(step) / 100.;
                let expected = cubic(&before.points[0], &before.points[1], u);
                let actual = if u <= 0.35 {
                    cubic(&after.points[0], &after.points[1], u / 0.35)
                } else {
                    cubic(&after.points[1], &after.points[2], (u - 0.35) / 0.65)
                };
                for axis in 0..2 {
                    assert!((expected[axis] - actual[axis]).abs() < 1e-8);
                }
            }
        }
    }
    let mut incompatible = bezier.clone();
    let Content::Path { contours, .. } =
        &mut incompatible.objects.get_mut(&object).unwrap().content
    else {
        unreachable!()
    };
    contours[0].points[0].position_track.as_mut().unwrap().keys[0].handles[0]
        .outgoing
        .fraction = 0.1;
    let before = incompatible.clone();
    assert!(
        tools::split_segment(&incompatible, object, contour.id, a.id, 0.35, Time::ZERO).is_err()
    );
    assert_eq!(incompatible, before);
    let mut unsupported = canvas.clone();
    let Content::Path { contours, .. } = &mut unsupported.objects.get_mut(&object).unwrap().content
    else {
        unreachable!()
    };
    contours[0].points[0].position_track.as_mut().unwrap().keys[0].interpolation =
        Interpolation::Hold;
    let before = unsupported.clone();
    assert!(
        tools::split_segment(&unsupported, object, contour.id, a.id, 0.35, Time::ZERO).is_err()
    );
    assert_eq!(unsupported, before);
}
