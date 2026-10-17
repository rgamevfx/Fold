//! Ordered document ownership shared by artist lists, graphs and authoring tools.
use crate::{
    editing::{self, Edit},
    model::{Canvas, Content, Object},
};
use fold_foundation::ObjectId;
use std::collections::BTreeSet;

pub fn children(canvas: &Canvas, parent: Option<ObjectId>) -> Result<&[ObjectId], String> {
    match parent {
        None => Ok(&canvas.roots),
        Some(id) => match &canvas
            .objects
            .get(&id)
            .ok_or("Missing parent group")?
            .content
        {
            Content::Group { children, .. } => Ok(children),
            _ => Err("Parent must be a group".into()),
        },
    }
}

pub fn placement(canvas: &Canvas, id: ObjectId) -> Result<(Option<ObjectId>, usize), String> {
    if let Some(index) = canvas.roots.iter().position(|child| *child == id) {
        return Ok((None, index));
    }
    for (&parent, object) in &canvas.objects {
        if let Content::Group { children, .. } = &object.content
            && let Some(index) = children.iter().position(|child| *child == id)
        {
            return Ok((Some(parent), index));
        }
    }
    Err("Missing Motion object".into())
}

/// Group sibling objects in their drawing order, preserving their transforms,
/// animation, IDs and extensions. The identity group occupies the frontmost
/// selected object's position; grouping may change overlap with unselected siblings.
pub fn group(canvas: &Canvas, selection: &[ObjectId]) -> Result<(ObjectId, Vec<Edit>), String> {
    canvas.validate()?;
    let selected: BTreeSet<_> = selection.iter().copied().collect();
    if selected.len() < 2 {
        return Err("Select at least two objects to group".into());
    }
    let first = *selected.first().expect("nonempty selection");
    let (parent, _) = placement(canvas, first)?;
    for &id in &selected {
        editing::editable(canvas, id)?;
        if placement(canvas, id)?.0 != parent {
            return Err("Select objects in the same group".into());
        }
    }
    let siblings = children(canvas, parent)?;
    let ordered: Vec<_> = siblings
        .iter()
        .copied()
        .filter(|id| selected.contains(id))
        .collect();
    let index = siblings
        .iter()
        .rposition(|id| selected.contains(id))
        .expect("selected siblings");
    let id = ObjectId::new();
    let mut group = Object::new(
        "Group",
        Content::Group {
            children: vec![],
            extensions: Default::default(),
        },
    );
    if let Some(bounds) =
        crate::design::selection_bounds(canvas, selection, fold_foundation::Time::ZERO)?
    {
        let transforms = crate::tools::transforms(canvas, fold_foundation::Time::ZERO)?;
        let space = parent.map_or(crate::tools::Transform::IDENTITY, |id| transforms[&id]);
        let center = space.inverse()?.point(bounds.center());
        group.transform.anchor_x.value = center[0];
        group.transform.anchor_y.value = center[1];
    }
    let mut edits = vec![Edit::Insert {
        id,
        object: Box::new(group),
        parent,
        index: index + 1,
    }];
    for (index, child) in ordered.into_iter().enumerate() {
        edits.push(Edit::Move {
            id: child,
            parent: Some(id),
            index,
        });
    }
    editing::apply(canvas, &edits)?;
    Ok((id, edits))
}

#[derive(Clone, Copy)]
pub enum Order {
    Forward,
    Front,
    Backward,
    Back,
}

/// Arrange selected siblings as an ordered set. Unselected objects are never
/// edited, so their protection does not prevent moving editable objects past them.
pub fn arrange(canvas: &Canvas, selection: &[ObjectId], order: Order) -> Result<Vec<Edit>, String> {
    canvas.validate()?;
    let selected: BTreeSet<_> = selection.iter().copied().collect();
    let first = *selected.first().ok_or("Select an object to arrange")?;
    let (parent, _) = placement(canvas, first)?;
    for &id in &selected {
        editing::editable(canvas, id)?;
        if placement(canvas, id)?.0 != parent {
            return Err("Select objects in the same group".into());
        }
    }
    let siblings = children(canvas, parent)?;
    let mut target = siblings.to_vec();
    match order {
        Order::Front => target.sort_by_key(|id| selected.contains(id)),
        Order::Back => target.sort_by_key(|id| !selected.contains(id)),
        Order::Forward => {
            for index in (0..target.len().saturating_sub(1)).rev() {
                if selected.contains(&target[index]) && !selected.contains(&target[index + 1]) {
                    target.swap(index, index + 1);
                }
            }
        }
        Order::Backward => {
            for index in 1..target.len() {
                if selected.contains(&target[index]) && !selected.contains(&target[index - 1]) {
                    target.swap(index, index - 1);
                }
            }
        }
    }
    let mut positions: Vec<_> = target
        .iter()
        .enumerate()
        .filter(|(_, id)| selected.contains(id))
        .collect();
    if matches!(order, Order::Forward | Order::Front) {
        positions.reverse();
    }
    let mut current = siblings.to_vec();
    let mut edits = vec![];
    for (index, &id) in positions {
        let before = current
            .iter()
            .position(|child| *child == id)
            .expect("selected sibling");
        if before != index {
            current.remove(before);
            current.insert(index, id);
            edits.push(Edit::Move { id, parent, index });
        }
    }
    editing::apply(canvas, &edits)?;
    Ok(edits)
}
