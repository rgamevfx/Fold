//! Compositor-owned authoring, static operators, and shared-engine lowering.
pub mod animation;
mod channels;
mod compile;
mod compile_image;
mod shapes;
mod transform;
pub use shapes::Shape;
pub use transform::{RelativeTransform, SizeReference, Transform};
mod source_channels;
pub use channels::{ChannelMapping, ChannelSource, Effect};
mod graph;
mod model;
pub mod package;
mod placement;
pub mod presets;
pub mod reusable;
pub mod shader;
pub use placement::{source_from_asset, source_from_profile};
#[cfg(feature = "ui")]
pub mod ui;
pub use model::*;
pub const PACKAGE: &str = "fold.compositor";
pub const COMPOSITE: &str = "fold.compositor.composite";

mod automation;

mod inputs;

mod keyed_parameters;

mod operation;

mod source;

mod kernel;
mod registry;
pub use kernel::OperationConfig;
pub use registry::{OPERATORS, Operator};

mod instance;

mod reconcile;
