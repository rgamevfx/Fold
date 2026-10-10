//! Immutable execution contracts borrowing authoritative native records.
//! Construction is the only decoding boundary; consumers share typed values and ID indexes.
use crate::{Composite, Node, OperationConfig};
use fold_foundation::{ObjectId, Time};
use fold_graph::types::Type;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreparationCounters {
    pub decoded_records: usize,
    pub decoded_channel_bindings: usize,
}

pub struct PreparedNode<'a> {
    native: &'a Node,
    config: OperationConfig,
    animation: crate::animation::PreparedChannels,
    pub(crate) instance: Option<crate::instance::Instance>,
    parameters: BTreeMap<ObjectId, &'a str>,
}
impl std::ops::Deref for PreparedNode<'_> {
    type Target = Node;
    fn deref(&self) -> &Node {
        self.native
    }
}
impl Node {
    pub fn prepare(&self) -> Result<PreparedNode<'_>, String> {
        self.prepare_counted(&mut Default::default())
    }
    fn prepare_counted(
        &self,
        counters: &mut PreparationCounters,
    ) -> Result<PreparedNode<'_>, String> {
        let (config, instance) = crate::operation::decode(&self.operation, counters)
            .map_err(|error| format!("Node {:?}: {error}", self.id))?;
        if self.inputs.len() != config.input_count() {
            return Err("Wrong compositor input count".into());
        }
        self.effect.validate()?;
        let mut parameters = BTreeMap::new();
        for (name, parameter) in &self.operation.parameters {
            if parameters.insert(parameter.id, name.as_str()).is_some() {
                return Err("Duplicate compositor parameter identity".into());
            }
        }
        let animation = self.animation.prepare()?;
        counters.decoded_channel_bindings += animation.len();
        let prepared = PreparedNode {
            native: self,
            config,
            animation,
            instance,
            parameters,
        };
        prepared.validate_animation()?;
        Ok(prepared)
    }
}
impl PreparedNode<'_> {
    pub(crate) fn channels(&self) -> &crate::animation::PreparedChannels {
        &self.animation
    }
    pub fn config(&self) -> &OperationConfig {
        &self.config
    }
    pub fn parameter_name(&self, id: ObjectId) -> Result<&str, String> {
        self.parameters
            .get(&id)
            .copied()
            .ok_or_else(|| "Missing parameter".into())
    }
    /// Inserting a Merge on B leaves A available without interrupting the pipe.
    pub(crate) fn background_only_merge(&self) -> bool {
        matches!(self.config(), OperationConfig::Composite { .. })
            && self.inputs.first().is_some_and(Option::is_none)
    }
}

pub struct PreparedComposite<'a> {
    native: &'a Composite,
    pub(crate) nodes: Vec<PreparedNode<'a>>,
    indexes: BTreeMap<ObjectId, usize>,
    counters: PreparationCounters,
    order: Vec<ObjectId>,
    reusable: Option<crate::reusable::Definition>,
    pub(crate) references: BTreeMap<fold_graph::model::Reference, (ObjectId, String, f64)>,
}
impl std::ops::Deref for PreparedComposite<'_> {
    type Target = Composite;
    fn deref(&self) -> &Composite {
        self.native
    }
}
impl Composite {
    pub fn prepare(&self) -> Result<PreparedComposite<'_>, String> {
        let mut counters = PreparationCounters::default();
        let mut nodes = Vec::with_capacity(self.nodes.len());
        let mut indexes = BTreeMap::new();
        for node in &self.nodes {
            if indexes.insert(node.id, nodes.len()).is_some() {
                return Err("Duplicate compositor node identity".into());
            }
            nodes.push(node.prepare_counted(&mut counters)?);
        }
        let mut prepared = PreparedComposite {
            references: BTreeMap::new(),
            native: self,
            order: self.evaluation_order()?,
            reusable: self
                .extensions
                .contains_key(crate::reusable::KEY)
                .then(|| self.reusable())
                .transpose()?,
            counters,
            nodes,
            indexes,
        };
        prepared.index_references()?;
        prepared.validate()?;
        Ok(prepared)
    }
}
impl<'a> PreparedComposite<'a> {
    pub(crate) fn with_invocation(
        mut self,
        invocation: &fold_platform::effects::Invocation<'_>,
    ) -> Result<Self, String> {
        let definition = self.reusable()?.clone();
        let values = definition.interface.values(&invocation.values)?;
        for binding in definition.bindings {
            let node = self.node_mut(binding.node)?;
            // Promoted instance values replace definition animation for that property.
            let prefix = format!("{}.", binding.property);
            node.animation.shadow(&prefix);
            let mut config = node.config.clone();
            config.set_property(&binding.property, &values[&binding.control])?;
            config.validate()?;
            let parameter = node
                .operation
                .parameters
                .get(config.parameter_for_property(&binding.property));
            if parameter.is_some_and(|p| p.protected) && config != node.config {
                return Err(format!(
                    "Protected compositor parameter {}",
                    config.parameter_for_property(&binding.property)
                ));
            }
            node.config = config;
        }
        self.index_references()?;
        Ok(self)
    }

    pub fn reusable(&self) -> Result<&crate::reusable::Definition, String> {
        self.reusable
            .as_ref()
            .ok_or_else(|| "Composite has no published effect interface".into())
    }
    pub fn evaluation_order(&self) -> &[ObjectId] {
        &self.order
    }
    pub fn nodes(&self) -> &[PreparedNode<'a>] {
        &self.nodes
    }
    pub fn node(&self, id: ObjectId) -> Result<&PreparedNode<'a>, String> {
        self.indexes
            .get(&id)
            .map(|index| &self.nodes[*index])
            .ok_or_else(|| "Missing node".into())
    }
    fn node_mut(&mut self, id: ObjectId) -> Result<&mut PreparedNode<'a>, String> {
        let index = *self.indexes.get(&id).ok_or("Missing node")?;
        Ok(&mut self.nodes[index])
    }
    pub fn counters(&self) -> PreparationCounters {
        self.counters
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.validate_references()?;
        self.info.validate()?;
        if self.nodes.is_empty()
            || self.nodes.len() > 512
            || self.info.time(self.info.frames)? > Time::new(600, 1).unwrap()
        {
            return Err("composite requires 1..512 nodes and at most ten minutes".into());
        }
        let mut ports = BTreeMap::new();
        for node in &self.nodes {
            if !matches!(node.config(), OperationConfig::Output) {
                ports.insert(node.id, node.config().operator().output.clone());
            }
        }
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            fold_graph::model::extension_keys(
                &node.operation.extensions,
                &["id", "name", "operation", "version", "mode", "parameters"],
            )?;
            if node.name.len() > 256 {
                return Err("Node name exceeds 256 bytes".into());
            }
            if let Some(instance) = node.instance.as_ref() {
                for id in instance.identities()? {
                    if !ids.insert(id) {
                        return Err("Duplicate instance control identity".into());
                    }
                }
            }
            for parameter in node.operation.parameters.values() {
                if !ids.insert(parameter.id) {
                    return Err("Duplicate compositor parameter identity".into());
                }
                fold_graph::model::extension_keys(
                    &parameter.extensions,
                    &["id", "value", "protected"],
                )?;
            }
            for &id in node.animation.identities() {
                if !ids.insert(id) {
                    return Err("Duplicate authored identity".into());
                }
            }
            if !ids.insert(node.effect.id) {
                return Err("Duplicate effect control identity".into());
            }
            if let Some(edge) = &node.effect.mask
                && !ids.insert(edge.id)
            {
                return Err("Duplicate effect mask connection identity".into());
            }
            if !node.config().operator().supports_effect() && !node.effect.is_default() {
                return Err("This node does not support effect controls".into());
            }
            if let Some(mask) = node.effect.mask_node()
                && !matches!(ports.get(&mask), Some(Type::Image | Type::Mask))
            {
                return Err("Mask input requires an image or mask output".into());
            }
            let op = node.config().operator();
            if !ids.insert(node.id)
                || node.inputs.len() != node.config().input_count()
                || node
                    .position
                    .is_some_and(|p| p.iter().any(|v| !v.is_finite() || v.abs() > 1e7))
            {
                return Err(format!(
                    "{}: duplicate identity or wrong input count",
                    op.id
                ));
            }
            for edge in node.inputs.iter().flatten() {
                edge.validate()?;
                if !matches!(edge.target, fold_graph::model::Reference::Node { .. })
                    || !ids.insert(edge.id)
                {
                    return Err(
                        "Compositor inputs require unique node connection identities".into(),
                    );
                }
            }
            for (slot, input) in node.input_ids().iter().enumerate() {
                let expected = node.config().input_type(slot);
                if let Some(input) = input
                    && ports.get(input) != Some(&expected)
                    && !(expected == Type::Mask && ports.get(input) == Some(&Type::Image))
                {
                    return Err(format!(
                        "{}: input requires {expected:?} (missing or incompatible connection)",
                        op.id
                    ));
                }
            }
        }
        if !self
            .nodes
            .iter()
            .any(|n| n.id == self.output && matches!(n.config(), OperationConfig::Output))
        {
            return Err("composite output must identify an Output node".into());
        }
        self.validate_reusable()?;
        Ok(())
    }
    pub fn render_order(&self) -> Result<Vec<ObjectId>, String> {
        let order = self.evaluation_order();
        let mut needed = BTreeSet::from([self.output]);
        for id in order.iter().rev() {
            if needed.contains(id) {
                let n = self.node(*id)?;
                if let Some(mask) = n
                    .effect
                    .mask_node()
                    .filter(|_| !n.effect.mask_disabled && !n.background_only_merge())
                {
                    needed.insert(mask);
                }
                for (slot, input) in n.input_ids().iter().enumerate() {
                    if input.is_none() && n.config().operator().optional_input(slot) {
                        continue;
                    }
                    needed.insert(input.ok_or_else(|| {
                        format!(
                            "{} {:?}: connect '{}' input",
                            n.config().operator().id,
                            n.id,
                            n.config().input_key(slot)
                        )
                    })?);
                }
            }
        }
        Ok(order
            .iter()
            .copied()
            .filter(|id| needed.contains(id))
            .collect())
    }
}

#[cfg(test)]
mod tests;
