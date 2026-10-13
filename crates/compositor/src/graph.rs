//! Authoring operations independent of storage order, canvas position, or UI.
use crate::{Composite, Node, OperationConfig};
use fold_foundation::ObjectId;
use fold_graph::types::Type;
use std::collections::BTreeMap;

impl crate::Operator {
    pub fn label(&self) -> &'static str {
        match self.id {
            "Solid" => "Constant",
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
            "Color Shift"
                | "Chromatic Aberration"
                | "Lens Distortion"
                | "Film Emulation"
                | "Grain"
                | "Crop"
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
        (self.id == "Roto" && slot == 0)
            || (self.id == "Shuffle" && slot == 1)
            || (self.id == "Merge" && slot == 0)
    }

    /// Stable socket names within the versioned operator schema, never UI labels.
    pub fn input_key(&self, slot: usize) -> &'static str {
        match (self.id, slot) {
            ("Shuffle", 0) => "B",
            ("Shuffle", 1) => "A",
            ("Merge", 0) => "foreground",
            ("Merge", 1) => "background",
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
            animation: crate::retime::initial_channels(&parameters),
            inputs,
            effect: crate::Effect::for_config(&parameters),
            disabled: false,
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
        if matches!(source_node.config(), OperationConfig::Viewer)
            || (matches!(source_node.config(), OperationConfig::Output)
                && !matches!(
                    self.node(target)?.prepare()?.config(),
                    OperationConfig::Viewer
                ))
        {
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
            && !matches!(parameters, OperationConfig::Viewer)
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
    /// Extract a selection atomically, bridging through its primary image pipe.
    /// Kept nodes lose their inputs; deletion uses the same topology operation.
    pub fn detach(&mut self, ids: &[ObjectId]) -> Result<(), String> {
        if ids.contains(&self.output) {
            return Err("The document Output cannot be disconnected".into());
        }
        let types = self
            .nodes
            .iter()
            .map(|node| Ok((node.id, node.prepare()?.config().operator().output.clone())))
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let mut replacements = BTreeMap::new();
        for &id in ids {
            let mut current = Some(id);
            let mut seen = std::collections::BTreeSet::new();
            while let Some(next) = current.filter(|next| ids.contains(next)) {
                if !seen.insert(next) {
                    return Err("Connection cycle".into());
                }
                let node = self.node(next)?;
                current = node
                    .prepare()?
                    .config()
                    .primary_slot()
                    .and_then(|slot| node.input_ids().get(slot).copied().flatten());
            }
            replacements.insert(id, current);
        }
        let mut next = self.clone();
        for node in &mut next.nodes {
            if ids.contains(&node.id) {
                node.inputs.fill(None);
                node.effect.mask = None;
                continue;
            }
            let config = node.prepare()?.config().clone();
            for (slot, edge) in node.inputs.iter_mut().enumerate() {
                let Some(fold_graph::model::Edge {
                    target: fold_graph::model::Reference::Node { node: source },
                    ..
                }) = edge
                else {
                    continue;
                };
                if let Some(replacement) = replacements.get(source) {
                    let compatible = replacement.filter(|id| {
                        types.get(id) == Some(&config.input_type(slot))
                            || (config.input_type(slot) == Type::Mask
                                && types.get(id) == Some(&Type::Image))
                            || matches!(config, OperationConfig::Viewer)
                    });
                    if let Some(id) = compatible {
                        *source = id;
                    } else {
                        *edge = None;
                    }
                }
            }
            if let Some(source) = node.effect.mask_node()
                && let Some(replacement) = replacements.get(&source)
            {
                if let Some(id) = replacement {
                    node.effect.set_mask(*id);
                } else {
                    node.effect.mask = None;
                }
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn remove(&mut self, ids: &[ObjectId]) -> Result<(), String> {
        self.detach(ids)?;
        self.nodes.retain(|n| !ids.contains(&n.id));
        Ok(())
    }
    /// Attach a newly created node to the selected pipe, preserving edge IDs.
    pub fn insert_after(&mut self, source: ObjectId, added: ObjectId) -> Result<(), String> {
        let config = self.node(added)?.prepare()?.config().clone();
        let Some(slot) = config.primary_slot() else {
            return Ok(());
        };
        let source_config = self.node(source)?.prepare()?.config().clone();
        if source_config.operator().output != config.input_type(slot)
            && !(config.input_type(slot) == Type::Mask
                && source_config.operator().output == Type::Image)
        {
            return Ok(());
        }
        let mut next = self.clone();
        if !matches!(
            config,
            OperationConfig::Viewer | OperationConfig::Write { .. } | OperationConfig::Output
        ) {
            for node in &self.nodes {
                if node.id == added {
                    continue;
                }
                let target = node.prepare()?.config().clone();
                for (i, input) in node.input_ids().iter().enumerate() {
                    if *input == Some(source) {
                        next.connect(added, node.id, target.input_key(i))?;
                    }
                }
                if node.effect.mask_node() == Some(source) {
                    next.connect(added, node.id, "mask")?;
                }
            }
        }
        next.connect(source, added, config.input_key(slot))?;
        if let Some(p) = self.node(source)?.position {
            let position = [p[0], p[1] + 80.];
            for node in &mut next.nodes {
                if node.id != added
                    && let Some(at) = &mut node.position
                    && (at[0] - p[0]).abs() < 100.
                    && at[1] > p[1] + 40.
                {
                    at[1] += 80.;
                }
            }
            next.node_mut(added)?.position = Some(position);
        }
        next.validate()?;
        *self = next;
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
