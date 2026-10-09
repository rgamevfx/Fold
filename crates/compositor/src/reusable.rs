//! Public interface mappings owned by the compositor, separate from host instances.
use crate::Composite;
use fold_foundation::ObjectId;
use fold_platform::effects::Interface;
use serde::{Deserialize, Serialize};

pub const KEY: &str = "fold.compositor.interface";
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    #[serde(flatten)]
    pub extensions: fold_project::Metadata,
    pub control: String,
    pub node: ObjectId,
    pub property: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    #[serde(flatten)]
    pub extensions: fold_project::Metadata,
    pub interface: Interface,
    pub bindings: Vec<Binding>,
}
impl Composite {
    pub fn reusable(&self) -> Result<Definition, String> {
        serde_json::from_value(
            self.extensions
                .get(KEY)
                .ok_or("Composite has no published effect interface")?
                .clone(),
        )
        .map_err(|e| e.to_string())
    }
    pub fn validate_reusable(&self) -> Result<(), String> {
        if !self.extensions.contains_key(KEY) {
            return Ok(());
        }
        let definition = self.reusable()?;
        definition.interface.validate()?;
        for image in &definition.interface.images {
            if !self
                .nodes
                .iter()
                .any(|n| matches!(&n.configuration(),crate::OperationConfig::Input { id } if id == image))
            {
                return Err(format!("Published image {image} has no Input node"));
            }
        }
        for node in &self.nodes {
            if let crate::OperationConfig::Input { id } = &node.configuration()
                && !definition.interface.images.contains(id)
            {
                return Err("Input is absent from the published interface".into());
            }
        }
        let mut targets = std::collections::BTreeSet::new();
        for binding in &definition.bindings {
            let control = definition
                .interface
                .controls
                .iter()
                .find(|c| c.id == binding.control)
                .ok_or("Missing published control")?;
            if !targets.insert((binding.node, &binding.property)) {
                return Err("Multiple controls target the same property".into());
            }
            let mut parameters = self.node(binding.node)?.configuration().clone();
            parameters.set_property(&binding.property, &control.default)?;
        }
        for control in &definition.interface.controls {
            if !definition.bindings.iter().any(|b| b.control == control.id) {
                return Err("Published control has no target".into());
            }
        }
        Ok(())
    }
    pub(crate) fn bind_values(
        &mut self,
        invocation: &fold_platform::effects::Invocation<'_>,
    ) -> Result<(), String> {
        let definition = self.reusable()?;
        let values = definition.interface.values(&invocation.values)?;
        for binding in definition.bindings {
            let node = self.node_mut(binding.node)?;
            // Promoted instance values replace definition animation for that property.
            let prefix = format!("{}.", binding.property);
            node.animation = node.animation.for_evaluation_without(&prefix);
            node.set_property(&binding.property, &values[&binding.control])?;
        }
        Ok(())
    }
}
