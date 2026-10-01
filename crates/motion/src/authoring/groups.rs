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
    let (inputs, outputs) = motion.signature(&original)?;
    if outputs.is_empty() {
        return Err("node has no groupable outputs".into());
    }
    let name = registry::find(&original.kind)?.name.to_owned();
    let mut inside = original.clone();
    let mut nodes = Vec::new();
    let mut ports = Vec::new();
    for input in inputs {
        let mut node = Node::new("fold.motion.group_input")?;
        node.settings = serde_json::json!({"key":input.id,"value_type":input.kind});
        inside.connect(&input.id, node.id, "value");
        nodes.push(node);
        ports.push(Port {
            id: input.id.clone(),
            name: input.id,
            kind: input.kind,
            default: input.default,
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
    instance.settings = serde_json::json!({"group":group});
    instance.inputs = original.inputs;
    instance.position = original.position;
    let instance_id = instance.id;
    motion.graph.nodes.retain(|n| n.id != id);
    for n in &mut motion.graph.nodes {
        for value in n.inputs.values_mut() {
            if let Input::Link(link) = value
                && link.node == id
            {
                link.node = instance_id;
            }
        }
    }
    motion.graph.nodes.push(instance);
    motion.validate()?;
    Ok(instance_id)
}
