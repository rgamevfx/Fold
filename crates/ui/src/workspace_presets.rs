//! Named layout presets. Project-bound session state stays in the sidecar store.
use crate::settings::Settings;
use dear_imgui_rs::Ui;
#[derive(Clone)]
pub(crate) enum Action {
    Save(String),
    Load(String),
    Rename(String, String),
    Delete(String),
    Default,
}
#[derive(Default)]
pub(crate) struct Menu {
    pub active: Option<String>,
    pub pending: Option<Action>,
    naming: bool,
    renaming: bool,
    name: String,
}
impl Menu {
    pub fn draw(&mut self, ui: &Ui, settings: Option<&Settings>) {
        let Some(_menu) = ui.begin_menu("Workspace") else {
            return;
        };
        let _disabled = ui.begin_disabled_with_cond(!settings.is_some_and(|s| s.ready));
        if let Some(settings) = settings {
            for name in settings.preferences.workspaces.keys() {
                if ui.menu_item_enabled_selected_no_shortcut(
                    name,
                    self.active.as_ref() == Some(name),
                    true,
                ) {
                    self.pending = Some(Action::Load(name.clone()));
                }
            }
        }
        ui.separator();
        if ui.menu_item("Save Workspace As…") {
            self.naming = true;
            self.renaming = false;
            self.name.clear();
        }
        if let Some(active) = &self.active {
            if ui.menu_item("Update Current Workspace") {
                self.pending = Some(Action::Save(active.clone()));
            }
            if ui.menu_item("Restore Saved Workspace") {
                self.pending = Some(Action::Load(active.clone()));
            }
            if ui.menu_item("Rename Workspace…") {
                self.naming = true;
                self.renaming = true;
                self.name = active.clone();
            }
            if ui.menu_item("Delete Workspace Preset") {
                self.pending = Some(Action::Delete(active.clone()));
            }
        }
        ui.separator();
        if ui.menu_item("Reset to Default") {
            self.pending = Some(Action::Default);
        }
    }
    pub fn dialog(&mut self, ui: &Ui, settings: Option<&Settings>) {
        if !self.naming {
            return;
        }
        ui.open_popup("Workspace name");
        if let Some(_popup) = ui
            .begin_modal_popup_config("Workspace name")
            .flags(
                dear_imgui_rs::WindowFlags::ALWAYS_AUTO_RESIZE
                    | dear_imgui_rs::WindowFlags::NO_SAVED_SETTINGS,
            )
            .begin()
        {
            ui.input_text("Name", &mut self.name).build();
            let name = self.name.trim();
            let valid = !name.is_empty()
                && name.len() <= 128
                && settings.is_some_and(|s| {
                    s.ready
                        && (self.renaming || s.preferences.workspaces.len() < 32)
                        && (!s.preferences.workspaces.contains_key(name)
                            || (self.renaming && self.active.as_deref() == Some(name)))
                });
            {
                let _disabled = ui.begin_disabled_with_cond(!valid);
                if ui.button("Save") {
                    self.pending = Some(if self.renaming {
                        Action::Rename(self.active.clone().unwrap(), name.into())
                    } else {
                        Action::Save(name.into())
                    });
                    self.naming = false;
                    ui.close_current_popup();
                }
            }
            if !valid {
                ui.text_wrapped("Use a short, unique name. Up to 32 presets.");
            }
            ui.same_line();
            if ui.button("Cancel") || ui.is_key_pressed(dear_imgui_rs::Key::Escape) {
                self.naming = false;
                ui.close_current_popup();
            }
        }
    }
}
