//! Authoritative editable 2D objects. The canvas, graph and agent edit this model.
//! Rendering belongs to fframes; there is no geometry evaluator or rasterizer here.
mod capture;
pub mod compilation;
pub mod editing;
pub mod hierarchy;
pub mod model;
pub mod protocol;
pub mod scripts;
pub mod tools;

pub const PACKAGE: &str = "fold.motion";
pub const TYPE: &str = "fold.motion.canvas";
pub const SCHEMA: u32 = 4;
