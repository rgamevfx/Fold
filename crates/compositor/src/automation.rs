use crate::{Composite, Node, OperationConfig};
use fold_foundation::ObjectId;
use fold_platform::authoring::{decode, encode, patch};
use fold_project::Document;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    Expression {
        node: ObjectId,
        property: String,
        component: usize,
        source: String,
    },
    Reference {
        node: ObjectId,
        property: String,
        component: usize,
        source_node: ObjectId,
        source_property: String,
        source_component: usize,
    },
    Source {
        #[serde(default)]
        nodes: Option<Vec<ObjectId>>,
        expected: String,
        source: String,
    },
    ShaderInterface {
        node: ObjectId,
        settings: crate::shader::Shader,
        #[serde(default)]
        renames: std::collections::BTreeMap<String, String>,
        #[serde(default)]
        remove: Vec<String>,
    },
    CreateNode {
        id: ObjectId,
        record: fold_graph::model::Node,
        #[serde(default)]
        inputs: std::collections::BTreeMap<String, ObjectId>,
    },
    Insert {
        node: ObjectId,
        input: String,
        modifier: ObjectId,
        modifier_input: String,
    },
    Duplicate {
        node: ObjectId,
        id: ObjectId,
    },
    Replace {
        node: ObjectId,
        record: fold_graph::model::Node,
    },
    LayoutNodes {
        nodes: Vec<ObjectId>,
        #[serde(default)]
        origin: [f32; 2],
    },
    Node {
        node: ObjectId,
        value: Value,
    },
    InputColor {
        node: ObjectId,
        space: Option<String>,
    },
    Connect {
        source: ObjectId,
        node: ObjectId,
        input: String,
    },
    Disconnect {
        node: ObjectId,
        input: String,
    },
    Remove {
        nodes: Vec<ObjectId>,
    },
    Output {
        node: ObjectId,
    },
    Info {
        value: Value,
    },
    Publish {
        definition: crate::reusable::Definition,
    },
    Layout,
}
pub fn catalog() -> Value {
    let info = fold_media::VideoInfo {
        width: 960,
        height: 540,
        rate: [24, 1],
        frames: 120,
    };
    json!({
        "parameter_references":{"scope":"same compositor document","components":"scalar numeric controls, including mix and shader controls","expression":"shared bounded expression syntax; ch(\"../Node/property/component\") resolves stable native identities"},
        "operations":[
            {"op":"expression","node":"node identity","property":"property key","component":0,"source":"ch(\"../Grade/gain/r\") * 0.5"},
            {"op":"reference","node":"node identity","property":"property key","component":0,"source_node":"source node identity","source_property":"property key","source_component":0},
            {"op":"create_node","id":"node identity","record":"shared versioned Node record","inputs":"map of input name to node identity"},
            {"op":"replace","node":"node identity","record":"shared versioned Node record; retain node and parameter identities"},
            {"op":"node","node":"node identity","value":"patch of operation/animation/effect/position"},
            {"op":"source","nodes":"optional array of node identities","expected":"exact scoped source","source":"replacement scoped source"},
            {"op":"shader_interface","node":"node identity","settings":"shader interface","renames":{},"remove":[]},
            {"op":"publish","definition":"effect interface and property bindings"},
            {"op":"connect","source":"source node","node":"target node","input":"named input"},
            {"op":"disconnect","node":"target node","input":"named input"},
            {"op":"duplicate","node":"source node","id":"new node identity"},
            {"op":"insert","node":"target node","input":"input name","modifier":"modifier node","modifier_input":"input name"},
            {"op":"remove","nodes":[]},{"op":"output","node":"output node"},
            {"op":"layout"},{"op":"layout_nodes","nodes":[],"origin":[0,0]},
            {"op":"info","value":"video format patch"},
            {"op":"input_color","node":"Read node","space":"OCIO input color space or null"}
        ],
        "operators":crate::OPERATORS.iter().enumerate().map(|(index,o)|json!({"id":o.id,"version":o.version,"record":example(o, index, &info),"inputs":(0..o.inputs.len()).map(|i|json!({"id":o.input_key(i),"type":o.inputs[i]})).collect::<Vec<_>>(),"output":o.output})).collect::<Vec<_>>(),
        "notes":"Native operation records own stable node/parameter identities. Catalog record IDs are deterministic examples; allocate fresh IDs for new nodes and parameters. Unknown versions are unresolved and non-executable. Source is an identity-preserving scoped projection. All edits pass the same protection and document transaction gates."
    })
}
fn example(op: &crate::Operator, index: usize, info: &fold_media::VideoInfo) -> Value {
    let mut value = serde_json::to_value(
        Node::disconnected(op.defaults(info).expect("registered default")).operation,
    )
    .expect("native operation example");
    // Discovery must be byte-stable across CLI/MCP calls. These are example
    // identities, not reservations in any project session.
    fn identities(value: &mut Value, counter: &mut usize) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    if matches!(key.as_str(), "id" | "document") && value.is_string() {
                        *counter += 1;
                        *value = json!(format!("00000000-0000-4000-8000-{:012x}", *counter));
                    } else {
                        identities(value, counter);
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    identities(value, counter);
                }
            }
            _ => {}
        }
    }
    identities(&mut value, &mut (index * 1024));
    value
}
pub fn inspect(d: &Document) -> Result<Value, String> {
    encode(Composite::from_document(d)?)
}
pub fn author(d: &Document, edits: &[Value]) -> Result<Document, String> {
    let mut c = Composite::from_document(d)?;
    for v in edits {
        match decode::<Edit>(v)? {
            Edit::Source {
                nodes,
                expected,
                source,
            } => {
                c.replace_source(nodes.as_deref(), &expected, &source)?;
            }
            Edit::ShaderInterface {
                node,
                settings,
                renames,
                remove,
            } => {
                c.migrate_shader(node, settings, &renames, &remove)?;
            }
            Edit::Publish { definition } => {
                c.extensions
                    .insert(crate::reusable::KEY.into(), encode(definition)?);
            }
            Edit::CreateNode { id, record, inputs } => {
                if c.nodes.iter().any(|n| n.id == id) {
                    return Err("node ID already exists".into());
                }
                let mut n = Node::from_record(record)?;
                n.id = id;
                c.nodes.push(n);
                for (input, source) in inputs {
                    c.connect(source, id, &input)?;
                }
            }
            Edit::Insert {
                node,
                input,
                modifier,
                modifier_input,
            } => {
                if node == modifier {
                    return Err("cannot insert a node into itself".into());
                }
                let n = c.node(node)?;
                let prepared = n.prepare()?;
                let config = prepared.config();
                let slot = (0..config.input_count())
                    .find(|&i| config.input_key(i) == input)
                    .ok_or("missing input")?;
                let previous = n.input_ids()[slot].ok_or("input is disconnected")?;
                c.connect(previous, modifier, &modifier_input)?;
                c.connect(modifier, node, &input)?;
            }
            Edit::Expression {
                node,
                property,
                component,
                source,
            } => {
                let graph = c.prepare()?;
                graph.node(node)?.property_reference(&property, component)?;
                let expression = graph.parse_expression(node, &source)?;
                c.node_mut(node)?.animation.set_binding(
                    crate::animation::path(&property, component),
                    fold_graph::binding::Binding::Expression { expression },
                )?;
            }
            Edit::Reference {
                node,
                property,
                component,
                source_node,
                source_property,
                source_component,
            } => {
                let graph = c.prepare()?;
                graph.node(node)?.property_reference(&property, component)?;
                let reference = graph
                    .node(source_node)?
                    .property_reference(&source_property, source_component)?;
                c.node_mut(node)?.animation.set_binding(
                    crate::animation::path(&property, component),
                    fold_graph::binding::Binding::Link {
                        reference: reference.into(),
                    },
                )?;
            }
            Edit::Duplicate { node, id } => {
                if c.nodes.iter().any(|n| n.id == id) {
                    return Err("node ID already exists".into());
                }
                let mut n = c.node(node)?.clone();
                n.id = id;
                let mut remap = std::collections::BTreeMap::new();
                let effect_id = ObjectId::new();
                remap.insert(n.effect.id, effect_id);
                n.effect.id = effect_id;
                n.duplicate_instance()?;
                for p in n.operation.parameters.values_mut() {
                    let id = ObjectId::new();
                    remap.insert(p.id, id);
                    p.id = id;
                }
                n.animation = n.animation.duplicate(&remap)?;
                if let Some(edge) = &mut n.effect.mask {
                    edge.id = ObjectId::new();
                }
                for edge in n.inputs.iter_mut().flatten() {
                    edge.id = ObjectId::new();
                }
                if let Some(p) = &mut n.position {
                    p[0] += 40.;
                    p[1] += 40.;
                }
                c.nodes.push(n);
            }
            Edit::Replace { node, record } => {
                let replacement = Node::from_record(record)?;
                let parameters = replacement.prepare()?.config().clone();
                let n = c.node(node)?;
                let prepared = n.prepare()?;
                let config = prepared.config();
                let a = config.operator();
                let b = parameters.operator();
                if config.input_count() != parameters.input_count()
                    || a.output != b.output
                    || (0..config.input_count()).any(|i| {
                        config.input_key(i) != parameters.input_key(i)
                            || config.input_type(i) != parameters.input_type(i)
                    })
                {
                    return Err("replacement has an incompatible socket interface".into());
                }
                if !n.animation.is_empty() && a.id != b.id {
                    return Err(
                        "replacement would reinterpret animation; migrate channels explicitly"
                            .into(),
                    );
                }
                c.node_mut(node)?.operation = replacement.operation;
            }
            Edit::LayoutNodes { nodes, origin } => {
                if nodes.len() > 4096 || !origin.iter().all(|v| v.is_finite()) {
                    return Err("invalid layout request".into());
                }
                for (i, id) in nodes.iter().enumerate() {
                    c.node_mut(*id)?.position = Some([
                        origin[0] + (i % 4) as f32 * 250.,
                        origin[1] + (i / 4) as f32 * 180.,
                    ]);
                }
            }
            Edit::Node { node, value } => {
                if value.as_object().is_none_or(|v| {
                    v.keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "operation" | "animation" | "effect" | "position"
                        )
                    })
                }) {
                    return Err("node patch allows operation/animation/effect/position".into());
                }
                let previous = c.node(node)?.prepare()?.config().clone();
                patch(c.node_mut(node)?, &value)?;
                if let (
                    OperationConfig::Shader { settings: a },
                    OperationConfig::Shader { settings: b },
                ) = (&previous, &c.node(node)?.prepare()?.config().clone())
                {
                    if a.images != b.images
                        || a.controls
                            .iter()
                            .map(|c| (&c.id, c.default.len()))
                            .collect::<Vec<_>>()
                            != b.controls
                                .iter()
                                .map(|c| (&c.id, c.default.len()))
                                .collect::<Vec<_>>()
                    {
                        return Err("Use shader_interface with explicit renames/removals to change Shader ports".into());
                    }
                }
            }
            Edit::InputColor { node, space } => {
                let n = c.node_mut(node)?;
                if !matches!(n.prepare()?.config().clone(), OperationConfig::Read { .. }) {
                    return Err("input_color requires a Read node".into());
                }
                let key = fold_platform::color::INPUT_KEY;
                if let Some(space) = space {
                    if space.trim().is_empty() || space.len() > 256 {
                        return Err("input color space must contain 1..256 bytes".into());
                    }
                    n.extensions.insert(key.into(), Value::String(space));
                } else {
                    n.extensions.remove(key);
                }
            }
            Edit::Connect {
                source,
                node,
                input,
            } => c.connect(source, node, &input)?,
            Edit::Disconnect { node, input } => c.disconnect(node, &input)?,
            Edit::Remove { nodes } => c.remove(&nodes)?,
            Edit::Output { node } => c.output = node,
            Edit::Info { value } => patch(&mut c.info, &value)?,
            Edit::Layout => c.auto_layout()?,
        }
    }
    Composite::from_document(d)?.check_protection(&c)?;
    let mut result = c.document(d.id)?;
    result.extensions = d.extensions.clone();
    Ok(result)
}

pub fn query(d: &Document, q: &Value) -> Result<Value, String> {
    let c = Composite::from_document(d)?;
    let node: Option<ObjectId> =
        serde_json::from_value(q.get("node").cloned().unwrap_or(Value::Null))
            .map_err(|e| e.to_string())?;
    let mut ids = std::collections::BTreeSet::new();
    if let Some(id) = node {
        c.node(id)?;
        ids.insert(id);
    }
    let mode = q["mode"].as_str().unwrap_or("nodes");
    match mode {
        "upstream" | "downstream" => {
            if node.is_none() {
                return Err("node required for neighborhood query".into());
            }
            for _ in 0..q["depth"].as_u64().unwrap_or(1).min(32) {
                let mut next = ids.clone();
                for n in &c.nodes {
                    for dep in n.dependencies() {
                        if mode == "upstream" && ids.contains(&n.id) {
                            next.insert(dep);
                        }
                        if mode == "downstream" && ids.contains(&dep) {
                            next.insert(n.id);
                        }
                    }
                }
                if next.len() > 200 {
                    return Err("neighborhood exceeds 200 nodes".into());
                }
                if next == ids {
                    break;
                }
                ids = next;
            }
        }
        "nodes" => {
            if node.is_none() {
                ids.extend(c.nodes.iter().map(|n| n.id));
            }
        }
        _ => return Err("mode must be nodes, upstream or downstream".into()),
    }
    let offset = q["offset"].as_u64().unwrap_or(0) as usize;
    let limit = q["limit"].as_u64().unwrap_or(50).clamp(1, 200) as usize;
    let nodes:Vec<_>=c.nodes.iter().filter(|n|ids.contains(&n.id)).skip(offset).take(limit).map(|n|{
        let prepared = n.prepare()?; let config = prepared.config(); let op=config.operator();let consumers:Vec<_>=c.nodes.iter().filter(|o|o.dependencies().any(|id|id==n.id)).map(|o|o.id).collect();
        Ok(json!({"node":n,"inputs":(0..config.input_count()).map(|i|json!({"id":config.input_key(i),"kind":format!("{:?}",config.input_type(i)),"source":n.input_ids()[i]})).collect::<Vec<_>>(),"output":{"id":op.output_key(),"kind":format!("{:?}",op.output)},"consumers":consumers}))
    }).collect::<Result<_, String>>()?;
    Ok(json!({"nodes":nodes,"total":ids.len(),"offset":offset}))
}
pub fn receipt(before: &Document, after: &Document) -> Result<Value, String> {
    let old = Composite::from_document(before)?;
    let new = Composite::from_document(after)?;
    let changed: Vec<_> = new
        .nodes
        .iter()
        .filter(|n| old.nodes.iter().find(|o| o.id == n.id) != Some(n))
        .collect();
    Ok(
        json!({"total_changed_nodes":changed.len(),"nodes":changed.iter().take(200).map(|n|json!({"node":n.id,"operator":n.operation.operation})).collect::<Vec<_>>(),"removed":old.nodes.iter().filter(|n|!new.nodes.iter().any(|v|v.id==n.id)).take(200).map(|n|n.id).collect::<Vec<_>>(),"output":new.output,"truncated":changed.len()>200}),
    )
}

#[cfg(test)]
mod color_tests {
    use super::*;
    use fold_foundation::{DocumentId, Time};

    #[test]
    fn read_color_override_preserves_extensions_and_can_be_cleared() {
        let info = fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 24,
        };
        let mut read = Node::new(
            OperationConfig::Read {
                options: Default::default(),
                source: crate::Source::Unassigned { info: info.clone() },
                start: Time::ZERO,
                source_start: Time::ZERO,
                duration: Time::new(1, 1).unwrap(),
            },
            vec![],
        );
        read.extensions
            .insert("vendor.extra".into(), json!({"keep":true}));
        let output = Node::new(OperationConfig::Output, vec![read.id]);
        let c = Composite {
            info,
            nodes: vec![read.clone(), output.clone()],
            output: output.id,
            extensions: Default::default(),
        };
        let d = c.document(DocumentId::new()).unwrap();
        let changed = author(
            &d,
            &[json!({"op":"input_color","node":read.id,"space":"Linear Rec.709 (sRGB)"})],
        )
        .unwrap();
        let c = Composite::from_document(&changed).unwrap();
        assert_eq!(
            fold_platform::color::input(&c.node(read.id).unwrap().extensions)
                .unwrap()
                .as_deref(),
            Some("Linear Rec.709 (sRGB)")
        );
        assert_eq!(
            c.node(read.id).unwrap().extensions["vendor.extra"],
            json!({"keep":true})
        );
        let reset = author(
            &changed,
            &[json!({"op":"input_color","node":read.id,"space":null})],
        )
        .unwrap();
        assert_eq!(
            Composite::from_document(&reset)
                .unwrap()
                .node(read.id)
                .unwrap(),
            &read
        );
        assert!(
            author(
                &d,
                &[json!({"op":"input_color","node":output.id,"space":"ACEScg"})]
            )
            .is_err()
        );
        assert!(
            author(
                &d,
                &[json!({"op":"input_color","node":read.id,"space":" "})]
            )
            .is_err()
        );
    }
}
