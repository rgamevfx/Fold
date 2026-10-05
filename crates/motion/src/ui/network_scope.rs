//! Construction membership is independent of the currently evaluated branch.
//! Stored as presentation metadata on the scene object; never used by evaluation.
use crate::{Motion, graph::Input};
use fold_foundation::ObjectId;
use std::collections::BTreeSet;

const MEMBERS: &str = "fold.motion.network_nodes";

pub(super) fn members(m: &Motion, object: ObjectId) -> BTreeSet<ObjectId> {
    let Some(object) = m
        .scene
        .as_ref()
        .and_then(|s| s.objects.iter().find(|o| o.id == object))
    else {
        return BTreeSet::new();
    };
    let mut pending = vec![object.source.node, object.output.node];
    pending.extend(object.transform);
    pending.extend(object.appearance);
    pending.extend(object.modifiers.iter().copied());
    if let Some(value) = object.extensions.get(MEMBERS)
        && let Ok(saved) = serde_json::from_value::<BTreeSet<ObjectId>>(value.clone())
    {
        pending.extend(saved);
    }
    let mut visible = BTreeSet::new();
    while let Some(id) = pending.pop() {
        let Ok(node) = m.graph.node(id) else {
            continue;
        };
        if !visible.insert(id) {
            continue;
        }
        pending.extend(node.inputs.values().filter_map(|input| match input {
            Input::Link(link) => Some(link.node),
            Input::Value(_) => None,
        }));
    }
    visible
}

pub(super) fn include(m: &mut Motion, object: ObjectId, nodes: impl IntoIterator<Item = ObjectId>) {
    let mut saved = members(m, object);
    saved.extend(nodes.into_iter().filter(|id| m.graph.node(*id).is_ok()));
    if let Some(object) = m
        .scene
        .as_mut()
        .and_then(|s| s.objects.iter_mut().find(|o| o.id == object))
    {
        object.extensions.insert(
            MEMBERS.into(),
            serde_json::to_value(saved).expect("node IDs serialize"),
        );
    }
}

/// Retain disconnected branches in the same transaction as the edit. Explicitly
/// deleted nodes are omitted, and unchanged constructions need no extra metadata.
pub(super) fn retain(before: &Motion, after: &mut Motion) {
    let Some(scene) = &before.scene else {
        return;
    };
    for object in &scene.objects {
        let previous = members(before, object.id);
        let current = members(after, object.id);
        let lost: Vec<_> = previous
            .difference(&current)
            .copied()
            .filter(|id| after.graph.node(*id).is_ok())
            .collect();
        if !lost.is_empty() {
            include(after, object.id, lost);
        }
    }
}
