//! Independent spatial authoring. Fold owns documents, evaluation and time;
//! Bevy runtime state stays behind the rendering adapter.
mod controls;
pub mod editing;
mod evaluation;
pub mod geometry;
pub mod interaction;
pub mod model;
pub mod package;
mod proposals;
pub mod tools;
#[cfg(feature = "ui")]
pub mod ui;

pub const PACKAGE: &str = "fold.3d";
pub const TYPE: &str = "fold.3d.scene";
pub const SCHEMA: u32 = 1;
