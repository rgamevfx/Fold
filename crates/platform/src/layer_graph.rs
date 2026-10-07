//! Editable image inputs exposed by a graph, independent of its graph schema.
use crate::image_source::Source;
#[derive(Clone, Debug)]
pub struct ImageInput {
    /// Provider-owned stable socket identity; never an artist-facing label.
    pub id: String,
    pub label: String,
    pub source: Option<Source>,
}

/// Immutable provider input for evaluating a layer graph in its owner's context.
/// The provider validates the named contract; the host never interprets its data.
pub struct EvaluationContext {
    pub kind: &'static str,
    pub data: serde_json::Value,
}
pub type Resolver<'a> = dyn Fn(
        &fold_project::DocumentRef,
        fold_foundation::Time,
        &EvaluationContext,
    ) -> Result<fold_render::RenderGraph, String>
    + 'a;
