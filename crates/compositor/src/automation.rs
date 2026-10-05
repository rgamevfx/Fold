use crate::{Composite, Node, Parameters};
use fold_foundation::ObjectId;
use fold_platform::authoring::{decode, encode, patch};
use fold_project::Document;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    Add {
        id: ObjectId,
        parameters: Parameters,
    },
    Node {
        node: ObjectId,
        value: Value,
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
    Layout,
}
pub fn catalog() -> Value {
    fold_platform::authoring::examples(json!({"operations":[
        {"op":"add","id":"$node","parameters":{"Solid":{"rgba":[0.2,0.3,0.4,1]}}},
        {"op":"node","node":"$node","value":{"parameters":{"Solid":{"rgba":[1,0,0,1]}}}},
        {"op":"connect","source":"$source","node":"$node","input":"image"},
        {"op":"disconnect","node":"$node","input":"image"},
        {"op":"remove","nodes":["$node"]},{"op":"output","node":"$node"},
        {"op":"info","value":{"width":960,"height":540,"rate":[24,1],"frames":120}},{"op":"layout"}
    ],"parameter_examples":[
        {"Solid":{"rgba":[0,0,0,1]}},{"GaussianBlur":{"size":[5,5],"edges":"Clamp"}},
        {"Composite":{"mode":"Over"}},"Output",{"Grade":{"gain":[1,1,1]}},
        {"Transform":{"translate":[0,0],"scale":[1,1],"opacity":1}},
        {"Read":{"source":{"Document":{"source":{"document":"$motion","output":"video"},"info":{"width":960,"height":540,"rate":[24,1],"frames":120}}},"start":{"numerator":0,"denominator":1},"source_start":{"numerator":0,"denominator":1},"duration":{"numerator":5,"denominator":1}}}
    ],"notes":"node merges parameters/animation/effect/position while retaining extensions. Groups are currently provided by Motion, not the compositor model.",
    "operators":crate::OPERATORS.iter().map(|o|json!({"id":o.id,"version":o.version,"inputs":(0..o.inputs.len()).map(|i|json!({"id":o.input_key(i),"type":format!("{:?}",o.inputs[i])})).collect::<Vec<_>>(),"output":o.output_key()})).collect::<Vec<_>>()}))
}
pub fn inspect(d: &Document) -> Result<Value, String> {
    encode(Composite::from_document(d)?)
}
pub fn author(d: &Document, edits: &[Value]) -> Result<Document, String> {
    let mut c = Composite::from_document(d)?;
    for v in edits {
        match decode::<Edit>(v)? {
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
                patch(c.node_mut(node)?, &value)?;
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
    let mut result = c.document(d.id)?;
    result.extensions = d.extensions.clone();
    Ok(result)
}
