//! Compositor-owned authoring, static operators, and shared-engine lowering.
mod compile;
mod graph;
mod model;
pub mod package;
#[cfg(feature = "ui")]
pub mod ui;
pub use model::*;
pub const PACKAGE: &str = "fold.compositor";
pub const COMPOSITE: &str = "fold.compositor.composite";
