use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    Motion,
    authoring::{self, scene::*},
    evaluation::{Evaluator, compile},
    fields::Datum,
    geometry,
    scene::{ConstraintKind, Mask, MaskOperation, MaskSpace},
};
fn object(m: &mut Motion, id: ObjectId) -> &mut fold_motion::scene::Object {
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap()
}
fn translate(m: &mut Motion, id: ObjectId, p: [f64; 2]) {
    let t = object(m, id).transform.unwrap();
    authoring::node(m, t)
        .unwrap()
        .set("translation", Datum::Vector(p));
}
fn close(a: [f64; 6], b: [f64; 6]) {
    for (a, b) in a.into_iter().zip(b) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }
}
#[test]
fn reparent_preserves_world_placement_and_follow_does_not_change_stacking() {
    let mut m = Motion::new_scene();
    let group = create_group(&mut m).unwrap();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    translate(&mut m, group, [100., 30.]);
    translate(&mut m, shape, [40., 20.]);
    let before = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .world_matrix(shape)
        .unwrap();
    reparent(&mut m, shape, Some(group), Time::ZERO, true).unwrap();
    close(
        Evaluator::new(&m, Time::ZERO)
            .unwrap()
            .world_matrix(shape)
            .unwrap(),
        before,
    );
    reparent(&mut m, shape, None, Time::ZERO, true).unwrap();
    close(
        Evaluator::new(&m, Time::ZERO)
            .unwrap()
            .world_matrix(shape)
            .unwrap(),
        before,
    );
    let order: Vec<_> = m
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .map(|o| o.id)
        .collect();
    add_constraint(
        &mut m,
        shape,
        group,
        ConstraintKind::Follow,
        Time::ZERO,
        true,
    )
    .unwrap();
    translate(&mut m, group, [120., 30.]);
    let world = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .world_matrix(shape)
        .unwrap();
    assert_eq!([world[4], world[5]], [60., 20.]);
    assert_eq!(
        order,
        m.scene
            .as_ref()
            .unwrap()
            .objects
            .iter()
            .map(|o| o.id)
            .collect::<Vec<_>>()
    );
    assert!(
        add_constraint(
            &mut m,
            group,
            shape,
            ConstraintKind::Follow,
            Time::ZERO,
            false
        )
        .is_err()
    );
    let document = m.document(DocumentId::new()).unwrap();
    assert_eq!(Motion::from_document(&document).unwrap(), m);
}
fn alpha(m: &Motion, x: usize, y: usize) -> f32 {
    fold_render::render(compile(m, Time::ZERO, 64, 64).unwrap())
        .unwrap()
        .pixels()[y * 64 + x][3]
}
#[test]
fn isolated_opacity_and_hidden_masks_have_explicit_coverage() {
    let mut m = Motion::new_scene();
    m.info.width = 64;
    m.info.height = 64;
    let group = create_group(&mut m).unwrap();
    for _ in 0..2 {
        let id = create_object(&mut m, "fold.motion.rectangle").unwrap();
        authoring::node(&mut m, id)
            .unwrap()
            .set("width", Datum::Scalar(32.));
        authoring::node(&mut m, id)
            .unwrap()
            .set("height", Datum::Scalar(32.));
        translate(&mut m, id, [0.; 2]);
        object(&mut m, id).parent = Some(group);
    }
    object(&mut m, group).opacity = 0.5;
    assert!((alpha(&m, 10, 10) - 0.75).abs() < 0.01);
    object(&mut m, group).isolated = true;
    assert!((alpha(&m, 10, 10) - 0.5).abs() < 0.01);
    let mask = create_object(&mut m, "fold.motion.rectangle").unwrap();
    authoring::node(&mut m, mask)
        .unwrap()
        .set("width", Datum::Scalar(16.));
    authoring::node(&mut m, mask)
        .unwrap()
        .set("height", Datum::Scalar(32.));
    translate(&mut m, mask, [0.; 2]);
    object(&mut m, mask).visible = false;
    object(&mut m, group)
        .masks
        .push(Mask::new(mask, MaskSpace::World));
    assert!((alpha(&m, 10, 10) - 0.5).abs() < 0.01);
    assert_eq!(alpha(&m, 24, 10), 0.);
    object(&mut m, group).masks[0].invert = true;
    assert_eq!(alpha(&m, 10, 10), 0.);
    assert!((alpha(&m, 24, 10) - 0.5).abs() < 0.01);
    object(&mut m, group).masks[0].invert = false;
    object(&mut m, group).masks[0].operation = MaskOperation::Subtract;
    assert_eq!(alpha(&m, 10, 10), 0.);
    assert!((alpha(&m, 24, 10) - 0.5).abs() < 0.01);
    object(&mut m, group).masks[0].opacity = 0.;
    assert!((alpha(&m, 10, 10) - 0.5).abs() < 0.01);
    object(&mut m, mask)
        .masks
        .push(Mask::new(group, MaskSpace::World));
    assert!(m.validate().is_err());
}
#[test]
fn attributes_sample_by_distance_and_reject_mixed_types() {
    let mut m = Motion::new_scene();
    let path = create_object(&mut m, "fold.motion.path").unwrap();
    authoring::node(&mut m, path).unwrap().settings = serde_json::json!({"segments":[{"Move":[0.,0.]},{"Line":[10.,0.]},{"Line":[100.,0.]}],"attributes":{"color":[{"position":0.,"value":{"Color":[1.,0.,0.,1.]}},{"position":1.,"value":{"Color":[0.,0.,1.,1.]}}]}});
    let content = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .world_content(path)
        .unwrap();
    let path = geometry::path::Path::from_content(&content).unwrap();
    assert_eq!(path.sample(0.5).0, [50., 0.]);
    assert_eq!(
        geometry::path::attribute(&path.attributes["color"], 0.5).unwrap(),
        Datum::Color([0.5, 0., 0.5, 1.])
    );
    let mut attrs = path.attributes;
    attrs.get_mut("color").unwrap()[1].value = Datum::Scalar(1.);
    assert!(geometry::path::validate(&attrs).is_err());
}
#[test]
fn staggered_path_instances_transfer_color_only_to_text_at_arbitrary_times() {
    let mut m = Motion::new_scene();
    let group = create_group(&mut m).unwrap();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    object(&mut m, shape).parent = Some(group);
    let text = create_object(&mut m, "fold.motion.text").unwrap();
    object(&mut m, text).parent = Some(group);
    authoring::node(&mut m, text)
        .unwrap()
        .set("text", Datum::Text("A".into()));
    let path = create_object(&mut m, "fold.motion.path").unwrap();
    object(&mut m, path).visible = false;
    let definition = new_path_modifier_definition(&mut m, path).unwrap();
    let modifier = add_modifier(&mut m, group, definition).unwrap();
    authoring::node(&mut m, modifier)
        .unwrap()
        .set("count", Datum::Scalar(3.));
    authoring::node(&mut m, modifier)
        .unwrap()
        .set("stagger", Datum::Scalar(1.));
    let sample = |time| {
        let mut e = Evaluator::new(&m, time).unwrap();
        let fold_motion::evaluation::Value::Content(c) = e.scene().unwrap() else {
            panic!()
        };
        geometry::drawings(&c, [1.; 2]).unwrap()
    };
    let a = sample(Time::new(2, 1).unwrap());
    let _b = sample(Time::ZERO);
    let c = sample(Time::new(2, 1).unwrap());
    assert_eq!(a.len(), 6);
    for (a, c) in a.iter().zip(&c) {
        assert_eq!(a.transform, c.transform);
        assert_eq!(a.fill, c.fill);
    }
    assert_eq!(a[0].fill, Some([1.; 4]));
    assert_ne!(a[1].fill, Some([1.; 4]));
    assert_ne!(a[1].fill, a[3].fill);
    assert_ne!(a[1].transform, a[3].transform);
    object(&mut m, path)
        .constraints
        .push(fold_motion::scene::Constraint::new(
            group,
            ConstraintKind::Path,
        ));
    assert!(m.validate().is_err());
}

#[test]
fn scene_animation_and_partial_rotation_constraints_are_stable() {
    let mut m = Motion::new_scene();
    m.info.width = 64;
    m.info.height = 64;
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    translate(&mut m, shape, [0.; 2]);
    object(&mut m, shape)
        .animation
        .entry("scene.opacity.0".into())
        .or_default()
        .insert(Time::ZERO, 0.25);
    assert!((alpha(&m, 10, 10) - 0.25).abs() < 0.01);
    let target = create_group(&mut m).unwrap();
    let t = object(&mut m, target).transform.unwrap();
    authoring::node(&mut m, t)
        .unwrap()
        .set("rotation", Datum::Scalar(180.));
    add_constraint(
        &mut m,
        shape,
        target,
        ConstraintKind::Follow,
        Time::ZERO,
        false,
    )
    .unwrap();
    object(&mut m, shape).constraints[0]
        .animation
        .entry("influence.0".into())
        .or_default()
        .insert(Time::ZERO, 0.5);
    let world = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .world_matrix(shape)
        .unwrap();
    assert!(
        (world[0] * world[3] - world[1] * world[2] - 1.).abs() < 1e-9,
        "half rotation must not collapse the shape"
    );
    let group = create_group(&mut m).unwrap();
    translate(&mut m, group, [90., 70.]);
    reparent(&mut m, shape, Some(group), Time::ZERO, true).unwrap();
    close(
        Evaluator::new(&m, Time::ZERO)
            .unwrap()
            .world_matrix(shape)
            .unwrap(),
        world,
    );
}

#[test]
fn reusable_path_modifier_resolves_the_current_owner_space() {
    let mut m = Motion::new_scene();
    let path = create_object(&mut m, "fold.motion.path").unwrap();
    object(&mut m, path).visible = false;
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    translate(&mut m, shape, [0.; 2]);
    let definition = new_path_modifier_definition(&mut m, path).unwrap();
    let modifier = add_modifier(&mut m, shape, definition).unwrap();
    authoring::node(&mut m, modifier)
        .unwrap()
        .set("text_only", Datum::Bool(false));
    authoring::node(&mut m, modifier)
        .unwrap()
        .set("count", Datum::Scalar(2.));
    let sample = |m: &Motion| {
        let fold_motion::evaluation::Value::Content(c) =
            Evaluator::new(m, Time::ZERO).unwrap().scene().unwrap()
        else {
            panic!()
        };
        geometry::drawings(&c, [1.; 2]).unwrap()
    };
    let before = sample(&m);
    let parent = create_group(&mut m).unwrap();
    translate(&mut m, parent, [130., 80.]);
    reparent(&mut m, shape, Some(parent), Time::ZERO, false).unwrap();
    let after = sample(&m);
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(after) {
        close(a.transform, b.transform);
        assert_eq!(a.fill, b.fill);
    }
}

#[test]
fn parent_opacity_respects_child_isolation_and_feather_scales_with_resolution() {
    let mut m = Motion::new_scene();
    m.info.width = 64;
    m.info.height = 64;
    let parent = create_group(&mut m).unwrap();
    object(&mut m, parent).opacity = 0.5;
    let child = create_group(&mut m).unwrap();
    object(&mut m, child).parent = Some(parent);
    object(&mut m, child).isolated = true;
    for _ in 0..2 {
        let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
        translate(&mut m, shape, [0.; 2]);
        object(&mut m, shape).parent = Some(child);
    }
    assert!((alpha(&m, 10, 10) - 0.5).abs() < 0.01);
    let mask = create_object(&mut m, "fold.motion.rectangle").unwrap();
    translate(&mut m, mask, [0.; 2]);
    object(&mut m, mask).visible = false;
    authoring::node(&mut m, mask)
        .unwrap()
        .set("width", Datum::Scalar(32.));
    let mut coverage = Mask::new(mask, MaskSpace::World);
    coverage.feather = 8.;
    object(&mut m, parent).masks.push(coverage);
    let small = fold_render::render(compile(&m, Time::ZERO, 64, 64).unwrap()).unwrap();
    let large = fold_render::render(compile(&m, Time::ZERO, 128, 128).unwrap()).unwrap();
    for x in 27..38 {
        let a = small.pixels()[20 * 64 + x][3];
        let b =
            (large.pixels()[40 * 128 + x * 2][3] + large.pixels()[40 * 128 + x * 2 + 1][3]) * 0.5;
        assert!(
            (a - b).abs() < 0.025,
            "coverage changes with resolution: {a} vs {b}"
        );
    }
}

#[test]
fn transfer_overrides_inherited_instance_color() {
    let mut m = Motion::new_scene();
    let text = create_object(&mut m, "fold.motion.text").unwrap();
    authoring::node(&mut m, text)
        .unwrap()
        .set("text", Datum::Text("A".into()));
    let path = create_object(&mut m, "fold.motion.path").unwrap();
    object(&mut m, path).visible = false;
    let definition = new_path_modifier_definition(&mut m, path).unwrap();
    let modifier = add_modifier(&mut m, text, definition).unwrap();
    authoring::node(&mut m, modifier)
        .unwrap()
        .set("count", Datum::Scalar(1.));
    let graph = &mut m.groups.get_mut(&definition).unwrap().graph;
    let motion = graph
        .nodes
        .iter()
        .find(|n| n.kind == "fold.motion.path_motion")
        .unwrap()
        .id;
    let mut paint = fold_motion::graph::Node::new("fold.motion.set_color_opacity").unwrap();
    paint.connect("content", motion, "content");
    paint.set("value", Datum::Color([1., 0., 0., 1.]));
    graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.transfer_color")
        .unwrap()
        .connect("content", paint.id, "content");
    graph.nodes.push(paint);
    let fold_motion::evaluation::Value::Content(content) =
        Evaluator::new(&m, Time::ZERO).unwrap().scene().unwrap()
    else {
        panic!()
    };
    let drawings = geometry::drawings(&content, [1.; 2]).unwrap();
    assert_eq!(drawings[0].fill, Some([0.05, 0.7, 1., 1.]));
}

#[test]
fn legacy_add_content_command_creates_a_visible_object_in_scene_documents() {
    let mut m = Motion::new_scene();
    let id = authoring::add_content(&mut m, "fold.motion.rectangle").unwrap();
    assert_eq!(m.scene.as_ref().unwrap().objects[0].id, id);
    let graph = compile(&m, Time::ZERO, 64, 64).unwrap();
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| matches!(n,fold_render::ImageOp::Vector(drawings)if !drawings.is_empty()))
    );
}

#[test]
fn extracting_animated_scene_content_preserves_identity_references_and_animation() {
    let mut m = Motion::new_scene();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    for (t, value) in [(0, 40.), (1, 80.)] {
        authoring::node(&mut m, shape)
            .unwrap()
            .animation
            .entry("width.0".into())
            .or_default()
            .insert(Time::new(t, 1).unwrap(), value);
    }
    let before = compile(&m, Time::new(1, 2).unwrap(), 64, 64).unwrap();
    assert_eq!(authoring::group_node(&mut m, shape).unwrap(), shape);
    assert_eq!(m.scene.as_ref().unwrap().objects[0].source.node, shape);
    m.validate().unwrap();
    let a = fold_render::render(before).unwrap();
    let b = fold_render::render(compile(&m, Time::new(1, 2).unwrap(), 64, 64).unwrap()).unwrap();
    assert_eq!(a.pixels(), b.pixels());
}

#[test]
fn scene_drops_preserve_placement_reorder_and_reject_invalid_changes_atomically() {
    let mut m = Motion::new_scene();
    let group = create_group(&mut m).unwrap();
    let nested = create_group(&mut m).unwrap();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    translate(&mut m, group, [100., 30.]);
    translate(&mut m, shape, [40., 20.]);
    let before = Evaluator::new(&m, Time::ZERO)
        .unwrap()
        .world_matrix(shape)
        .unwrap();
    place_object(&mut m, nested, Some(group), None, Time::ZERO).unwrap();
    drop_beside(&mut m, shape, nested, false, Time::ZERO).unwrap();
    assert_eq!(object(&mut m, shape).parent, Some(group));
    let objects = &m.scene.as_ref().unwrap().objects;
    assert!(
        objects.iter().position(|o| o.id == shape) < objects.iter().position(|o| o.id == nested)
    );
    close(
        Evaluator::new(&m, Time::ZERO)
            .unwrap()
            .world_matrix(shape)
            .unwrap(),
        before,
    );
    let saved = m.clone();
    assert!(place_object(&mut m, group, Some(nested), None, Time::ZERO).is_err());
    assert_eq!(m, saved);
    assert!(place_object(&mut m, nested, Some(shape), None, Time::ZERO).is_err());
    assert_eq!(m, saved);
    object(&mut m, group).locked = true;
    let saved = m.clone();
    assert!(place_object(&mut m, shape, Some(group), None, Time::ZERO).is_err());
    assert_eq!(m, saved);
    object(&mut m, group).locked = false;
    place_object(&mut m, shape, None, None, Time::ZERO).unwrap();
    assert_eq!(object(&mut m, shape).parent, None);
    close(
        Evaluator::new(&m, Time::ZERO)
            .unwrap()
            .world_matrix(shape)
            .unwrap(),
        before,
    );
    assert_eq!(
        Motion::from_document(&m.document(DocumentId::new()).unwrap()).unwrap(),
        m
    );
}
