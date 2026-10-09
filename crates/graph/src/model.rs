//! Shared typed graph records; document ownership and evaluation remain provider-owned.
use crate::types::{Clock, Literal, Type};
use fold_foundation::ObjectId;
use fold_project::Metadata;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub id: ObjectId,
    pub name: String,
    pub inputs: Vec<Input>,
    pub nodes: Vec<Node>,
    pub instances: Vec<Instance>,
    pub outputs: Vec<Output>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub id: ObjectId,
    pub name: String,
    pub ty: Type,
    pub default: Literal,
    #[serde(default)]
    pub protected: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: ObjectId,
    pub name: String,
    pub operation: String,
    pub version: u32,
    pub mode: String,
    pub parameters: BTreeMap<String, Parameter>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub id: ObjectId,
    /// Unknown operation/version arguments remain opaque until their owner resolves them.
    pub value: serde_json::Value,
    #[serde(default)]
    pub protected: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub id: ObjectId,
    pub name: String,
    pub ty: Type,
    pub value: Edge,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reference {
    Parameter {
        parameter: ObjectId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        component: Option<u8>,
    },
    Input {
        input: ObjectId,
    },
    Node {
        node: ObjectId,
    },
    Instance {
        instance: ObjectId,
        output: ObjectId,
    },
}
/// Authored connection identity is separate from its target and display name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Edge {
    pub id: ObjectId,
    pub target: Reference,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl From<Reference> for Edge {
    fn from(target: Reference) -> Self {
        Self {
            id: ObjectId::new(),
            target,
            extensions: Metadata::new(),
        }
    }
}
impl Edge {
    pub fn validate(&self) -> Result<(), String> {
        extension_keys(&self.extensions, &["id", "target"])
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: ObjectId,
    pub name: String,
    pub definition: ObjectId,
    pub clock: Clock,
    /// Keys are definition input IDs; overrides never mutate their definitions.
    pub overrides: BTreeMap<ObjectId, Parameter>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Presentation {
    pub positions: BTreeMap<ObjectId, [f64; 2]>,
    pub groups: Vec<Group>,
    pub annotations: BTreeMap<ObjectId, String>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: ObjectId,
    pub name: String,
    pub members: Vec<ObjectId>,
    pub locked: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}

pub fn extension_keys(extensions: &Metadata, reserved: &[&str]) -> Result<(), String> {
    if extensions
        .keys()
        .any(|key| reserved.contains(&key.as_str()))
    {
        return Err("Extension key collides with an authored field".into());
    }
    Ok(())
}
