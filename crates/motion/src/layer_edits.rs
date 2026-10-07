//! Explicit MoGraph authoring updates its owning layer hierarchy in the same undo
//! transaction. Ordinary node/property edits do not rewrite layer state.
use crate::Motion;
use fold_composition::{Composition, Layer};
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::image_source::Source;
use fold_project::{DocumentRef, Mutation, Snapshot};
use std::collections::BTreeMap;

pub(crate) fn append(
    snapshot: &Snapshot,
    document: DocumentId,
    before: Option<&crate::scene::Scene>,
    after: &Motion,
    mutations: &mut Vec<Mutation>,
) -> Result<(), String> {
    if before == after.scene.as_ref() {
        return Ok(());
    }
    let previous = before.map(|s| s.objects.as_slice()).unwrap_or(&[]);
    let objects = after
        .scene
        .as_ref()
        .map(|s| s.objects.as_slice())
        .unwrap_or(&[]);
    for root in snapshot
        .state()
        .documents
        .values()
        .filter(|d| d.type_id == fold_composition::COMPOSITION)
    {
        let mut composition = Composition::from_document(root)?;
        let belongs = |l: &Layer| {
            matches!(&l.source, Some(Source::Output(s)) if s.document == document)
                && l.graph_object().is_some()
        };
        if !composition.layers.iter().any(belongs) {
            continue;
        }
        let original = composition.clone();
        let mut ids: BTreeMap<_, _> = composition
            .layers
            .iter()
            .filter(|l| belongs(l))
            .map(|l| (l.graph_object().unwrap(), l.id))
            .collect();
        for object in objects
            .iter()
            .filter(|o| !previous.iter().any(|p| p.id == o.id))
        {
            let id = if composition.layers.iter().any(|l| l.id == object.id) {
                ObjectId::new()
            } else {
                object.id
            };
            ids.insert(object.id, id);
        }
        composition
            .layers
            .retain(|l| !belongs(l) || objects.iter().any(|o| Some(o.id) == l.graph_object()));
        for object in objects {
            let Some(&id) = ids.get(&object.id) else {
                continue;
            };
            let old = previous.iter().find(|o| o.id == object.id);
            let index = composition.layers.iter().position(|l| l.id == id);
            let layer = if let Some(index) = index {
                &mut composition.layers[index]
            } else {
                let reference = DocumentRef {
                    document,
                    output: "video".into(),
                    extensions: BTreeMap::from([(
                        fold_composition::graph::OBJECT.into(),
                        serde_json::json!(object.id),
                    )]),
                };
                composition.layers.push(Layer::new(
                    &object.name,
                    Some(Source::Output(reference)),
                    after.info.clone(),
                )?);
                composition.layers.last_mut().unwrap().id = id;
                composition.layers.last_mut().unwrap()
            };
            macro_rules! changed {
                ($($field:ident),*) => { $(if old.is_none_or(|old| old.$field != object.$field) { layer.$field = object.$field.clone(); })* };
            }
            changed!(
                name, visible, locked, start, end, offset, opacity, animation
            );
            if old.is_none_or(|old| old.parent != object.parent) {
                layer.parent = object.parent.and_then(|parent| ids.get(&parent).copied());
            }
            layer.graph_children =
                after.graph.node(object.source.node)?.kind == "fold.motion.scene_children";
        }
        let previous_order: Vec<_> = previous.iter().map(|o| o.id).collect();
        let order: Vec<_> = objects.iter().map(|o| o.id).collect();
        if previous_order != order {
            let mut ordered: Vec<_> = composition
                .layers
                .iter()
                .filter(|l| belongs(l))
                .cloned()
                .collect();
            ordered.sort_by_key(|l| order.iter().position(|id| Some(*id) == l.graph_object()));
            let mut ordered = ordered.into_iter();
            for layer in &mut composition.layers {
                if belongs(layer) {
                    *layer = ordered.next().unwrap();
                }
            }
        }
        if composition != original {
            let mut updated = composition.document(root.id)?;
            updated.extensions = root.extensions.clone();
            mutations.push(Mutation::PutDocument(updated));
        }
    }
    Ok(())
}
