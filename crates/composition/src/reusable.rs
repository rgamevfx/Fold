//! Composition-owned input and control mappings over the common invocation contract.
use crate::Composition;
use fold_foundation::ObjectId;
use fold_platform::effects::{Interface, Invocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const KEY: &str = "fold.composition.interface";
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Definition {
    #[serde(flatten)]
    pub extensions: fold_project::Metadata,
    pub interface: Interface,
    /// Image names map to ordinary source-less leaf layers. Repeated image use
    /// may be authored by mapping multiple leaves to the same published input.
    pub images: BTreeMap<ObjectId, String>,
    #[serde(default)]
    pub graphs: BTreeMap<ObjectId, Graph>,
    pub bindings: Vec<Binding>,
}
/// Invocation-local routing into a referenced graph layer. Keys belong to the
/// child interface; values name ports in this Composition's published interface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Graph {
    #[serde(flatten)]
    pub extensions: fold_project::Metadata,
    pub version: u32,
    pub images: BTreeMap<String, String>,
    pub controls: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Binding {
    #[serde(flatten)]
    pub extensions: fold_project::Metadata,
    pub control: String,
    pub layer: ObjectId,
    pub target: Target,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Opacity,
    Position,
    Effect { instance: ObjectId, control: String },
}
impl Composition {
    pub fn reusable(&self) -> Result<Definition, String> {
        serde_json::from_value(
            self.extensions
                .get(KEY)
                .ok_or("Composition has no published effect interface")?
                .clone(),
        )
        .map_err(|e| e.to_string())
    }
    pub(crate) fn validate_reusable(&self) -> Result<(), String> {
        if !self.extensions.contains_key(KEY) {
            return Ok(());
        }
        let definition = self.reusable()?;
        definition.interface.validate()?;
        for (id, graph) in &definition.graphs {
            let layer = self.layer(*id)?;
            if !matches!(
                layer.source,
                Some(fold_platform::image_source::Source::Output(_))
            ) || layer.graph_children
                || graph.version == 0
                || graph
                    .images
                    .values()
                    .any(|n| !definition.interface.images.contains(n))
                || graph
                    .controls
                    .values()
                    .any(|n| !definition.interface.controls.iter().any(|c| &c.id == n))
            {
                return Err("Invalid published graph-layer invocation".into());
            }
        }
        for (id, name) in &definition.images {
            let layer = self.layer(*id)?;
            if layer.source.is_some()
                || layer.adjustment
                || self.layers.iter().any(|l| l.parent == Some(*id))
                || !definition.interface.images.contains(name)
            {
                return Err("Published image inputs require source-less leaf layers and a declared image port".into());
            }
        }
        if definition.interface.images.iter().any(|n| {
            !definition.images.values().any(|v| v == n)
                && !definition
                    .graphs
                    .values()
                    .any(|g| g.images.values().any(|v| v == n))
        }) {
            return Err("Published image has no layer binding".into());
        }
        let mut targets = BTreeSet::new();
        for binding in &definition.bindings {
            let control = definition
                .interface
                .controls
                .iter()
                .find(|c| c.id == binding.control)
                .ok_or("Unknown promoted control")?;
            if !targets.insert((
                binding.layer,
                serde_json::to_string(&binding.target).map_err(|e| e.to_string())?,
            )) {
                return Err("Multiple controls target one layer property".into());
            }
            let layer = self.layer(binding.layer)?;
            let components = match &binding.target {
                Target::Opacity => 1,
                Target::Position => 2,
                Target::Effect {
                    instance,
                    control: target,
                } => {
                    let effect = layer
                        .effects
                        .iter()
                        .find(|e| e.id == *instance)
                        .ok_or("Missing target effect instance")?;
                    effect
                        .values
                        .get(target)
                        .ok_or("Set an effect control value before promoting it")?
                        .len()
                }
            };
            if components != control.default.len() {
                return Err("Promoted control components do not match the target".into());
            }
        }
        if definition.interface.controls.iter().any(|c| {
            !definition.bindings.iter().any(|b| b.control == c.id)
                && !definition
                    .graphs
                    .values()
                    .any(|g| g.controls.values().any(|n| n == &c.id))
        }) {
            return Err("Promoted control has no binding".into());
        }
        Ok(())
    }
    pub(crate) fn bind_values(&mut self, invocation: &Invocation<'_>) -> Result<(), String> {
        let definition = self.reusable()?;
        let values = definition.interface.values(&invocation.values)?;
        for binding in definition.bindings {
            let value = &values[&binding.control];
            let layer = self.layer_mut(binding.layer)?;
            match binding.target {
                Target::Opacity => {
                    layer.opacity = value[0];
                    layer.animation.remove("scene.opacity.0");
                }
                Target::Position => {
                    layer.offset[4..6].copy_from_slice(value);
                    for key in ["transform.4", "transform.5"] {
                        layer.animation.remove(key);
                    }
                }
                Target::Effect { instance, control } => {
                    let effect = layer
                        .effects
                        .iter_mut()
                        .find(|e| e.id == instance)
                        .ok_or("Missing target effect instance")?;
                    effect.animation.remove(&control);
                    effect.values.insert(control, value.clone());
                }
            }
        }
        self.validate()
    }
}
