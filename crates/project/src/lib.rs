//! Feature-neutral project state. Only the coordinator publishes committed roots.
//! Opaque payloads and extension metadata survive without their owning providers.

mod coordinator;
mod model;
mod organization;
mod persistence;
pub use organization::{Bin, Item, ItemId, Organization, Parent};

pub use coordinator::{EditBatch, EditSession, Mutation, Project, ProjectError, Retention};
pub use model::{
    Asset, CommittedSnapshot, Document, DocumentRef, Metadata, ProjectState, Revision, Snapshot,
};
pub use persistence::{LoadError, SaveError, load, save};
