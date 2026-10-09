//! Native typed Motion construction. No legacy payload, compiler or runtime support.
//!
//! The native document, CPU evaluation and editing services back the application
//! provider. Ordered Screen/World outputs compile through the shared video
//! service; World passes require the spatial GPU build. Camera Plane and full
//! rendering acceptance remain under qualification.
pub use fold_graph::{binding, execution, expression, expression_source, ramp, types};
pub mod candidates;
pub mod connections;
pub mod diff;
pub mod editing;
pub mod evaluation;
pub mod keyed;
pub mod merge;
pub mod model;
pub mod package;
pub mod parameters;
pub mod patch;
pub mod protection;
pub mod reconcile;
pub mod registry;
pub mod rendering;
pub mod scene;
pub mod source;
mod source_parser;
#[cfg(feature = "ui")]
pub mod ui;
pub mod validation;
mod video;

pub const PACKAGE: &str = "fold.motion.native";
pub const CONSTRUCTION: &str = "fold.motion.construction";
pub const SCHEMA_VERSION: u32 = 1;
