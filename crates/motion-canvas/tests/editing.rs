use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion_canvas::{
    editing::{self, Edit},
    model::*,
};
use fold_project::{EditBatch, Mutation, Project};

#[test]
fn editable_capture_keeps_identity_data_and_one_undo_and_rejects_stale_or_protected_scripts() {
    let id = ObjectId::new();
    let control = ObjectId::new();
    let mut script = fold_motion_canvas::tools::new_object("script").unwrap();
    script.graph_position = Some([40., 50.]);
    script.transform.x.value = 123.;
    if let Content::Script { controls, .. } = &mut script.content {
        controls.insert(
            control,
            Control {
                name: "Count".into(),
                value: ControlValue::Number(Scalar::new(3.)),
                protected: false,
                extensions: [("vendor.control".into(), serde_json::json!({"keep":true}))].into(),
            },
        );
    }
    let canvas = Canvas {
        roots: vec![id],
        objects: [(id, script.clone())].into(),
        ..Default::default()
    };
    let child = ObjectId::new();
    let captured = Captured {
        roots: vec![child],
        objects: [(
            child,
            fold_motion_canvas::tools::new_object("rectangle").unwrap(),
        )]
        .into(),
    };
    let time = Time::new(1, 2).unwrap();
    let convert = |expected: &Object, value: Captured| Edit::Capture {
        id,
        expected: Box::new(expected.clone()),
        value: Box::new(value),
        time,
    };
    let edits = vec![convert(&script, captured.clone())];
    let after = editing::apply(&canvas, &edits).unwrap();
    assert_eq!(after.roots, canvas.roots);
    assert_eq!(after.objects[&id].transform, script.transform);
    assert_eq!(after.objects[&id].graph_position, script.graph_position);
    assert_eq!(
        after.objects[&child].extensions["fold.motion.capture"]["source"],
        serde_json::to_value(&script.content).unwrap()
    );
    assert!(
        matches!(&after.objects[&id].content, Content::Group { children, .. } if children == &vec![child])
    );
    let mut envelope = canvas.document(DocumentId::new()).unwrap();
    envelope.assets.push(fold_foundation::AssetId::new());
    envelope.dependencies.push(fold_project::DocumentRef {
        document: DocumentId::new(),
        output: "video".into(),
        extensions: [("vendor.reference".into(), serde_json::json!(true))].into(),
    });
    assert!(after.replace_document(&envelope).is_err());
    envelope.assets.clear();
    envelope.dependencies.clear();
    envelope
        .extensions
        .insert("vendor.document".into(), serde_json::json!({"keep":true}));
    let prepared = after.replace_document(&envelope).unwrap();
    assert_eq!(prepared.extensions, envelope.extensions);
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
    project.undo().unwrap();
    assert_eq!(
        Canvas::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        canvas
    );
    project.redo().unwrap();
    assert_eq!(
        Canvas::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        after
    );
    let mut stale = script.clone();
    stale.name = "Concurrent edit".into();
    assert!(editing::apply(&canvas, &[convert(&stale, captured.clone())]).is_err());
    let mut locked = canvas.clone();
    if let Content::Script { controls, .. } = &mut locked.objects.get_mut(&id).unwrap().content {
        controls.get_mut(&control).unwrap().protected = true;
    }
    assert!(editing::apply(&locked, &[convert(&locked.objects[&id], captured)]).is_err());
}

fn artwork() -> (Canvas, ObjectId, Point) {
    let id = ObjectId::new();
    let point = Point {
        position_track: None,
        incoming_track: None,
        outgoing_track: None,
        id: ObjectId::new(),
        position: [10., 20.],
        incoming: [-3., 0.],
        outgoing: [3., 0.],
        extensions: [("vendor.point".into(), serde_json::json!({"keep":true}))].into(),
    };
    let object = Object::new(
        "Curve",
        Content::Path {
            contours: vec![Contour {
                id: ObjectId::new(),
                points: vec![point.clone()],
                closed: false,
                extensions: Default::default(),
            }],
            extensions: [("vendor.path".into(), serde_json::json!([1, null, 3]))].into(),
        },
    );
    (
        Canvas {
            roots: vec![id],
            objects: [(id, object)].into(),
            ..Default::default()
        },
        id,
        point,
    )
}

#[test]
fn artist_then_agent_edit_same_point_save_reopen_and_undo() {
    let (mut canvas, object, point) = artwork();
    canvas.objects.get_mut(&object).unwrap().paint.miter_limit = 6.;
    canvas.objects.get_mut(&object).unwrap().paint.fill = Some(PaintSource::Linear {
        start: [0., 0.],
        end: [100., 20.],
        transform: [1., 0., 0.2, 1., 0., 0.],
        spread: Spread::Reflect,
        stops: vec![
            Stop {
                id: ObjectId::new(),
                offset: 0.,
                color: [1., 0., 0., 0.5],
                extensions: [("vendor.stop".into(), serde_json::json!(null))].into(),
            },
            Stop {
                id: ObjectId::new(),
                offset: 1.,
                color: [0., 0., 1., 1.],
                extensions: Default::default(),
            },
        ],
        extensions: [("vendor.paint".into(), serde_json::json!([1, 2]))].into(),
    });
    let id = DocumentId::new();
    let mut document = canvas.document(id).unwrap();
    document
        .extensions
        .insert("vendor.wrapper".into(), serde_json::json!([null, 2]));
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let mut artist_point = point.clone();
    artist_point.position = [25., 30.];
    let artist = Edit::Point {
        object,
        expected: point.clone(),
        value: artist_point.clone(),
    };
    project
        .commit(editing::stage(&project.snapshot(), id, &[artist]).unwrap())
        .unwrap();
    let mut agent_point = artist_point.clone();
    agent_point.outgoing = [8., -4.];
    let agent = Edit::Point {
        object,
        expected: artist_point,
        value: agent_point.clone(),
    };
    project
        .commit(editing::stage(&project.snapshot(), id, &[agent]).unwrap())
        .unwrap();
    let stale = Edit::Point {
        object,
        expected: point,
        value: agent_point,
    };
    assert!(editing::stage(&project.snapshot(), id, &[stale]).is_err());
    let current = Canvas::from_document(&project.snapshot().state().documents[&id]).unwrap();
    let expected = current.objects[&object].clone();
    let mut painted = expected.clone();
    let Some(PaintSource::Linear { stops, .. }) = &mut painted.paint.fill else {
        unreachable!()
    };
    stops[0].color = [0., 1., 0., 0.5];
    let mut discarded = painted.clone();
    let Some(PaintSource::Linear { stops, .. }) = &mut discarded.paint.fill else {
        unreachable!()
    };
    stops[0].extensions.clear();
    assert!(
        editing::stage(
            &project.snapshot(),
            id,
            &[Edit::Replace {
                id: object,
                expected: Box::new(expected.clone()),
                object: Box::new(discarded)
            }]
        )
        .is_err()
    );
    project
        .commit(
            editing::stage(
                &project.snapshot(),
                id,
                &[Edit::Replace {
                    id: object,
                    expected: Box::new(expected),
                    object: Box::new(painted.clone()),
                }],
            )
            .unwrap(),
        )
        .unwrap();
    let mut radial = painted.clone();
    let Some(PaintSource::Linear {
        transform,
        spread,
        stops,
        extensions,
        ..
    }) = &painted.paint.fill
    else {
        unreachable!()
    };
    radial.paint.fill = Some(PaintSource::Radial {
        center: [50., 20.],
        focus: [45., 15.],
        radius: 50.,
        transform: *transform,
        spread: *spread,
        stops: stops.clone(),
        extensions: extensions.clone(),
    });
    project
        .commit(
            editing::stage(
                &project.snapshot(),
                id,
                &[Edit::Replace {
                    id: object,
                    expected: Box::new(painted),
                    object: Box::new(radial.clone()),
                }],
            )
            .unwrap(),
        )
        .unwrap();
    let painted = radial;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("artwork.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let loaded = fold_project::load(&path, 8).unwrap();
    let snapshot = loaded.snapshot();
    let saved = &snapshot.state().documents[&id];
    assert_eq!(
        Canvas::from_document(saved).unwrap().objects[&object].paint,
        painted.paint
    );
    assert_eq!(
        Canvas::from_document(saved).unwrap().objects[&object]
            .paint
            .miter_limit,
        6.
    );
    let mut invalid = Canvas::from_document(saved).unwrap();
    if let Some(PaintSource::Radial { radius, .. }) =
        &mut invalid.objects.get_mut(&object).unwrap().paint.fill
    {
        *radius = 0.;
    }
    assert!(invalid.validate().is_err());
    let mut invalid = Canvas::from_document(saved).unwrap();
    invalid.objects.get_mut(&object).unwrap().paint.miter_limit = 0.5;
    assert!(invalid.validate().is_err());
    assert_eq!(
        Canvas::from_document(saved).unwrap().objects[&object]
            .content
            .extensions(),
        canvas.objects[&object].content.extensions()
    );
    assert_eq!(
        saved.extensions["vendor.wrapper"],
        serde_json::json!([null, 2])
    );
    let final_payload = project.snapshot().state().documents[&id].payload.clone();
    project.undo().unwrap();
    project.redo().unwrap();
    assert_eq!(
        project.snapshot().state().documents[&id].payload,
        final_payload
    );
}

#[test]
fn group_protection_and_cycles_reject_atomic_edits() {
    let (mut canvas, child, point) = artwork();
    let group = ObjectId::new();
    let mut object = Object::new(
        "Group",
        Content::Group {
            children: vec![child],
            extensions: Default::default(),
        },
    );
    object.protected = true;
    canvas.roots = vec![group];
    canvas.objects.insert(group, object);
    assert!(
        editing::apply(
            &canvas,
            &[Edit::Point {
                object: child,
                expected: point.clone(),
                value: point
            }]
        )
        .unwrap_err()
        .contains("protected")
    );
    canvas.objects.get_mut(&group).unwrap().protected = false;
    assert!(
        editing::apply(
            &canvas,
            &[Edit::Move {
                id: group,
                parent: Some(group),
                index: 0
            }]
        )
        .is_err()
    );
    canvas.validate().unwrap();
}

#[test]
fn script_controls_sample_new_values_without_changing_source() {
    let control = ObjectId::new();
    let source = "fn render(input: Context<'_>) { /* fframes source */ }".to_owned();
    let mut object = Object::new(
        "Dots",
        Content::Script {
            source: source.clone(),
            controls: [(
                control,
                Control {
                    name: "Count".into(),
                    value: ControlValue::Number(Scalar::new(840.)),
                    protected: false,
                    extensions: Default::default(),
                },
            )]
            .into(),
            extensions: Default::default(),
        },
    );
    let canvas = Canvas::default();
    let input = fold_motion_canvas::scripts::Context {
        canvas: &canvas,
        object: &object,
        time: Time::ZERO,
    };
    assert_eq!(input.number("Count").unwrap(), 840.);
    let script_id = ObjectId::new();
    let before = Canvas {
        roots: vec![script_id],
        objects: [(script_id, object.clone())].into(),
        ..Default::default()
    };
    let compiled = fold_motion_canvas::compilation::program(
        &before,
        std::path::Path::new("/runtime"),
        "build-1",
    )
    .unwrap();
    let Content::Script { controls, .. } = &mut object.content else {
        unreachable!()
    };
    let ControlValue::Number(number) = &mut controls.get_mut(&control).unwrap().value else {
        unreachable!()
    };
    number.value = 10_000.;
    let input = fold_motion_canvas::scripts::Context {
        canvas: &canvas,
        object: &object,
        time: Time::ZERO,
    };
    assert_eq!(input.number("Count").unwrap(), 10_000.);
    assert!(matches!(&object.content, Content::Script { source: s, .. } if s == &source));
    let mut after = before;
    after.objects.insert(script_id, object);
    let native_id = ObjectId::new();
    after.objects.insert(
        native_id,
        Object::new(
            "New native shape",
            Content::Ellipse {
                radius_x: Scalar::new(4.),
                radius_y: Scalar::new(4.),
                extensions: Default::default(),
            },
        ),
    );
    after.roots.push(native_id);
    let updated = fold_motion_canvas::compilation::program(
        &after,
        std::path::Path::new("/runtime"),
        "build-1",
    )
    .unwrap();
    assert_eq!(updated.identity, compiled.identity);
}

#[test]
fn native_image_asset_edits_save_reopen_and_undo_share_the_project_transaction() {
    let asset = fold_foundation::AssetId::new();
    let object = ObjectId::new();
    let document = DocumentId::new();
    let mut image = fold_motion_canvas::tools::image(asset, [320, 180]).unwrap();
    if let Content::Image { extensions, .. } = &mut image.content {
        extensions.insert("vendor.image".into(), serde_json::json!({"keep":true}));
    }
    let canvas = Canvas {
        roots: vec![object],
        objects: [(object, image.clone())].into(),
        ..Default::default()
    };
    let original = canvas.document(document).unwrap();
    assert_eq!(original.assets, vec![asset]);
    let mut wrong = original.clone();
    wrong.assets.clear();
    assert!(Canvas::from_document(&wrong).is_err());
    let mut project = Project::new(8);
    assert!(
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(original.clone())]
            })
            .is_err()
    );
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutAsset(fold_project::Asset {
                    id: asset,
                    location: "image.png".into(),
                    fingerprint: "fixture".into(),
                    extensions: Default::default(),
                }),
                Mutation::PutDocument(original),
            ],
        })
        .unwrap();
    let mut edited = image.clone();
    if let Content::Image { width, fit, .. } = &mut edited.content {
        width.value = 640.;
        *fit = ImageFit::Cover;
    }
    project
        .commit(
            editing::stage(
                &project.snapshot(),
                document,
                &[Edit::Replace {
                    id: object,
                    expected: Box::new(image.clone()),
                    object: Box::new(edited.clone()),
                }],
            )
            .unwrap(),
        )
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("image.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let loaded = fold_project::load(&path, 8).unwrap();
    let snapshot = loaded.snapshot();
    let saved = &snapshot.state().documents[&document];
    assert_eq!(saved.assets, vec![asset]);
    assert_eq!(
        Canvas::from_document(saved).unwrap().objects[&object],
        edited
    );
    project.undo().unwrap();
    assert_eq!(
        Canvas::from_document(&project.snapshot().state().documents[&document])
            .unwrap()
            .objects[&object],
        image
    );
    project.redo().unwrap();
    let deleting = editing::stage(
        &project.snapshot(),
        document,
        &[Edit::Delete {
            id: object,
            expected: Box::new(edited),
        }],
    )
    .unwrap();
    project.commit(deleting).unwrap();
    assert!(
        project.snapshot().state().documents[&document]
            .assets
            .is_empty()
    );
    assert!(project.snapshot().state().assets.contains_key(&asset));
}

#[test]
fn contour_reversal_retains_bezier_geometry_identity_metadata_and_undo() {
    let mut contour = Contour {
        id: ObjectId::new(),
        closed: true,
        extensions: [("vendor.contour".into(), serde_json::json!(true))].into(),
        points: [[0., 0.], [80., 30.], [10., 90.]]
            .into_iter()
            .map(|position| Point {
                position_track: None,
                incoming_track: None,
                outgoing_track: None,
                id: ObjectId::new(),
                position,
                incoming: [-9., -17.],
                outgoing: [24., 11.],
                extensions: [("vendor.point".into(), serde_json::json!([1, 2]))].into(),
            })
            .collect(),
    };
    let original = contour.clone();
    let sample = |a: &Point, b: &Point, t: f64| {
        let u = 1. - t;
        std::array::from_fn::<_, 2, _>(|i| {
            u * u * u * a.position[i]
                + 3. * u * u * t * (a.position[i] + a.outgoing[i])
                + 3. * u * t * t * (b.position[i] + b.incoming[i])
                + t * t * t * b.position[i]
        })
    };
    contour.reverse();
    for (i, a) in original.points.iter().enumerate() {
        let b = &original.points[(i + 1) % original.points.len()];
        let reversed_a = contour.points.iter().find(|p| p.id == a.id).unwrap();
        let reversed_b = contour.points.iter().find(|p| p.id == b.id).unwrap();
        assert_eq!(reversed_a.extensions, a.extensions);
        for t in [0., 0.2, 0.5, 0.8, 1.] {
            assert!(
                sample(a, b, t)
                    .iter()
                    .zip(sample(reversed_b, reversed_a, 1. - t))
                    .all(|(a, b)| (a - b).abs() < 1e-10)
            );
        }
    }
    contour.reverse();
    assert_eq!(contour, original);
    let object_id = ObjectId::new();
    let object = Object::new(
        "Curve",
        Content::Path {
            contours: vec![original],
            extensions: Default::default(),
        },
    );
    let canvas = Canvas {
        roots: vec![object_id],
        objects: [(object_id, object.clone())].into(),
        ..Default::default()
    };
    let mut value = object.clone();
    let Content::Path { contours, .. } = &mut value.content else {
        unreachable!()
    };
    contours[0].reverse();
    let after = editing::apply(
        &canvas,
        &[Edit::Replace {
            id: object_id,
            expected: Box::new(object),
            object: Box::new(value),
        }],
    )
    .unwrap();
    let document = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(canvas.document(document).unwrap())],
        })
        .unwrap();
    let before = project.snapshot();
    project
        .commit(EditBatch {
            base: before.revision(),
            mutations: vec![Mutation::PutDocument(
                after
                    .replace_document(&before.state().documents[&document])
                    .unwrap(),
            )],
        })
        .unwrap();
    project.undo().unwrap();
    assert_eq!(
        Canvas::from_document(&project.snapshot().state().documents[&document]).unwrap(),
        canvas
    );
}

#[test]
fn first_anchor_changes_only_closed_contour_order_and_is_guarded() {
    let id = ObjectId::new();
    let contour = Contour {
        id: ObjectId::new(),
        closed: true,
        extensions: Default::default(),
        points: [[0., 0.], [20., 0.], [20., 20.]]
            .into_iter()
            .map(|position| Point {
                position_track: None,
                incoming_track: None,
                outgoing_track: None,
                id: ObjectId::new(),
                position,
                incoming: [-2., 3.],
                outgoing: [4., -5.],
                extensions: [("vendor.point".into(), serde_json::json!(true))].into(),
            })
            .collect(),
    };
    let start = contour.points[1].id;
    let object = Object::new(
        "Loop",
        Content::Path {
            contours: vec![contour.clone()],
            extensions: Default::default(),
        },
    );
    let mut canvas = Canvas {
        roots: vec![id],
        objects: [(id, object)].into(),
        ..Default::default()
    };
    let edits = fold_motion_canvas::tools::first_anchor(&canvas, id, start).unwrap();
    let after = editing::apply(&canvas, &edits).unwrap();
    let Content::Path { contours, .. } = &after.objects[&id].content else {
        unreachable!()
    };
    let mut expected = contour.clone();
    expected.points.rotate_left(1);
    assert_eq!(contours[0], expected);
    assert!(
        fold_motion_canvas::tools::first_anchor(&after, id, start)
            .unwrap()
            .is_empty()
    );
    assert!(editing::apply(&after, &edits).is_err());
    let restored = editing::apply(
        &after,
        &fold_motion_canvas::tools::first_anchor(&after, id, contour.points[0].id).unwrap(),
    )
    .unwrap();
    assert_eq!(restored, canvas);
    canvas.objects.get_mut(&id).unwrap().protected = true;
    assert!(fold_motion_canvas::tools::first_anchor(&canvas, id, start).is_err());
    canvas.objects.get_mut(&id).unwrap().protected = false;
    let Content::Path { contours, .. } = &mut canvas.objects.get_mut(&id).unwrap().content else {
        unreachable!()
    };
    contours[0].closed = false;
    assert!(fold_motion_canvas::tools::first_anchor(&canvas, id, start).is_err());
}
