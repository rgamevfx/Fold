//! Editable image inputs exposed by a graph, independent of its graph schema.
use crate::image_source::Source;
#[derive(Clone, Debug)]
pub struct ImageInput {
    /// Provider-owned stable socket identity; never an artist-facing label.
    pub id: String,
    pub label: String,
    pub source: Option<Source>,
}
