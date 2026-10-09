//! Versioned native authoring services. Native desktop commands are not agent capabilities.
pub mod dispatch;
pub mod jobs;
pub mod retry;
pub mod wire;

#[cfg(target_os = "linux")]
pub mod local;

#[cfg(target_os = "linux")]
pub mod mcp;
