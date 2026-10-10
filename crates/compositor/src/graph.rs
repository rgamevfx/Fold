//! Authoring operations independent of storage order, canvas position, or UI.
use crate::{Composite, Node, OperationConfig};
use fold_foundation::ObjectId;
use fold_graph::types::Type;
use std::collections::BTreeMap;

impl crate::Operator {
    pub fn label(&self) -> &'static str {
        match self.id {
            "Solid" => "Constant",
            "ApplyMask" => "Multiply Alpha",
            "RemoveChannels" => "Remove / Keep Channels",
            _ => self.id,
        }
    }
    pub fn input_label(&self, slot: usize) -> &'static str {
        match (self.id, slot) {
            ("Merge", 0) => "A",
            ("Merge", 1) => "B",
            _ => self.input_key(slot),
        }
    }
    pub fn supports_effect(&self) -> bool {
        matches!(
            self.id,
            "Crop"
                | "Blur"
                | "Grade"
                | "Merge"
                | "Exposure"
                | "Invert"
                | "Clamp"
                | "Premult"
                | "Unpremult"
        )
    }
    pub fn optional_input(&self, slot: usize) -> bool {
        (self.id == "Shuffle" && slot == 1) || (self.id == "Merge" && slot == 0)
    }

    /// Stable socket names within the versioned operator schema, never UI labels.
    pub fn input_key(&self, slot: usize) -> &'static str {
        match (self.id, slot) {
            ("Shuffle", 0) => "A",
            ("Shuffle", 1) => "B",
            ("Merge", 0) => "foreground",
            ("Merge", 1) => "background",
            ("ApplyMask", 1) => "mask",
            _ => "image",
        }
    }
    pub fn output_key(&self) -> &'static str {
        if self.output == Type::Mask {
            "mask"
        } else {
            "image"
        }
    }
}
impl Node {
    pub fn dependencies(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.inputs
            .iter()
            .flatten()
            .filter_map(|edge| match edge.target {
                fold_graph::model::Reference::Node { node } => Some(node),
                _ => None,
            })
            .chain(self.effect.mask_node())
    }

    pub fn disconnected(parameters: OperationConfig) -> Self {
        let inputs = vec![None; parameters.input_count()];
        Self {
            operation: crate::operation::record(&parameters),
            animation: Default::default(),
            inputs,
            effect: crate::Effect::for_config(&parameters),
            position: None,
            extensions: Default::default(),
        }
    }
}
impl Composite {
    pub fn node(&self, id: ObjectId) -> Result<&Node, String> {
        self.nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| "missing node".into())
    }
    pub fn node_mut(&mut self, id: ObjectId) -> Result<&mut Node, String> {
        self.nodes
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| "missing node".into())
    }
    /// Kahn ordering validates the whole graph without recursive stack growth.
    pub fn evaluation_order(&self) -> Result<Vec<ObjectId>, String> {
        let mut pending: BTreeMap<_, _> = self
            .nodes
            .iter()
            .map(|n| (n.id, n.dependencies().count()))
            .collect();
        let mut consumers: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
        for n in &self.nodes {
            for input in n.dependencies() {
                if !pending.contains_key(&input) {
                    return Err("connection references a missing node".into());
                }
                consumers.entry(input).or_default().push(n.id);
            }
        }
        let mut ready: Vec<_> = pending
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut order = Vec::new();
        while let Some(id) = ready.pop() {
            order.push(id);
            for next in consumers.get(&id).into_iter().flatten() {
                let count = pending.get_mut(next).unwrap();
                *count -= 1;
                if *count == 0 {
                    ready.push(*next);
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err("connection would create a cycle".into());
        }
        Ok(order)
    }
    /// Only reachable work is required to be connected. Incomplete spare nodes
    /// are editable; a missing socket in the output closure is an explicit error.
    pub fn render_order(&self) -> Result<Vec<ObjectId>, String> {
        self.prepare()?.render_order()
    }
    pub fn connect(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        port: &str,
    ) -> Result<(), String> {
        let source_node = self.node(source)?.prepare()?;
        if matches!(source_node.config(), OperationConfig::Output) {
            return Err("Output is a terminal node, not a source socket".into());
        }
        let source_type = source_node.config().operator().output.clone();
        let target_node = self.node(target)?.prepare()?;
        let op = target_node.config().operator();
        if port == "mask" && op.supports_effect() {
            if !matches!(source_type, Type::Image | Type::Mask) {
                return Err("Mask requires an image".into());
            }
            let previous = self.node(target)?.effect.mask.clone();
            self.node_mut(target)?.effect.set_mask(source);
            if let Err(error) = self.evaluation_order() {
                self.node_mut(target)?.effect.mask = previous;
                return Err(error);
            }
            return Ok(());
        }
        let parameters = target_node.config();
        let slot = (0..parameters.input_count())
            .find(|i| parameters.input_key(*i) == port)
            .ok_or("unknown input port")?;
        if source_type != parameters.input_type(slot)
            && !(parameters.input_type(slot) == Type::Mask && source_type == Type::Image)
        {
            return Err(format!(
                "{} requires {:?}, not {:?}",
                port,
                parameters.input_type(slot),
                source_type
            ));
        }
        let previous = self
            .node(target)?
            .inputs
            .get(slot)
            .ok_or("Wrong input count")?
            .clone();
        self.node_mut(target)?.set_input(slot, source)?;
        if let Err(error) = self.evaluation_order() {
            self.node_mut(target)?.inputs[slot] = previous;
            return Err(error);
        }
        Ok(())
    }
    pub fn disconnect(&mut self, target: ObjectId, port: &str) -> Result<(), String> {
        let n = self.node_mut(target)?;
        let prepared = n.prepare()?;
        let config = prepared.config();
        if port == "mask" && config.operator().supports_effect() {
            n.effect.mask = None;
            return Ok(());
        }
        let slot = (0..n.inputs.len())
            .find(|i| config.input_key(*i) == port)
            .ok_or("unknown input port")?;
        n.inputs[slot] = None;
        Ok(())
    }
    pub fn remove(&mut self, ids: &[ObjectId]) -> Result<(), String> {
        if ids.contains(&self.output) {
            return Err("the document Output cannot be deleted".into());
        }
        self.nodes.retain(|n| !ids.contains(&n.id));
        for n in &mut self.nodes {
            if n.effect.mask_node().is_some_and(|id| ids.contains(&id)) {
                n.effect.mask = None;
            }
            for input in &mut n.inputs {
                if input.as_ref().is_some_and(|edge| matches!(edge.target, fold_graph::model::Reference::Node { node } if ids.contains(&node))) {
                    *input = None;
                }
            }
        }
        Ok(())
    }
    pub fn auto_layout(&mut self) -> Result<(), String> {
        let mut depths = BTreeMap::new();
        let mut lanes = BTreeMap::new();
        for id in self.evaluation_order()? {
            let n = self.node(id)?;
            let depth = n
                .input_ids()
                .iter()
                .flatten()
                .map(|input| depths[input] + 1usize)
                .max()
                .unwrap_or(0);
            let lane = lanes.entry(depth).or_insert(0);
            self.node_mut(id)?.position = Some([depth as f32 * 260.0, *lane as f32 * 160.0]);
            *lane += 1;
            depths.insert(id, depth);
        }
        Ok(())
    }
}
