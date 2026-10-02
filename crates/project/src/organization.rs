//! Organization has no evaluation meaning. Item identities are deterministic,
//! typed target identities: exactly one organizational location per target.
use crate::{Metadata, ProjectError, ProjectState};
use fold_foundation::{AssetId, BinId, DocumentId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ItemId {
    Asset(AssetId),
    Document(DocumentId),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Parent {
    #[default]
    Root,
    Bin(BinId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bin {
    pub id: BinId,
    pub name: String,
    pub parent: Parent,
    pub order: u64,
    #[serde(flatten)]
    pub extensions: Metadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub name: String,
    pub parent: Parent,
    pub order: u64,
    #[serde(flatten)]
    pub extensions: Metadata,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Organization {
    pub bins: BTreeMap<BinId, Bin>,
    // A sequence avoids JSON's restriction to string map keys for typed IDs.
    pub items: Vec<Item>,
    #[serde(flatten)]
    pub extensions: Metadata,
}

impl ProjectState {
    /// Used at legacy load and after existing creation commands. Never rewrite
    /// existing names, ordering or locations, nor interpret provider payloads.
    pub(crate) fn discover_items(&mut self) {
        let existing: BTreeSet<_> = self.organization.items.iter().map(|i| i.id).collect();
        let targets = self
            .assets
            .values()
            .map(|a| {
                (
                    ItemId::Asset(a.id),
                    std::path::Path::new(&a.location)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&a.location)
                        .to_owned(),
                )
            })
            .chain(
                self.documents
                    .values()
                    .map(|d| (ItemId::Document(d.id), format!("{} {:?}", d.type_id, d.id))),
            );
        for (id, name) in targets {
            if !existing.contains(&id) {
                self.organization.items.push(Item {
                    id,
                    name,
                    parent: Parent::Root,
                    order: self.organization.items.len() as u64,
                    extensions: Metadata::new(),
                });
            }
        }
    }
}

pub(crate) fn validate(state: &ProjectState) -> Result<(), ProjectError> {
    let org = &state.organization;
    let invalid = || ProjectError::InvalidOrganization;
    let parent_exists = |parent| match parent {
        Parent::Root => true,
        Parent::Bin(id) => org.bins.contains_key(&id),
    };
    let reserved =
        |metadata: &Metadata, fields: &[&str]| fields.iter().any(|k| metadata.contains_key(*k));
    if reserved(&org.extensions, &["bins", "items"]) {
        return Err(invalid());
    }
    for (id, bin) in &org.bins {
        if *id != bin.id
            || !parent_exists(bin.parent)
            || reserved(&bin.extensions, &["id", "name", "parent", "order"])
        {
            return Err(invalid());
        }
        let mut visited = BTreeSet::from([*id]);
        let mut parent = bin.parent;
        while let Parent::Bin(id) = parent {
            if !visited.insert(id) {
                return Err(ProjectError::Cycle);
            }
            parent = org.bins.get(&id).ok_or_else(invalid)?.parent;
        }
    }
    let mut seen = BTreeSet::new();
    for item in &org.items {
        let exists = match item.id {
            ItemId::Asset(id) => state.assets.contains_key(&id),
            ItemId::Document(id) => state.documents.contains_key(&id),
        };
        if !exists
            || !seen.insert(item.id)
            || !parent_exists(item.parent)
            || reserved(&item.extensions, &["id", "name", "parent", "order"])
        {
            return Err(invalid());
        }
    }
    if seen.len() != state.assets.len() + state.documents.len() {
        return Err(invalid());
    }
    Ok(())
}
