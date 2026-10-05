//! Shipped tools are ordinary editable groups. References belong to each instance.
use super::{create_procedural, modifiers::group_input};
use crate::{
    Motion,
    authoring::node,
    fields::{Datum, Kind},
    graph::{Graph, Input, Link, Node},
    nodes::interface::GroupDefinition,
};
use fold_foundation::ObjectId;
use std::collections::BTreeMap;

/// Public content ports retain actual Object Reference nodes, so custom wiring,
/// validation, and the compact reference picker all edit the same construction.
pub fn set_reference(
    m: &mut Motion,
    instance: ObjectId,
    port: &str,
    target: ObjectId,
    local: bool,
) -> Result<(), String> {
    if !m
        .scene
        .as_ref()
        .is_some_and(|s| s.objects.iter().any(|o| o.id == target))
    {
        return Err("Choose an object in this scene".into());
    }
    let existing = match m.graph.node(instance)?.inputs.get(port) {
        Some(Input::Link(link))
            if m.graph.node(link.node)?.kind == "fold.motion.object_reference" =>
        {
            Some(link.node)
        }
        _ => None,
    };
    let id = if let Some(id) = existing {
        id
    } else {
        let mut reference = Node::new("fold.motion.object_reference")?;
        reference.position = [-360., if port == "source" { 0. } else { 240. }];
        let id = reference.id;
        m.graph.nodes.push(reference);
        node(m, instance)?.connect(port, id, "content");
        id
    };
    let settings = &mut node(m, id)?.settings;
    settings["object"] = serde_json::to_value(target).map_err(|e| e.to_string())?;
    if existing.is_none() {
        settings["local"] = local.into();
        settings["world"] = (!local).into();
    }
    Ok(())
}

pub fn reference(m: &Motion, instance: ObjectId, port: &str) -> Option<ObjectId> {
    let Input::Link(link) = m.graph.node(instance).ok()?.inputs.get(port)? else {
        return None;
    };
    let n = m.graph.node(link.node).ok()?;
    (n.kind == "fold.motion.object_reference")
        .then(|| serde_json::from_value(n.settings.get("object")?.clone()).ok())
        .flatten()
}

pub fn along_path(m: &mut Motion, source: ObjectId, path: ObjectId) -> Result<ObjectId, String> {
    let mut proposal = m.clone();
    let id = create_procedural(&mut proposal)?;
    let group = node(&mut proposal, id)?
        .settings::<crate::nodes::interface::GroupSettings>()?
        .group
        .unwrap();
    let mut nodes = vec![];
    let mut inputs = vec![];
    let source_input = group_input(
        &mut nodes,
        &mut inputs,
        "source",
        "Source",
        Kind::Content,
        None,
    )?;
    let path_input = group_input(&mut nodes, &mut inputs, "path", "Path", Kind::Content, None)?;
    let mut points = Node::new("fold.motion.path_points")?;
    points.connect("path", path_input, "value");
    for (key, label) in [
        ("count", "Copies"),
        ("closed", "Closed path"),
        ("orient", "Align to path"),
        ("offset", "Position"),
        ("travel", "Travel"),
        ("duration", "Seconds per loop"),
        ("wave", "Sideways wave"),
        ("frequency", "Wave frequency"),
        ("pulse", "Scale pulse"),
    ] {
        let Some(Input::Value(value)) = points.inputs.get(key).cloned() else {
            return Err("Missing tool default".into());
        };
        let input = group_input(
            &mut nodes,
            &mut inputs,
            key,
            label,
            value.kind(),
            Some(value),
        )?;
        inputs.last_mut().unwrap().unit = proposal
            .signature(&points)?
            .0
            .iter()
            .find(|s| s.id == key)
            .map(|s| s.unit)
            .unwrap_or("")
            .into();
        points.connect(key, input, "value");
    }
    let mut copy = Node::new("fold.motion.instance_on_points")?;
    copy.connect("source", source_input, "value");
    copy.connect("points", points.id, "points");
    let mut color = Node::new("fold.motion.transfer_color")?;
    color.connect("content", copy.id, "content");
    for (key, label, value) in [
        ("enabled", "Path colors", Datum::Bool(false)),
        ("text_only", "Text only", Datum::Bool(true)),
    ] {
        let input = group_input(
            &mut nodes,
            &mut inputs,
            key,
            label,
            value.kind(),
            Some(value),
        )?;
        color.connect(key, input, "value");
    }
    // Keep the useful content flow on one row; parameter inputs form a separate column.
    for (i, n) in nodes.iter_mut().enumerate() {
        n.position = [-420., i as f32 * 130.];
    }
    points.position = [0., 240.];
    copy.position = [360., 0.];
    color.position = [720., 0.];
    let mut out = Node::new("fold.motion.group_output")?;
    out.settings = serde_json::json!({"value_type":"Content"});
    out.connect("value", color.id, "content");
    out.position = [1080., 0.];
    let output = Link {
        node: out.id,
        socket: "value".into(),
    };
    nodes.extend([points, copy, color, out]);
    for port in &mut inputs {
        port.section = match port.id.as_str() {
            "count" | "closed" | "orient" => "Distribution",
            "offset" | "travel" | "duration" => "Travel",
            "wave" | "frequency" | "pulse" => "Secondary motion",
            "enabled" | "text_only" => "Color",
            _ => "Sources",
        }
        .into();
        port.advanced = port.section == "Secondary motion";
    }
    let definition = GroupDefinition {
        name: "Along Path".into(),
        version: 1,
        inputs,
        outputs: BTreeMap::from([("content".into(), output.clone())]),
        graph: Graph { nodes, output },
    };
    for port in &definition.inputs {
        if let Some(value) = &port.default {
            node(&mut proposal, id)?.set(&port.id, value.clone());
        }
    }
    proposal.groups.insert(group, definition);
    let closed = proposal
        .graph
        .node(path)
        .ok()
        .and_then(|n| n.settings.get("segments"))
        .and_then(|v| v.as_array())
        .is_some_and(|segments| segments.iter().any(|s| s.as_str() == Some("Close")));
    node(&mut proposal, id)?.set("closed", Datum::Bool(closed));
    set_reference(&mut proposal, id, "source", source, true)?;
    set_reference(&mut proposal, id, "path", path, false)?;
    let transform = proposal
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| o.id == id)
        .and_then(|o| o.transform)
        .unwrap();
    node(&mut proposal, id)?.position = [0., 0.];
    node(&mut proposal, transform)?.position = [380., 0.];
    let scene = proposal.scene.as_mut().unwrap();
    scene.objects.iter_mut().find(|o| o.id == id).unwrap().name = "Along Path".into();
    // The source remains editable but is no longer drawn independently.
    scene
        .objects
        .iter_mut()
        .find(|o| o.id == source)
        .unwrap()
        .visible = false;
    proposal.validate()?;
    *m = proposal;
    Ok(id)
}

/// A useful editable source: its text and shape remain ordinary scene children.
pub fn badge(m: &mut Motion) -> Result<ObjectId, String> {
    let group = super::create_group(m)?;
    let shape = super::create_object(m, "fold.motion.rectangle")?;
    node(m, shape)?.set("width", Datum::Scalar(100.));
    node(m, shape)?.set("height", Datum::Scalar(44.));
    node(m, shape)?.set("radius", Datum::Scalar(10.));
    let text = super::create_object(m, "fold.motion.text")?;
    node(m, text)?.set("text", Datum::Text("FLOW".into()));
    node(m, text)?.set("size", Datum::Scalar(22.));
    node(m, text)?.set("alignment", Datum::Scalar(0.5));
    for (id, name, xy, color) in [
        (shape, "Background", [0., 0.], [0.065, 0.085, 0.14, 1.]),
        (text, "Text", [0., -15.], [0.9, 0.95, 1., 1.]),
    ] {
        let object = m
            .scene
            .as_mut()
            .unwrap()
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .unwrap();
        object.name = name.into();
        object.parent = Some(group);
        let transform = object.transform.unwrap();
        let appearance = object.appearance.unwrap();
        node(m, transform)?.set("translation", Datum::Vector(xy));
        node(m, appearance)?.set("color", Datum::Color(color));
    }
    let mut reference = Node::new("fold.motion.object_reference")?;
    reference.settings = serde_json::json!({"object":text});
    let reference_id = reference.id;
    m.graph.nodes.push(reference);
    let mut fitted = Node::new("fold.motion.fit_background")?;
    fitted.id = shape;
    fitted.connect("content", reference_id, "content");
    *node(m, shape)? = fitted;
    let object = m
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == group)
        .unwrap();
    object.name = "Badge".into();
    let transform = object.transform.unwrap();
    let center = [m.info.width as f64 * 0.5, m.info.height as f64 * 0.5];
    node(m, transform)?.set("translation", Datum::Vector(center));
    Ok(group)
}

/// Wrap a primitive as an editable published tool, using the same extraction
/// command available to artists; no parallel built-in evaluation path.
pub fn apply_style(m: &mut Motion, object: ObjectId, kind: &str) -> Result<ObjectId, String> {
    let id = crate::authoring::add_node(m, kind)?;
    crate::authoring::group_node(m, id)?;
    let group = m
        .graph
        .node(id)?
        .settings::<crate::nodes::interface::GroupSettings>()?
        .group
        .ok_or("Missing tool definition")?;
    crate::authoring::remove(m, id)?;
    super::add_modifier(m, object, group)
}

pub fn oscillate(m: &mut Motion, object: ObjectId) -> Result<ObjectId, String> {
    let group = super::new_modifier_definition(m, false)?;
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
    let time = Node::new("fold.motion.time")?;
    let mut wave = Node::new("fold.motion.wave")?;
    wave.connect("coordinate", time.id, "value");
    for (key, name, value) in [
        ("amplitude", "Distance", 20.),
        ("frequency", "Frequency", 0.5),
        ("phase", "Phase", 0.),
    ] {
        let input = group_input(
            &mut nodes,
            &mut inputs,
            key,
            name,
            Kind::Scalar,
            Some(Datum::Scalar(value)),
        )?;
        inputs.last_mut().unwrap().unit = match key {
            "amplitude" => "px",
            "frequency" => "Hz",
            _ => "factor",
        }
        .into();
        wave.connect(key, input, "value");
    }
    let mut vector = Node::new("fold.motion.combine_xy")?;
    vector.connect("y", wave.id, "value");
    let mut transform = Node::new("fold.motion.transform")?;
    transform.connect("content", content, "value");
    transform.connect("translation", vector.id, "value");
    let mut output = Node::new("fold.motion.group_output")?;
    output.settings = serde_json::json!({"value_type":"Content"});
    output.connect("value", transform.id, "content");
    let link = Link {
        node: output.id,
        socket: "value".into(),
    };
    nodes.extend([time, wave, vector, transform, output]);
    for (i, n) in nodes.iter_mut().enumerate() {
        n.position = [(i % 4) as f32 * 350., (i / 4) as f32 * 250.];
    }
    m.groups.insert(
        group,
        GroupDefinition {
            name: "Oscillate".into(),
            version: 1,
            inputs,
            outputs: BTreeMap::from([("content".into(), link.clone())]),
            graph: Graph {
                nodes,
                output: link,
            },
        },
    );
    super::add_modifier(m, object, group)
}

#[path = "duplicator.rs"]
mod duplicator;
pub use duplicator::{
    attach_copy_palette, attach_copy_wave, copy_driver, duplicate, is_duplicator,
};

#[path = "oscillator.rs"]
mod oscillator;
pub use oscillator::attach_oscillator;

#[path = "color_ramp.rs"]
mod color_ramp;
pub use color_ramp::{attach_color_ramp, set_color_ramp};
