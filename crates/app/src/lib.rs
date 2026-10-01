//! Application assembly shared by desktop, headless delivery, and tests.
pub mod media_workflow;
pub mod packages;
#[cfg(feature = "desktop")]
pub mod playback;
pub mod session;
pub mod timeline_workflow;
