//! Published labels/defaults are edited without changing stable socket identities.
use super::state::State;
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::{EditResponse, imgui::Ui, toolbar::menu_button};

pub(super) fn draw(ui: &Ui, state: &mut State) {
    let Some(id) = state.group else {
        return;
    };
    if let Some(_menu) = menu_button(ui, "Interface", "Edit this group's public controls") {
        if ui.menu_item("Edit node interface") {
            state.interface_target = Some(id);
        }
    }
}

pub(super) fn window(ui: &Ui, host: &mut dyn DesktopClient, state: &mut State) {
    let Some(id) = state.interface_target else {
        return;
    };
    let Some(mut motion) = state.motion.clone() else {
        return;
    };
    let Some(group) = motion.groups.get_mut(&id) else {
        state.interface_target = None;
        return;
    };
    let aces = host
        .snapshot()
        .is_some_and(|s| fold_platform::color::project(&s).ok().flatten().is_some());
    let mut response = EditResponse::default();
    let mut open = true;
    let mut add = None;
    ui.window("Edit node interface")
        .opened(&mut open)
        .size([420., 540.], fold_ui::sdk::imgui::Condition::FirstUseEver)
        .build(|| {
            ui.text_wrapped("Changes apply to every instance of this group.");
            if let Some(_menu) = menu_button(
                ui,
                "Add input",
                "Create a public input and internal Group Input node",
            ) {
                for (label, value) in [
                    ("Number", crate::fields::Datum::Scalar(0.)),
                    ("Vector", crate::fields::Datum::Vector([0., 0.])),
                    ("Color", crate::fields::Datum::Color([1.; 4])),
                    ("Toggle", crate::fields::Datum::Bool(false)),
                    ("Text", crate::fields::Datum::Text(String::new())),
                ] {
                    if ui.menu_item(label) {
                        add = Some(value);
                    }
                }
            }
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
        });
    if let Some(value) = add {
        match crate::authoring::add_group_input(&mut motion, id, value) {
            Ok(_) => {
                response.changed = true;
                response.finished = true;
            }
            Err(error) => state.error = error,
        }
    }
    if !open {
        state.interface_target = None;
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
