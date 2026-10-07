//! The layer hierarchy supplied to a graph evaluation. This is current authored
//! state, not a second saved scene or a project-format conversion.
use crate::{Composition, Layer};
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::image_source::Source;
use serde::{Deserialize, Serialize};

pub const OBJECT: &str = "fold.layer.object";
pub const CONTEXT: &str = "fold.layer.context";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Context {
    pub selected: ObjectId,
    pub layers: Vec<Layer>,
}
impl Context {
    pub fn new(composition: &Composition, document: DocumentId, selected: ObjectId) -> Self {
        Self {
            selected,
            layers: composition.layers.iter().filter(|layer| {
                matches!(&layer.source, Some(Source::Output(source)) if source.document == document && layer.graph_object().is_some())
            }).cloned().collect(),
        }
    }
}
impl Layer {
    /// Express shared layer properties in the graph object's own clock and IDs.
    pub fn graph_state(&self, layers: &[Layer]) -> Result<crate::layer::LayerState, String> {
        let mut state = self.state.clone();
        state.id = self.graph_object().ok_or("Layer has no graph object")?;
        state.parent = self
            .parent
            .and_then(|id| layers.iter().find(|l| l.id == id))
            .and_then(|l| l.graph_object());
        let offset = self.local_time(fold_foundation::Time::ZERO)?;
        state.start = state.start.checked_add(offset).map_err(|e| e.to_string())?;
        state.end = state.end.checked_add(offset).map_err(|e| e.to_string())?;
        for curve in state.animation.values_mut() {
            for key in &mut curve.keys {
                key.time = key.time.checked_add(offset).map_err(|e| e.to_string())?;
            }
        }
        Ok(state)
    }
    pub fn graph_object(&self) -> Option<ObjectId> {
        let Some(Source::Output(source)) = &self.source else {
            return None;
        };
        source
            .extensions
            .get(OBJECT)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}
