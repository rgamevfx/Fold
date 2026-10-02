use fold_foundation::{AssetId, DocumentId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

/// Unknown JSON metadata is retained semantically, including nested values.
pub type Metadata = BTreeMap<String, serde_json::Value>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentRef {
    pub document: DocumentId,
    pub output: String,
    #[serde(flatten)]
    pub extensions: Metadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub type_id: String,
    pub package_id: String,
    pub schema_version: u32,
    pub revision: Revision,
    pub dependencies: Vec<DocumentRef>,
    pub assets: Vec<AssetId>,
    /// Never parse or migrate these bytes without the owning provider.
    pub payload: Vec<u8>,
    #[serde(flatten)]
    pub extensions: Metadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub location: String,
    pub fingerprint: String,
    #[serde(flatten)]
    pub extensions: Metadata,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectState {
    /// Unknown outer archive fields, retained with the snapshot during save.
    #[serde(skip)]
    pub archive_extensions: Metadata,
    pub revision: Revision,
    pub documents: BTreeMap<DocumentId, Arc<Document>>,
    pub assets: BTreeMap<AssetId, Arc<Asset>>,
    #[serde(default)]
    pub organization: crate::Organization,
    pub settings: Metadata,
    pub environment: Metadata,
    #[serde(flatten)]
    pub extensions: Metadata,
}

/// Immutable evaluation state, either committed or transient. Cloning retains roots,
/// not payload copies. This type cannot be saved or exported.
#[derive(Clone, Debug)]
pub struct Snapshot(pub(crate) Arc<ProjectState>);

/// A coordinator-published revision. Only this handle can be saved or exported.
/// Dereferencing exposes read-only evaluation state, never the reverse conversion.
#[derive(Clone, Debug)]
pub struct CommittedSnapshot(pub(crate) Snapshot);
impl std::ops::Deref for CommittedSnapshot {
    type Target = Snapshot;
    fn deref(&self) -> &Snapshot {
        &self.0
    }
}
impl CommittedSnapshot {
    /// Retain this revision for evaluation without retaining persistence authority.
    pub fn evaluation(&self) -> Snapshot {
        self.0.clone()
    }
}
impl Snapshot {
    pub fn state(&self) -> &ProjectState {
        &self.0
    }
    pub fn revision(&self) -> Revision {
        self.0.revision
    }
}
