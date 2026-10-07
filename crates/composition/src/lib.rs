//! Common composition ownership. Graph implementations are resolved through
//! registered output capabilities; this package never imports a creative engine.
mod automation;
mod compile;
pub mod graph;
pub mod layer;
mod model;
pub mod package;
pub mod reusable;
pub use model::{Composition, Layer};
#[cfg(feature = "ui")]
pub mod ui;
pub const PACKAGE: &str = "fold.composition";
pub const COMPOSITION: &str = "fold.composition.document";

/// Persisted role: editable content inside a composition, not a standalone composition.
pub const LAYER_GRAPH: &str = "fold.composition.layer_graph";
