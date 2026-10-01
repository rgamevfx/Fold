//! Feature-neutral project state. Only the coordinator publishes committed roots.
//! Opaque payloads and extension metadata survive without their owning providers.

mod coordinator;
mod model;
mod persistence;

pub use coordinator::{EditBatch, EditSession, Mutation, Project, ProjectError};
pub use model::{Asset, Document, DocumentRef, Metadata, ProjectState, Revision, Snapshot};
pub use persistence::{LoadError, SaveError, load, save};
