//! Application assembly shared by desktop, headless delivery, and tests.
pub mod agent;
pub mod browser;
mod browser_ingest;
pub mod color;
pub mod composition;
pub mod composition_layers;
pub mod document_placement;
pub mod ingest;
pub mod output;
pub mod packages;
#[cfg(feature = "desktop")]
pub mod playback;
#[cfg(feature = "desktop")]
pub mod project_panel;
pub mod session;
pub mod settings;
mod timeline_transition;
pub mod timeline_workflow;

mod read_source;
mod write_destination;
