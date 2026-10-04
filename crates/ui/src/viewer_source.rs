//! One row for group membership and independent viewer source selection.
use crate::{group_selector, sdk::imgui::Ui};
use fold_foundation::DocumentId;
use fold_platform::workspace::{self, OutputDescriptor, PanelInstanceId, ViewerBinding, Workspace};
use std::collections::BTreeMap;

pub(crate) fn draw(
    ui: &Ui,
    workspace: &mut Workspace,
    id: PanelInstanceId,
    labels: &BTreeMap<DocumentId, String>,
    reserved_width: f32,
    outputs: impl Fn(DocumentId) -> Vec<OutputDescriptor>,
) -> bool {
    let mut source_changed = false;
    let viewer = &workspace.viewers[&id];
    let group = if viewer.binding == ViewerBinding::Linked {
        viewer.group
    } else {
        workspace::LinkGroup::Unlinked
    };
    if let Some(group) = group_selector::draw(ui, group) {
        workspace.set_viewer_group(id, group);
        source_changed = true;
    }
    ui.same_line();
    let current = workspace.resolve(id);

    let label = current
        .as_ref()
        .and_then(|o| labels.get(&o.document))
        .map(String::as_str)
        .unwrap_or(if current.is_some() {
            "Missing network"
        } else {
            "Choose source"
        });
    ui.set_next_item_width((ui.content_region_avail()[0] - reserved_width).max(30.));
    if let Some(_combo) = ui.begin_combo("##viewer-source", label) {
        let group = workspace.viewers[&id].group;
        if group != workspace::LinkGroup::Unlinked {
            ui.text_disabled(format!("Group {}", group.label()));
            let sources: Vec<_> = workspace
                .editors
                .iter()
                .filter(|(_, e)| e.group == group && e.document().is_some())
                .map(|(&id, e)| (id, e.document()))
                .collect();
            if sources.is_empty() {
                ui.text_disabled("No networks open in this group");
            }
            for (editor, document) in sources {
                let _id = ui.push_id(&format!("editor-{}", editor.0));
                let _disabled = ui
                    .begin_disabled_with_cond(document.is_none_or(|id| !labels.contains_key(&id)));
                let name = document
                    .and_then(|id| labels.get(&id))
                    .map(String::as_str)
                    .unwrap_or("Missing network");
                if ui
                    .selectable_config(name)
                    .selected(
                        workspace.viewers[&id].binding == ViewerBinding::Linked
                            && workspace.editor_for_viewer(id) == Some(editor),
                    )
                    .build()
                {
                    workspace.follow_source(id, editor);
                    source_changed = true;
                }
            }
            ui.separator();
        }
        ui.text_disabled("All networks · Unlinked");
        for (&document, name) in labels {
            let _id = ui.push_id(&format!("{document:?}"));
            for output in outputs(document) {
                let _output_id = ui.push_id(&output.reference.output);
                let _disabled = ui.begin_disabled_with_cond(output.info.is_err());
                let title = if output.reference.output == "video" {
                    name.clone()
                } else {
                    format!("{name} · {}", output.label)
                };
                if ui
                    .selectable_config(&title)
                    .selected(
                        workspace.viewers[&id].binding
                            == ViewerBinding::Pinned(output.reference.clone()),
                    )
                    .build()
                {
                    workspace.pin_output(id, output.reference);
                    source_changed = true;
                }
            }
        }
    }
    crate::sdk::toolbar::tooltip(
        ui,
        &format!("{label} — group sources follow their editor; all networks unlink this viewer"),
    );
    source_changed
}

#[cfg(test)]
#[path = "viewer_source_tests.rs"]
mod tests;
