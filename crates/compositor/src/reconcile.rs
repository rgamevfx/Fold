//! Native compositor rebase: disjoint controls survive; competing property,
//! topology and protection changes require a fresh inspected proposal.
use crate::{Composite, Node};
use fold_graph::reconcile::{map, property};
use std::collections::{BTreeMap, BTreeSet};
fn node(base: &Node, proposed: &Node, current: &Node) -> Result<Node, String> {
    let mut result = current.clone();
    result.operation =
        fold_graph::reconcile::node(&base.operation, &proposed.operation, &current.operation)?;
    result.animation = crate::animation::Channels::reconcile(
        &base.animation,
        &proposed.animation,
        &current.animation,
    )?;
    result.position = property(&base.position, &proposed.position, &current.position)?;
    result.extensions = map(&base.extensions, &proposed.extensions, &current.extensions)?;
    result.inputs = if base.inputs.len() == proposed.inputs.len()
        && base.inputs.len() == current.inputs.len()
    {
        base.inputs
            .iter()
            .zip(&proposed.inputs)
            .zip(&current.inputs)
            .map(|((b, p), c)| property(b, p, c))
            .collect::<Result<_, _>>()?
    } else {
        property(&base.inputs, &proposed.inputs, &current.inputs)?
    };
    macro_rules! effect {
        ($field:ident) => {
            result.effect.$field = property(
                &base.effect.$field,
                &proposed.effect.$field,
                &current.effect.$field,
            )?;
        };
    }
    effect!(id);
    effect!(protected);
    effect!(mask);
    effect!(mask_disabled);
    effect!(mask_channel);
    effect!(invert);
    effect!(mix);
    effect!(channels);
    result.effect.extensions = map(
        &base.effect.extensions,
        &proposed.effect.extensions,
        &current.effect.extensions,
    )?;
    Ok(result)
}
impl Composite {
    pub fn rebase(base: &Self, proposed: &Self, current: &Self) -> Result<Self, String> {
        base.validate()?;
        proposed.validate()?;
        current.validate()?;
        let order = |graph: &Self| graph.nodes.iter().map(|n| n.id).collect::<Vec<_>>();
        let order = property(&order(base), &order(proposed), &order(current))?;
        let ids: BTreeSet<_> = base
            .nodes
            .iter()
            .chain(&proposed.nodes)
            .chain(&current.nodes)
            .map(|n| n.id)
            .collect();
        let mut nodes = BTreeMap::new();
        for id in ids {
            let find = |graph: &Self| graph.nodes.iter().find(|n| n.id == id).cloned();
            let (b, p, c) = (find(base), find(proposed), find(current));
            let merged = match (&b, &p, &c) {
                (Some(b), Some(p), Some(c)) => Some(node(b, p, c)?),
                _ => property(&b, &p, &c)?,
            };
            if let Some(merged) = merged {
                nodes.insert(id, merged);
            }
        }
        let merged = Self {
            info: property(&base.info, &proposed.info, &current.info)?,
            nodes: order
                .into_iter()
                .map(|id| nodes.remove(&id).ok_or("Merged compositor lost a node"))
                .collect::<Result<_, _>>()?,
            output: property(&base.output, &proposed.output, &current.output)?,
            extensions: map(&base.extensions, &proposed.extensions, &current.extensions)?,
        };
        if !nodes.is_empty() {
            return Err("Merged compositor contains unordered nodes".into());
        }
        merged.validate()?;
        current.check_protection(&merged)?;
        Ok(merged)
    }
}
