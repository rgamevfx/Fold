//! Authoring operations independent of storage order, canvas position, or UI.
use crate::{Composite, Node, Parameters, PortType};
use fold_foundation::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

impl crate::Operator {
    /// Stable socket names within the versioned operator schema, never UI labels.
    pub fn input_key(&self, slot: usize) -> &'static str {
        match (self.id, slot) {
            ("Merge", 0) => "foreground",
            ("Merge", 1) => "background",
            ("ApplyMask", 1) => "mask",
            _ => "image",
        }
    }
    pub fn output_key(&self) -> &'static str {
        if self.output == PortType::Mask {
            "mask"
        } else {
            "image"
        }
    }
}
impl Node {
    pub fn disconnected(parameters: Parameters) -> Self {
        let inputs = vec![None; parameters.operator().inputs.len()];
        Self {
            id: ObjectId::new(),
            parameters,
            inputs,
            position: None,
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
            .map(|n| (n.id, n.inputs.iter().flatten().count()))
            .collect();
        let mut consumers: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
        for n in &self.nodes {
            for input in n.inputs.iter().flatten() {
                if !pending.contains_key(input) {
                    return Err("connection references a missing node".into());
                }
                consumers.entry(*input).or_default().push(n.id);
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
        let order = self.evaluation_order()?;
        let mut needed = BTreeSet::from([self.output]);
        for id in order.iter().rev() {
            if needed.contains(id) {
                let n = self.node(*id)?;
                for (slot, input) in n.inputs.iter().enumerate() {
                    needed.insert(input.ok_or_else(|| {
                        format!(
                            "{} {:?}: connect '{}' input",
                            n.parameters.operator().id,
                            n.id,
                            n.parameters.operator().input_key(slot)
                        )
                    })?);
                }
            }
        }
        Ok(order.into_iter().filter(|id| needed.contains(id)).collect())
    }
    pub fn connect(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        port: &str,
    ) -> Result<(), String> {
        let source_node = self.node(source)?;
        if matches!(source_node.parameters, Parameters::Output) {
            return Err("Output is a terminal node, not a source socket".into());
        }
        let source_type = source_node.parameters.operator().output;
        let target_node = self.node(target)?;
        let op = target_node.parameters.operator();
        let slot = (0..op.inputs.len())
            .find(|i| op.input_key(*i) == port)
            .ok_or("unknown input port")?;
        if source_type != op.inputs[slot] {
            return Err(format!(
                "{} requires {:?}, not {:?}",
                port, op.inputs[slot], source_type
            ));
        }
        let previous = self.node_mut(target)?.inputs[slot].replace(source);
        if let Err(error) = self.evaluation_order() {
            self.node_mut(target)?.inputs[slot] = previous;
            return Err(error);
        }
        Ok(())
    }
    pub fn disconnect(&mut self, target: ObjectId, port: &str) -> Result<(), String> {
        let n = self.node_mut(target)?;
        let slot = (0..n.inputs.len())
            .find(|i| n.parameters.operator().input_key(*i) == port)
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
            for input in &mut n.inputs {
                if input.is_some_and(|id| ids.contains(&id)) {
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
                .inputs
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
