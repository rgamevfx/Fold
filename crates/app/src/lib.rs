//! Application assembly shared by desktop, headless delivery, and tests.
pub mod agent;
pub mod browser;
mod browser_ingest;
pub mod color;
pub mod composition;
pub mod composition_layers;
pub mod document_placement;
pub mod gallery;
pub mod ingest;
mod motion_runtime;
pub mod output;
pub mod packages;
#[cfg(feature = "desktop")]
pub mod playback;
#[cfg(feature = "desktop")]
pub mod project_panel;
#[cfg(feature = "gpu")]
pub mod runtime_check;
pub mod session;
pub mod settings;
mod timeline_transition;
pub mod timeline_workflow;

mod motion_conversion;
mod read_source;
mod write_destination;
