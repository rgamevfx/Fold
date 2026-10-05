//! Published labels/defaults are edited without changing stable socket identities.
use super::state::State;
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::{EditResponse, imgui::Ui, toolbar::menu_button};

pub(super) fn draw(ui: &Ui, host: &mut dyn DesktopClient, state: &mut State) {
    let Some(id) = state.group else {
        return;
    };
    let Some(mut motion) = state.motion.clone() else {
        return;
    };
    let Some(group) = motion.groups.get_mut(&id) else {
        return;
    };
    let aces = host
        .snapshot()
        .is_some_and(|s| fold_platform::color::project(&s).ok().flatten().is_some());
    let mut response = EditResponse::default();
    if let Some(_menu) = menu_button(
        ui,
        "Interface",
        "Name this reusable group and its exposed controls",
    ) {
        let changed = ui.input_text("Group name", &mut group.name).build();
        response.item(ui, changed);
        for port in &mut group.inputs {
            let _id = ui.push_id(&port.id);
            ui.separator();
            let changed = ui.input_text("Control name", &mut port.name).build();
            response.item(ui, changed);
            let changed = ui.input_text("Section", &mut port.section).build();
            response.item(ui, changed);
            let changed = ui.checkbox("Collapsed initially", &mut port.advanced);
            response.item(ui, changed);
            if let Some(default) = &mut port.default {
                ui.text_disabled("Default for new instances");
                super::inspector::datum(ui, default, aces, &mut response);
            }
        }
    }
    response.cancel_on_escape(ui, state.interface_editing);
    if response.cancelled {
        state.cancel(host);
    } else {
        if response.changed {
            state.motion = Some(motion);
            state.interface_editing = true;
            state.preview(host);
        }
        if state.interface_editing && (response.finished || !ui.is_any_item_active()) {
            state.commit(host);
        }
    }
}
