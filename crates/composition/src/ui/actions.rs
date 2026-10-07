//! Composition and layer actions; all mutations use the shared transaction state.
use super::state::State;
use crate::{Layer, package};
use fold_foundation::DocumentId;
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_ui::sdk::{imgui::Ui, toolbar::menu_button};
pub(super) fn draw(ui: &Ui, host: &mut dyn DesktopClient, state: &mut State) {
    if let Some(_menu) = menu_button(ui, "Composition", "Composition and layer actions") {
        if ui.menu_item("New composition") {
            host.command(DesktopCommand::Browser(
                fold_platform::browser::BrowserCommand::Create {
                    kind: crate::COMPOSITION.into(),
                    parent: Default::default(),
                },
            ));
        }
        if let Some(document) = state.document {
            for (label, kind) in [
                ("Add MoGraph layer", "motion"),
                ("Add Compositing layer", "image"),
            ] {
                if ui.menu_item(label) {
                    state.command(
                        host,
                        "fold.app.add-layer-graph",
                        serde_json::json!({"document":document,"kind":kind}),
                    );
                }
            }
            if ui.menu_item("New nested composition") {
                state.command(host,package::NEST,serde_json::json!({"document":document,"child":DocumentId::new(),"name":"Nested composition","layers":[]}));
            }
            if ui.menu_item("New group") {
                let c = state.composition.as_mut().unwrap();
                match Layer::new("Group", None, c.info.clone()) {
                    Ok(layer) => {
                        c.layers.insert(0, layer);
                        state.commit(host);
                    }
                    Err(e) => state.error = e,
                }
            }
        }
    }
    if state.document.is_some() {
        ui.same_line();
        if let Some(_menu) = menu_button(ui, "Layer", "Selected layer actions") {
            let selected = state.selected.clone();
            if !selected.is_empty() {
                if ui.menu_item("Precompose selected layers") {
                    let document = state.document.unwrap();
                    state.command(host,package::NEST,serde_json::json!({"document":document,"child":DocumentId::new(),"name":"Nested composition","layers":selected}));
                }
                if ui.menu_item("Delete selected layers (Delete)") {
                    delete_selected(host, state);
                }
            }
            if let Some(id) = state.selected.first().copied() {
                if ui.menu_item("Edit layer graph") {
                    if let Err(e) = state.open(host, id) {
                        state.error = e;
                    }
                }
                let locked = state.composition.as_ref().unwrap().locked(id);
                let _disabled = ui.begin_disabled_with_cond(locked);
                if ui.menu_item("Add image processing graph") {
                    let document = state.document.unwrap();
                    state.command(
                        host,
                        "fold.app.add-layer-graph",
                        serde_json::json!({"document":document,"kind":"process","layer":id}),
                    );
                }
            }
        }
    }
}

/// Shared by the Layer menu and the focused layer editor shortcut.
pub(super) fn delete_selected(host: &mut dyn DesktopClient, state: &mut State) {
    let selected = state.selected.clone();
    if selected.is_empty() {
        return;
    }
    let c = state.composition.as_mut().unwrap();
    if selected.iter().any(|id| c.locked(*id)) {
        state.error = "Unlock layers before deleting".into();
    } else {
        let mut removed = selected;
        loop {
            let before = removed.len();
            for l in &c.layers {
                if l.parent.is_some_and(|p| removed.contains(&p)) && !removed.contains(&l.id) {
                    removed.push(l.id);
                }
            }
            if before == removed.len() {
                break;
            }
        }
        if removed.iter().any(|id| c.locked(*id)) {
            state.error = "Unlock descendant layers before deleting".into();
        } else {
            c.layers.retain(|l| !removed.contains(&l.id));
            state.commit(host);
        }
    }
}
