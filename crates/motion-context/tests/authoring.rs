use fold_animation::{Curve, Key};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_graph::types::Literal;
use fold_media::Cancel;
use fold_motion_canvas::{TYPE, editing::Edit, model::*};
use fold_motion_context::package::{self, EditArgs, Provider};
use fold_platform::{
    animation,
    packages::{Animation, CommandRequest, PackageRegistry, Parameters},
};
use fold_project::{EditBatch, Mutation, Project};

#[test]
fn artist_and_agent_share_controls_animation_and_undo_through_host_contracts() {
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let document_id = DocumentId::new();
    let empty = registry.create(TYPE, document_id).unwrap();
    let object_id = ObjectId::new();
    let count = Scalar::new(100.);
    let control_id = ObjectId::new();
    let boolean_id = ObjectId::new();
    let boolean = Control {
        name: "Show".into(),
        value: ControlValue::Boolean(true),
        protected: false,
        extensions: [("vendor.control".into(), serde_json::json!([2, 3]))].into(),
    };
    let object = Object::new("Dots", Content::Script {
        source: "pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> { todo!() }".into(),
        controls: [(control_id, Control { name: "Count".into(), value: ControlValue::Number(count.clone()),
            protected: false, extensions: Default::default() }), (boolean_id, boolean.clone())].into(),
        extensions: Default::default(),
    });
    let document = registry
        .author(
            &empty,
            &[serde_json::to_value(Edit::Insert {
                id: object_id,
                object: Box::new(object),
                parent: None,
                index: 0,
            })
            .unwrap()],
        )
        .unwrap();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();

    let mut artist_value = count.clone();
    artist_value.value = 8000.;
    let arguments = EditArgs {
        document: document_id,
        edits: vec![Edit::Parameter {
            object: object_id,
            expected: count.clone(),
            value: artist_value.clone(),
        }],
    };
    let snapshot = project.snapshot();
    let batch = registry
        .stage(
            &snapshot,
            &CommandRequest {
                id: package::EDIT.into(),
                base: snapshot.revision(),
                arguments: serde_json::to_vec(&arguments).unwrap(),
            },
            &Cancel::default(),
        )
        .unwrap();
    project.commit(batch).unwrap();
    let snapshot = project.snapshot();
    let document = &snapshot.state().documents[&document_id];
    let inspected_control =
        match &Canvas::from_document(document).unwrap().objects[&object_id].content {
            Content::Script { controls, .. } => controls[&control_id].clone(),
            _ => unreachable!(),
        };
    let mut changed_identity = inspected_control.clone();
    changed_identity.value = ControlValue::Number(Scalar::new(8000.));
    assert!(
        registry
            .author(
                document,
                &[serde_json::to_value(Edit::Control {
                    object: object_id,
                    id: control_id,
                    expected: inspected_control,
                    value: changed_identity,
                })
                .unwrap()]
            )
            .is_err()
    );
    let inspected = registry.inspect(document).unwrap();
    let inspected: Canvas = serde_json::from_value(inspected).unwrap();
    assert_eq!(
        inspected.objects[&object_id]
            .scalars()
            .into_iter()
            .find(|s| s.id == count.id)
            .unwrap()
            .value,
        8000.
    );

    let mut agent_value = boolean.clone();
    agent_value.value = ControlValue::Boolean(false);
    let agent_edit = Edit::Control {
        object: object_id,
        id: boolean_id,
        expected: boolean.clone(),
        value: agent_value,
    };
    let after = registry
        .author(document, &[serde_json::to_value(&agent_edit).unwrap()])
        .unwrap();
    assert!(
        registry
            .author(&after, &[serde_json::to_value(agent_edit).unwrap()])
            .is_err()
    );
    project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(after)],
        })
        .unwrap();

    let snapshot = project.snapshot();
    let document = &snapshot.state().documents[&document_id];
    let time = Time::new(1, 2).unwrap();
    let channel = animation::Channel {
        target: animation::Target {
            group: None,
            object: object_id,
            path: count.id.to_string(),
        },
        node_label: "Dots".into(),
        property: count.id.to_string(),
        property_label: "Count".into(),
        component: String::new(),
        curve: Curve {
            keys: vec![
                Key::new(Time::ZERO, 8000.),
                Key::new(Time::new(1, 1).unwrap(), 10000.),
            ],
        },
    };
    let animated = Provider::default()
        .edit_animation(document, &[channel], time)
        .unwrap();
    let catalog = Provider::default().parameters(&animated).unwrap();
    let count_target = &catalog
        .iter()
        .find(|d| d.target.parameter.native() == Some(count.id))
        .unwrap()
        .target;
    assert_eq!(
        Provider::default()
            .sample_parameter(&animated, count_target, time, &Cancel::default())
            .unwrap(),
        Literal::Scalar { value: 9000. }
    );
    let boolean_target = &catalog
        .iter()
        .find(|d| d.target.parameter.native() == Some(boolean_id))
        .unwrap()
        .target;
    assert_eq!(
        Provider::default()
            .sample_parameter(&animated, boolean_target, time, &Cancel::default())
            .unwrap(),
        Literal::Bool { value: false }
    );
    assert_eq!(
        Provider::default()
            .animation_channels(&animated, Some(object_id))
            .unwrap()
            .len(),
        1
    );
    project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(animated)],
        })
        .unwrap();
    project.undo().unwrap();
    assert!(
        Provider::default()
            .animation_channels(&project.snapshot().state().documents[&document_id], None)
            .unwrap()
            .is_empty()
    );
    project.redo().unwrap();
    assert_eq!(
        Provider::default()
            .animation_channels(&project.snapshot().state().documents[&document_id], None)
            .unwrap()
            .len(),
        1
    );

    let mut locked =
        Canvas::from_document(&project.snapshot().state().documents[&document_id]).unwrap();
    if let Content::Script { controls, .. } =
        &mut locked.objects.get_mut(&object_id).unwrap().content
    {
        controls.get_mut(&control_id).unwrap().protected = true;
    }
    let locked = locked.document(document_id).unwrap();
    assert!(
        !Provider::default()
            .parameters(&locked)
            .unwrap()
            .iter()
            .find(|d| d.target.parameter.native() == Some(count.id))
            .unwrap()
            .editable
    );
    assert!(
        Provider::default()
            .edit_animation(
                &locked,
                &Provider::default()
                    .animation_channels(&locked, None)
                    .unwrap(),
                time
            )
            .is_err()
    );
    assert!(registry.authoring_catalog()[0]["catalog"]["edits"]["oneOf"].is_array());
}

#[test]
fn image_controls_exclude_unused_paint_but_storage_still_validates_it() {
    let mut canvas = Canvas::default();
    let id = ObjectId::new();
    let image =
        fold_motion_canvas::tools::image(fold_foundation::AssetId::new(), [20, 10]).unwrap();
    let stroke = image.paint.stroke_width.clone();
    let width = match &image.content {
        Content::Image { width, .. } => width.clone(),
        _ => unreachable!(),
    };
    canvas.roots.push(id);
    canvas.objects.insert(id, image);
    let document = canvas.document(DocumentId::new()).unwrap();
    let provider = Provider::default();
    let catalog = provider.parameters(&document).unwrap();
    assert!(
        !catalog
            .iter()
            .any(|d| d.target.parameter.native() == Some(stroke.id))
    );
    let mut target = catalog
        .iter()
        .find(|d| d.target.parameter.native() == Some(width.id))
        .unwrap()
        .target
        .clone();
    assert!(
        provider
            .sample_parameter(&document, &target, Time::ZERO, &Cancel::default())
            .is_ok()
    );
    target.parameter = stroke.id.into();
    assert!(
        provider
            .sample_parameter(&document, &target, Time::ZERO, &Cancel::default())
            .is_err()
    );
    let mut value = stroke.clone();
    value.value = 5.;
    assert!(
        fold_motion_canvas::editing::apply(
            &canvas,
            &[Edit::Parameter {
                object: id,
                expected: stroke,
                value
            }]
        )
        .is_err()
    );
    canvas
        .objects
        .get_mut(&id)
        .unwrap()
        .paint
        .stroke_width
        .value = f64::NAN;
    assert!(canvas.document(document.id).is_err());
}

#[test]
fn anchor_parameters_sample_shared_canvas_coordinates_with_stable_targets() {
    use fold_graph::{
        binding::{Interpolation, Key, Track},
        types::{Space, Unit},
    };
    let literal = |value: [f64; 2]| Literal::Vector {
        value: value.to_vec(),
        unit: Unit::Canvas,
        space: Space::Screen,
    };
    let mut track = Track::constant(literal([10., 20.]), Time::ZERO).unwrap();
    track.keys.push(Key {
        id: ObjectId::new(),
        time: Time::new(1, 1).unwrap(),
        value: literal([30., 60.]),
        interpolation: Interpolation::Linear,
        handles: vec![],
        extensions: Default::default(),
    });
    let mut handle_track = Track::constant(literal([8., 12.]), Time::ZERO).unwrap();
    handle_track.keys.push(Key {
        id: ObjectId::new(),
        time: Time::new(1, 1).unwrap(),
        value: literal([16., 20.]),
        interpolation: Interpolation::Linear,
        handles: vec![],
        extensions: Default::default(),
    });
    let point = Point {
        id: ObjectId::new(),
        position: [0., 0.],
        position_track: Some(track),
        incoming_track: None,
        outgoing_track: Some(handle_track),
        incoming: [0.; 2],
        outgoing: [0.; 2],
        extensions: Default::default(),
    };
    let id = ObjectId::new();
    let mut canvas = Canvas {
        roots: vec![id],
        objects: [(
            id,
            Object::new(
                "Curve",
                Content::Path {
                    contours: vec![Contour {
                        id: ObjectId::new(),
                        points: vec![point.clone()],
                        closed: false,
                        extensions: Default::default(),
                    }],
                    extensions: Default::default(),
                },
            ),
        )]
        .into(),
        ..Default::default()
    };
    let document = canvas.document(DocumentId::new()).unwrap();
    let provider = Provider::default();
    let catalog = provider.parameters(&document).unwrap();
    let anchors = catalog
        .iter()
        .filter(|d| d.target.parameter.native() == Some(point.id))
        .collect::<Vec<_>>();
    assert_eq!(anchors.len(), 3);
    let time = Time::new(1, 2).unwrap();
    for entry in anchors {
        assert!(entry.editable);
        let sampled = provider
            .sample_parameter(&document, &entry.target, time, &Cancel::default())
            .unwrap();
        let expected = match entry.target.component {
            None => literal(point.position_at(time).unwrap()),
            Some(0) => Literal::Distance {
                value: 20.,
                unit: Unit::Canvas,
            },
            Some(1) => Literal::Distance {
                value: 40.,
                unit: Unit::Canvas,
            },
            _ => unreachable!(),
        };
        assert_eq!(sampled, expected);
    }
    let handles = catalog
        .iter()
        .filter(|d| d.target.parameter.property().is_some())
        .collect::<Vec<_>>();
    assert_eq!(handles.len(), 6);
    for entry in handles {
        assert!(entry.editable);
        let value = if entry
            .target
            .parameter
            .property()
            .unwrap()
            .ends_with("/outgoing")
        {
            [12., 16.]
        } else {
            [0.; 2]
        };
        let expected = match entry.target.component {
            None => literal(value),
            Some(axis) => Literal::Distance {
                value: value[usize::from(axis)],
                unit: Unit::Canvas,
            },
        };
        assert_eq!(
            provider
                .sample_parameter(&document, &entry.target, time, &Cancel::default())
                .unwrap(),
            expected
        );
    }
    let channels = provider.animation_channels(&document, Some(id)).unwrap();
    assert_eq!(channels.len(), 4);
    let mut x = channels
        .iter()
        .find(|c| c.component == "X" && c.property == point.id.to_string())
        .unwrap()
        .clone();
    x.curve.keys[1].value = 50.;
    let mut handle_y = channels
        .iter()
        .find(|c| c.component == "Y" && c.property.ends_with("/outgoing"))
        .unwrap()
        .clone();
    handle_y.curve.keys[1].value = 36.;
    let edited = provider
        .edit_animation(&document, &[x.clone(), handle_y], time)
        .unwrap();
    let after = Canvas::from_document(&edited).unwrap();
    let Content::Path { contours, .. } = &after.objects[&id].content else {
        unreachable!()
    };
    let point_after = &contours[0].points[0];
    assert_eq!(point_after.id, point.id);
    assert_eq!(point_after.position_at(time).unwrap(), [30., 40.]);
    assert_eq!(point_after.outgoing_at(time).unwrap(), [12., 24.]);
    assert_eq!(
        point_after.outgoing_track.as_ref().unwrap().id,
        point.outgoing_track.as_ref().unwrap().id
    );
    assert_eq!(
        point_after.outgoing_track.as_ref().unwrap().keys[1].id,
        point.outgoing_track.as_ref().unwrap().keys[1].id
    );
    assert_eq!(
        point_after.position_track.as_ref().unwrap().id,
        point.position_track.as_ref().unwrap().id
    );
    assert_eq!(
        point_after
            .position_track
            .as_ref()
            .unwrap()
            .keys
            .iter()
            .map(|k| k.id)
            .collect::<Vec<_>>(),
        point
            .position_track
            .as_ref()
            .unwrap()
            .keys
            .iter()
            .map(|k| k.id)
            .collect::<Vec<_>>()
    );
    assert!(
        provider
            .edit_animation(&document, &[x.clone(), x.clone()], time)
            .is_err()
    );
    x.component = "Y".into();
    assert!(provider.edit_animation(&document, &[x], time).is_err());
    let mut cleared = provider.animation_channels(&edited, Some(id)).unwrap();
    for channel in &mut cleared {
        channel.curve.keys.clear();
    }
    let stopped = provider.edit_animation(&edited, &cleared, time).unwrap();
    let stopped = Canvas::from_document(&stopped).unwrap();
    let Content::Path { contours, .. } = &stopped.objects[&id].content else {
        unreachable!()
    };
    assert_eq!(contours[0].points[0].position, [30., 40.]);
    assert!(contours[0].points[0].position_track.is_none());
    assert_eq!(contours[0].points[0].outgoing, [12., 24.]);
    assert!(contours[0].points[0].outgoing_track.is_none());
    canvas.objects.get_mut(&id).unwrap().protected = true;
    let protected = canvas.document(document.id).unwrap();
    let mut locked_handle = provider
        .animation_channels(&protected, Some(id))
        .unwrap()
        .into_iter()
        .find(|c| c.property.ends_with("/outgoing") && c.component == "X")
        .unwrap();
    locked_handle.curve.keys[0].value += 1.;
    assert!(
        provider
            .edit_animation(&protected, &[locked_handle], time)
            .is_err()
    );
    assert!(
        provider
            .parameters(&protected)
            .unwrap()
            .iter()
            .filter(|d| d.target.parameter.native() == Some(point.id)
                || d.target
                    .parameter
                    .property()
                    .is_some_and(|key| key.starts_with(&format!("{}/", point.id))))
            .all(|d| !d.editable)
    );
}

#[test]
fn script_image_controls_declare_resources_share_guards_and_do_not_change_program_identity() {
    use fold_foundation::AssetId;
    use fold_graph::types::{AssetReference, Type};
    use fold_motion_canvas::{compilation, scripts::Context};
    use fold_platform::packages::Authoring;
    let first = AssetId::new();
    let second = AssetId::new();
    let script_id = ObjectId::new();
    let control_id = ObjectId::new();
    let control = Control {
        name: "Photo".into(),
        value: ControlValue::Image(first),
        protected: false,
        extensions: [("vendor.note".into(), serde_json::json!("keep"))].into(),
    };
    let object = Object::new("Image script", Content::Script {
        source: "pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> { let href = input.image(\"Photo\")?; Ok(fframes::svgr! { <image href={href} width=\"64\" height=\"64\" /> }) }".into(),
        controls: [(control_id, control.clone())].into(), extensions: Default::default(),
    });
    let canvas = Canvas {
        roots: vec![script_id],
        objects: [(script_id, object)].into(),
        ..Default::default()
    };
    let id = DocumentId::new();
    let document = canvas.document(id).unwrap();
    assert_eq!(document.assets, vec![first]);
    let provider = Provider::default();
    let descriptor = provider
        .parameters(&document)
        .unwrap()
        .into_iter()
        .find(|d| d.target.parameter.native() == Some(control_id))
        .unwrap();
    assert_eq!(descriptor.value_type, Some(Type::AssetRef));
    assert_eq!(
        provider
            .sample_parameter(
                &document,
                &descriptor.target,
                Time::ZERO,
                &Cancel::default()
            )
            .unwrap(),
        Literal::AssetRef {
            value: AssetReference::Resource(first)
        }
    );
    let mut value = control.clone();
    value.value = ControlValue::Image(second);
    let edit = serde_json::to_value(Edit::Control {
        object: script_id,
        id: control_id,
        expected: control.clone(),
        value,
    })
    .unwrap();
    let changed = provider
        .author(&document, std::slice::from_ref(&edit))
        .unwrap();
    assert_eq!(changed.assets, vec![second]);
    assert!(
        provider
            .author(&changed, std::slice::from_ref(&edit))
            .is_err()
    );
    let after = Canvas::from_document(&changed).unwrap();
    let context = Context {
        canvas: &after,
        object: &after.objects[&script_id],
        time: Time::ZERO,
    };
    assert_eq!(context.image("Photo").unwrap(), second.to_string());
    assert!(context.text("Photo").is_err());
    let program = |canvas| {
        compilation::program(canvas, std::path::Path::new("/sdk"), "test")
            .unwrap()
            .identity
    };
    assert_eq!(program(&canvas), program(&after));
    let mut forged = changed.clone();
    forged.assets = vec![first];
    assert!(Canvas::from_document(&forged).is_err());
    let mut locked = canvas.clone();
    if let Content::Script { controls, .. } =
        &mut locked.objects.get_mut(&script_id).unwrap().content
    {
        controls.get_mut(&control_id).unwrap().protected = true;
    }
    assert!(
        provider
            .author(&locked.document(id).unwrap(), &[edit])
            .is_err()
    );
    // Artist command, project publication and Undo use the same control edit.
    let mut project = Project::new(8);
    let assets = [first, second].map(|id| {
        Mutation::PutAsset(fold_project::Asset {
            id,
            location: format!("/tmp/{id}.ppm"),
            fingerprint: "fixture".into(),
            extensions: Default::default(),
        })
    });
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: assets
                .into_iter()
                .chain([Mutation::PutDocument(document.clone())])
                .collect(),
        })
        .unwrap();
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let request = CommandRequest {
        id: package::EDIT.into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(&EditArgs {
            document: id,
            edits: vec![Edit::Control {
                object: script_id,
                id: control_id,
                expected: control.clone(),
                value: Control {
                    value: ControlValue::Image(second),
                    ..control
                },
            }],
        })
        .unwrap(),
    };
    let batch = registry
        .stage(&project.snapshot(), &request, &Cancel::default())
        .unwrap();
    project.commit(batch).unwrap();
    assert_eq!(
        project.snapshot().state().documents[&id].assets,
        vec![second]
    );
    project.undo().unwrap();
    assert_eq!(
        Canvas::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        canvas
    );
    let mut patterned = after.clone();
    patterned.objects.get_mut(&script_id).unwrap().paint.fill = Some(PaintSource::ImagePattern {
        asset: first,
        origin: [0.; 2],
        size: [16.; 2],
        transform: [1., 0., 0., 1., 0., 0.],
        extensions: Default::default(),
    });
    let mut declared = vec![first, second];
    declared.sort();
    assert_eq!(patterned.document(id).unwrap().assets, declared);
    if let Some(PaintSource::ImagePattern { size, .. }) =
        &mut patterned.objects.get_mut(&script_id).unwrap().paint.fill
    {
        size[0] = 0.;
    }
    assert!(patterned.document(id).is_err());
}
