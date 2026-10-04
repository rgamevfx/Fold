//! Application assembly shared by desktop, headless delivery, and tests.
pub mod browser;
mod browser_ingest;
pub mod color;
pub mod composition;
pub mod document_placement;
pub mod ingest;
pub mod media_workflow;
pub mod output;
pub mod packages;
#[cfg(feature = "desktop")]
pub mod playback;
#[cfg(feature = "desktop")]
pub mod project_panel;
pub mod session;
pub mod timeline_workflow;

mod read_source;
