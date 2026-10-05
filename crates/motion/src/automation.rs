//! Agent authoring uses the same typed graph and direct tools as the editor.
use crate::{
    Motion,
    fields::Datum,
    graph::{Input, Link, Node},
};
use fold_foundation::ObjectId;
use fold_platform::authoring::{decode, patch};
use fold_project::Document;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    Info {
        value: Value,
    },
    Add {
        id: ObjectId,
        kind: String,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Object {
        id: ObjectId,
        kind: String,
        name: String,
    },
    Set {
        node: ObjectId,
        input: String,
        value: Datum,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Connect {
        node: ObjectId,
        input: String,
        source: Link,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Disconnect {
        node: ObjectId,
        input: String,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Node {
        node: ObjectId,
        value: Value,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Remove {
        node: ObjectId,
        #[serde(default)]
        group: Option<ObjectId>,
    },
    Group {
        id: ObjectId,
        value: Value,
    },
    Extract {
        node: ObjectId,
    },
    Scene {
        value: Value,
    },
    Controls {
        value: Value,
    },
}
pub fn catalog() -> Value {
    fold_platform::authoring::examples(json!({"operations": [
        {"op":"info","value":{"width":960,"height":540,"rate":[24,1],"frames":120}},
        {"op":"add","id":"$node","kind":"fold.motion.math","group":null},
        {"op":"object","id":"$text","kind":"fold.motion.text","name":"Title"},
        {"op":"set","node":"$node","input":"a","value":{"Scalar":2},"group":null},
        {"op":"connect","node":"$node","input":"a","source":{"node":"$source","socket":"value"}},
        {"op":"disconnect","node":"$node","input":"a"},
        {"op":"node","node":"$node","value":{"settings":{"operation":"Multiply"}}},
        {"op":"remove","node":"$node"},
        {"op":"extract","node":"$node"},
        {"op":"group","id":"$group","value":{"name":"Custom","version":1,"inputs":[],"outputs":{},"graph":{"nodes":[],"output":{"node":"$output","socket":"value"}}}},
        {"op":"scene","value":{"objects":[]}}, {"op":"controls","value":{}}
    ],"notes":"Examples describe operation arguments. Optional group selects an internal graph. node merges inputs, settings, position and fold.motion.animation; id/kind/version stay stable. group merges a typed GroupDefinition; inputs have stable id, name, kind, default, unit, section, advanced. Inspect existing groups for complete examples. extract preserves the node ID as an instance; inspect that node settings.group to find its generated definition ID. Resources are omitted from inspection. All edits validate together. Datum is externally tagged: Scalar, Vector, Color, Bool, Text.",
    "nodes": crate::graph::registry::definitions().iter().map(|d| json!({
        "id":d.id,"name":d.name,"category":d.category,"description":d.description,
        "settings":(d.defaults)(),
        "inputs":d.inputs.iter().map(|p|json!({"id":p.id,"kind":p.kind,"default":p.default,"unit":p.unit})).collect::<Vec<_>>(),
        "outputs":d.outputs
    })).collect::<Vec<_>>()}))
}
pub fn inspect(document: &Document) -> Result<Value, String> {
    let m = Motion::from_document(document)?;
    Ok(
        json!({"info":m.info,"graph":m.graph,"groups":m.groups,"scene":m.scene,"controls":m.controls,
        "fonts":m.fonts.iter().map(|(id,f)|json!({"id":id,"name":f.name})).collect::<Vec<_>>()}),
    )
}
fn graph(m: &mut Motion, group: Option<ObjectId>) -> Result<&mut crate::graph::Graph, String> {
    match group {
        Some(id) => Ok(&mut m.groups.get_mut(&id).ok_or("missing group")?.graph),
        None => Ok(&mut m.graph),
    }
}
pub fn author(document: &Document, edits: &[Value]) -> Result<Document, String> {
    let mut m = Motion::from_document(document)?;
    for value in edits {
        match decode::<Edit>(value)? {
            Edit::Info { value } => patch(&mut m.info, &value)?,
            Edit::Add { id, kind, group } => {
                let g = graph(&mut m, group)?;
                if g.nodes.iter().any(|n| n.id == id) {
                    return Err("node ID already exists".into());
                }
                let mut n = Node::new(&kind)?;
                n.id = id;
                g.nodes.push(n);
            }
            Edit::Object { id, kind, name } => {
                if m.graph.nodes.iter().any(|n| n.id == id) {
                    return Err("node ID already exists".into());
                }
                let old = crate::authoring::scene::create_object(&mut m, &kind)?;
                for n in &mut m.graph.nodes {
                    if n.id == old {
                        n.id = id;
                    }
                    for input in n.inputs.values_mut() {
                        if let Input::Link(l) = input
                            && l.node == old
                        {
                            l.node = id;
                        }
                    }
                }
                let o = m
                    .scene
                    .as_mut()
                    .unwrap()
                    .objects
                    .iter_mut()
                    .find(|o| o.id == old)
                    .unwrap();
                o.id = id;
                o.source.node = id;
                o.name = name;
            }
            Edit::Set {
                node,
                input,
                value,
                group,
            } => {
                let g = graph(&mut m, group)?;
                g.nodes
                    .iter_mut()
                    .find(|n| n.id == node)
                    .ok_or("missing node")?
                    .set(&input, value);
            }
            Edit::Connect {
                node,
                input,
                source,
                group,
            } => {
                let g = graph(&mut m, group)?;
                g.nodes
                    .iter_mut()
                    .find(|n| n.id == node)
                    .ok_or("missing node")?
                    .inputs
                    .insert(input, Input::Link(source));
            }
            Edit::Disconnect { node, input, group } => {
                let g = graph(&mut m, group)?;
                g.nodes
                    .iter_mut()
                    .find(|n| n.id == node)
                    .ok_or("missing node")?
                    .inputs
                    .remove(&input);
            }
            Edit::Node { node, value, group } => {
                if value.as_object().is_none_or(|v| {
                    v.keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "inputs" | "settings" | "position" | "fold.motion.animation"
                        )
                    })
                }) {
                    return Err(
                        "node patch allows inputs/settings/position/fold.motion.animation".into(),
                    );
                }
                let g = graph(&mut m, group)?;
                patch(
                    g.nodes
                        .iter_mut()
                        .find(|n| n.id == node)
                        .ok_or("missing node")?,
                    &value,
                )?;
            }
            Edit::Remove { node, group } => {
                let g = graph(&mut m, group)?;
                if !g.nodes.iter().any(|n| n.id == node) {
                    return Err("missing node".into());
                }
                g.nodes.retain(|n| n.id != node);
            }
            Edit::Group { id, value } => {
                if let Some(g) = m.groups.get_mut(&id) {
                    patch(g, &value)?;
                } else {
                    m.groups.insert(id, decode(&value)?);
                }
            }
            Edit::Extract { node } => {
                crate::authoring::group_node(&mut m, node)?;
            }
            Edit::Scene { value } => {
                patch(&mut m.scene, &value)?;
            }
            Edit::Controls { value } => {
                patch(&mut m.controls, &value)?;
            }
        }
    }
    let mut result = m.document(document.id)?;
    result.extensions = document.extensions.clone();
    Ok(result)
}
