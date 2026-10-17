//! Shared Gallery identities and commands. Creative payloads stay with their providers.
use fold_foundation::{DocumentId, ObjectId, Time};
use serde::{Deserialize, Serialize};

pub const PLACE: &str = "fold.gallery.place";
pub use fold_project::components::{Kind, PROVENANCE, Reference};
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub component: Reference,
    pub document: DocumentId,
    #[serde(default)]
    pub objects: Vec<ObjectId>,
    pub at: Time,
    pub position: Option<[f64; 2]>,
    pub track: Option<ObjectId>,
    /// Explicitly replace this instance. No library operation updates projects.
    pub update: Option<ObjectId>,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    List,
    Inspect {
        component: Reference,
    },
    Preview {
        component: Reference,
    },
    Save {
        document: DocumentId,
        objects: Vec<ObjectId>,
        name: String,
        category: String,
        kind: Kind,
        update: Option<Reference>,
    },
    Import {
        path: String,
    },
    Export {
        component: Reference,
        path: String,
    },
    Favorite {
        component: Reference,
        value: bool,
    },
}
