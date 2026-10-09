//! Project lifecycle intent and save-before-replace confirmation.
use dear_imgui_rs::{Key, Ui};
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) enum Action {
    New,
    Open(Option<PathBuf>),
    Quit,
}
#[derive(Default)]
pub(crate) struct ProjectMenu {
    pending: Option<Action>,
    waiting_save: Option<u64>,
    pub quit: bool,
    seen_result: u64,
    error: Option<String>,
}
impl ProjectMenu {
    pub fn request(&mut self, action: Action, client: &mut dyn DesktopClient) {
        if self.pending.is_some() {
            return;
        }
        if client.state().busy {
            let message = "Wait for background jobs, or cancel them in Delivery, before replacing or closing the project".to_owned();
            self.error = Some(message.clone());
            client.command(DesktopCommand::Notify(message));
        } else if client.state().dirty || client.state().transient {
            self.pending = Some(action);
        } else {
            self.perform(action, client);
        }
    }
    fn perform(&mut self, action: Action, client: &mut dyn DesktopClient) {
        match action {
            Action::New => client.command(DesktopCommand::NewProject),
            Action::Open(Some(path)) => client.command(DesktopCommand::Open(path)),
            Action::Open(None) => client.command(DesktopCommand::ChooseOpen),
            Action::Quit => self.quit = true,
        }
    }
    fn save(client: &mut dyn DesktopClient, save_as: bool) {
        client.command(
            if let Some(path) = client.state().project_path.clone().filter(|_| !save_as) {
                DesktopCommand::Save(path)
            } else {
                DesktopCommand::ChooseSave
            },
        );
    }
    /// Called inside the shell's one menu bar.
    pub fn menus(&mut self, ui: &Ui, client: &mut dyn DesktopClient, recent: &[PathBuf]) {
        let state = client.state().clone();
        let available = !state.busy && self.pending.is_none();
        let mut action = None;
        if let Some(_menu) = ui.begin_menu("File") {
            let _disabled = ui.begin_disabled_with_cond(!available);
            if ui.menu_item_with_shortcut("New Project", "Ctrl+N") {
                action = Some(Action::New);
            }
            if ui.menu_item_with_shortcut("Open…", "Ctrl+O") {
                action = Some(Action::Open(None));
            }
            if let Some(_recent) = ui.begin_menu("Open Recent") {
                if recent.is_empty() {
                    ui.text_disabled("No recent projects");
                }
                for path in recent {
                    if ui.menu_item(path.to_string_lossy()) {
                        action = Some(Action::Open(Some(path.clone())));
                    }
                }
            }
            ui.separator();
            if ui.menu_item_enabled_selected_with_shortcut(
                "Save",
                "Ctrl+S",
                false,
                !state.transient,
            ) {
                Self::save(client, false);
            }
            if ui.menu_item_enabled_selected_with_shortcut(
                "Save As…",
                "Ctrl+Shift+S",
                false,
                !state.transient,
            ) {
                Self::save(client, true);
            }
            if ui.menu_item_with_shortcut("Close Project", "Ctrl+W") {
                action = Some(Action::New);
            }
            ui.separator();
            if ui.menu_item("Import…") {
                client.command(DesktopCommand::Browser(
                    fold_platform::browser::BrowserCommand::ChooseImport(Default::default()),
                ));
            }
            ui.separator();
            if ui.menu_item_with_shortcut("Quit", "Ctrl+Q") {
                action = Some(Action::Quit);
            }
        }
        if let Some(_menu) = ui.begin_menu("Edit") {
            if ui.menu_item_enabled_selected_with_shortcut(
                "Undo",
                "Ctrl+Z",
                false,
                state.can_undo && !state.transient,
            ) {
                client.command(DesktopCommand::Undo);
            }
            if ui.menu_item_enabled_selected_with_shortcut(
                "Redo",
                "Ctrl+Shift+Z",
                false,
                state.can_redo && !state.transient,
            ) {
                client.command(DesktopCommand::Redo);
            }
        }
        if available
            && !ui.io().want_text_input()
            && ui.io().key_ctrl()
            && !ui.is_popup_open_with_flags("", dear_imgui_rs::PopupQueryFlags::ANY_POPUP)
        {
            if ui.is_key_pressed(Key::N) {
                action = Some(Action::New);
            }
            if ui.is_key_pressed(Key::O) {
                action = Some(Action::Open(None));
            }
            if ui.is_key_pressed(Key::W) {
                action = Some(Action::New);
            }
            if ui.is_key_pressed(Key::Q) {
                action = Some(Action::Quit);
            }
            if ui.is_key_pressed(Key::S) && !state.transient {
                Self::save(client, ui.io().key_shift());
            }
        }
        if let Some(action) = action {
            self.request(action, client);
        }
    }
    fn poll_save(&mut self, client: &mut dyn DesktopClient) {
        if let Some(serial) = self.waiting_save {
            if client.state().file_busy {
                return;
            }
            self.waiting_save = None;
            if client.state().save_serial != serial
                && !client.state().dirty
                && !client.state().transient
            {
                if let Some(action) = self.pending.take() {
                    self.perform(action, client);
                }
            }
            // Failure, dialog cancellation or intervening edits keep the decision pending.
        }
    }
    pub fn confirmation(&mut self, ui: &Ui, client: &mut dyn DesktopClient) {
        self.poll_save(client);
        if self.seen_result != client.state().file_result_serial {
            self.seen_result = client.state().file_result_serial;
            if self.pending.is_none() {
                self.error = client.state().project_error.clone();
            }
        }
        if let Some(error) = &self.error {
            ui.open_popup("Project operation");
            if let Some(_popup) = ui
                .begin_modal_popup_config("Project operation")
                .flags(
                    dear_imgui_rs::WindowFlags::ALWAYS_AUTO_RESIZE
                        | dear_imgui_rs::WindowFlags::NO_SAVED_SETTINGS,
                )
                .begin()
            {
                let _wrap =
                    ui.push_text_wrap_pos((ui.io().display_size()[0] - 40.).clamp(200., 400.));
                ui.text(error);
                if ui.button("Close") || ui.is_key_pressed(Key::Escape) {
                    self.error = None;
                    ui.close_current_popup();
                }
            }
        }
        if self.pending.is_none() || self.waiting_save.is_some() {
            return;
        }
        ui.open_popup("Unsaved project");
        if let Some(_popup) = ui
            .begin_modal_popup_config("Unsaved project")
            .flags(
                dear_imgui_rs::WindowFlags::ALWAYS_AUTO_RESIZE
                    | dear_imgui_rs::WindowFlags::NO_SAVED_SETTINGS,
            )
            .begin()
        {
            ui.text("Save changes before continuing?");
            if client.state().transient {
                ui.text_wrapped("Finish or cancel the active edit before saving.");
            }
            if let Some(error) = &client.state().project_error {
                let _wrap =
                    ui.push_text_wrap_pos((ui.io().display_size()[0] - 40.).clamp(200., 400.));
                ui.text(error);
            }
            {
                let _disabled = ui.begin_disabled_with_cond(client.state().transient);
                if ui.button("Save") {
                    self.waiting_save = Some(client.state().save_serial);
                    Self::save(client, false);
                    ui.close_current_popup();
                }
            }
            ui.same_line();
            if ui.button("Discard") {
                client.command(DesktopCommand::CancelPreviewEdit);
                if let Some(action) = self.pending.take() {
                    self.perform(action, client);
                }
                ui.close_current_popup();
            }
            ui.same_line();
            if ui.button("Cancel") || ui.is_key_pressed(Key::Escape) {
                self.pending = None;
                ui.close_current_popup();
            }
        }
    }
}

#[cfg(test)]
#[path = "project_menu_tests.rs"]
mod tests;
