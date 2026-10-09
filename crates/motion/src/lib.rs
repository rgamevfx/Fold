//! Native typed Motion construction. No legacy payload, compiler or runtime support.
//!
//! The native document, CPU evaluation and editing services back the application
//! provider. Ordered Screen/World outputs compile through the shared video
//! service; World passes require the spatial GPU build. Camera Plane and full
//! rendering acceptance remain under qualification.
pub mod binding;
pub mod candidates;
pub mod connections;
pub mod diff;
pub mod editing;
pub mod evaluation;
pub mod execution;
pub mod expression;
pub mod expression_source;
pub mod keyed;
pub mod merge;
pub mod model;
pub mod package;
pub mod patch;
pub mod protection;
pub mod ramp;
pub mod reconcile;
pub mod registry;
pub mod rendering;
pub mod source;
mod source_parser;
pub mod types;
#[cfg(feature = "ui")]
pub mod ui;
pub mod validation;
mod video;

pub const PACKAGE: &str = "fold.motion.native";
pub const CONSTRUCTION: &str = "fold.motion.construction";
pub const SCHEMA_VERSION: u32 = 1;
