//! Provider-neutral object hierarchy shared by Outliner and Composition.
use fold_foundation::{DocumentId, ObjectId};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: ObjectId,
    pub parent: Option<ObjectId>,
    pub name: String,
    pub kind: String,
    pub visible: bool,
    pub locked: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Hierarchy {
    pub rows: Vec<Row>,
    pub create: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Create {
        kind: String,
    },
    Rename {
        object: ObjectId,
        name: String,
    },
    Reparent {
        object: ObjectId,
        parent: Option<ObjectId>,
    },
    Visibility {
        object: ObjectId,
        value: bool,
    },
    Lock {
        object: ObjectId,
        value: bool,
    },
    Remove {
        object: ObjectId,
    },
    ActiveCamera {
        object: ObjectId,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditArgs {
    pub document: DocumentId,
    pub edits: Vec<Edit>,
}
pub const EDIT: &str = "fold.app.edit-hierarchy";
