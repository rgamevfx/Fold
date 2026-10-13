//! Convert a script's evaluated local output through existing guarded edits.
use crate::{
    editing,
    model::{Canvas, Captured, Content, Object},
};
use fold_foundation::{ObjectId, Time};

/// Keep the authored object's identity, placement, transform and paint. Its
/// generated local output becomes static children. Preserve the source and its
/// unknown control data as provenance on a new child, while undo restores the
/// actual script. Provenance is inert; it is never an alternate execution path.
pub(crate) fn apply(
    before: &Canvas,
    id: ObjectId,
    expected: &Object,
    mut captured: Captured,
    time: Time,
) -> Result<Canvas, String> {
    before.validate()?;
    if before.objects.get(&id) != Some(expected) {
        return Err("Script changed during capture; inspect it again".into());
    }
    if !matches!(expected.content, Content::Script { .. }) {
        return Err("Only script objects can be converted to editable artwork".into());
    }
    if time < Time::ZERO || time >= before.duration {
        return Err("Capture time is outside the document".into());
    }
    if captured.objects.len() > 100_000 {
        return Err("Editable capture exceeds 100,000 objects".into());
    }
    let points: usize = captured
        .objects
        .values()
        .filter_map(|object| match &object.content {
            Content::Path { contours, .. } => Some(
                contours
                    .iter()
                    .map(|contour| contour.points.len())
                    .sum::<usize>(),
            ),
            _ => None,
        })
        .sum();
    if points > 1_000_000 {
        return Err("Editable capture exceeds 1,000,000 path points".into());
    }
    let mut pending: Vec<_> = captured.roots.iter().map(|id| (*id, 0)).collect();
    let mut seen = std::collections::BTreeSet::new();
    while let Some((id, depth)) = pending.pop() {
        if depth >= 128 || !seen.insert(id) {
            return Err("Invalid or excessively deep capture hierarchy".into());
        }
        let object = captured
            .objects
            .get(&id)
            .ok_or("Capture object is missing")?;
        if let Content::Group { children, .. } = &object.content {
            pending.extend(children.iter().map(|id| (*id, depth + 1)));
        }
    }
    let isolated = Canvas {
        roots: captured.roots.clone(),
        objects: captured.objects.clone(),
        width: before.width,
        height: before.height,
        rate: before.rate,
        duration: before.duration,
        extensions: before.extensions.clone(),
    };
    isolated.validate()?;
    if captured
        .objects
        .keys()
        .any(|id| before.objects.contains_key(id))
    {
        return Err("Captured identities overlap existing artwork".into());
    }
    if captured
        .objects
        .values()
        .any(|object| matches!(object.content, Content::Script { .. }))
    {
        return Err("Editable capture cannot contain scripts".into());
    }
    let root = *captured.roots.first().ok_or("Capture has no root")?;
    let provenance = &mut captured
        .objects
        .get_mut(&root)
        .ok_or("Capture root is missing")?
        .extensions;
    if provenance.contains_key("fold.motion.capture") {
        return Err("Capture provenance already exists".into());
    }
    provenance.insert(
        "fold.motion.capture".into(),
        serde_json::json!({"time":time,"source":expected.content}),
    );
    let mut object = expected.clone();
    object.content = Content::Group {
        children: captured.roots,
        extensions: expected.content.extensions().clone(),
    };
    let mut after = before.clone();
    after.objects.insert(id, object);
    after.objects.extend(captured.objects);
    // Includes baseline protection, global identity and hierarchy validation.
    after.validate()?;
    editing::check_change(before, &after)?;
    Ok(after)
}
