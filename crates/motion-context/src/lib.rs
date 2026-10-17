//! Motion's host-facing document adapter. Artist tools and agent authoring both
//! use the canvas edit implementation; renderer state never enters the document.
pub mod components;
pub mod conversion;
mod image_source;
pub mod package;
mod placement;
mod proposals;
mod resources;
pub mod runtime;
#[cfg(feature = "ui")]
pub mod ui;
pub mod video;
