//! Agent authoring uses the same typed graph and direct tools as the editor.
pub mod discovery;
mod fonts;
mod graph_edits;
mod observation;
mod tools;
mod verification;
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

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    ObjectInputs {
        object: ObjectId,
        role: String,
        inputs: std::collections::BTreeMap<String, Input>,
    },
    ObjectProperties {
        object: ObjectId,
        value: Value,
    },
    Reorder {
        object: ObjectId,
        target: ObjectId,
        #[serde(default)]
        after: bool,
    },
    ImportFont {
        id: ObjectId,
        path: std::path::PathBuf,
        #[serde(default)]
        face_index: u32,
    },
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
    fold_platform::authoring::examples(
        json!({"query_schemas":{"verify":verification::schema(),"evaluated":observation::schema()},"verification":{"checks":["movement","direction","displacement","periodicity","nonempty","relative_position","control_response"],"relative_position":"relative_to is another object or node/output target; relation left/right/above/below compares outline edges, offset compares centers to expected [dx,dy] at every sampled time","control_response":"control is a Published Control node, or an asset instance with input naming the port. values contains two tagged Datum values. Instance probes privately replace that input binding and keys. Evaluates both on private clones, with no project mutation. Bounded geometric/paint or uniform-field change is sampled evidence; intended magnitude and pixel visibility need separate checks."},"operation_schemas":[fold_platform::authoring::reference_schema(serde_json::to_value(schemars::schema_for!(Edit)).expect("operation schema")),graph_edits::schema(),tools::schema()],"queries":{"bake_status":{"mode":"bake_status","node":"UUID"},"construction":{"mode":"construction","node":"UUID"},"fonts":{"mode":"fonts","query":"sans","offset":0,"limit":50},"text_metrics":{"mode":"text_metrics","node":"UUID","time":{"numerator":0,"denominator":1}},"parameters":{"mode":"parameters","node":"UUID","time":{"numerator":0,"denominator":1}},"evaluated":{"mode":"evaluated","time":{"numerator":0,"denominator":1},"fields":["counts","elements"],"limit":32},"signature":{"mode":"signature","kind":"fold.motion.math","settings":{"value_type":"Vector"}},"verify":{"mode":"verify","check":"movement","times":[{"numerator":0,"denominator":1},{"numerator":1,"denominator":1}],"tolerance":0.01},"validate":{"mode":"validate","edits":[]}},"query_notes":{"evaluated":"Defaults to the scene. Optionally select object UUID, or node UUID plus output socket; object and node are exclusive. Supported fields are counts, elements, bounds and fingerprint; limit is 1..200. Bounds measure outlines/points, excluding stroke, effects and clipping. Pixel visibility requires rendered evidence. Unknown fields are errors."},"operations": [
        {"op":"object_inputs","object":"$object","role":"transform","inputs":{"translation":{"Value":{"Vector":[100,100]}}}},
        {"op":"object_properties","object":"$object","value":{"visible":true,"name":"Title"}},
        {"op":"reorder","object":"$front","target":"$back","after":false},
        {"op":"import_font","id":"$font","path":"/absolute/font.ttf","face_index":0},
        {"op":"create_node","id":"$node","kind":"fold.motion.math","settings":{"operation":"Multiply","value_type":"Vector"},"inputs":{"a":{"Value":{"Vector":[1,2]}},"b":{"Value":{"Vector":[2,2]}}}},
        {"op":"publish","group":"$group","node":"$node","input":"a","port":"amplitude","name":"Amplitude","section":"Motion"},
        {"op":"port","group":"$group","port":"amplitude","name":"Height","advanced":false},
        {"op":"insert","node":"$target","input":"content","modifier":"$modifier","modifier_input":"content","output":"content"},
        {"op":"replace","node":"$node","kind":"fold.motion.math","settings":{"operation":"Add"}},
        {"op":"duplicate","node":"$node","id":"$copy","independent":true},
        {"op":"group_nodes","nodes":["$a","$b"],"id":"$instance","definition":"$definition","name":"Custom Tool"},
        {"op":"layout","nodes":["$a","$b"],"origin":[0,0]},
        {"op":"rust_asset","definition":"$definition","instance":"$instance","source":"let mut a=Asset::new(\"Circle\"); let c=a.circle(\"circle\",30.); a.output(&c); a"},
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
        "settings":(d.defaults)(),"settings_schema":d.settings_schema.map(|f|f()),"atomic_settings":d.atomic_settings,"metadata":discovery::metadata(d.id,&(d.defaults)()),
        "inputs":d.inputs.iter().map(|p|json!({"id":p.id,"kind":p.kind,"default":p.default,"unit":p.unit,"sampling":discovery::sampling(d.id,&p.id,p.kind)})).collect::<Vec<_>>(),
        "outputs":d.outputs
    })).collect::<Vec<_>>()}),
    )
}
pub fn inspect(document: &Document) -> Result<Value, String> {
    let m = Motion::from_document(document)?;
    Ok(
        json!({"info":m.info,"graph":m.graph,"groups":m.groups,"scene":m.scene,"controls":m.controls,
        "rust_assets":m.extensions.get(crate::program::METADATA),
        "fonts":m.fonts.iter().map(|(id,f)|json!({"id":id,"name":f.name,"face_index":f.face_index,"bytes":f.bytes().len(),"fingerprint":font_identity(f)})).collect::<Vec<_>>()}),
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
        if value["op"].as_str().is_some_and(tools::accepts) {
            tools::apply(&mut m, value).map_err(|e| format!("edit {index}: {e}"))?;
            continue;
        }
        if value["op"].as_str().is_some_and(graph_edits::accepts) {
            graph_edits::apply(&mut m, value, cancel).map_err(|e| format!("edit {index}: {e}"))?;
            continue;
        }
        match decode::<Edit>(value)? {
            Edit::ObjectInputs {
                object,
                role,
                inputs,
            } => {
                let o = m
                    .scene
                    .as_ref()
                    .and_then(|s| s.objects.iter().find(|o| o.id == object))
                    .ok_or("missing scene object")?;
                let target = match role.as_str() {
                    "source" => o.source.node,
                    "transform" => o.transform.ok_or("object has no transform")?,
                    "appearance" => o.appearance.ok_or("object has no appearance")?,
                    _ => return Err("role must be source, transform or appearance".into()),
                };
                let n = m
                    .graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == target)
                    .ok_or("missing role node")?;
                n.inputs.extend(inputs);
            }
            Edit::ObjectProperties { object, value } => {
                let allowed = [
                    "name", "visible", "locked", "start", "end", "masks", "isolated",
                ];
                if value
                    .as_object()
                    .is_none_or(|v| v.keys().any(|k| !allowed.contains(&k.as_str())))
                {
                    return Err("object_properties accepts name, visible, locked, start, end, masks, isolated; edit layer opacity in Composition".into());
                }
                let o = m
                    .scene
                    .as_mut()
                    .and_then(|s| s.objects.iter_mut().find(|o| o.id == object))
                    .ok_or("missing scene object")?;
                patch(o, &value)?;
            }
            Edit::Reorder {
                object,
                target,
                after,
            } => crate::authoring::scene::reorder(&mut m, object, target, after)?,
            Edit::ImportFont {
                id,
                path,
                face_index,
            } => {
                if !path.is_absolute() {
                    return Err("font import requires an absolute path".into());
                }
                let metadata =
                    std::fs::metadata(&path).map_err(|e| format!("font resource: {e}"))?;
                if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
                    return Err("font import requires a file of at most 8 MiB".into());
                }
                use std::io::Read;
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 8 * 1024 * 1024 {
                    return Err("font exceeds 8 MiB".into());
                }
                let face = rustybuzz::Face::from_slice(&bytes, face_index)
                    .ok_or("invalid font face/index")?;
                let name = face
                    .names()
                    .into_iter()
                    .find(|n| n.name_id == rustybuzz::ttf_parser::name_id::FULL_NAME)
                    .and_then(|n| n.to_string())
                    .unwrap_or_else(|| "Imported font".into());
                m.fonts.insert(
                    id,
                    crate::document::Font {
                        name,
                        face_index,
                        source: crate::document::FontSource::Embedded(bytes),
                    },
                );
            }
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
                let target = g
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == node)
                    .ok_or("missing node")?;
                // Input is a tagged union. Replacing a binding must remove the old
                // variant (Link/Value), while settings retain recursive patch semantics.
                let mut patch_value = value.clone();
                if let Some(inputs) = patch_value.as_object_mut().unwrap().remove("inputs") {
                    let inputs: std::collections::BTreeMap<String, Input> = decode(&inputs)?;
                    target.inputs.extend(inputs);
                }
                if let Some(settings) = patch_value.as_object_mut().unwrap().remove("settings") {
                    target.patch_settings(&settings)?;
                }
                patch(target, &patch_value)?;
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

/// Stable diagnostic identity; FNV-1a is explicitly not a cryptographic integrity claim.
fn font_identity(font: &crate::document::Font) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in font
        .bytes()
        .iter()
        .chain(font.face_index.to_le_bytes().iter())
    {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}
