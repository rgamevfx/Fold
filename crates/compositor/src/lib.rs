//! Compositor-owned authoring, static operators, and shared-engine lowering.
pub mod animation;
mod channels;
mod compile;
mod compile_image;
mod shapes;
mod transform;
pub use shapes::Shape;
pub use transform::Transform;
mod source_channels;
pub use channels::{ChannelMapping, ChannelSource, Effect};
mod graph;
mod model;
pub mod package;
mod placement;
pub use placement::source_from_profile;
#[cfg(feature = "ui")]
pub mod ui;
pub use model::*;
pub const PACKAGE: &str = "fold.compositor";
pub const COMPOSITE: &str = "fold.compositor.composite";
