//! Structural authoring shared by the public batch interface. All edits apply to
//! a candidate Motion; the caller validates and commits the complete transaction.
use crate::{
    Motion,
    graph::{Graph, Input, Link, Node},
    nodes::interface::{GroupDefinition, Port},
};
use fold_foundation::ObjectId;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Edit {
    Wrangle {
        node: ObjectId,
        source: String,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    CreateControls {
        node: ObjectId,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    CreateNode {
        id: ObjectId,
        kind: String,
        #[serde(default)]
        settings: Option<Value>,
        #[serde(default)]
        inputs: BTreeMap<String, Input>,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Publish {
        group: ObjectId,
        node: ObjectId,
        input: String,
        port: String,
        name: String,
        #[serde(default)]
        section: String,
    },
    Port {
        group: ObjectId,
        port: String,
        name: Option<String>,
        section: Option<String>,
        advanced: Option<bool>,
    },
    Insert {
        node: ObjectId,
        input: String,
        modifier: ObjectId,
        modifier_input: String,
        output: String,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Replace {
        node: ObjectId,
        kind: String,
        #[serde(default)]
        settings: Option<Value>,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Duplicate {
        node: ObjectId,
        id: ObjectId,
        independent: bool,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    GroupNodes {
        nodes: Vec<ObjectId>,
        id: ObjectId,
        definition: ObjectId,
        name: String,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Layout {
        nodes: Vec<ObjectId>,
        #[serde(default)]
        group: Option<ObjectId>,
        #[serde(default)]
        origin: [f32; 2],
    },
}
pub(super) fn accepts(op: &str) -> bool {
    matches!(
        op,
        "wrangle"
            | "create_controls"
            | "create_node"
            | "publish"
            | "port"
            | "insert"
            | "replace"
            | "duplicate"
            | "group_nodes"
            | "layout"
    )
}
fn node_mut(g: &mut Graph, id: ObjectId) -> Result<&mut Node, String> {
    g.nodes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or("missing node".into())
}
fn fresh(g: &Graph, id: ObjectId) -> Result<(), String> {
    if g.nodes.iter().any(|n| n.id == id) {
        Err("node ID already exists".into())
    } else {
        Ok(())
    }
}
fn configured(
    m: &Motion,
    id: ObjectId,
    kind: &str,
    settings: Option<Value>,
    inputs: BTreeMap<String, Input>,
) -> Result<Node, String> {
    let mut n = Node::new(kind)?;
    n.id = id;
    if let Some(settings) = settings {
        fold_platform::authoring::merge(&mut n.settings, &settings);
    }
    // Instantiate defaults after specialization, not before (e.g. Vector Math).
    n.inputs = m
        .signature(&n)?
        .0
        .into_iter()
        .filter_map(|s| s.default.map(|v| (s.id, Input::Value(v))))
        .collect();
    n.inputs.extend(inputs);
    Ok(n)
}
pub(super) fn apply(
    m: &mut Motion,
    value: &Value,
    cancel: &fold_media::Cancel,
) -> Result<(), String> {
    match fold_platform::authoring::decode::<Edit>(value)? {
        Edit::Wrangle {
            node,
            source,
            group,
        } => {
            let n = node_mut(super::graph(m, group)?, node)?;
            if n.kind != "fold.motion.wrangle" {
                return Err("select a Rust Wrangle".into());
            }
            n.settings["source"] = source.into();
            crate::wrangle::sync_controls(n)?;
            let s = n.settings::<crate::wrangle::Settings>()?;
            crate::wrangle::compiler::compile(&s.source, &s.channels, cancel)?;
        }
        Edit::CreateControls { node, group } => {
            let n = node_mut(super::graph(m, group)?, node)?;
            if n.kind != "fold.motion.wrangle" {
                return Err("select a Rust Wrangle".into());
            }
            crate::wrangle::sync_controls(n)?;
        }
        Edit::CreateNode {
            id,
            kind,
            settings,
            inputs,
            group,
        } => {
            let mut n = configured(m, id, &kind, settings, inputs)?;
            if kind == "fold.motion.wrangle" {
                crate::wrangle::sync_controls(&mut n)?;
                let s = n.settings::<crate::wrangle::Settings>()?;
                crate::wrangle::compiler::compile(&s.source, &s.channels, cancel)?;
            }
            let g = super::graph(m, group)?;
            fresh(g, id)?;
            g.nodes.push(n);
        }
        Edit::Publish {
            group,
            node,
            input,
            port,
            name,
            section,
        } => {
            if port.is_empty() || name.is_empty() {
                return Err("port ID and name must be nonempty".into());
            }
            let definition = m.groups.get(&group).ok_or("missing group")?;
            if definition.inputs.iter().any(|p| p.id == port) {
                return Err("port ID already exists; use port to edit metadata".into());
            }
            let target = definition.graph.node(node)?;
            let animation: Vec<_> = target
                .animation
                .iter()
                .filter_map(|(path, curve)| {
                    path.rsplit_once('.')
                        .filter(|(key, _)| *key == input)
                        .map(|(_, component)| (component.to_owned(), curve.clone()))
                })
                .collect();
            if !animation.is_empty()
                && !std::iter::once(&m.graph)
                    .chain(m.groups.values().map(|g| &g.graph))
                    .any(|g| {
                        g.nodes.iter().any(|n| {
                            n.kind == "fold.motion.group_instance"
                                && n.settings["group"] == json!(group)
                        })
                    })
            {
                return Err(
                    "cannot move animation to an unused definition; create an instance first"
                        .into(),
                );
            }
            let socket = m
                .signature(target)?
                .0
                .into_iter()
                .find(|s| s.id == input)
                .ok_or("unknown input")?;
            let default = match target.inputs.get(&input) {
                Some(Input::Value(v)) => Some(v.clone()),
                Some(Input::Link(_)) => {
                    return Err("publishing a connected input would replace its driver".into());
                }
                None => socket.default,
            };
            let mut n = Node::new("fold.motion.group_input")?;
            n.settings = json!({"key":port,"value_type":socket.kind});
            let definition = m.groups.get_mut(&group).unwrap();
            let target = node_mut(&mut definition.graph, node)?;
            target
                .animation
                .retain(|path, _| !path.rsplit_once('.').is_some_and(|(key, _)| key == input));
            target.connect(&input, n.id, "value");
            definition.graph.nodes.push(n);
            definition.inputs.push(Port {
                id: port.clone(),
                name,
                kind: socket.kind,
                default: default.clone(),
                unit: socket.unit.into(),
                section,
                advanced: false,
            });
            if !animation.is_empty() {
                for graph in
                    std::iter::once(&mut m.graph).chain(m.groups.values_mut().map(|g| &mut g.graph))
                {
                    for n in &mut graph.nodes {
                        if n.kind == "fold.motion.group_instance"
                            && n.settings["group"] == json!(group)
                        {
                            n.inputs.insert(
                                port.clone(),
                                Input::Value(
                                    default.clone().ok_or("animated input has no base value")?,
                                ),
                            );
                            for (component, curve) in &animation {
                                n.animation
                                    .insert(format!("{port}.{component}"), curve.clone());
                            }
                        }
                    }
                }
            }
        }
        Edit::Port {
            group,
            port,
            name,
            section,
            advanced,
        } => {
            let p = m
                .groups
                .get_mut(&group)
                .ok_or("missing group")?
                .inputs
                .iter_mut()
                .find(|p| p.id == port)
                .ok_or("missing port")?;
            if let Some(name) = name {
                if name.is_empty() {
                    return Err("name must not be empty".into());
                }
                p.name = name;
            }
            if let Some(section) = section {
                p.section = section;
            }
            if let Some(advanced) = advanced {
                p.advanced = advanced;
            }
        }
        Edit::Insert {
            node,
            input,
            modifier,
            modifier_input,
            output,
            group,
        } => {
            if node == modifier {
                return Err("cannot insert a node into itself".into());
            }
            let g = super::graph(m, group)?;
            let previous = g
                .node(node)?
                .inputs
                .get(&input)
                .cloned()
                .ok_or("input has no authored value/link")?;
            node_mut(g, modifier)?
                .inputs
                .insert(modifier_input, previous);
            node_mut(g, node)?.connect(&input, modifier, &output);
        }
        Edit::Replace {
            node,
            kind,
            settings,
            group,
        } => {
            let old = super::graph(m, group)?.node(node)?.clone();
            let mut replacement = configured(m, node, &kind, settings, BTreeMap::new())?;
            let old_sig = m.signature(&old)?;
            let new_sig = m.signature(&replacement)?;
            for (key, value) in &old.inputs {
                if !new_sig.0.iter().any(|p| {
                    p.id == *key && old_sig.0.iter().any(|o| o.id == *key && o.kind == p.kind)
                }) {
                    return Err(format!(
                        "replacement cannot preserve input {key}; disconnect/remove explicitly first"
                    ));
                }
                replacement.inputs.insert(key.clone(), value.clone());
            }
            // Preserve outputs even if disconnected: scene/group references may use them.
            if old_sig.1.iter().any(|p| !new_sig.1.contains(p)) {
                return Err("replacement cannot preserve output interface".into());
            }
            replacement.animation = old.animation;
            replacement.position = old.position;
            replacement.extensions = old.extensions;
            *node_mut(super::graph(m, group)?, node)? = replacement;
        }
        Edit::Duplicate {
            node,
            id,
            independent,
            group,
        } => {
            let mut n = super::graph(m, group)?.node(node)?.clone();
            fresh(super::graph(m, group)?, id)?;
            n.id = id;
            n.position[0] += 40.;
            n.position[1] += 40.;
            if independent
                && matches!(
                    n.kind.as_str(),
                    "fold.motion.group_instance" | "fold.motion.solver"
                )
            {
                let old: ObjectId = serde_json::from_value(n.settings["group"].clone())
                    .map_err(|e| e.to_string())?;
                let new = clone_definition(m, old, &mut BTreeMap::new())?;
                n.settings["group"] = json!(new);
            }
            if independent && n.kind == "fold.motion.published_control" {
                clone_control(m, &mut n)?;
            }
            super::graph(m, group)?.nodes.push(n);
        }
        Edit::GroupNodes {
            nodes,
            id,
            definition,
            name,
            group,
        } => group_nodes(m, nodes, id, definition, name, group)?,
        Edit::Layout {
            nodes,
            group,
            origin,
        } => {
            if nodes.len() > 4096 || !origin.iter().all(|v| v.is_finite()) {
                return Err("invalid layout request".into());
            }
            let g = super::graph(m, group)?;
            for (i, id) in nodes.iter().enumerate() {
                node_mut(g, *id)?.position = [
                    origin[0] + (i % 4) as f32 * 250.,
                    origin[1] + (i / 4) as f32 * 180.,
                ];
            }
        }
    }
    Ok(())
}
fn clone_definition(
    m: &mut Motion,
    id: ObjectId,
    mapping: &mut BTreeMap<ObjectId, ObjectId>,
) -> Result<ObjectId, String> {
    if let Some(id) = mapping.get(&id) {
        return Ok(*id);
    }
    if mapping.len() >= 128 {
        return Err("group copy exceeds group budget".into());
    }
    let mut g = m.groups.get(&id).ok_or("missing group")?.clone();
    let new = ObjectId::new();
    mapping.insert(id, new);
    for n in &mut g.graph.nodes {
        if n.kind == "fold.motion.published_control" {
            clone_control(m, n)?;
        }
        if matches!(
            n.kind.as_str(),
            "fold.motion.group_instance" | "fold.motion.solver"
        ) && !n.settings["group"].is_null()
        {
            let old =
                serde_json::from_value(n.settings["group"].clone()).map_err(|e| e.to_string())?;
            n.settings["group"] = json!(clone_definition(m, old, mapping)?);
        }
    }
    m.groups.insert(new, g);
    Ok(new)
}
fn clone_control(m: &mut Motion, node: &mut Node) -> Result<(), String> {
    let old: ObjectId =
        serde_json::from_value(node.settings["id"].clone()).map_err(|e| e.to_string())?;
    let new = ObjectId::new();
    node.settings["id"] = json!(new);
    if let Some(value) = m.controls.get(&old).cloned() {
        m.controls.insert(new, value);
    }
    Ok(())
}
fn group_nodes(
    m: &mut Motion,
    ids: Vec<ObjectId>,
    id: ObjectId,
    definition: ObjectId,
    name: String,
    scope: Option<ObjectId>,
) -> Result<(), String> {
    let selected: BTreeSet<_> = ids.iter().copied().collect();
    if selected.is_empty() || selected.len() != ids.len() || m.groups.contains_key(&definition) {
        return Err("invalid selection or existing definition ID".into());
    }
    if scope.is_none()
        && m.scene.as_ref().is_some_and(|s| {
            s.objects.iter().any(|o| {
                selected
                    .iter()
                    .any(|id| o.owns(*id) || o.source.node == *id || o.output.node == *id)
            })
        })
    {
        return Err("group internal processing nodes, not scene-owned nodes".into());
    }
    let original = super::graph(m, scope)?.clone();
    fresh(&original, id)?;
    if selected.contains(&original.output.node) {
        return Err("cannot group the graph output node".into());
    }
    let mut nodes: Vec<_> = ids
        .iter()
        .map(|id| original.node(*id).cloned())
        .collect::<Result<_, _>>()?;
    if nodes.iter().any(|n| {
        matches!(
            n.kind.as_str(),
            "fold.motion.group_input" | "fold.motion.group_output"
        )
    }) {
        return Err("keep group interface nodes outside extracted processing groups".into());
    }
    let mut ports = Vec::new();
    let mut bindings = BTreeMap::new();
    let mut inputs = Vec::new();
    let mut exports = BTreeMap::new();
    for n in &mut nodes {
        let signature = m.signature(n)?;
        for (key, value) in &mut n.inputs {
            if matches!(value,Input::Link(l) if !selected.contains(&l.node)) {
                let port = format!("input_{}", ports.len());
                let socket = signature
                    .0
                    .iter()
                    .find(|s| s.id == *key)
                    .ok_or("unknown input")?;
                let mut input = Node::new("fold.motion.group_input")?;
                input.settings = json!({"key":port,"value_type":socket.kind});
                bindings.insert(port.clone(), value.clone());
                *value = Input::Link(Link {
                    node: input.id,
                    socket: "value".into(),
                });
                inputs.push(input);
                ports.push(Port {
                    id: port,
                    name: key.clone(),
                    kind: socket.kind,
                    default: None,
                    unit: socket.unit.into(),
                    section: String::new(),
                    advanced: false,
                });
            }
        }
        for (socket, kind) in signature.1 {
            let external = original
                .nodes
                .iter()
                .filter(|o| !selected.contains(&o.id))
                .any(|o| {
                    o.inputs
                        .values()
                        .any(|v| matches!(v,Input::Link(l)if l.node==n.id&&l.socket==socket))
                });
            if external {
                exports.insert((n.id, socket), kind);
            }
        }
    }
    // A detached branch still needs a usable output.
    if exports.is_empty() {
        let n = nodes.last().unwrap();
        for (socket, kind) in m.signature(n)?.1 {
            exports.insert((n.id, socket), kind);
        }
    }
    if exports.is_empty() {
        return Err("selection has no outputs".into());
    }
    nodes.extend(inputs);
    let mut outputs = BTreeMap::new();
    let mut remap = BTreeMap::new();
    for ((source, socket), kind) in exports {
        let key = format!("output_{}", outputs.len());
        let mut out = Node::new("fold.motion.group_output")?;
        out.settings = json!({"value_type":kind});
        out.connect("value", source, &socket);
        outputs.insert(
            key.clone(),
            Link {
                node: out.id,
                socket: "value".into(),
            },
        );
        nodes.push(out);
        remap.insert((source, socket), key);
    }
    let mut instance = Node::new("fold.motion.group_instance")?;
    instance.id = id;
    instance.settings = json!({"group":definition});
    instance.inputs = bindings;
    instance.position = original.node(ids[0])?.position;
    let g = super::graph(m, scope)?;
    g.nodes.retain(|n| !selected.contains(&n.id));
    for n in &mut g.nodes {
        for v in n.inputs.values_mut() {
            if let Input::Link(l) = v {
                if let Some(key) = remap.get(&(l.node, l.socket.clone())) {
                    *l = Link {
                        node: id,
                        socket: key.clone(),
                    };
                }
            }
        }
    }
    g.nodes.push(instance);
    m.groups.insert(
        definition,
        GroupDefinition {
            name,
            version: 1,
            inputs: ports,
            graph: Graph {
                nodes,
                output: outputs.values().next().unwrap().clone(),
            },
            outputs,
        },
    );
    Ok(())
}
