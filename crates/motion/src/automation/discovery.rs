//! Bounded graph queries with resolved instance signatures and stable role handles.
use crate::{
    Motion,
    fields::Kind,
    graph::{Input, Node},
};
use fold_foundation::ObjectId;
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub fn signature(m: &Motion, n: &Node) -> Result<Value, String> {
    let (inputs, outputs) = m.signature(n)?;
    Ok(
        json!({"inputs":inputs.iter().map(|s|json!({"id":s.id,"kind":s.kind,"default":s.default,"unit":s.unit,"sampling":sampling(&n.kind,&s.id,s.kind),"binding":n.inputs.get(&s.id)})).collect::<Vec<_>>(),"outputs":outputs,"settings":n.settings,"animation":n.animation}),
    )
}
pub fn query(document: &fold_project::Document, q: &Value) -> Result<Value, String> {
    query_cancellable(document, q, &fold_media::Cancel::default())
}
pub fn query_cancellable(
    document: &fold_project::Document,
    q: &Value,
    cancel: &fold_media::Cancel,
) -> Result<Value, String> {
    cancel.check()?;
    let m = Motion::from_document(document)?;
    if q["mode"] == "signature" {
        let kind = q["kind"]
            .as_str()
            .ok_or("kind required for a candidate signature")?;
        let mut node = Node::new(kind)?;
        if let Some(settings) = q.get("settings") {
            fold_platform::authoring::merge(&mut node.settings, settings);
        }
        if node.kind == "fold.motion.wrangle" {
            crate::wrangle::sync_controls(&mut node)?;
        }
        (crate::graph::registry::find(kind)?.validate)(&node)?;
        let mut value = signature(&m, &node)?;
        value["metadata"] = metadata(kind, &node.settings);
        return Ok(value);
    }
    if q["mode"] == "validate" {
        let edits = q["edits"]
            .as_array()
            .ok_or("edits required for validation")?;
        if edits.len() > 512 {
            return Err("at most 512 edits".into());
        }
        return Ok(match super::author_cancellable(document, edits, cancel) {
            Ok(_) => json!({"valid":true,"diagnostics":[]}),
            Err(message) => {
                let detail = message
                    .find('{')
                    .and_then(|start| serde_json::from_str::<Value>(&message[start..]).ok());
                json!({"valid":false,"diagnostics":detail.as_ref().and_then(|d|d.get("diagnostics")).cloned().unwrap_or_else(||json!([{"message":message}]))})
            }
        });
    }

    let group: Option<ObjectId> =
        serde_json::from_value(q.get("group").cloned().unwrap_or(Value::Null))
            .map_err(|e| e.to_string())?;
    let g = match group {
        Some(id) => &m.groups.get(&id).ok_or("missing group")?.graph,
        None => &m.graph,
    };
    let limit = q["limit"].as_u64().unwrap_or(50).clamp(1, 200) as usize;
    if q["mode"] == "handles" {
        let ids: Vec<ObjectId> =
            serde_json::from_value(q["nodes"].clone()).map_err(|e| e.to_string())?;
        if ids.len() > 200 {
            return Err("at most 200 handle targets".into());
        }
        return Ok(json!(ids.iter().map(|id| {
            let object=m.scene.as_ref().and_then(|s|s.objects.iter().find(|o|o.id==*id));
            let node=g.nodes.iter().find(|n|n.id==*id);
            json!({"id":id,"object":object,"definition":node.and_then(|n|n.settings.get("group")),"signature":node.map(|n|signature(&m,n).unwrap_or_else(|e|json!({"error":e})))}) }).collect::<Vec<_>>()));
    }
    let node: Option<ObjectId> =
        serde_json::from_value(q.get("node").cloned().unwrap_or(Value::Null))
            .map_err(|e| e.to_string())?;
    let mut ids = BTreeSet::new();
    if let Some(id) = node {
        g.node(id)?;
        ids.insert(id);
    }
    match q["mode"].as_str().unwrap_or("nodes") {
        "upstream" | "downstream" => {
            if node.is_none() {
                return Err("node is required for neighborhood queries".into());
            }
            let depth = q["depth"].as_u64().unwrap_or(1).min(32);
            for _ in 0..depth {
                let mut next = ids.clone();
                for n in &g.nodes {
                    for v in n.inputs.values() {
                        if let Input::Link(l) = v {
                            if q["mode"] == "upstream" && ids.contains(&n.id) {
                                next.insert(l.node);
                            }
                            if q["mode"] == "downstream" && ids.contains(&l.node) {
                                next.insert(n.id);
                            }
                        }
                    }
                }
                if next.len() > 200 {
                    return Err("neighborhood exceeds 200 nodes; reduce depth".into());
                }
                if ids == next {
                    break;
                }
                ids = next;
            }
        }
        "nodes" => {
            if node.is_none() {
                ids.extend(g.nodes.iter().map(|n| n.id));
            }
        }
        _ => return Err("mode must be nodes, upstream, downstream or handles".into()),
    }
    let offset = q["offset"].as_u64().unwrap_or(0) as usize;
    let nodes:Vec<_>=g.nodes.iter().filter(|n|ids.contains(&n.id)).skip(offset).take(limit).map(|n|{
        let consumers:Vec<_>=g.nodes.iter().flat_map(|other|other.inputs.iter().filter_map(|(socket,v)|match v{Input::Link(l)if l.node==n.id=>Some(json!({"node":other.id,"input":socket,"output":l.socket})),_=>None})).collect();
        json!({"node":n,"resolved":signature(&m,n).unwrap_or_else(|e|json!({"error":e})),"consumers":consumers})
    }).collect();
    Ok(json!({"group":group,"total":ids.len(),"offset":offset,"nodes":nodes}))
}
pub fn metadata(id: &str, defaults: &Value) -> Value {
    let mut choices = json!({});
    if defaults.get("domain").is_some() {
        choices["domain"] = json!(["Points", "Instances", "Glyphs", "Children"]);
    }
    if defaults.get("combine").is_some() {
        choices["combine"] = json!(["Replace", "Add", "Multiply"]);
    }
    match id {
        "fold.motion.math" => {
            choices["operation"] = json!([
                "Add", "Subtract", "Multiply", "Divide", "Modulo", "Min", "Max", "Absolute",
                "Negate", "Round", "Floor", "Ceil", "Truncate"
            ]);
            choices["value_type"] = json!([Kind::Scalar, Kind::Vector, Kind::Color]);
        }
        "fold.motion.mix" => {
            choices["value_type"] = json!([Kind::Scalar, Kind::Vector, Kind::Color]);
        }
        "fold.motion.compare" => {
            choices["operation"] = json!([
                "Less",
                "LessEqual",
                "Equal",
                "Greater",
                "GreaterEqual",
                "NotEqual"
            ]);
        }
        "fold.motion.value" => {
            choices["value_type"] = json!(["Scalar", "Vector", "Color", "Bool", "Text"]);
        }
        "fold.motion.color_ramp" => {
            choices["interpolation"] = json!(["Linear", "Smooth", "Stepped"]);
        }
        "fold.motion.keyframes" => {
            choices["keys[].interpolation"] = json!(["Hold", "Linear", "Ease"]);
        }
        "fold.motion.published_control" => {
            choices["value_type"] = json!(["Scalar", "Vector", "Color", "Bool"]);
        }
        "fold.motion.group_input" | "fold.motion.group_output" => {
            choices["value_type"] = json!([
                "Scalar", "Vector", "Color", "Bool", "Text", "Id", "Content", "Points"
            ]);
        }
        "fold.motion.oscillator" => {
            choices["value_type"] = json!(["Scalar", "Vector", "Color"]);
        }
        "fold.motion.read_attribute" | "fold.motion.store_attribute" => {
            choices["geometry_kind"] = json!(["Content", "Points"]);
            choices["value_type"] = json!(["Scalar", "Vector", "Color", "Bool"]);
        }
        "fold.motion.solver" => {
            choices["state_kind"] = json!(["Content", "Points"]);
        }
        "fold.motion.stroke" => {
            choices["cap"] = json!(["Butt", "Round", "Square"]);
            choices["join"] = json!(["Miter", "Round", "Bevel"]);
        }
        "fold.motion.wrangle" => {
            choices["geometry_kind"] = json!(["Content", "Points"]);
        }
        "fold.motion.wave" => {
            choices["waveform"] = json!(["Sine", "Triangle", "Square"]);
        }
        _ => {}
    }
    let input_choices = if id == "fold.motion.oscillator" {
        json!({"shape":["Sine","Triangle","Square","Sawtooth","Custom"],"time_mode":["Seconds","BPM"]})
    } else {
        json!({})
    };
    json!({"choices":choices,"input_choices":input_choices,"settings_shape":shape(defaults),"instance_signature":"inspect with query.node returns resolved sockets; no implicit type conversion"})
}

pub fn receipt(
    before: &fold_project::Document,
    after: &fold_project::Document,
) -> Result<Value, String> {
    let old = Motion::from_document(before)?;
    let new = Motion::from_document(after)?;
    let mut changes = Vec::new();
    for (scope, g) in std::iter::once((None, &new.graph))
        .chain(new.groups.iter().map(|(id, g)| (Some(*id), &g.graph)))
    {
        let prior = match scope {
            None => Some(&old.graph),
            Some(id) => old.groups.get(&id).map(|g| &g.graph),
        };
        for n in &g.nodes {
            if prior.and_then(|g| g.nodes.iter().find(|o| o.id == n.id)) != Some(n) {
                changes.push(json!({"node":n.id,"group":scope,"kind":n.kind,"definition":n.settings.get("group")}));
            }
        }
    }
    let mut removed = Vec::new();
    for (scope, g) in std::iter::once((None, &old.graph))
        .chain(old.groups.iter().map(|(id, g)| (Some(*id), &g.graph)))
    {
        let current = match scope {
            None => Some(&new.graph),
            Some(id) => new.groups.get(&id).map(|g| &g.graph),
        };
        for n in &g.nodes {
            if !current.is_some_and(|g| g.nodes.iter().any(|v| v.id == n.id)) {
                removed.push(json!({"node":n.id,"group":scope}));
            }
        }
    }
    let total_removed = removed.len();
    removed.truncate(200);
    let total = changes.len();
    changes.truncate(200);
    let objects:Vec<_>=new.scene.as_ref().into_iter().flat_map(|s|&s.objects).filter(|o|!old.scene.as_ref().is_some_and(|s|s.objects.contains(o))).map(|o|json!({"object":o.id,"source":o.source,"output":o.output,"transform":o.transform,"appearance":o.appearance,"modifiers":o.modifiers})).collect();
    let groups: Vec<_> = new.groups.iter().filter(|(id, group)| old.groups.get(id) != Some(group)).map(|(id, group)| json!({"group":id,"name":group.name,"inputs":group.inputs,"outputs":group.outputs})).collect();
    let removed_groups: Vec<_> = old
        .groups
        .keys()
        .filter(|id| !new.groups.contains_key(id))
        .collect();
    let removed_objects: Vec<_> = old
        .scene
        .as_ref()
        .into_iter()
        .flat_map(|s| &s.objects)
        .filter(|o| {
            !new.scene
                .as_ref()
                .is_some_and(|s| s.objects.iter().any(|n| n.id == o.id))
        })
        .map(|o| o.id)
        .collect();
    let truncated = [
        total,
        total_removed,
        objects.len(),
        groups.len(),
        removed_groups.len(),
        removed_objects.len(),
    ]
    .into_iter()
    .any(|n| n > 200);
    Ok(
        json!({"nodes":changes,"removed":removed,"total_removed_nodes":total_removed,"total_changed_nodes":total,"truncated":truncated,"objects":objects.iter().take(200).collect::<Vec<_>>(),"groups":groups.iter().take(200).collect::<Vec<_>>(),"removed_groups":removed_groups.iter().take(200).collect::<Vec<_>>(),"removed_objects":removed_objects.iter().take(200).collect::<Vec<_>>()}),
    )
}

pub fn sampling(node: &str, socket: &str, kind: Kind) -> &'static str {
    if matches!(kind, Kind::Content | Kind::Points) {
        return "geometry";
    }
    let field = match node.strip_prefix("fold.motion.").unwrap_or(node) {
        "group_instance" | "group_input" | "group_output" => return "consumer-dependent",
        "math" | "compare" | "value" | "combine_xy" | "separate_xy" | "mix" | "wave"
        | "stagger" | "direction" | "wrangle" => true,
        "map_range" | "color_ramp" => socket == "value",
        "random_value" => socket == "id",
        "range_falloff" => socket == "coordinate",
        "radial_falloff" => matches!(socket, "position" | "center"),
        "text_on_points" => matches!(socket, "text" | "size" | "offset" | "selection"),
        "format_text" => matches!(socket, "value" | "x" | "y"),
        "set_position" | "set_scale" | "set_rotation" | "set_color_opacity" | "set_opacity" => {
            matches!(socket, "value" | "selection" | "weight")
        }
        "oscillator" => !matches!(
            socket,
            "shape" | "time_mode" | "phase_spread" | "channel_phase"
        ),
        "published_control" => socket == "default",
        "read_attribute" => socket == "default",
        "store_attribute" => socket == "value",
        "convex_sweep" => socket == "offset",
        "palette" => matches!(socket, "start" | "end"),
        "apply_copy_color" => socket == "color",
        _ => false,
    };
    if field { "field" } else { "uniform" }
}

fn shape(value: &Value) -> Value {
    match value {
        Value::Object(o) => {
            json!({"type":"object","properties":o.iter().map(|(k,v)|(k.clone(),shape(v))).collect::<serde_json::Map<_,_>>()})
        }
        Value::Array(a) => {
            json!({"type":"array","example_items":a.iter().take(1).map(shape).collect::<Vec<_>>()})
        }
        Value::String(_) => json!({"type":"string","default":value}),
        Value::Bool(_) => json!({"type":"boolean","default":value}),
        Value::Number(_) => json!({"type":"number","default":value}),
        Value::Null => json!({"nullable":true,"default":null}),
    }
}
