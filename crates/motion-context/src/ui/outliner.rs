//! Artist object hierarchy; all actions stage the shared document operations.
use super::state::State;
use fold_foundation::ObjectId;
use fold_motion_canvas::{
    editing::{self, Edit},
    hierarchy,
    model::{Canvas, Content},
    tools,
};
use fold_ui::sdk::{self, ExtensionUi, imgui::Key};

fn rows(canvas: &Canvas, ids: &[ObjectId], depth: usize, output: &mut Vec<(ObjectId, usize)>) {
    for id in ids {
        output.push((*id, depth));
        if let Content::Group { children, .. } = &canvas.objects[id].content {
            rows(canvas, children, depth + 1, output);
        }
    }
}

pub(super) fn draw(context: ExtensionUi<'_>, state: &mut State) {
    let ExtensionUi { ui, host } = context;
    let Some(canvas) = state.canvas().cloned() else {
        ui.text_disabled("Select a Motion document.");
        return;
    };
    let mut action: Option<Result<(Vec<Edit>, Vec<ObjectId>), String>> = None;
    let disabled = ui.begin_disabled_with_cond(state.busy() || host.state().playing);
    if let Some(_menu) = sdk::toolbar::menu_button(ui, "Add", "Add an object") {
        for (kind, label) in [("text", "Text"), ("script", "Script"), ("group", "Group")] {
            if ui.menu_item(label) {
                let id = ObjectId::new();
                let object = tools::new_object(kind).expect("registered object kind");
                action = Some(Ok((
                    vec![Edit::Insert {
                        id,
                        object: Box::new(object),
                        parent: None,
                        index: canvas.roots.len(),
                    }],
                    vec![id],
                )));
            }
        }
    }
    if ui.is_window_focused()
        && !ui.io().want_text_input()
        && ui.io().key_ctrl()
        && ui.is_key_pressed(Key::G)
        && !state.busy()
        && !host.state().playing
    {
        action =
            Some(hierarchy::group(&canvas, &state.selected).map(|(id, edits)| (edits, vec![id])));
    }
    let mut ordered = vec![];
    rows(&canvas, &canvas.roots, 0, &mut ordered);
    for (id, depth) in ordered {
        let _id = ui.push_id(&id.to_string());
        let indentation = depth as f32 * ui.clone_style().indent_spacing();
        ui.indent_by(indentation);
        if ui
            .selectable_config(&canvas.objects[&id].name)
            .selected(state.selected.contains(&id))
            .build()
        {
            let mut selection = if ui.io().key_ctrl() {
                state.selected.clone()
            } else {
                vec![]
            };
            if let Some(index) = selection.iter().position(|selected| *selected == id) {
                selection.remove(index);
            } else {
                selection.push(id);
            }
            state.select(host, selection);
        }
        if let Some(_popup) = ui.begin_popup_context_item_with_label(Some("motion-object")) {
            if !state.selected.contains(&id) {
                state.select(host, vec![id]);
            }
            let editable = editing::editable(&canvas, id).is_ok();
            let _locked = ui.begin_disabled_with_cond(!editable);
            {
                let _single = ui.begin_disabled_with_cond(state.selected.len() < 2);
                if ui.menu_item("Group selection (Ctrl+G)") {
                    action = Some(
                        hierarchy::group(&canvas, &state.selected)
                            .map(|(id, edits)| (edits, vec![id])),
                    );
                }
            }
            ui.separator();
            for (label, order) in [
                ("Bring forward", hierarchy::Order::Forward),
                ("Bring to front", hierarchy::Order::Front),
                ("Send backward", hierarchy::Order::Backward),
                ("Send to back", hierarchy::Order::Back),
            ] {
                if ui.menu_item(label) {
                    action = Some(
                        hierarchy::arrange(&canvas, &state.selected, order)
                            .map(|edits| (edits, state.selected.clone())),
                    );
                }
            }
            ui.separator();
            if ui.menu_item("Delete object") {
                action = Some(Ok((
                    vec![Edit::Delete {
                        id,
                        expected: Box::new(canvas.objects[&id].clone()),
                    }],
                    state.selected.clone(),
                )));
            }
        }
        ui.unindent_by(indentation);
    }
    drop(disabled);
    if let Some(action) = action {
        state.cancel(host);
        match action.and_then(|(edits, selected)| {
            state.preview(host, edits)?;
            state.commit(host)?;
            Ok(selected)
        }) {
            Ok(mut selected) => {
                // Removing a group also removes its descendants.
                if let Some(canvas) = state.canvas() {
                    selected.retain(|id| canvas.objects.contains_key(id));
                }
                state.select(host, selected);
            }
            Err(error) => {
                state.cancel(host);
                state.error = error;
            }
        }
    }
    if !state.error.is_empty() {
        ui.text_wrapped(&state.error);
    }
}
