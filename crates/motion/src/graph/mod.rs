//! Editable graph identities and explicit static definitions.
pub mod definition;
pub mod registry;
mod signature;
use crate::fields::Datum;
use fold_foundation::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub node: ObjectId,
    pub socket: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Input {
    Value(Datum),
    Link(Link),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: ObjectId,
    pub kind: String,
    pub version: u32,
    pub inputs: BTreeMap<String, Input>,
    #[serde(
        rename = "fold.motion.animation",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub animation: crate::animation::parameters::Channels,
    pub settings: serde_json::Value,
    pub position: [f32; 2],
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
impl Node {
    pub fn new(kind: &str) -> Result<Self, String> {
        let d = registry::find(kind)?;
        Ok(Self {
            id: ObjectId::new(),
            kind: kind.into(),
            version: 1,
            inputs: d
                .inputs
                .iter()
                .filter_map(|s| s.default.clone().map(|v| (s.id.clone(), Input::Value(v))))
                .collect(),
            animation: Default::default(),
            settings: (d.defaults)(),
            position: [0.; 2],
            extensions: BTreeMap::new(),
        })
    }
    pub fn settings<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_value(self.settings.clone())
            .map_err(|e| format!("{} settings: {e}", self.kind))
    }
    pub fn set(&mut self, socket: &str, value: Datum) {
        self.inputs.insert(socket.into(), Input::Value(value));
    }
    pub fn connect(&mut self, socket: &str, node: ObjectId, output: &str) {
        self.inputs.insert(
            socket.into(),
            Input::Link(Link {
                node,
                socket: output.into(),
            }),
        );
    }
    pub fn seed(&self) -> u64 {
        serde_json::to_string(&self.id)
            .expect("UUID serialization")
            .bytes()
            .fold(0xcbf29ce484222325, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
            })
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub output: Link,
}
impl Graph {
    pub fn node(&self, id: ObjectId) -> Result<&Node, String> {
        self.nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| format!("missing node {id:?}"))
    }
}
