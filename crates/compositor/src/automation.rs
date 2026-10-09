use crate::{Composite, Node, Parameters};
use fold_foundation::ObjectId;
use fold_platform::authoring::{decode, encode, patch};
use fold_project::Document;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
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
        parameters: Parameters,
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
        parameters: Parameters,
    },
    LayoutNodes {
        nodes: Vec<ObjectId>,
        #[serde(default)]
        origin: [f32; 2],
    },
    Add {
        id: ObjectId,
        parameters: Parameters,
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
    json!({"operations":[
        {"op":"create_node","id":"97f4a7c0-804d-5452-a10a-38f9af9e35c8","parameters":{"GaussianBlur":{"size":[5,5],"edges":"Clamp"}},"inputs":{"image":"45fec05e-da75-57de-bb23-335187db0a78"}},
        {"op":"insert","node":"eae6c19d-d2ed-520a-8790-d672c729eebc","input":"image","modifier":"cb386cc1-79f7-5e4a-ad9f-684f6b697e89","modifier_input":"image"},
        {"op":"duplicate","node":"97f4a7c0-804d-5452-a10a-38f9af9e35c8","id":"62b06dd9-6e65-5266-a403-e3448b90de6d"},
        {"op":"replace","node":"97f4a7c0-804d-5452-a10a-38f9af9e35c8","parameters":{"Grade":{"gain":[1,1,1]}}},
        {"op":"layout_nodes","nodes":["93da3047-4efc-5d39-ae9d-84026511cbac","4e297ef5-0a71-54df-b42d-aee60642b1e8"],"origin":[0,0]},
        {"op":"add","id":"97f4a7c0-804d-5452-a10a-38f9af9e35c8","parameters":{"Solid":{"rgba":[0.2,0.3,0.4,1]}}},
        {"op":"node","node":"97f4a7c0-804d-5452-a10a-38f9af9e35c8","value":{"parameters":{"Solid":{"rgba":[1,0,0,1]}}}},
        {"op":"input_color","node":"1b2d072f-52b3-5124-93d3-569a8da76eaa","space":"Linear Rec.709 (sRGB)"},
        {"op":"connect","source":"45fec05e-da75-57de-bb23-335187db0a78","node":"97f4a7c0-804d-5452-a10a-38f9af9e35c8","input":"image"},
        {"op":"disconnect","node":"97f4a7c0-804d-5452-a10a-38f9af9e35c8","input":"image"},
        {"op":"remove","nodes":["97f4a7c0-804d-5452-a10a-38f9af9e35c8"]},{"op":"output","node":"97f4a7c0-804d-5452-a10a-38f9af9e35c8"},
        {"op":"info","value":{"width":960,"height":540,"rate":[24,1],"frames":120}},{"op":"layout"}
    ],"effect_commands":[{"command":"fold.compositor.create-effect","arguments":{"id":"fb2f30e8-eb14-5186-9c28-8bd23ab2d50e","preset":"Glow"}}],
    "effect_presets":["CustomEffect","Glow","Dissolve","CircularWipe","FilmLook","Grain"],
    "effect_contract":{"image_stack_inputs":["source"],"transition_inputs":["outgoing","incoming"],"instance_values":"Stable promoted control IDs map to scalar/vector/color component arrays; per-instance animation uses the host clock.","gpu":"Shader definitions require GPU execution; loop-free WGSL only. Native graph operators implement convolution kernels."},
    "parameter_examples":[
        {"Input":{"id":"source"}},
        {"Dissolve":{"curve":{"curve":[[0,0],[1,1]],"smooth":false}}},
        {"Shader":{"settings":{"source":"fn effect(uv:vec2<f32>)->vec4<f32>{return source(uv);}","images":["source"],"controls":[]}}},
        {"RelativeTransform":{"settings":{"source_size":[1920,1080],"anchor":[0.5,0.5],"position":[0,0],"size":100,"size_reference":"Placement","fit":"Fit","filter":"Bilinear"}}},
        {"Solid":{"rgba":[0,0,0,1]}},{"GaussianBlur":{"size":[5,5],"edges":"Clamp"}},
        {"Composite":{"mode":"Over"}},"Output",{"Grade":{"gain":[1,1,1]}},
        {"Transform":{"translate":[0,0],"scale":[1,1],"opacity":1}},
        {"Read":{"source":{"Document":{"source":{"document":"c09727f2-5ea1-5380-9457-49c8f088923a","output":"video"},"info":{"width":960,"height":540,"rate":[24,1],"frames":120}}},"start":{"numerator":0,"denominator":1},"source_start":{"numerator":0,"denominator":1},"duration":{"numerator":5,"denominator":1}}},
        {"Shuffle":{"mappings":[{"destination":"rgba.red","source":{"A":"diffuse.red"}},{"destination":"rgba.green","source":{"A":"diffuse.green"}},{"destination":"rgba.blue","source":{"A":"diffuse.blue"}},{"destination":"rgba.alpha","source":{"A":"diffuse.alpha"}}]}}
    ],"notes":"node merges parameters/animation/effect/position while retaining extensions. input_color sets a Read's per-use OCIO input space; null restores the asset/default interpretation. EXR Read color_layers selects which named RGB layers receive that transform. Reusable effects use publish with {interface:{version,images,controls},bindings:[{control,node,property}]}. Instantiate with Instance {instance:{id,definition,version,values,animation,enabled},images:[stable input IDs]}. shader_interface {node,settings,renames,remove} performs explicit port migration; node patches cannot change Shader ports. Provider node queries return dynamic sockets.",
    "operators":crate::OPERATORS.iter().map(|o|json!({"id":o.id,"version":o.version,"inputs":(0..o.inputs.len()).map(|i|json!({"id":o.input_key(i),"type":format!("{:?}",o.inputs[i])})).collect::<Vec<_>>(),"output":o.output_key()})).collect::<Vec<_>>()})
}
pub fn inspect(d: &Document) -> Result<Value, String> {
    encode(Composite::from_document(d)?)
}
pub fn author(d: &Document, edits: &[Value]) -> Result<Document, String> {
    let mut c = Composite::from_document(d)?;
    for v in edits {
        match decode::<Edit>(v)? {
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
            Edit::CreateNode {
                id,
                parameters,
                inputs,
            } => {
                if c.nodes.iter().any(|n| n.id == id) {
                    return Err("node ID already exists".into());
                }
                let mut n = Node::disconnected(parameters);
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
                let slot = (0..n.parameters.input_count())
                    .find(|&i| n.parameters.input_key(i) == input)
                    .ok_or("missing input")?;
                let previous = n.input_ids()[slot].ok_or("input is disconnected")?;
                c.connect(previous, modifier, &modifier_input)?;
                c.connect(modifier, node, &input)?;
            }
            Edit::Duplicate { node, id } => {
                if c.nodes.iter().any(|n| n.id == id) {
                    return Err("node ID already exists".into());
                }
                let mut n = c.node(node)?.clone();
                n.id = id;
                n.animation = n.animation.duplicate()?;
                for edge in n.inputs.iter_mut().flatten() {
                    edge.id = ObjectId::new();
                }
                if let Some(p) = &mut n.position {
                    p[0] += 40.;
                    p[1] += 40.;
                }
                c.nodes.push(n);
            }
            Edit::Replace { node, parameters } => {
                let n = c.node(node)?;
                let a = n.parameters.operator();
                let b = parameters.operator();
                if n.parameters.input_count() != parameters.input_count()
                    || a.output != b.output
                    || (0..n.parameters.input_count()).any(|i| {
                        n.parameters.input_key(i) != parameters.input_key(i)
                            || n.parameters.input_type(i) != parameters.input_type(i)
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
                c.node_mut(node)?.parameters = parameters;
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
            Edit::Add { id, parameters } => {
                if c.nodes.iter().any(|n| n.id == id) {
                    return Err("node ID already exists".into());
                }
                let mut n = Node::disconnected(parameters);
                n.id = id;
                c.nodes.push(n);
            }
            Edit::Node { node, value } => {
                if value.as_object().is_none_or(|v| {
                    v.keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "parameters" | "animation" | "effect" | "position"
                        )
                    })
                }) {
                    return Err("node patch allows parameters/animation/effect/position".into());
                }
                let previous = c.node(node)?.parameters.clone();
                patch(c.node_mut(node)?, &value)?;
                if let (Parameters::Shader { settings: a }, Parameters::Shader { settings: b }) =
                    (&previous, &c.node(node)?.parameters)
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
                if !matches!(n.parameters, Parameters::Read { .. }) {
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
        let op=n.parameters.operator();let consumers:Vec<_>=c.nodes.iter().filter(|o|o.dependencies().any(|id|id==n.id)).map(|o|o.id).collect();
        json!({"node":n,"inputs":(0..n.parameters.input_count()).map(|i|json!({"id":n.parameters.input_key(i),"kind":format!("{:?}",n.parameters.input_type(i)),"source":n.input_ids()[i]})).collect::<Vec<_>>(),"output":{"id":op.output_key(),"kind":format!("{:?}",op.output)},"consumers":consumers})
    }).collect();
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
        json!({"total_changed_nodes":changed.len(),"nodes":changed.iter().take(200).map(|n|json!({"node":n.id,"operator":n.parameters.operator().id})).collect::<Vec<_>>(),"removed":old.nodes.iter().filter(|n|!new.nodes.iter().any(|v|v.id==n.id)).take(200).map(|n|n.id).collect::<Vec<_>>(),"output":new.output,"truncated":changed.len()>200}),
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
            Parameters::Read {
                source: crate::Source::Unassigned { info: info.clone() },
                start: Time::ZERO,
                source_start: Time::ZERO,
                duration: Time::new(1, 1).unwrap(),
            },
            vec![],
        );
        read.extensions
            .insert("vendor.extra".into(), json!({"keep":true}));
        let output = Node::new(Parameters::Output, vec![read.id]);
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
