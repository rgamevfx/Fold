//! Published parameters and reusable graph interfaces have stable IDs, not labels.
use super::*;
use crate::{
    document::Motion,
    evaluation::{Value, field, output},
    fields::Field,
    graph::{Graph, Link},
};
use fold_foundation::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Port {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub default: Option<Datum>,
    #[serde(default)]
    pub unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub section: String,
    #[serde(default)]
    pub advanced: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupDefinition {
    pub name: String,
    pub version: u32,
    pub inputs: Vec<Port>,
    pub outputs: BTreeMap<String, Link>,
    pub graph: Graph,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlSettings {
    pub id: ObjectId,
    pub name: String,
    pub value_type: Kind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputSettings {
    pub key: String,
    pub value_type: Kind,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GroupSettings {
    pub group: Option<ObjectId>,
}
pub fn definitions() -> Vec<Definition> {
    let mut control = definition(
        "fold.motion.published_control",
        "Published Control",
        "Interface",
        vec![scalar("default", 0., "")],
        vec![("value", Kind::Generic)],
        |e, n| {
            let settings = n.settings::<ControlSettings>()?;
            if let Some(value) = e.motion.controls.get(&settings.id) {
                if value.kind() != settings.value_type {
                    return Err("published override type mismatch".into());
                }
                Ok(field(Field::constant(value.clone())))
            } else {
                Ok(field(e.field(n, "default")?))
            }
        },
    );
    control.inputs[0].kind = Kind::Generic;
    control.defaults = || {
        serde_json::to_value(ControlSettings {
            id: ObjectId::new(),
            name: "Energy".into(),
            value_type: Kind::Scalar,
        })
        .unwrap()
    };
    control.validate = |n| {
        let s = n.settings::<ControlSettings>()?;
        if !matches!(
            s.value_type,
            Kind::Scalar | Kind::Vector | Kind::Color | Kind::Bool
        ) {
            return Err("unsupported published control type".into());
        }
        Ok(())
    };
    let mut input = definition(
        "fold.motion.group_input",
        "Group Input",
        "Interface",
        vec![],
        vec![("value", Kind::Generic)],
        |e, n| {
            let settings = n.settings::<InputSettings>()?;
            let value = e
                .bindings
                .get(&settings.key)
                .cloned()
                .ok_or("Group Input must be evaluated inside its graph group")?;
            if value.kind() != settings.value_type {
                return Err("group input type mismatch".into());
            }
            Ok(output("value", value))
        },
    );
    input.defaults = || {
        serde_json::to_value(InputSettings {
            key: "input".into(),
            value_type: Kind::Scalar,
        })
        .unwrap()
    };
    input.validate = |n| n.settings::<InputSettings>().map(|_| ());
    let mut out = definition(
        "fold.motion.group_output",
        "Group Output",
        "Interface",
        vec![Socket::required("value", Kind::Generic)],
        vec![("value", Kind::Generic)],
        |e, n| Ok(output("value", e.input(n, "value")?)),
    );
    out.defaults = || serde_json::json!({"value_type":"Scalar"});
    let mut group = definition(
        "fold.motion.group_instance",
        "Node Group",
        "Interface",
        vec![],
        vec![],
        |e, n| {
            if n.settings
                .get("bypass")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && n.inputs.contains_key("content")
            {
                return Ok(output("content", e.input(n, "content")?));
            }
            let id = n
                .settings::<GroupSettings>()?
                .group
                .ok_or("choose a node group")?;
            let group = e.motion.groups.get(&id).ok_or("missing node group")?;
            if e.group_depth >= 32 {
                return Err("recursive or excessive group nesting".into());
            }
            let mut bindings = BTreeMap::new();
            for input in &group.inputs {
                bindings.insert(input.id.clone(), e.input(n, &input.id)?);
            }
            let mut nested = Evaluator::for_graph(e.motion, &group.graph, e.time)?;
            nested.bindings = bindings;
            nested.group_depth = e.group_depth + 1;
            nested.object_path = e.object_path.clone();
            nested.current_object = e.current_object;
            nested.budget = std::mem::take(&mut e.budget);
            let mut result = Outputs::new();
            for (name, link) in &group.outputs {
                result.insert(name.clone(), nested.resolve(link)?);
            }
            e.budget = nested.budget;
            Ok(result)
        },
    );
    group.defaults = || serde_json::to_value(GroupSettings::default()).unwrap();
    group.validate = |n| n.settings::<GroupSettings>().map(|_| ());
    let mut reference = definition(
        "fold.motion.object_reference",
        "Object Reference",
        "Scene",
        vec![],
        vec![("content", Kind::Content)],
        |e, n| {
            let id: ObjectId = serde_json::from_value(
                n.settings
                    .get("object")
                    .cloned()
                    .ok_or("choose a scene object")?,
            )
            .map_err(|e| e.to_string())?;
            if n.settings
                .get("local")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                let content = e.world_content(id)?;
                let mut wrapper = crate::geometry::Element::new(
                    n.seed(),
                    crate::geometry::Geometry::Group(content),
                );
                wrapper.transform = crate::geometry::inverse(e.world_matrix(id)?)?;
                return Ok(output(
                    "content",
                    Value::Content(std::sync::Arc::new(vec![wrapper])),
                ));
            }
            if n.settings
                .get("world")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                let content = e.world_content(id)?;
                let content = if n
                    .settings
                    .get("owner_space")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    let owner = e
                        .current_object
                        .ok_or("owner-relative reference requires a scene object")?;
                    let mut wrapper = crate::geometry::Element::new(
                        n.seed(),
                        crate::geometry::Geometry::Group(content),
                    );
                    wrapper.transform = crate::geometry::inverse(e.world_matrix(owner)?)?;
                    std::sync::Arc::new(vec![wrapper])
                } else {
                    content
                };
                Ok(output("content", Value::Content(content)))
            } else {
                Ok(output("content", e.object(id)?))
            }
        },
    );
    reference.defaults = || serde_json::json!({});
    let empty = definition(
        "fold.motion.empty",
        "Empty Content",
        "Scene",
        vec![],
        vec![("content", Kind::Content)],
        |_, _| Ok(crate::evaluation::content(std::sync::Arc::new(vec![]))),
    );
    let mut children = definition(
        "fold.motion.scene_children",
        "Scene Group",
        "Scene",
        vec![],
        vec![("content", Kind::Content)],
        |e, n| {
            let id =
                serde_json::from_value(n.settings["object"].clone()).map_err(|e| e.to_string())?;
            Ok(output("content", e.children(id)?))
        },
    );
    children.defaults = || serde_json::json!({});
    vec![control, input, out, group, reference, empty, children]
}
pub fn signature(
    motion: &Motion,
    node: &Node,
) -> Result<(Vec<Socket>, Vec<(String, Kind)>), String> {
    let id = node
        .settings::<GroupSettings>()?
        .group
        .ok_or("choose a node group")?;
    let group = motion.groups.get(&id).ok_or("missing node group")?;
    if group.version != 1 || group.inputs.len() > 256 || group.outputs.len() > 256 {
        return Err("unsupported node group schema or port count".into());
    }
    let inputs = group
        .inputs
        .iter()
        .map(|p| Socket {
            id: p.id.clone(),
            kind: p.kind,
            default: p.default.clone(),
            unit: match p.unit.as_str() {
                "integer" => "integer",
                "px" => "px",
                "degrees" => "degrees",
                "seconds" => "seconds",
                "Hz" => "Hz",
                "cycles/s" => "cycles/s",
                "factor" => "factor",
                "0–1" => "0–1",
                "BPM" => "BPM",
                "cycles" => "cycles",
                "cycles across copies" => "cycles across copies",
                "cycles / channel" => "cycles / channel",
                _ => "",
            },
        })
        .collect();
    let mut outputs = Vec::new();
    for (name, link) in &group.outputs {
        let source = group.graph.node(link.node)?;
        // Public group outputs are explicit Group Output nodes; this avoids
        // resolving arbitrary nested signatures recursively during validation.
        if source.kind != "fold.motion.group_output" || link.socket != "value" {
            return Err("group outputs must reference a Group Output socket".into());
        }
        let kind = serde_json::from_value(
            source
                .settings
                .get("value_type")
                .cloned()
                .ok_or("missing group output type")?,
        )
        .map_err(|e| e.to_string())?;
        outputs.push((name.clone(), kind));
    }
    Ok((inputs, outputs))
}
