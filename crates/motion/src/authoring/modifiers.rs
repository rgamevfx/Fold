//! Editable modifier definitions, exposed interfaces and ordered stack wiring.
use crate::{
    Motion,
    authoring::node,
    fields::Datum,
    graph::{Graph, Input, Link, Node},
    nodes::interface::{GroupDefinition, Port},
};
use fold_foundation::ObjectId;
use std::collections::BTreeMap;
pub(super) fn group_input(
    nodes: &mut Vec<Node>,
    ports: &mut Vec<Port>,
    id: &str,
    name: &str,
    kind: crate::fields::Kind,
    default: Option<Datum>,
) -> Result<ObjectId, String> {
    let mut n = Node::new("fold.motion.group_input")?;
    n.settings = serde_json::json!({"key":id,"value_type":kind});
    let node = n.id;
    n.position = [0., nodes.len() as f32 * 160.];
    nodes.push(n);
    ports.push(Port {
        section: String::new(),
        advanced: false,
        id: id.into(),
        name: name.into(),
        kind,
        default,
        unit: String::new(),
    });
    Ok(node)
}

/// Built-ins are normal reusable groups with the same public interface as custom tools.
pub fn new_modifier_definition(m: &mut Motion, radial: bool) -> Result<ObjectId, String> {
    use crate::fields::Kind;
    let mut nodes = vec![];
    let mut inputs = vec![];
    let input = group_input(
        &mut nodes,
        &mut inputs,
        "content",
        "Content",
        Kind::Content,
        None,
    )?;
    let mut result = Link {
        node: input,
        socket: "value".into(),
    };
    if radial {
        let mut points = Node::new("fold.motion.radial_points")?;
        for key in ["count", "radius", "start", "sweep", "orient"] {
            let value = match points.inputs.get(key) {
                Some(Input::Value(v)) => v.clone(),
                _ => return Err("missing radial default".into()),
            };
            let source = group_input(&mut nodes, &mut inputs, key, key, value.kind(), Some(value))?;
            inputs.last_mut().unwrap().unit = m
                .signature(&points)?
                .0
                .iter()
                .find(|s| s.id == key)
                .map(|s| s.unit)
                .unwrap_or("")
                .into();
            points.connect(key, source, "value");
        }
        points.position = [280., 0.];
        let mut repeat = Node::new("fold.motion.instance_on_points")?;
        repeat.connect("source", input, "value");
        repeat.connect("points", points.id, "points");
        repeat.position = [560., 0.];
        result = Link {
            node: repeat.id,
            socket: "content".into(),
        };
        nodes.extend([points, repeat]);
    }
    let mut output = Node::new("fold.motion.group_output")?;
    output.settings = serde_json::json!({"value_type":"Content"});
    output.inputs.insert("value".into(), Input::Link(result));
    output.position = [840., 0.];
    let output_link = Link {
        node: output.id,
        socket: "value".into(),
    };
    nodes.push(output);
    let id = ObjectId::new();
    m.groups.insert(
        id,
        GroupDefinition {
            name: if radial {
                "Radial Array"
            } else {
                "Custom Modifier"
            }
            .into(),
            version: 1,
            inputs,
            outputs: BTreeMap::from([("content".into(), output_link.clone())]),
            graph: Graph {
                nodes,
                output: output_link,
            },
        },
    );
    Ok(id)
}

pub fn add_modifier(m: &mut Motion, object: ObjectId, group: ObjectId) -> Result<ObjectId, String> {
    let obj = m
        .scene
        .as_ref()
        .and_then(|s| s.objects.iter().find(|o| o.id == object))
        .ok_or("missing scene object")?
        .clone();
    if obj.locked {
        return Err("unlock this object before editing".into());
    }
    let definition = m
        .groups
        .get(&group)
        .ok_or("missing modifier definition")?
        .clone();
    let mut instance = Node::new("fold.motion.group_instance")?;
    instance.settings = serde_json::json!({"group":group});
    for port in definition.inputs {
        if let Some(value) = port.default {
            instance.set(&port.id, value);
        }
    }
    let input = if let Some(last) = obj.modifiers.last() {
        Link {
            node: *last,
            socket: "content".into(),
        }
    } else if let Some(fill) = obj.appearance {
        Link {
            node: fill,
            socket: "content".into(),
        }
    } else {
        obj.source.clone()
    };
    instance.inputs.insert("content".into(), Input::Link(input));
    let id = instance.id;
    m.graph.nodes.push(instance);
    if let Some(transform) = obj.transform {
        node(m, transform)?.connect("content", id, "content");
    }
    let object = m
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == object)
        .unwrap();
    object.modifiers.push(id);
    if object.transform.is_none() {
        object.output = Link {
            node: id,
            socket: "content".into(),
        };
    }
    Ok(id)
}

/// Expose a literal inside the current group as a stable instance parameter.
pub fn expose_group_input(
    m: &mut Motion,
    group: ObjectId,
    target: ObjectId,
    key: &str,
) -> Result<ObjectId, String> {
    let value = match m.graph.node(target)?.inputs.get(key) {
        Some(Input::Value(v)) => v.clone(),
        _ => return Err("choose a literal input to expose".into()),
    };
    let unit = m
        .signature(m.graph.node(target)?)?
        .0
        .iter()
        .find(|s| s.id == key)
        .map(|s| s.unit)
        .unwrap_or("")
        .to_owned();
    let port = format!("p_{:?}_{}", target, key);
    let mut input = Node::new("fold.motion.group_input")?;
    input.settings = serde_json::json!({"key":port,"value_type":value.kind()});
    let id = input.id;
    m.graph.nodes.push(input);
    node(m, target)?.connect(key, id, "value");
    m.groups
        .get_mut(&group)
        .ok_or("missing group")?
        .inputs
        .push(Port {
            section: String::new(),
            advanced: false,
            id: port,
            name: key.into(),
            kind: value.kind(),
            default: Some(value),
            unit,
        });
    Ok(id)
}

pub fn move_modifier(
    m: &mut Motion,
    object: ObjectId,
    modifier: ObjectId,
    delta: isize,
) -> Result<(), String> {
    let o = m
        .scene
        .as_mut()
        .and_then(|s| s.objects.iter_mut().find(|o| o.id == object))
        .ok_or("missing object")?;
    if o.locked {
        return Err("unlock this object before editing".into());
    }
    let index = o
        .modifiers
        .iter()
        .position(|&id| id == modifier)
        .ok_or("missing modifier")?;
    let target = index
        .saturating_add_signed(delta)
        .min(o.modifiers.len() - 1);
    o.modifiers.swap(index, target);
    reconnect_modifiers(m, object)
}

pub fn remove_modifier(m: &mut Motion, object: ObjectId, modifier: ObjectId) -> Result<(), String> {
    let o = m
        .scene
        .as_mut()
        .and_then(|s| s.objects.iter_mut().find(|o| o.id == object))
        .ok_or("missing object")?;
    if o.locked {
        return Err("unlock this object before editing".into());
    }
    o.modifiers.retain(|&id| id != modifier);
    reconnect_modifiers(m, object)?;
    crate::authoring::remove(m, modifier)
}

pub(super) fn reconnect_modifiers(m: &mut Motion, id: ObjectId) -> Result<(), String> {
    let object = m
        .scene
        .as_ref()
        .and_then(|s| s.objects.iter().find(|o| o.id == id))
        .ok_or("missing object")?
        .clone();
    let mut link = object
        .appearance
        .map(|node| Link {
            node,
            socket: "content".into(),
        })
        .unwrap_or(object.source);
    for modifier in object.modifiers {
        node(m, modifier)?
            .inputs
            .insert("content".into(), Input::Link(link));
        link = Link {
            node: modifier,
            socket: "content".into(),
        };
    }
    if let Some(transform) = object.transform {
        node(m, transform)?
            .inputs
            .insert("content".into(), Input::Link(link));
    } else {
        m.scene
            .as_mut()
            .unwrap()
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .unwrap()
            .output = link;
    }
    Ok(())
}

/// Editable path-motion preset: distribution and color transfer remain ordinary nodes.
pub fn new_path_modifier_definition(m: &mut Motion, path: ObjectId) -> Result<ObjectId, String> {
    use crate::fields::Kind;
    let mut nodes = vec![];
    let mut inputs = vec![];
    let content = group_input(
        &mut nodes,
        &mut inputs,
        "content",
        "Content",
        Kind::Content,
        None,
    )?;
    let mut reference = Node::new("fold.motion.object_reference")?;
    reference.settings = serde_json::json!({"object":path,"world":true,"owner_space":true});
    reference.position = [0., -180.];
    let mut motion = Node::new("fold.motion.path_motion")?;
    motion.connect("content", content, "value");
    motion.connect("path", reference.id, "content");
    for (key, label) in [
        ("count", "Count"),
        ("progress", "Progress"),
        ("speed", "Speed"),
        ("stagger", "Stagger"),
        ("wave", "Wave amplitude"),
        ("frequency", "Wave frequency"),
        ("pulse", "Pulse"),
        ("orient", "Orient to path"),
        ("loop", "Loop"),
    ] {
        let Some(Input::Value(value)) = motion.inputs.get(key).cloned() else {
            return Err("missing path default".into());
        };
        let source = group_input(
            &mut nodes,
            &mut inputs,
            key,
            label,
            value.kind(),
            Some(value),
        )?;
        inputs.last_mut().unwrap().unit = m
            .signature(&motion)?
            .0
            .iter()
            .find(|p| p.id == key)
            .map(|p| p.unit)
            .unwrap_or("")
            .into();
        motion.connect(key, source, "value");
    }
    let mut transfer = Node::new("fold.motion.transfer_color")?;
    transfer.connect("content", motion.id, "content");
    let text_only = group_input(
        &mut nodes,
        &mut inputs,
        "text_only",
        "Text only",
        Kind::Bool,
        Some(Datum::Bool(true)),
    )?;
    transfer.connect("text_only", text_only, "value");
    let mut output = Node::new("fold.motion.group_output")?;
    output.settings = serde_json::json!({"value_type":"Content"});
    output.connect("value", transfer.id, "content");
    motion.position = [350., 0.];
    transfer.position = [650., 0.];
    output.position = [950., 0.];
    let link = Link {
        node: output.id,
        socket: "value".into(),
    };
    nodes.extend([reference, motion, transfer, output]);
    let id = ObjectId::new();
    m.groups.insert(
        id,
        GroupDefinition {
            name: "Path Motion".into(),
            version: 1,
            inputs,
            outputs: BTreeMap::from([("content".into(), link.clone())]),
            graph: Graph {
                nodes,
                output: link,
            },
        },
    );
    Ok(id)
}
