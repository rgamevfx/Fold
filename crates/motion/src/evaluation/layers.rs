//! Bind MoGraph objects to their owning Composition's layer hierarchy.
use crate::Motion;
use fold_composition::{graph, layer::IDENTITY};
use fold_foundation::{ObjectId, Time};
use std::collections::BTreeMap;

pub(crate) fn bind(
    motion: &mut Motion,
    context: &graph::Context,
) -> Result<BTreeMap<ObjectId, Time>, String> {
    let scene = motion.scene.as_mut().ok_or("Layer graph has no objects")?;
    let selected = context
        .layers
        .iter()
        .find(|l| l.graph_object() == Some(context.selected))
        .ok_or("Selected graph is absent from the Composition")?;
    let selected_offset = selected
        .source_start
        .checked_sub(selected.origin)
        .map_err(|e| e.to_string())?;
    let mut offsets = BTreeMap::new();
    let mut bound = Vec::new();
    for layer in &context.layers {
        let id = layer.graph_object().ok_or("Layer has no graph object")?;
        let mut object = scene
            .objects
            .iter()
            .find(|o| o.id == id)
            .cloned()
            .ok_or("Layer graph object is missing")?;
        let offset = layer
            .source_start
            .checked_sub(layer.origin)
            .map_err(|e| e.to_string())?;
        offsets.insert(
            id,
            offset
                .checked_sub(selected_offset)
                .map_err(|e| e.to_string())?,
        );
        object.state = layer.graph_state(&context.layers)?;
        if id == context.selected {
            // The common compositor applies this layer's state after its graph.
            // Descendants and referenced objects apply their state in geometry.
            object.offset = IDENTITY;
            object.opacity = 1.;
            object.animation.clear();
        }
        bound.push(object);
    }
    scene.objects = bound;
    scene.output = None;
    Ok(offsets)
}
