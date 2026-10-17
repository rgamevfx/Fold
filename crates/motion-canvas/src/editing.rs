//! Shared artist/agent edits. Stage against an immutable snapshot; commit once.
use crate::model::{Canvas, Captured, Content, Control, Object, Point, Scalar};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Lock {
        object: ObjectId,
        expected: bool,
        value: bool,
    },
    Settings {
        expected: Box<Canvas>,
        width: u32,
        height: u32,
        rate: [u32; 2],
        duration: Time,
    },
    Align {
        objects: Vec<ObjectId>,
        time: Time,
        mode: crate::design::Align,
        auto_key: bool,
    },
    /// One atomic conversion regardless of generated object count.
    Capture {
        id: ObjectId,
        expected: Box<Object>,
        value: Box<Captured>,
        time: Time,
    },
    /// Presentation has its own guard. Moving a graph node cannot edit artwork
    /// or release a protected object; locked content can still be arranged.
    GraphPosition {
        object: ObjectId,
        expected: Option<[f32; 2]>,
        value: Option<[f32; 2]>,
    },
    Insert {
        id: ObjectId,
        object: Box<Object>,
        parent: Option<ObjectId>,
        index: usize,
    },
    Replace {
        id: ObjectId,
        expected: Box<Object>,
        object: Box<Object>,
    },
    Parameter {
        object: ObjectId,
        expected: Scalar,
        value: Scalar,
    },
    /// Subdivide the selected native segment for its whole animation timeline.
    SplitSegment {
        object: ObjectId,
        expected: Box<Object>,
        contour: ObjectId,
        start: ObjectId,
        fraction: f64,
        time: Time,
    },
    RemovePoint {
        object: ObjectId,
        expected: Box<Object>,
        point: ObjectId,
    },
    /// Canvas-space gesture intent, shared with direct anchor/handle editing.
    MovePoint {
        object: ObjectId,
        expected: Box<Object>,
        point: ObjectId,
        part: crate::tools::Part,
        position: [f64; 2],
        time: Time,
        linked: bool,
        auto_key: bool,
    },
    Point {
        object: ObjectId,
        expected: Point,
        value: Point,
    },
    Control {
        object: ObjectId,
        id: ObjectId,
        expected: Control,
        value: Control,
    },
    Move {
        id: ObjectId,
        parent: Option<ObjectId>,
        index: usize,
    },
    Delete {
        id: ObjectId,
        expected: Box<Object>,
    },
}

pub fn apply(before: &Canvas, edits: &[Edit]) -> Result<Canvas, String> {
    before.validate()?;
    let mut after = before.clone();
    for edit in edits {
        match edit {
            Edit::Lock {
                object,
                expected,
                value,
            } => {
                let owner = after
                    .objects
                    .get_mut(object)
                    .ok_or("Missing Motion object")?;
                if owner.locked != *expected || owner.protected {
                    return Err("Layer lock changed or object is protected".into());
                }
                owner.locked = false;
                editable(&after, *object)?;
                after.objects.get_mut(object).unwrap().locked = *value;
            }
            Edit::Settings {
                expected,
                width,
                height,
                rate,
                duration,
            } => {
                if &after != expected.as_ref() {
                    return Err("Motion document changed; inspect it again".into());
                }
                after.width = *width;
                after.height = *height;
                after.rate = *rate;
                after.duration = *duration;
            }
            Edit::Align {
                objects,
                time,
                mode,
                auto_key,
            } => {
                let edits = crate::design::align(&after, objects, *time, *mode, *auto_key)?;
                after = apply(&after, &edits)?;
            }
            Edit::MovePoint {
                object,
                expected,
                point,
                part,
                position,
                time,
                linked,
                auto_key,
            } => {
                if after.objects.get(object) != Some(expected.as_ref()) {
                    return Err("Motion object changed; inspect it again".into());
                }
                let replacement = crate::tools::move_point(
                    &after, *object, *point, *part, *position, *time, *linked, *auto_key,
                )?;
                after = apply(&after, &[replacement])?;
            }
            Edit::SplitSegment {
                object,
                expected,
                contour,
                start,
                fraction,
                time,
            } => {
                if after.objects.get(object) != Some(expected.as_ref()) {
                    return Err("Motion object changed; inspect it again".into());
                }
                let (_, replacement) = crate::tools::split_segment(
                    &after, *object, *contour, *start, *fraction, *time,
                )?;
                after = apply(&after, &[replacement])?;
            }
            Edit::RemovePoint {
                object,
                expected,
                point,
            } => {
                if after.objects.get(object) != Some(expected.as_ref()) {
                    return Err("Motion object changed; inspect it again".into());
                }
                let replacement = crate::tools::remove_point(&after, *object, *point)?;
                after = apply(&after, &[replacement])?;
            }
            Edit::Capture {
                id,
                expected,
                value,
                time,
            } => {
                after =
                    crate::capture::apply(&after, *id, expected, value.as_ref().clone(), *time)?;
            }
            Edit::GraphPosition {
                object,
                expected,
                value,
            } => {
                let object = after
                    .objects
                    .get_mut(object)
                    .ok_or("Missing Motion object")?;
                if object.graph_position != *expected {
                    return Err("Graph position changed; inspect it again".into());
                }
                object.graph_position = *value;
            }
            Edit::Insert {
                id,
                object,
                parent,
                index,
            } => {
                if after.objects.contains_key(id) {
                    return Err("Motion object already exists".into());
                }
                if let Some(parent) = parent {
                    editable(&after, *parent)?;
                }
                let children = children(&mut after, *parent)?;
                if *index > children.len() {
                    return Err("Invalid object insertion position".into());
                }
                children.insert(*index, *id);
                after.objects.insert(*id, object.as_ref().clone());
            }
            Edit::Replace {
                id,
                expected,
                object,
            } => {
                editable(&after, *id)?;
                if after.objects.get(id) != Some(expected.as_ref()) {
                    return Err("Motion object changed; inspect it again".into());
                }
                check_protection(expected, object)?;
                after.objects.insert(*id, object.as_ref().clone());
            }
            Edit::Parameter {
                object,
                expected,
                value,
            } => {
                editable(&after, *object)?;
                if expected.id != value.id {
                    return Err("Parameter identity cannot change".into());
                }
                let owner = after
                    .objects
                    .get_mut(object)
                    .ok_or("Missing Motion object")?;
                if !owner
                    .named_controls()
                    .iter()
                    .any(|(_, scalar)| scalar.id == expected.id)
                {
                    return Err("Missing Motion control".into());
                }
                if let Content::Script { controls, .. } = &owner.content
                    && controls.values().any(|control| control.protected && matches!(&control.value, crate::model::ControlValue::Number(v) if v.id == expected.id)) {
                    return Err("Script control is protected".into());
                }
                let parameter = owner
                    .scalar_mut(expected.id)
                    .ok_or("Missing Motion parameter")?;
                if parameter != expected {
                    return Err("Motion parameter changed; inspect it again".into());
                }
                if parameter.protected
                    || parameter.extensions != value.extensions
                    || parameter.protected != value.protected
                {
                    return Err("Cannot edit protection or discard parameter extensions".into());
                }
                *parameter = value.clone();
            }
            Edit::Point {
                object,
                expected,
                value,
            } => {
                editable(&after, *object)?;
                point_animation_guard(expected, value)?;
                if expected.id != value.id || expected.extensions != value.extensions {
                    return Err("Path point identity and extensions must remain unchanged".into());
                }
                let Content::Path { contours, .. } = &mut after
                    .objects
                    .get_mut(object)
                    .ok_or("Missing Motion object")?
                    .content
                else {
                    return Err("Object is not an editable path".into());
                };
                let point = contours
                    .iter_mut()
                    .flat_map(|c| &mut c.points)
                    .find(|p| p.id == expected.id)
                    .ok_or("Missing path point")?;
                if point != expected {
                    return Err("Path point changed; inspect it again".into());
                }
                *point = value.clone();
            }
            Edit::Move { id, parent, index } => {
                editable(&after, *id)?;
                if let Some(parent) = parent {
                    editable(&after, *parent)?;
                }
                detach(&mut after, *id)?;
                let children = children(&mut after, *parent)?;
                if *index > children.len() {
                    return Err("Invalid object insertion position".into());
                }
                children.insert(*index, *id);
            }
            Edit::Control {
                object,
                id,
                expected,
                value,
            } => {
                editable(&after, *object)?;
                let before = after.objects.get(object).ok_or("Missing Motion object")?;
                let mut next = before.clone();
                let Content::Script { controls, .. } = &mut next.content else {
                    return Err("Object is not a script".into());
                };
                if controls.get(id) != Some(expected) {
                    return Err("Script control changed; inspect it again".into());
                }
                if std::mem::discriminant(&expected.value) != std::mem::discriminant(&value.value)
                    || matches!((&expected.value, &value.value),
                        (crate::model::ControlValue::Number(a), crate::model::ControlValue::Number(b)) if a.id != b.id)
                {
                    return Err("Control type and parameter identity cannot change".into());
                }
                controls.insert(*id, value.clone());
                check_protection(before, &next)?;
                after.objects.insert(*object, next);
            }
            Edit::Delete { id, expected } => {
                editable(&after, *id)?;
                if after.objects.get(id) != Some(expected.as_ref()) {
                    return Err("Motion object changed; inspect it again".into());
                }
                let mut remove = vec![];
                collect(&after, *id, &mut remove)?;
                for id in &remove {
                    editable(&after, *id)?;
                    let object = &after.objects[id];
                    if object.scalars().iter().any(|p| p.protected)
                        || matches!(&object.content, Content::Script { controls, .. } if controls.values().any(|c| c.protected))
                    {
                        return Err("Object contains protected controls".into());
                    }
                }
                detach(&mut after, *id)?;
                for id in remove {
                    after.objects.remove(&id);
                }
            }
        }
    }
    after.validate()?;
    check_change(before, &after)?;
    Ok(after)
}

pub fn stage(
    snapshot: &Snapshot,
    document: DocumentId,
    edits: &[Edit],
) -> Result<EditBatch, String> {
    let original = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or("Missing Motion document")?;
    let before = Canvas::from_document(original)?;
    let after = apply(&before, edits)?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(after.replace_document(original)?)],
    })
}

fn check_protection(before: &Object, after: &Object) -> Result<(), String> {
    if before.motion != after.motion
        && [
            &before.transform.x,
            &before.transform.y,
            &before.transform.rotation,
            &before.transform.scale_x,
            &before.transform.scale_y,
            &before.transform.opacity,
        ]
        .iter()
        .any(|v| v.protected)
    {
        return Err("Motion cannot alter protected transform controls".into());
    }
    for (before, after) in [
        (&before.paint.fill, &after.paint.fill),
        (&before.paint.stroke, &after.paint.stroke),
    ] {
        if let (Some(before), Some(after)) = (before, after) {
            if before.extensions() != after.extensions() {
                return Err("Cannot discard paint source extensions".into());
            }
            if let (Some(before), Some(after)) = (before.stops(), after.stops()) {
                for stop in before {
                    if let Some(next) = after.iter().find(|next| next.id == stop.id)
                        && next.extensions != stop.extensions
                    {
                        return Err("Cannot discard gradient stop extensions".into());
                    }
                }
            }
        }
    }
    if before.protected != after.protected
        || !fold_project::components::preserves_extensions(&before.extensions, &after.extensions)
        || before.transform.extensions != after.transform.extensions
        || before.paint.extensions != after.paint.extensions
        || before.content.extensions() != after.content.extensions()
    {
        return Err("Cannot edit object protection or discard extensions".into());
    }
    if let (
        Content::Path {
            contours: before, ..
        },
        Content::Path {
            contours: after, ..
        },
    ) = (&before.content, &after.content)
    {
        for contour in before {
            if let Some(next) = after.iter().find(|c| c.id == contour.id) {
                if next.extensions != contour.extensions {
                    return Err("Cannot discard contour extensions".into());
                }
                for point in &contour.points {
                    if let Some(next) = next.points.iter().find(|p| p.id == point.id) {
                        if next.extensions != point.extensions {
                            return Err("Cannot discard path point extensions".into());
                        }
                        point_animation_guard(point, next)?;
                    }
                }
            }
        }
    }
    for parameter in before.scalars() {
        let next = after.scalars().into_iter().find(|p| p.id == parameter.id);
        if parameter.protected && next != Some(parameter) {
            return Err("Motion parameter is protected".into());
        }
        if let Some(next) = next
            && (next.protected != parameter.protected || next.extensions != parameter.extensions)
        {
            return Err("Cannot edit parameter protection or discard extensions".into());
        }
    }
    if let Content::Script { controls, .. } = &before.content {
        for (id, control) in controls {
            let next = match &after.content {
                Content::Script { controls, .. } => controls.get(id),
                _ => None,
            };
            if control.protected && next != Some(control) {
                return Err("Script control is protected".into());
            }
            if let Some(next) = next
                && (control.protected != next.protected || control.extensions != next.extensions)
            {
                return Err("Cannot edit control protection or discard extensions".into());
            }
        }
    }
    Ok(())
}
fn point_animation_guard(before: &Point, after: &Point) -> Result<(), String> {
    for track in before.tracks() {
        let next = if before
            .position_track
            .as_ref()
            .is_some_and(|t| t.id == track.id)
        {
            after.position_track.as_ref()
        } else {
            after.tracks().find(|t| t.id == track.id).or_else(|| {
                if before
                    .incoming_track
                    .as_ref()
                    .is_some_and(|t| t.id == track.id)
                {
                    after.incoming_track.as_ref()
                } else {
                    after.outgoing_track.as_ref()
                }
            })
        };
        if let Some(next) = next {
            if next.id != track.id || next.extensions != track.extensions {
                return Err("Path animation identity and extensions must remain unchanged".into());
            }
        } else if !track.extensions.is_empty() {
            return Err("Cannot discard path animation extensions".into());
        }
        for key in &track.keys {
            let next = next.and_then(|track| track.keys.iter().find(|k| k.id == key.id));
            if next.is_some_and(|next| next.extensions != key.extensions)
                || next.is_none() && !key.extensions.is_empty()
            {
                return Err("Cannot discard path animation key extensions".into());
            }
        }
    }
    Ok(())
}
pub fn editable(canvas: &Canvas, id: ObjectId) -> Result<(), String> {
    fn visit(
        canvas: &Canvas,
        id: ObjectId,
        active: &mut std::collections::BTreeSet<ObjectId>,
    ) -> Result<(), String> {
        if !active.insert(id) {
            return Err("Cyclic Motion hierarchy".into());
        }
        let object = canvas.objects.get(&id).ok_or("Missing Motion object")?;
        if object.protected || object.locked {
            return Err("Motion object is protected".into());
        }
        // Parent groups protect their whole subtree.
        for (&parent, object) in &canvas.objects {
            if let Content::Group { children, .. } = &object.content
                && children.contains(&id)
            {
                visit(canvas, parent, active)?;
            }
        }
        active.remove(&id);
        Ok(())
    }
    visit(canvas, id, &mut Default::default())
}
fn children(canvas: &mut Canvas, parent: Option<ObjectId>) -> Result<&mut Vec<ObjectId>, String> {
    match parent {
        None => Ok(&mut canvas.roots),
        Some(id) => match &mut canvas
            .objects
            .get_mut(&id)
            .ok_or("Missing parent group")?
            .content
        {
            Content::Group { children, .. } => Ok(children),
            _ => Err("Parent must be a group".into()),
        },
    }
}
fn detach(canvas: &mut Canvas, id: ObjectId) -> Result<(), String> {
    if let Some(index) = canvas.roots.iter().position(|v| *v == id) {
        canvas.roots.remove(index);
        return Ok(());
    }
    for object in canvas.objects.values_mut() {
        if let Content::Group { children, .. } = &mut object.content
            && let Some(index) = children.iter().position(|v| *v == id)
        {
            children.remove(index);
            return Ok(());
        }
    }
    Err("Object is not in the hierarchy".into())
}
fn collect(canvas: &Canvas, id: ObjectId, result: &mut Vec<ObjectId>) -> Result<(), String> {
    if result.contains(&id) {
        return Err("Cyclic Motion hierarchy".into());
    }
    result.push(id);
    if let Content::Group { children, .. } = &canvas
        .objects
        .get(&id)
        .ok_or("Missing Motion object")?
        .content
    {
        for child in children {
            collect(canvas, *child, result)?;
        }
    }
    Ok(())
}

/// Final baseline guards shared by typed edits and whole-document proposals.
/// Both documents must already pass Canvas::validate.
pub fn check_change(before: &Canvas, after: &Canvas) -> Result<(), String> {
    if before.extensions != after.extensions {
        return Err("Cannot discard canvas extensions".into());
    }
    let owners = |canvas: &Canvas| {
        let mut map: std::collections::BTreeMap<_, _> =
            canvas.roots.iter().map(|id| (*id, None)).collect();
        for (&id, object) in &canvas.objects {
            if let Content::Group { children, .. } = &object.content {
                map.extend(children.iter().map(|child| (*child, Some(id))));
            }
        }
        map
    };
    let old_owners = owners(before);
    let new_owners = owners(after);
    for (&id, object) in &before.objects {
        if let Some(next) = after.objects.get(&id) {
            let parent_changed = old_owners.get(&id) != new_owners.get(&id);
            if next == object && !parent_changed {
                continue;
            }
            check_protection(object, next)?;
            let mut artwork = next.clone();
            artwork.graph_position = object.graph_position;
            let lock_changed = artwork.locked != object.locked;
            artwork.locked = object.locked;
            if artwork != *object || parent_changed {
                editable(before, id)?;
            } else if lock_changed {
                let mut unlocked = before.clone();
                unlocked.objects.get_mut(&id).unwrap().locked = false;
                editable(&unlocked, id)?;
            }
        } else {
            editable(before, id)?;
            if object.scalars().iter().any(|p| p.protected)
                || matches!(&object.content, Content::Script { controls, .. } if controls.values().any(|c| c.protected))
            {
                return Err("Cannot remove protected controls".into());
            }
        }
    }
    Ok(())
}
