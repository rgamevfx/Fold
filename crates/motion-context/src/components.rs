//! Native component extraction and insertion. No library storage or UI policy.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion_canvas::{
    design,
    editing::{self, Edit},
    model::*,
    tools,
};
use fold_project::Document;
use std::collections::{BTreeMap, BTreeSet};

pub fn capture(document: &Document, selection: &[ObjectId]) -> Result<Document, String> {
    let mut canvas = Canvas::from_document(document)?;
    if !selection.is_empty() {
        let mut keep = BTreeSet::new();
        fn descendants(
            canvas: &Canvas,
            id: ObjectId,
            keep: &mut BTreeSet<ObjectId>,
        ) -> Result<(), String> {
            if !keep.insert(id) {
                return Ok(());
            }
            if let Content::Group { children, .. } = &canvas
                .objects
                .get(&id)
                .ok_or("Missing selected artwork")?
                .content
            {
                for child in children {
                    descendants(canvas, *child, keep)?;
                }
            }
            Ok(())
        }
        for id in selection {
            descendants(&canvas, *id, &mut keep)?;
        }
        // Retain parent transforms and animation without rasterizing or baking them.
        for id in selection {
            let mut child = *id;
            while let Some(parent) = fold_motion_canvas::hierarchy::placement(&canvas, child)?.0 {
                keep.insert(parent);
                child = parent;
            }
        }
        canvas.roots.retain(|id| keep.contains(id));
        canvas.objects.retain(|id, _| keep.contains(id));
        for object in canvas.objects.values_mut() {
            if let Content::Group { children, .. } = &mut object.content {
                children.retain(|id| keep.contains(id));
            }
        }
    }
    if canvas.roots.is_empty() {
        return Err("Select artwork to save".into());
    }
    canvas.document(DocumentId::new())
}
fn fresh_curve(curve: &mut fold_animation::Curve) {
    for key in &mut curve.keys {
        key.id = ObjectId::new();
    }
}
/// Clone provider-owned identities only. Unknown extension bytes remain untouched.
pub fn duplicate(canvas: &Canvas) -> Result<Canvas, String> {
    let mut out = canvas.clone();
    let ids: BTreeMap<_, _> = canvas
        .objects
        .keys()
        .map(|id| (*id, ObjectId::new()))
        .collect();
    out.roots = out.roots.iter().map(|id| ids[id]).collect();
    out.objects = std::mem::take(&mut out.objects)
        .into_iter()
        .map(|(id, mut object)| {
            let scalars: Vec<_> = object.named_scalars().iter().map(|(_, s)| s.id).collect();
            for id in scalars {
                let s = object.scalar_mut(id).unwrap();
                s.id = ObjectId::new();
                if let Some(c) = &mut s.animation {
                    fresh_curve(c);
                }
            }
            if let Some(m) = &mut object.motion {
                for c in &mut m.curves {
                    fresh_curve(c);
                }
            }
            for paint in [&mut object.paint.fill, &mut object.paint.stroke]
                .into_iter()
                .flatten()
            {
                if let PaintSource::Linear { stops, .. } | PaintSource::Radial { stops, .. } = paint
                {
                    for s in stops {
                        s.id = ObjectId::new();
                    }
                }
            }
            match &mut object.content {
                Content::Group { children, .. } => {
                    *children = children.iter().map(|id| ids[id]).collect()
                }
                Content::Script { controls, .. } => {
                    *controls = std::mem::take(controls)
                        .into_values()
                        .map(|c| (ObjectId::new(), c))
                        .collect()
                }
                Content::Path { contours, .. } => {
                    for contour in contours {
                        contour.id = ObjectId::new();
                        for point in &mut contour.points {
                            point.id = ObjectId::new();
                            for t in [
                                &mut point.position_track,
                                &mut point.incoming_track,
                                &mut point.outgoing_track,
                            ]
                            .into_iter()
                            .flatten()
                            {
                                t.id = ObjectId::new();
                                for k in &mut t.keys {
                                    k.id = ObjectId::new();
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            (ids[&id], object)
        })
        .collect();
    out.validate()?;
    Ok(out)
}
pub fn insert(
    target: &Document,
    source: &Document,
    name: &str,
    position: Option<[f64; 2]>,
    update: Option<ObjectId>,
    provenance: serde_json::Value,
) -> Result<Document, String> {
    let canvas = Canvas::from_document(target)?;
    let source = duplicate(&Canvas::from_document(source)?)?;
    let mut edits = Vec::new();
    let (root, transform, index) = if let Some(id) = update {
        editing::editable(&canvas, id)?;
        let expected = &canvas.objects[&id];
        if !expected
            .extensions
            .contains_key(fold_platform::gallery::PROVENANCE)
        {
            return Err("Select a placed Gallery component to update".into());
        }
        let (parent, index) = fold_motion_canvas::hierarchy::placement(&canvas, id)?;
        if parent.is_some() {
            return Err("Update the component from its document root".into());
        }
        edits.push(Edit::Delete {
            id,
            expected: Box::new(expected.clone()),
        });
        (id, expected.transform.clone(), index)
    } else {
        let center = design::selection_bounds(&source, &source.roots, Time::ZERO)?
            .map(|b| b.center())
            .unwrap_or([source.width as f64 / 2., source.height as f64 / 2.]);
        let at = position.unwrap_or([canvas.width as f64 / 2., canvas.height as f64 / 2.]);
        if at.iter().any(|v| !v.is_finite()) {
            return Err("Invalid component position".into());
        }
        let mut transform = Transform::default();
        transform.x.value = at[0] - center[0];
        transform.y.value = at[1] - center[1];
        (ObjectId::new(), transform, canvas.roots.len())
    };
    // A script already is an editable component container. Keep its public
    // controls on the selected root instead of hiding them in an extra group.
    if source.roots.len() == 1
        && matches!(
            source.objects[&source.roots[0]].content,
            Content::Script { .. }
        )
    {
        let mut object = source.objects[&source.roots[0]].clone();
        object.name = name.into();
        if update.is_some() {
            object.transform = transform;
            object
                .extensions
                .extend(canvas.objects[&root].extensions.clone());
        } else {
            object.transform.x.value += transform.x.value;
            object.transform.y.value += transform.y.value;
        }
        object
            .extensions
            .insert(fold_platform::gallery::PROVENANCE.into(), provenance);
        edits.push(Edit::Insert {
            id: root,
            object: Box::new(object),
            parent: None,
            index,
        });
        return editing::apply(&canvas, &edits)?.replace_document(target);
    }
    let mut group = if let Some(id) = update {
        let mut object = canvas.objects[&id].clone();
        let Content::Group { children, .. } = &mut object.content else {
            return Err("Component root is not a group".into());
        };
        children.clear();
        object
    } else {
        tools::new_object("group")?
    };
    group.name = name.into();
    group.transform = transform;
    group
        .extensions
        .insert(fold_platform::gallery::PROVENANCE.into(), provenance);
    edits.push(Edit::Insert {
        id: root,
        object: Box::new(group),
        parent: None,
        index,
    });
    fn append(canvas: &Canvas, ids: &[ObjectId], parent: ObjectId, edits: &mut Vec<Edit>) {
        for (index, id) in ids.iter().enumerate() {
            let mut object = canvas.objects[id].clone();
            let children = if let Content::Group { children, .. } = &mut object.content {
                std::mem::take(children)
            } else {
                vec![]
            };
            edits.push(Edit::Insert {
                id: *id,
                object: Box::new(object),
                parent: Some(parent),
                index,
            });
            append(canvas, &children, *id, edits);
        }
    }
    append(&source, &source.roots, root, &mut edits);
    editing::apply(&canvas, &edits)?.replace_document(target)
}
