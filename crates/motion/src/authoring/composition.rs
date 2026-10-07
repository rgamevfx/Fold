//! Atomic nesting of editable layers; documents keep their own clocks and IDs.
use super::*;
use crate::nodes::composition::{KIND, Settings};
use fold_foundation::{DocumentId, Time};
use fold_project::{DocumentRef, EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const NEST: &str = "fold.motion.precompose";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    pub document: DocumentId,
    pub child: DocumentId,
    pub name: String,
    /// Empty selection creates a new empty nested composition.
    pub objects: Vec<ObjectId>,
}

pub fn place(
    m: &mut Motion,
    source: DocumentRef,
    info: fold_media::VideoInfo,
    at: Time,
    source_start: Time,
    duration: Time,
    name: String,
) -> Result<ObjectId, String> {
    let id = scene::create_object(m, KIND)?;
    node(m, id)?.settings = serde_json::to_value(Settings {
        source: Some(source),
        info: info.clone(),
        start: at,
        source_start,
    })
    .map_err(|e| e.to_string())?;
    let object = m
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap();
    object.name = name;
    object.start = at;
    object.end = at.checked_add(duration).map_err(|e| e.to_string())?;
    let transform = object.transform.unwrap();
    node(m, transform)?.set(
        "translation",
        Datum::Vector([info.width as f64 / 2., info.height as f64 / 2.]),
    );
    Ok(id)
}

/// Moving a strip moves its source clock; trimming preserves the existing clock.
pub fn set_range(m: &mut Motion, id: ObjectId, start: Time, end: Time) -> Result<(), String> {
    if start >= end {
        return Err("Layer range must have positive duration".into());
    }
    let object = m
        .scene
        .as_ref()
        .and_then(|s| s.objects.iter().find(|o| o.id == id))
        .ok_or("Missing layer")?;
    let delta = start.checked_sub(object.start).map_err(|e| e.to_string())?;
    let moving = end.checked_sub(object.end).map_err(|e| e.to_string())? == delta;
    let source = object.source.node;
    let source_node = m.graph.node(source)?;
    let origin = if moving && source_node.kind == KIND {
        Some(
            source_node
                .settings::<Settings>()?
                .start
                .checked_add(delta)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let object = m
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap();
    object.start = start;
    object.end = end;
    if let Some(origin) = origin {
        node(m, source)?.settings["start"] =
            serde_json::to_value(origin).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn reachable(
    m: &Motion,
    roots: impl IntoIterator<Item = ObjectId>,
) -> Result<BTreeSet<ObjectId>, String> {
    let mut pending: Vec<_> = roots.into_iter().collect();
    let mut ids = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !ids.insert(id) {
            continue;
        }
        for input in m.graph.node(id)?.inputs.values() {
            if let Input::Link(link) = input {
                pending.push(link.node);
            }
        }
    }
    Ok(ids)
}
fn roots(object: &crate::scene::Object) -> Vec<ObjectId> {
    [object.source.node, object.output.node]
        .into_iter()
        .chain(object.transform)
        .chain(object.appearance)
        .chain(object.modifiers.iter().copied())
        .collect()
}
fn retain_groups(m: &mut Motion) -> Result<(), String> {
    let groups = |nodes: &[Node]| -> Result<Vec<ObjectId>, String> {
        nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.kind.as_str(),
                    "fold.motion.group_instance" | "fold.motion.solver"
                )
            })
            .filter_map(|n| n.settings.get("group").filter(|v| !v.is_null()))
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
            .collect()
    };
    let mut pending = groups(&m.graph.nodes)?;
    let mut used = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if used.insert(id) {
            pending.extend(groups(
                &m.groups.get(&id).ok_or("Missing group")?.graph.nodes,
            )?);
        }
    }
    m.groups.retain(|id, _| used.contains(id));
    Ok(())
}

pub fn stage(
    snapshot: &Snapshot,
    bytes: &[u8],
    cancel: &fold_media::Cancel,
) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: Args = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if snapshot.state().documents.contains_key(&args.child) || args.name.trim().is_empty() {
        return Err("Choose a new composition ID and a nonempty name".into());
    }
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing composition")?;
    let mut parent = Motion::from_document(original)?;
    let mut child = Motion::new_scene();
    child.info = parent.info.clone();
    let mut insertion = 0;
    let mut parent_id = None;
    let mut replaces_output = false;
    if !args.objects.is_empty() {
        let scene = parent
            .scene
            .as_ref()
            .ok_or("Organize the document as layers before nesting")?;
        let selected: BTreeSet<_> = args.objects.iter().copied().collect();
        if selected
            .iter()
            .any(|id| !scene.objects.iter().any(|o| o.id == *id))
        {
            return Err("Select layers to precompose".into());
        }
        // Reduce a selection of a group plus its descendants to its top-level roots.
        let top: Vec<_> = scene
            .objects
            .iter()
            .filter(|o| selected.contains(&o.id))
            .filter(|o| {
                let mut ancestor = o.parent;
                while let Some(id) = ancestor {
                    if selected.contains(&id) {
                        return false;
                    }
                    ancestor = scene
                        .objects
                        .iter()
                        .find(|v| v.id == id)
                        .and_then(|v| v.parent);
                }
                true
            })
            .map(|o| o.id)
            .collect();
        parent_id = scene
            .objects
            .iter()
            .find(|o| o.id == top[0])
            .unwrap()
            .parent;
        if top
            .iter()
            .any(|id| scene.objects.iter().find(|o| o.id == *id).unwrap().parent != parent_id)
        {
            return Err("Precompose layers with the same parent".into());
        }
        let siblings: Vec<_> = scene
            .objects
            .iter()
            .filter(|o| o.parent == parent_id)
            .map(|o| o.id)
            .collect();
        let indices: Vec<_> = siblings
            .iter()
            .enumerate()
            .filter(|(_, id)| top.contains(id))
            .map(|(i, _)| i)
            .collect();
        if indices.last().unwrap() - indices[0] + 1 != indices.len() {
            return Err("Precompose adjacent layers to preserve stacking order".into());
        }
        let mut moved: BTreeSet<_> = top.iter().copied().collect();
        loop {
            let before = moved.len();
            for o in &scene.objects {
                if o.parent.is_some_and(|id| moved.contains(&id)) {
                    moved.insert(o.id);
                }
            }
            if before == moved.len() {
                break;
            }
        }
        if scene
            .objects
            .iter()
            .any(|o| moved.contains(&o.id) && o.locked)
        {
            return Err("Unlock selected layers and descendants before nesting".into());
        }
        let first = scene.objects.iter().position(|o| o.id == top[0]).unwrap();
        insertion = scene.objects[..first]
            .iter()
            .filter(|o| !moved.contains(&o.id))
            .count();
        replaces_output = scene.output.is_some_and(|id| moved.contains(&id));
        let nodes = reachable(
            &parent,
            scene
                .objects
                .iter()
                .filter(|o| moved.contains(&o.id))
                .flat_map(roots),
        )?;
        child.fonts = parent.fonts.clone();
        child.controls = parent.controls.clone();
        child.groups = parent.groups.clone();
        child.graph.nodes.extend(
            parent
                .graph
                .nodes
                .iter()
                .filter(|n| nodes.contains(&n.id))
                .cloned(),
        );
        child.scene = Some(crate::scene::Scene {
            objects: scene
                .objects
                .iter()
                .filter(|o| moved.contains(&o.id))
                .cloned()
                .map(|mut o| {
                    if top.contains(&o.id) {
                        o.parent = None;
                    }
                    o
                })
                .collect(),
            output: scene.output.filter(|id| moved.contains(id)),
            extensions: Default::default(),
        });
        retain_groups(&mut child)?;
        child
            .validate()
            .map_err(|e| format!("Selected layers depend on layers outside the selection: {e}"))?;
        // Retain shared dependencies still referenced by any remaining authored node.
        let keep = reachable(
            &parent,
            parent
                .graph
                .nodes
                .iter()
                .filter(|n| !nodes.contains(&n.id))
                .map(|n| n.id),
        )?;
        parent
            .graph
            .nodes
            .retain(|n| !nodes.contains(&n.id) || keep.contains(&n.id));
        parent
            .scene
            .as_mut()
            .unwrap()
            .objects
            .retain(|o| !moved.contains(&o.id));
        if replaces_output {
            parent.scene.as_mut().unwrap().output = None;
        }
    }
    let duration = child.info.time(child.info.frames)?;
    let id = place(
        &mut parent,
        DocumentRef {
            document: args.child,
            output: "video".into(),
            extensions: Default::default(),
        },
        child.info.clone(),
        Time::ZERO,
        Time::ZERO,
        duration,
        args.name.clone(),
    )?;
    let scene = parent.scene.as_mut().unwrap();
    let mut object = scene.objects.remove(0);
    object.parent = parent_id;
    scene
        .objects
        .insert(insertion.min(scene.objects.len()), object);
    if replaces_output {
        scene.output = Some(id);
    }
    let mut parent_document = parent.document(args.document).map_err(|e| {
        format!(
            "Remaining layers depend on the selection; include those layers before nesting: {e}"
        )
    })?;
    parent_document.extensions = original.extensions.clone();
    let mut child_document = child.document(args.child)?;
    child_document
        .extensions
        .insert("name".into(), args.name.clone().into());
    let parent_bin = snapshot
        .state()
        .organization
        .items
        .iter()
        .find(|item| item.id == fold_project::ItemId::Document(args.document))
        .map(|item| item.parent)
        .unwrap_or_default();
    let child_item = fold_project::Item {
        id: fold_project::ItemId::Document(args.child),
        name: args.name,
        parent: parent_bin,
        order: snapshot
            .state()
            .organization
            .items
            .iter()
            .map(|item| item.order)
            .max()
            .unwrap_or(0)
            .saturating_add(1),
        extensions: Default::default(),
    };
    cancel.check()?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![
            Mutation::PutDocument(child_document),
            Mutation::PutDocument(parent_document),
            Mutation::PutItem(child_item),
        ],
    })
}
