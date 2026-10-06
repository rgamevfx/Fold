use super::*;
use crate::{
    evaluation::field,
    fields::{Field, expression::Expression},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct Settings {
    expression: Expression,
    #[serde(default)]
    inputs: BTreeMap<String, Datum>,
}
fn signature(n: &Node) -> Result<(Vec<Socket>, Vec<(String, Kind)>), String> {
    let s = n.settings::<Settings>()?;
    if s.inputs.len() > 64 || s.inputs.keys().any(|id| id.is_empty() || id.len() > 80) {
        return Err("expression requires at most 64 named inputs, each 1..80 bytes".into());
    }
    let inputs = s
        .inputs
        .iter()
        .map(|(k, v)| (k.clone(), Field::constant(v.clone())))
        .collect();
    let kind = s
        .expression
        .compile(&inputs, fold_foundation::Time::ZERO)?
        .kind;
    Ok((
        s.inputs
            .into_iter()
            .map(|(id, v)| Socket {
                id,
                kind: v.kind(),
                default: Some(v),
                unit: "",
            })
            .collect(),
        vec![("value".into(), kind)],
    ))
}
pub fn definition_expression() -> Definition {
    let mut d = definition(
        "fold.motion.expression",
        "Expression",
        "Values",
        vec![],
        vec![("value", Kind::Scalar)],
        |e, n| {
            let s = n.settings::<Settings>()?;
            let mut inputs = BTreeMap::new();
            for key in s.inputs.keys() {
                inputs.insert(key.clone(), e.field(n, key)?);
            }
            Ok(field(s.expression.compile(&inputs, e.time)?))
        },
    );
    d.defaults =
        || serde_json::json!({"expression":{"op":"literal","value":{"Scalar":0}},"inputs":{}});
    d.atomic_settings = &["expression"];
    d.settings_schema =
        Some(|| serde_json::to_value(schemars::schema_for!(Settings)).expect("expression schema"));
    d.signature = Some(signature);
    d.validate = |n| signature(n).map(|_| ());
    d.description = "Typed native expression with explicit named dependencies, exact evaluation time and consumer-domain sampling. No implicit scalar/vector conversions. Settings expression is a typed AST; inputs declares typed defaults.";
    d
}
