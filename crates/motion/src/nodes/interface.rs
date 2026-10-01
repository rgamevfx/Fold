//! Published parameters and reusable graph interfaces have stable IDs, not labels.
use super::*;
use crate::{
    document::Motion,
    evaluation::{field, output},
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
    vec![control, input, out, group]
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
            unit: "",
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
