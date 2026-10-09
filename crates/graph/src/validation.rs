//! Diagnostics shared by typed graph validation, source and execution.
use fold_foundation::ObjectId;
use serde::{Deserialize, Serialize};

/// Native categories survive registry callbacks and transport boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Type,
    Cycle,
    MissingReference,
    Unsupported,
    Budget,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub kind: Kind,
    pub message: String,
    pub native_ids: Vec<ObjectId>,
}
impl From<&str> for Diagnostic {
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
}
impl From<String> for Diagnostic {
    fn from(message: String) -> Self {
        Self {
            kind: Kind::Type,
            message,
            native_ids: vec![],
        }
    }
}
