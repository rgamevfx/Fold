use crate::{
    evaluation::{Evaluator, Outputs},
    fields::{Datum, Kind},
    graph::Node,
};
#[derive(Clone)]
pub struct Socket {
    pub id: String,
    pub kind: Kind,
    pub default: Option<Datum>,
    pub unit: &'static str,
}
impl Socket {
    pub fn required(id: &'static str, kind: Kind) -> Self {
        Self {
            id: id.into(),
            kind,
            default: None,
            unit: "",
        }
    }
    pub fn value(id: &'static str, value: Datum, unit: &'static str) -> Self {
        Self {
            id: id.into(),
            kind: value.kind(),
            default: Some(value),
            unit,
        }
    }
}
pub struct Definition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub inputs: Vec<Socket>,
    pub outputs: Vec<(&'static str, Kind)>,
    pub signature: Option<fn(&Node) -> Result<(Vec<Socket>, Vec<(String, Kind)>), String>>,
    pub defaults: fn() -> serde_json::Value,
    pub validate: fn(&Node) -> Result<(), String>,
    pub evaluate: fn(&mut Evaluator<'_>, &Node) -> Result<Outputs, String>,
}
pub fn no_settings() -> serde_json::Value {
    serde_json::json!({})
}
pub fn validate_empty(node: &Node) -> Result<(), String> {
    if node.settings.is_object() {
        Ok(())
    } else {
        Err("settings must be an object".into())
    }
}
