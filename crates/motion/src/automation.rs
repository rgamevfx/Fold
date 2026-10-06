//! Agent authoring uses the same typed graph and direct tools as the editor.
pub mod discovery;
mod graph_edits;
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
    AttractParticles {
        id: ObjectId,
        attractor: ObjectId,
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
        {"op":"create_node","id":"$node","kind":"fold.motion.math","settings":{"operation":"Multiply","value_type":"Vector"},"inputs":{"a":{"Value":{"Vector":[1,2]}},"b":{"Value":{"Vector":[2,2]}}}},
        {"op":"publish","group":"$group","node":"$node","input":"a","port":"amplitude","name":"Amplitude","section":"Motion"},
        {"op":"port","group":"$group","port":"amplitude","name":"Height","advanced":false},
        {"op":"insert","node":"$target","input":"content","modifier":"$modifier","modifier_input":"content","output":"content"},
        {"op":"replace","node":"$node","kind":"fold.motion.math","settings":{"operation":"Add"}},
        {"op":"duplicate","node":"$node","id":"$copy","independent":true},
        {"op":"group_nodes","nodes":["$a","$b"],"id":"$instance","definition":"$definition","name":"Custom Tool"},
        {"op":"layout","nodes":["$a","$b"],"origin":[0,0]},
        {"op":"wrangle","node":"$node","source":"position[1] += chf(\"amplitude\");"},
        {"op":"create_controls","node":"$node"},
        {"op":"info","value":{"width":960,"height":540,"rate":[24,1],"frames":120}},
        {"op":"add","id":"$node","kind":"fold.motion.math","group":null},
        {"op":"object","id":"$text","kind":"fold.motion.text","name":"Title"},
        {"op":"attract_particles","id":"$particles","attractor":"$text","name":"Attract Particles"},
        {"op":"set","node":"$node","input":"a","value":{"Scalar":2},"group":null},
        {"op":"connect","node":"$node","input":"a","source":{"node":"$source","socket":"value"}},
        {"op":"disconnect","node":"$node","input":"a"},
        {"op":"node","node":"$node","value":{"settings":{"operation":"Multiply"}}},
        {"op":"remove","node":"$node"},
        {"op":"extract","node":"$node"},
        {"op":"group","id":"$group","value":{"name":"Custom","version":1,"inputs":[],"outputs":{},"graph":{"nodes":[],"output":{"node":"$output","socket":"value"}}}},
        {"op":"scene","value":{"objects":[]}}, {"op":"controls","value":{}}
    ],"notes":"Examples describe operation arguments. Optional group selects an internal graph. node merges inputs, settings, position and fold.motion.animation; id/kind/version stay stable. group merges a typed GroupDefinition; inputs have stable id, name, kind, default, unit, section, advanced. Inspect existing groups for complete examples. extract preserves the node ID as an instance; inspect that node settings.group to find its generated definition ID. Resources are omitted from inspection. All edits validate together. Datum is externally tagged: Scalar, Vector, Color, Bool, Text.",
    "wrangle":{"language":"Rust 2024","abi":1,"channels":{"chf":"Scalar","chv":"Vector","chc":"Color","chb":"Bool"},"attributes":{"read":["attrf","attrv","attrc","attrb"],"write":["set_attrf","set_attrv","set_attrc","set_attrb"],"names":"literal strings; writes take a second typed argument"},"context":["position: [f64;2] (writable)","color: [f64;4] (writable)","opacity: f64 (writable)","time.seconds: f64","time.numerator: i64","time.denominator: u32","index: usize","count: usize","id: u64","delta_time: f64","tau: f64"],"limits":{"source_bytes":32768,"open_delimiters":128,"punctuation":1024,"words":512,"channels":64,"attributes":64,"memory_bytes":16777216,"fuel_per_element":100000,"fuel_per_node":10000000},"compiler":"rustc with wasm32-unknown-unknown, prlimit; optional FOLD_RUSTC","notes":"Source is a Rust function body. No macros, attributes, modules, imports, static state or external dependencies. Channels require literal names. wrangle synchronizes controls and compiles before commit. Existing unused controls are preserved."},
    "nodes": crate::graph::registry::definitions().iter().map(|d| json!({
        "id":d.id,"name":d.name,"category":d.category,"description":d.description,
        "settings":(d.defaults)(),"metadata":discovery::metadata(d.id,&(d.defaults)()),
        "inputs":d.inputs.iter().map(|p|json!({"id":p.id,"kind":p.kind,"default":p.default,"unit":p.unit,"sampling":discovery::sampling(d.id,&p.id,p.kind)})).collect::<Vec<_>>(),
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
    author_cancellable(document, edits, &fold_media::Cancel::default())
}
pub fn author_cancellable(
    document: &Document,
    edits: &[Value],
    cancel: &fold_media::Cancel,
) -> Result<Document, String> {
    let mut m = Motion::from_document(document)?;
    let before = m.clone();
    for (index, value) in edits.iter().enumerate() {
        cancel.check()?;
        if value["op"].as_str().is_some_and(graph_edits::accepts) {
            graph_edits::apply(&mut m, value, cancel).map_err(|e| format!("edit {index}: {e}"))?;
            continue;
        }
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
                rename_object(&mut m, old, id, name);
            }
            Edit::AttractParticles {
                id,
                attractor,
                name,
            } => {
                if m.graph.nodes.iter().any(|n| n.id == id) {
                    return Err("node ID already exists".into());
                }
                let old = crate::authoring::particles::from_object(&mut m, attractor)?;
                rename_object(&mut m, old, id, name);
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
    for (scope, graph) in std::iter::once((None, &mut m.graph))
        .chain(m.groups.iter_mut().map(|(id, g)| (Some(*id), &mut g.graph)))
    {
        let old = match scope {
            None => Some(&before.graph),
            Some(id) => before.groups.get(&id).map(|g| &g.graph),
        };
        for node in &mut graph.nodes {
            if node.kind == "fold.motion.wrangle"
                && old
                    .and_then(|g| g.nodes.iter().find(|n| n.id == node.id))
                    .is_none_or(|n| n.settings != node.settings)
            {
                crate::wrangle::sync_controls(node)?;
                let s = node.settings::<crate::wrangle::Settings>()?;
                crate::wrangle::compiler::compile(&s.source, &s.channels, cancel)?;
            }
        }
    }
    let mut result = m.document(document.id)?;
    result.extensions = document.extensions.clone();
    Ok(result)
}

fn rename_object(m: &mut Motion, old: ObjectId, id: ObjectId, name: String) {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn particle_public_authoring_creates_editable_tool_with_requested_identity() {
        let m = Motion::empty();
        let id = ObjectId::new();
        let target = ObjectId::new();
        let document = m.document(Default::default()).unwrap();
        let document=author(&document,&[
        serde_json::json!({"op":"object","id":target,"kind":"fold.motion.text","name":"Target"}),
        serde_json::json!({"op":"attract_particles","id":id,"attractor":target,"name":"Particles"}),
    ]).unwrap();
        let m = Motion::from_document(&document).unwrap();
        assert_eq!(
            m.scene
                .as_ref()
                .unwrap()
                .objects
                .iter()
                .find(|o| o.id == id)
                .unwrap()
                .name,
            "Particles"
        );
        assert!(
            m.graph
                .node(id)
                .unwrap()
                .animation
                .contains_key("attraction.0")
        );
        let mut evaluator =
            crate::evaluation::Evaluator::new(&m, fold_foundation::Time::new(9, 1).unwrap())
                .unwrap();
        evaluator
            .resolve(&Link {
                node: id,
                socket: "content".into(),
            })
            .unwrap();
    }
}
