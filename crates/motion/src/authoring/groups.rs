//! Group extraction preserves internal node identity and external socket names.
use super::*;
use crate::{
    graph::{Graph, registry},
    nodes::interface::{GroupDefinition, Port},
};
use std::collections::BTreeMap;
pub fn group_node(motion: &mut Motion, id: ObjectId) -> Result<ObjectId, String> {
    if id == motion.graph.output.node {
        return Err("document Output cannot be extracted into a group".into());
    }
    let original = motion.graph.node(id)?.clone();
    if original.kind == "fold.motion.scene_children" {
        return Err("scene groups already own a child hierarchy; extract a modifier or content node instead".into());
    }
    let (inputs, outputs) = motion.signature(&original)?;
    if outputs.is_empty() {
        return Err("node has no groupable outputs".into());
    }
    let name = registry::find(&original.kind)?.name.to_owned();
    let mut inside = original.clone();
    inside.animation.clear();
    let mut nodes = Vec::new();
    let mut ports = Vec::new();
    for input in inputs {
        let mut node = Node::new("fold.motion.group_input")?;
        node.settings = serde_json::json!({"key":input.id,"value_type":input.kind});
        inside.connect(&input.id, node.id, "value");
        nodes.push(node);
        ports.push(Port {
            section: String::new(),
            advanced: false,
            id: input.id.clone(),
            name: input.id,
            kind: input.kind,
            default: input.default,
            unit: input.unit.into(),
        });
    }
    nodes.push(inside);
    let mut group_outputs = BTreeMap::new();
    for (name, kind) in outputs {
        let mut node = Node::new("fold.motion.group_output")?;
        node.settings = serde_json::json!({"value_type":kind});
        node.connect("value", id, &name);
        group_outputs.insert(
            name,
            Link {
                node: node.id,
                socket: "value".into(),
            },
        );
        nodes.push(node);
    }
    for (i, n) in nodes.iter_mut().enumerate() {
        n.position = [(i % 4) as f32 * 250., (i / 4) as f32 * 230.];
    }
    let group = ObjectId::new();
    motion.groups.insert(
        group,
        GroupDefinition {
            name,
            version: 1,
            inputs: ports,
            graph: Graph {
                nodes,
                output: group_outputs.values().next().unwrap().clone(),
            },
            outputs: group_outputs,
        },
    );
    let mut instance = Node::new("fold.motion.group_instance")?;
    instance.id = id;
    instance.settings = serde_json::json!({"group":group});
    instance.animation = original.animation;
    instance.inputs = original.inputs;
    instance.position = original.position;
    motion.graph.nodes.retain(|n| n.id != id);
    motion.graph.nodes.push(instance);
    motion.validate()?;
    Ok(id)
}

/// Changing a definition preserves compatible wiring and animation targets.
pub fn assign_group(motion: &mut Motion, id: ObjectId, group: ObjectId) -> Result<(), String> {
    let definition = motion.groups.get(&group).ok_or("missing group")?.clone();
    let original = motion.graph.node(id)?;
    if original.kind != "fold.motion.group_instance" {
        return Err("select a group instance".into());
    }
    let previous = motion.signature(original).map(|s| s.0).unwrap_or_default();
    let node = node(motion, id)?;
    node.inputs.retain(|key, _| {
        definition.inputs.iter().any(|port| {
            port.id == *key
                && previous
                    .iter()
                    .any(|old| old.id == *key && old.kind == port.kind)
        })
    });
    node.animation.retain(|path, _| {
        path.rsplit_once('.')
            .is_some_and(|(key, _)| node.inputs.contains_key(key))
    });
    for port in definition.inputs {
        if let Some(default) = port.default {
            node.inputs.entry(port.id).or_insert(Input::Value(default));
        }
    }
    node.settings["group"] = serde_json::to_value(group).unwrap();
    Ok(())
}
