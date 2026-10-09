//! Artist grouping and notes over native presentation metadata.
use crate::model::{Construction, Group};
use fold_foundation::ObjectId;
use fold_ui::sdk::{
    EditResponse,
    imgui::{TreeNodeFlags, Ui},
};
fn choices(value: &Construction, definition: ObjectId) -> Result<Vec<(ObjectId, String)>, String> {
    let asset = value.definition(definition)?;
    Ok(asset
        .inputs
        .iter()
        .map(|v| (v.id, v.name.clone()))
        .chain(asset.nodes.iter().map(|v| (v.id, v.name.clone())))
        .chain(asset.instances.iter().map(|v| (v.id, v.name.clone())))
        .chain(asset.outputs.iter().map(|v| (v.id, v.name.clone())))
        .collect())
}
pub(super) fn group(
    value: &Construction,
    definition: ObjectId,
    selected: &[ObjectId],
) -> Result<Construction, String> {
    let allowed = choices(value, definition)?;
    let members: Vec<_> = allowed
        .iter()
        .filter(|(id, _)| selected.contains(id))
        .map(|(id, _)| *id)
        .collect();
    if members.is_empty() || selected.iter().any(|id| !members.contains(id)) {
        return Err("Select graph items in this asset to group".into());
    }
    let mut after = value.clone();
    after.presentation.groups.push(Group {
        id: ObjectId::new(),
        name: "Group".into(),
        members,
        locked: false,
        extensions: Default::default(),
    });
    checked(value, after)
}
pub(super) fn ungroup(value: &Construction, id: ObjectId) -> Result<Construction, String> {
    let members = value
        .presentation
        .groups
        .iter()
        .find(|g| g.id == id)
        .ok_or("Missing group")?
        .members
        .clone();
    let mut after = value.clone();
    after.presentation.groups.retain(|g| g.id != id);
    for group in &mut after.presentation.groups {
        if let Some(index) = group.members.iter().position(|member| *member == id) {
            group.members.remove(index);
            let additions: Vec<_> = members
                .iter()
                .filter(|member| !group.members.contains(member))
                .copied()
                .collect();
            group.members.splice(index..index, additions);
        }
    }
    after.presentation.positions.remove(&id);
    after.presentation.annotations.remove(&id);
    checked(value, after)
}
fn checked(before: &Construction, after: Construction) -> Result<Construction, String> {
    after.validate_structure()?;
    crate::protection::validate(before, &after).map_err(|e| e.reason)?;
    Ok(after)
}
fn note(ui: &Ui, value: &mut Construction, id: ObjectId, response: &mut EditResponse) {
    let mut text = value
        .presentation
        .annotations
        .get(&id)
        .cloned()
        .unwrap_or_default();
    ui.text("Notes");
    let changed = ui
        .input_text_multiline(
            "##notes",
            &mut text,
            [ui.content_region_avail()[0].max(1.), 72.],
        )
        .build();
    response.item(ui, changed);
    if changed {
        if text.is_empty() {
            value.presentation.annotations.remove(&id);
        } else {
            value.presentation.annotations.insert(id, text);
        }
    }
}
/// Returns the group to dissolve. Text edits share the inspector gesture lifecycle.
pub(super) fn draw(
    ui: &Ui,
    value: &mut Construction,
    definition: ObjectId,
    selected: &[ObjectId],
    response: &mut EditResponse,
) -> Option<ObjectId> {
    let entries = choices(value, definition).unwrap_or_default();
    let locked = crate::protection::locked_ids(value).unwrap_or_default();
    if selected.len() == 1 && entries.iter().any(|(id, _)| *id == selected[0]) {
        let _id = ui.push_id("item-notes");
        let _disabled = ui.begin_disabled_with_cond(locked.contains(&selected[0]));
        note(ui, value, selected[0], response);
    }
    if !value.presentation.groups.is_empty()
        && ui.collapsing_header("Groups", TreeNodeFlags::DEFAULT_OPEN)
    {
        let groups: Vec<_> = value
            .presentation
            .groups
            .iter()
            .map(|g| (g.id, g.name.clone()))
            .collect();
        for (id, name) in &groups {
            let _id = ui.push_id(&id.to_string());
            let selected_member = value
                .presentation
                .groups
                .iter()
                .find(|g| g.id == *id)
                .is_some_and(|g| g.members.iter().any(|member| selected.contains(member)));
            if let Some(_node) = ui
                .tree_node_config("group")
                .label(name)
                .opened(selected_member, fold_ui::sdk::imgui::Condition::Once)
                .push()
            {
                let _disabled = ui.begin_disabled_with_cond(locked.contains(id));
                let group = value
                    .presentation
                    .groups
                    .iter_mut()
                    .find(|g| g.id == *id)
                    .expect("retained group");
                let changed = ui.input_text("Name", &mut group.name).build();
                response.item(ui, changed);
                if let Some(_menu) = ui.begin_menu("Members") {
                    for (member, label) in entries
                        .iter()
                        .chain(groups.iter())
                        .filter(|(member, _)| member != id)
                    {
                        let _id = ui.push_id(&member.to_string());
                        let mut included = group.members.contains(member);
                        let changed = ui.checkbox(label, &mut included);
                        response.item(ui, changed);
                        if changed {
                            if included {
                                group.members.push(*member);
                            } else {
                                group.members.retain(|old| old != member);
                            }
                        }
                    }
                }
                note(ui, value, *id, response);
                if ui.small_button("Ungroup") {
                    return Some(*id);
                }
            }
        }
    }
    None
}
