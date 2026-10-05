//! Direct tools edit the same persistent graph as socket wiring. No hidden evaluator.
mod controls;
mod groups;
pub mod path;
pub mod scene;
use crate::{
    Motion,
    fields::Datum,
    geometry::Domain,
    graph::{Input, Link, Node},
};
pub use controls::{expose_input, keyframe_input};
use fold_foundation::ObjectId;
pub use groups::{add_group_input, assign_group, group_node};
pub fn add_node(motion: &mut Motion, kind: &str) -> Result<ObjectId, String> {
    let mut node = Node::new(kind)?;
    node.position = [
        (motion.graph.nodes.len() % 5) as f32 * 240.,
        (motion.graph.nodes.len() / 5) as f32 * 220.,
    ];
    let id = node.id;
    motion.graph.nodes.push(node);
    Ok(id)
}
pub fn connect(
    motion: &mut Motion,
    target: ObjectId,
    input: &str,
    source: Link,
) -> Result<(), String> {
    let mut candidate = motion.clone();
    candidate
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == target)
        .ok_or("missing target")?
        .inputs
        .insert(input.into(), Input::Link(source));
    candidate.validate()?;
    *motion = candidate;
    Ok(())
}
pub fn remove(motion: &mut Motion, id: ObjectId) -> Result<(), String> {
    if id == motion.graph.output.node {
        return Err("cannot delete document Output".into());
    }
    motion.graph.nodes.retain(|n| n.id != id);
    for n in &mut motion.graph.nodes {
        n.inputs
            .retain(|_, input| !matches!(input,Input::Link(l)if l.node==id));
    }
    Ok(())
}
/// Add content as another ordered layer, preserving the previous construction.
pub(super) fn content_nodes(
    motion: &mut Motion,
    kind: &str,
) -> Result<(ObjectId, ObjectId, ObjectId), String> {
    let source = add_node(motion, kind)?;
    if kind == "fold.motion.text" {
        let font = motion.fonts.keys().next().copied().unwrap_or_default();
        motion
            .fonts
            .entry(font)
            .or_insert_with(crate::document::Font::builtin);
        motion
            .graph
            .nodes
            .iter_mut()
            .find(|n| n.id == source)
            .unwrap()
            .settings = serde_json::json!({"font":font});
    }
    let fill = add_node(motion, "fold.motion.fill")?;
    node(motion, fill)?.connect("content", source, "content");
    let transform = add_node(motion, "fold.motion.transform")?;
    let center = [
        motion.info.width as f64 * 0.25,
        motion.info.height as f64 * 0.4,
    ];
    node(motion, transform)?.connect("content", fill, "content");
    node(motion, transform)?.set("translation", Datum::Vector(center));
    Ok((source, fill, transform))
}
pub fn add_content(motion: &mut Motion, kind: &str) -> Result<ObjectId, String> {
    if motion.scene.is_some() {
        return scene::create_object(motion, kind);
    }
    let (source, _, transform) = content_nodes(motion, kind)?;
    let out = motion.graph.output.node;
    let previous = node(motion, out)?.inputs.get("content").cloned();
    if let Some(Input::Link(previous)) = previous {
        let group = add_node(motion, "fold.motion.group")?;
        node(motion, group)?
            .inputs
            .insert("back".into(), Input::Link(previous));
        node(motion, group)?.connect("front", transform, "content");
        node(motion, out)?.connect("content", group, "content");
    } else {
        node(motion, out)?.connect("content", transform, "content");
    }
    Ok(source)
}
pub fn node(motion: &mut Motion, id: ObjectId) -> Result<&mut Node, String> {
    motion
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or("missing motion node".into())
}
/// Insert a modifier after a particular content node, redirecting its existing
/// consumers only. Branches introduced later remain ordinary editable wiring.
pub fn insert_after(
    motion: &mut Motion,
    source: ObjectId,
    modifier: ObjectId,
    input: &str,
) -> Result<(), String> {
    for n in &mut motion.graph.nodes {
        if n.id != modifier {
            for v in n.inputs.values_mut() {
                if let Input::Link(l) = v
                    && l.node == source
                    && l.socket == "content"
                {
                    l.node = modifier;
                }
            }
        }
    }
    node(motion, modifier)?.connect(input, source, "content");
    Ok(())
}
pub fn repeat(
    motion: &mut Motion,
    source: ObjectId,
    distribution: &str,
) -> Result<ObjectId, String> {
    let points = add_node(motion, distribution)?;
    let instances = add_node(motion, "fold.motion.instance_on_points")?;
    node(motion, instances)?.connect("points", points, "points");
    insert_after(motion, source, instances, "source")?;
    Ok(instances)
}
pub fn wave_position(
    motion: &mut Motion,
    source: ObjectId,
    domain: Domain,
) -> Result<ObjectId, String> {
    let time = add_node(motion, "fold.motion.time")?;
    let index = add_node(motion, "fold.motion.index")?;
    let stagger = add_node(motion, "fold.motion.math")?;
    node(motion, stagger)?.settings = serde_json::json!({"operation":"Multiply"});
    node(motion, stagger)?.connect("a", index, "value");
    node(motion, stagger)?.set("b", Datum::Scalar(0.1));
    let phase = add_node(motion, "fold.motion.math")?;
    node(motion, phase)?.connect("a", time, "value");
    node(motion, phase)?.connect("b", stagger, "value");
    let wave = add_node(motion, "fold.motion.wave")?;
    node(motion, wave)?.connect("coordinate", phase, "value");
    node(motion, wave)?.set("amplitude", Datum::Scalar(30.));
    let vector = add_node(motion, "fold.motion.combine_xy")?;
    node(motion, vector)?.connect("y", wave, "value");
    let response = add_node(motion, "fold.motion.set_position")?;
    node(motion, response)?.settings = serde_json::json!({"domain":domain,"combine":"Add"});
    node(motion, response)?.connect("value", vector, "value");
    insert_after(motion, source, response, "content")?;
    Ok(response)
}
pub fn radial_influence(motion: &mut Motion, response: ObjectId) -> Result<ObjectId, String> {
    let position = add_node(motion, "fold.motion.position")?;
    let falloff = add_node(motion, "fold.motion.radial_falloff")?;
    node(motion, falloff)?.connect("position", position, "value");
    node(motion, falloff)?.set("outer", Datum::Scalar(600.));
    node(motion, response)?.connect("weight", falloff, "value");
    Ok(falloff)
}
