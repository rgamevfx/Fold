//! Guarded native edits shared by artist controls and agent authoring. A batch
//! validates completely before one project transaction can publish it.
use crate::model::{Content, Geometry, Material, Object, Scene};
use fold_foundation::{DocumentId, ObjectId};
use fold_graph::{binding::Binding, model::Parameter};
use fold_media::Cancel;
use fold_project::{EditBatch, Metadata, Mutation, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Placement {
    pub parent: Option<ObjectId>,
    pub index: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Insert {
        id: ObjectId,
        object: Box<Object>,
        placement: Placement,
    },
    /// Insert a complete hierarchy atomically, retaining copied protection.
    InsertTree {
        root: ObjectId,
        objects: BTreeMap<ObjectId, Object>,
        placement: Placement,
    },
    Replace {
        id: ObjectId,
        expected: Box<Object>,
        value: Box<Object>,
    },
    Parameter {
        object: ObjectId,
        expected: Parameter,
        value: Parameter,
    },
    Move {
        id: ObjectId,
        expected: Placement,
        value: Placement,
    },
    /// Preserve world animation, including parent motion and affine shear.
    MoveWorld {
        id: ObjectId,
        expected: Placement,
        parent: Option<ObjectId>,
    },
    Delete {
        id: ObjectId,
        expected: Box<Object>,
    },
    Geometry {
        id: ObjectId,
        expected: Option<Geometry>,
        value: Option<Geometry>,
    },
    Material {
        id: ObjectId,
        expected: Option<Material>,
        value: Option<Material>,
    },
    ActiveCamera {
        expected: ObjectId,
        value: ObjectId,
    },
}
fn placements(scene: &Scene) -> BTreeMap<ObjectId, Placement> {
    let mut output = BTreeMap::new();
    for (index, id) in scene.roots.iter().enumerate() {
        output.insert(
            *id,
            Placement {
                parent: None,
                index,
            },
        );
    }
    for (&parent, object) in &scene.objects {
        if let Content::Group { children, .. } = &object.content {
            for (index, id) in children.iter().enumerate() {
                output.insert(
                    *id,
                    Placement {
                        parent: Some(parent),
                        index,
                    },
                );
            }
        }
    }
    output
}
pub fn placement(scene: &Scene, id: ObjectId) -> Result<Placement, String> {
    placements(scene)
        .remove(&id)
        .ok_or("3D object has no owner".into())
}
pub fn editable(scene: &Scene, id: ObjectId) -> Result<(), String> {
    editable_with(scene, &placements(scene), id)
}
pub(crate) fn editable_ids(scene: &Scene) -> BTreeSet<ObjectId> {
    let owners = placements(scene);
    scene
        .objects
        .keys()
        .copied()
        .filter(|id| editable_with(scene, &owners, *id).is_ok())
        .collect()
}
fn editable_with(
    scene: &Scene,
    owners: &BTreeMap<ObjectId, Placement>,
    id: ObjectId,
) -> Result<(), String> {
    let mut next = Some(id);
    let mut depth = 0;
    while let Some(id) = next {
        depth += 1;
        if depth > 129 {
            return Err("3D hierarchy cycle or depth limit".into());
        }
        if scene.objects.get(&id).ok_or("Missing 3D object")?.protected {
            return Err("3D object or ancestor is protected".into());
        }
        next = owners.get(&id).ok_or("3D object has no owner")?.parent;
    }
    Ok(())
}
fn children(scene: &mut Scene, parent: Option<ObjectId>) -> Result<&mut Vec<ObjectId>, String> {
    match parent {
        None => Ok(&mut scene.roots),
        Some(id) => match &mut scene
            .objects
            .get_mut(&id)
            .ok_or("Missing 3D parent")?
            .content
        {
            Content::Group { children, .. } => Ok(children),
            _ => Err("3D parent is not a group".into()),
        },
    }
}
fn content_extensions(content: &Content) -> &Metadata {
    match content {
        Content::Group { extensions, .. }
        | Content::Instance { extensions, .. }
        | Content::Camera { extensions, .. }
        | Content::DirectionalLight { extensions, .. } => extensions,
    }
}
fn geometry_extensions(value: &Geometry) -> &Metadata {
    match value {
        Geometry::Box { extensions, .. }
        | Geometry::Sphere { extensions, .. }
        | Geometry::Mesh { extensions, .. } => extensions,
    }
}
fn parameters(object: &Object) -> [&Parameter; 3] {
    object.named_parameters().map(|(_, parameter)| parameter)
}
fn parameter_guard(before: &Parameter, after: &Parameter) -> Result<(), String> {
    if before.id != after.id
        || before.protected != after.protected
        || before.extensions != after.extensions
        || before.protected && before != after
    {
        return Err("Cannot change parameter identity, protection or extensions".into());
    }
    if let Binding::Track { track: old } = before.binding()? {
        match after.binding()? {
            Binding::Track { track: new } => {
                if old.id != new.id || old.extensions != new.extensions {
                    return Err("Cannot change track identity or extensions".into());
                }
                for key in &old.keys {
                    if let Some(new) = new.keys.iter().find(|value| value.id == key.id) {
                        if key.extensions != new.extensions {
                            return Err("Cannot discard key extensions".into());
                        }
                    }
                }
            }
            _ if !old.extensions.is_empty() => {
                return Err("Removing this track would discard extension data".into());
            }
            _ => {}
        }
    }
    Ok(())
}
fn object_guard(before: &Object, after: &Object) -> Result<(), String> {
    if before.protected != after.protected
        || before.extensions != after.extensions
        || before.transform.extensions != after.transform.extensions
        || content_extensions(&before.content) != content_extensions(&after.content)
    {
        return Err("Cannot change object protection or discard extensions".into());
    }
    // Frozen compensation may be prepended by parenting, never rewritten or
    // dropped by a whole-object edit. This also preserves unknown factor data.
    if !after
        .transform
        .parent_space
        .ends_with(&before.transform.parent_space)
    {
        return Err("Cannot rewrite frozen parent motion".into());
    }
    for (old, new) in parameters(before).into_iter().zip(parameters(after)) {
        parameter_guard(old, new)?;
    }
    Ok(())
}
pub(crate) fn resource_editable(scene: &Scene, id: ObjectId, geometry: bool) -> Result<(), String> {
    let owners = placements(scene);
    for (&object, value) in &scene.objects {
        if let Content::Instance {
            geometry: mesh,
            material,
            ..
        } = &value.content
        {
            if (if geometry { *mesh } else { *material }) == id {
                editable_with(scene, &owners, object)?;
            }
        }
    }
    Ok(())
}
fn remove(scene: &mut Scene, id: ObjectId, cancel: &Cancel) -> Result<(), String> {
    let owners = placements(scene);
    let mut pending = vec![id];
    let mut removed = BTreeSet::new();
    while let Some(next) = pending.pop() {
        cancel.check().map_err(|e| e.to_string())?;
        if !removed.insert(next) {
            return Err("3D hierarchy cycle or duplicate ownership".into());
        }
        editable_with(scene, &owners, next)?;
        let object = scene.objects.get(&next).ok_or("Missing 3D object")?;
        if parameters(object).iter().any(|value| value.protected) {
            return Err("Cannot remove protected parameters".into());
        }
        if let Content::Group { children, .. } = &object.content {
            pending.extend(children);
        }
    }
    let owner = owners.get(&id).ok_or("3D object has no owner")?;
    children(scene, owner.parent)?.remove(owner.index);
    for id in removed {
        scene.objects.remove(&id);
    }
    Ok(())
}
pub fn apply(before: &Scene, edits: &[Edit], cancel: &Cancel) -> Result<Scene, String> {
    cancel.check().map_err(|e| e.to_string())?;
    before.validate()?;
    let mut after = before.clone();
    for edit in edits {
        cancel.check().map_err(|e| e.to_string())?;
        match edit {
            Edit::Insert {
                id,
                object,
                placement,
            } => {
                if after.objects.contains_key(id) {
                    return Err("3D object already exists".into());
                }
                if let Some(parent) = placement.parent {
                    editable(&after, parent)?;
                }
                let children = children(&mut after, placement.parent)?;
                if placement.index > children.len() {
                    return Err("Invalid 3D insertion position".into());
                }
                children.insert(placement.index, *id);
                after.objects.insert(*id, object.as_ref().clone());
            }
            Edit::InsertTree {
                root,
                objects,
                placement,
            } => {
                if !objects.contains_key(root)
                    || objects.keys().any(|id| after.objects.contains_key(id))
                {
                    return Err("3D tree root is missing or an object already exists".into());
                }
                // New trees may reference shared resources, never existing children.
                for object in objects.values() {
                    if let Content::Group { children, .. } = &object.content
                        && children.iter().any(|id| !objects.contains_key(id))
                    {
                        return Err("Inserted 3D tree contains an external child".into());
                    }
                }
                if let Some(parent) = placement.parent {
                    editable(&after, parent)?;
                }
                let children = children(&mut after, placement.parent)?;
                if placement.index > children.len() {
                    return Err("Invalid 3D insertion position".into());
                }
                children.insert(placement.index, *root);
                after.objects.extend(objects.clone());
            }
            Edit::Replace {
                id,
                expected,
                value,
            } => {
                editable(&after, *id)?;
                if after.objects.get(id) != Some(expected.as_ref()) {
                    return Err("3D object changed; inspect it again".into());
                }
                object_guard(expected, value)?;
                after.objects.insert(*id, value.as_ref().clone());
            }
            Edit::Parameter {
                object,
                expected,
                value,
            } => {
                editable(&after, *object)?;
                parameter_guard(expected, value)?;
                let owner = after.objects.get_mut(object).ok_or("Missing 3D object")?;
                let parameter = [
                    &mut owner.transform.position,
                    &mut owner.transform.rotation,
                    &mut owner.transform.scale,
                ]
                .into_iter()
                .find(|parameter| parameter.id == expected.id)
                .ok_or("Missing 3D parameter")?;
                if parameter != expected {
                    return Err("3D parameter changed; inspect it again".into());
                }
                *parameter = value.clone();
            }
            Edit::Move {
                id,
                expected,
                value,
            } => {
                editable(&after, *id)?;
                if placement(&after, *id)? != *expected {
                    return Err("3D placement changed; inspect it again".into());
                }
                if let Some(parent) = expected.parent {
                    editable(&after, parent)?;
                }
                if let Some(parent) = value.parent {
                    editable(&after, parent)?;
                }
                children(&mut after, expected.parent)?.remove(expected.index);
                let destination = children(&mut after, value.parent)?;
                if value.index > destination.len() {
                    return Err("Invalid 3D move position".into());
                }
                destination.insert(value.index, *id);
            }
            Edit::MoveWorld {
                id,
                expected,
                parent,
            } => {
                if placement(&after, *id)? != *expected {
                    return Err("3D placement changed; inspect it again".into());
                }
                let generated = crate::tools::move_to_world(&after, *id, *parent)?;
                after = apply(&after, &generated, cancel)?;
            }
            Edit::Delete { id, expected } => {
                if after.objects.get(id) != Some(expected.as_ref()) {
                    return Err("3D object changed; inspect it again".into());
                }
                remove(&mut after, *id, cancel)?;
            }
            Edit::Geometry {
                id,
                expected,
                value,
            } => {
                if after.geometry.get(id) != expected.as_ref() {
                    return Err("3D geometry changed; inspect it again".into());
                }
                resource_editable(&after, *id, true)?;
                if let (Some(old), Some(new)) = (expected, value) {
                    if geometry_extensions(old) != geometry_extensions(new) {
                        return Err("Cannot discard geometry extensions".into());
                    }
                }
                if let Some(value) = value {
                    after.geometry.insert(*id, value.clone());
                } else {
                    after.geometry.remove(id);
                }
            }
            Edit::Material {
                id,
                expected,
                value,
            } => {
                if after.materials.get(id) != expected.as_ref() {
                    return Err("3D material changed; inspect it again".into());
                }
                resource_editable(&after, *id, false)?;
                if let (Some(old), Some(new)) = (expected, value) {
                    if old.extensions != new.extensions {
                        return Err("Cannot discard material extensions".into());
                    }
                }
                if let Some(value) = value {
                    after.materials.insert(*id, value.clone());
                } else {
                    after.materials.remove(id);
                }
            }
            Edit::ActiveCamera { expected, value } => {
                if after.active_camera != *expected {
                    return Err("3D active camera changed; inspect it again".into());
                }
                after.active_camera = *value;
            }
        }
    }
    after.validate()?;
    check_change(before, &after)?;
    cancel.check().map_err(|e| e.to_string())?;
    Ok(after)
}
pub fn stage(
    snapshot: &Snapshot,
    document: DocumentId,
    edits: &[Edit],
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    let original = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or("Missing 3D document")?;
    let before = Scene::from_document(original)?;
    let after = apply(&before, edits, cancel)?;
    let mut value = after.document(document)?;
    value.revision = original.revision;
    value.extensions = original.extensions.clone();
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(value)],
    })
}

/// Final baseline guards for validated typed edits and document proposals.
pub fn check_change(before: &Scene, after: &Scene) -> Result<(), String> {
    if before.extensions != after.extensions {
        return Err("Cannot discard scene extensions".into());
    }
    // Whole-object/group edits cannot bypass protection of descendants or
    // shared resources by first disconnecting a protected instance.
    let old_owners = placements(before);
    let new_owners = placements(after);
    for (&id, object) in &before.objects {
        if editable_with(before, &old_owners, id).is_err() {
            if after.objects.get(&id) != Some(object)
                || old_owners[&id].parent
                    != new_owners
                        .get(&id)
                        .ok_or("Protected 3D object was removed")?
                        .parent
            {
                return Err("Cannot alter a protected 3D object or its placement".into());
            }
            if let Content::Instance {
                geometry, material, ..
            } = &object.content
            {
                if before.geometry.get(geometry) != after.geometry.get(geometry)
                    || before.materials.get(material) != after.materials.get(material)
                {
                    return Err("Cannot alter a protected instance's resources".into());
                }
            }
        }
        if let Some(new) = after.objects.get(&id) {
            object_guard(object, new)?;
        } else if parameters(object).iter().any(|value| value.protected) {
            return Err("Cannot remove protected parameters".into());
        }
    }
    for (&id, old) in &before.geometry {
        if let Some(new) = after.geometry.get(&id) {
            if geometry_extensions(old) != geometry_extensions(new) {
                return Err("Cannot discard geometry extensions".into());
            }
        }
    }
    for (&id, old) in &before.materials {
        if let Some(new) = after.materials.get(&id) {
            if old.extensions != new.extensions {
                return Err("Cannot discard material extensions".into());
            }
        }
    }
    Ok(())
}
