use fold_foundation::{DocumentId, Time};
use fold_motion::{
    Motion,
    authoring::{self, scene::*},
    evaluation::{Evaluator, Value},
    fields::Datum,
    graph::{Input, Node},
};

fn drawings(m: &Motion, time: Time) -> usize {
    let Value::Content(content) = Evaluator::new(m, time).unwrap().scene().unwrap() else {
        panic!("expected content")
    };
    fold_motion::geometry::drawings(&content, [1., 1.])
        .unwrap()
        .len()
}
#[test]
fn radial_modifier_is_an_editable_group_and_round_trips() {
    let mut m = Motion::empty();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    let group = new_modifier_definition(&mut m, true).unwrap();
    let modifier = add_modifier(&mut m, shape, group).unwrap();
    authoring::node(&mut m, modifier)
        .unwrap()
        .set("count", Datum::Scalar(7.));
    assert_eq!(drawings(&m, Time::ZERO), 7);
    m.scene.as_mut().unwrap().objects[0]
        .extensions
        .insert("future".into(), serde_json::json!({"v":3}));
    let d = m.document(DocumentId::new()).unwrap();
    assert_eq!(d.schema_version, 4);
    assert_eq!(Motion::from_document(&d).unwrap(), m);
    let node = m
        .groups
        .get_mut(&group)
        .unwrap()
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.radial_points")
        .unwrap();
    node.set("radius", Datum::Scalar(30.));
    assert_eq!(drawings(&m, Time::ZERO), 7);
}
#[test]
fn hidden_sources_still_feed_procedural_objects_and_cycles_are_rejected() {
    let mut m = Motion::empty();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    let procedural = create_procedural(&mut m).unwrap();
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == shape)
        .unwrap()
        .visible = false;
    let group = m
        .graph
        .node(procedural)
        .unwrap()
        .settings::<fold_motion::nodes::interface::GroupSettings>()
        .unwrap()
        .group
        .unwrap();
    let mut reference = Node::new("fold.motion.object_reference").unwrap();
    reference.settings = serde_json::json!({"object":shape});
    let reference_id = reference.id;
    let g = m.groups.get_mut(&group).unwrap();
    g.graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.group_output")
        .unwrap()
        .connect("value", reference_id, "content");
    g.graph.nodes.push(reference);
    assert_eq!(drawings(&m, Time::ZERO), 1);
    let g = m.groups.get_mut(&group).unwrap();
    g.graph
        .nodes
        .iter_mut()
        .find(|n| n.id == reference_id)
        .unwrap()
        .settings["object"] = serde_json::to_value(procedural).unwrap();
    assert!(m.validate().unwrap_err().contains("cyclic"));
}
#[test]
fn grouping_active_ranges_and_removing_modifiers_preserve_scene_semantics() {
    let mut m = Motion::empty();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    let group = create_group(&mut m).unwrap();
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == shape)
        .unwrap()
        .parent = Some(group);
    let radial = new_modifier_definition(&mut m, true).unwrap();
    let modifier = add_modifier(&mut m, group, radial).unwrap();
    assert_eq!(drawings(&m, Time::ZERO), 12);
    remove_modifier(&mut m, group, modifier).unwrap();
    assert_eq!(drawings(&m, Time::ZERO), 1);
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == group)
        .unwrap()
        .end = Time::new(1, 1).unwrap();
    assert_eq!(drawings(&m, Time::new(1, 1).unwrap()), 0);
    assert_eq!(drawings(&m, Time::ZERO), 1);
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == group)
        .unwrap()
        .parent = Some(group);
    assert!(m.validate().is_err());
}
#[test]
fn legacy_conversion_preserves_original_once_when_new_content_is_added() {
    let mut m = Motion::empty();
    authoring::add_content(&mut m, "fold.motion.rectangle").unwrap();
    assert_eq!(drawings(&m, Time::ZERO), 1);
    create_object(&mut m, "fold.motion.ellipse").unwrap();
    assert_eq!(drawings(&m, Time::ZERO), 2);
    create_object(&mut m, "fold.motion.rectangle").unwrap();
    assert_eq!(drawings(&m, Time::ZERO), 3);
}
#[test]
fn exposed_group_inputs_keep_existing_instance_defaults() {
    let mut m = Motion::empty();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    let group = new_modifier_definition(&mut m, true).unwrap();
    let modifier = add_modifier(&mut m, shape, group).unwrap();
    let mut view = m.clone();
    view.scene = None;
    view.graph = m.groups[&group].graph.clone();
    let points = view
        .graph
        .nodes
        .iter()
        .find(|n| n.kind == "fold.motion.radial_points")
        .unwrap()
        .id;
    let input = expose_group_input(&mut view, group, points, "center").unwrap();
    let definition = view.groups.get_mut(&group).unwrap();
    definition.graph = view.graph.clone();
    m.groups = view.groups;
    assert!(
        m.graph
            .node(modifier)
            .unwrap()
            .inputs
            .values()
            .any(|v| matches!(v, Input::Link(_)))
    );
    assert!(m.groups[&group].graph.node(input).is_ok());
    assert_eq!(drawings(&m, Time::ZERO), 12);
}

#[test]
fn changing_modifier_definition_preserves_content_and_compatible_animation() {
    let mut m = Motion::new_scene();
    let shape = create_object(&mut m, "fold.motion.rectangle").unwrap();
    let group = new_modifier_definition(&mut m, true).unwrap();
    let modifier = add_modifier(&mut m, shape, group).unwrap();
    let content = m.graph.node(modifier).unwrap().inputs["content"].clone();
    authoring::node(&mut m, modifier)
        .unwrap()
        .set("count", Datum::Scalar(7.));
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 40.);
    authoring::node(&mut m, modifier)
        .unwrap()
        .animation
        .insert("radius.0".into(), curve.clone());
    let another = new_modifier_definition(&mut m, true).unwrap();
    authoring::assign_group(&mut m, modifier, another).unwrap();
    assert_eq!(m.graph.node(modifier).unwrap().inputs["content"], content);
    assert_eq!(m.graph.node(modifier).unwrap().animation["radius.0"], curve);
    m.validate().unwrap();
    assert_eq!(drawings(&m, Time::ZERO), 7);
    let passthrough = new_modifier_definition(&mut m, false).unwrap();
    authoring::assign_group(&mut m, modifier, passthrough).unwrap();
    assert!(m.graph.node(modifier).unwrap().animation.is_empty());
    assert_eq!(m.graph.node(modifier).unwrap().inputs["content"], content);
    m.validate().unwrap();
    assert_eq!(drawings(&m, Time::ZERO), 1);
}
