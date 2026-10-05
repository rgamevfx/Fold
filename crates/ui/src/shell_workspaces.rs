use super::*;
use crate::workspace_presets::Action;
impl Shell {
    pub(super) fn apply_preset(
        &mut self,
        mut preset: Workspace,
        context: &mut dear_imgui_rs::Context,
        client: &mut dyn DesktopClient,
        retain_targets: bool,
    ) {
        for &id in self.workspace.viewers.keys() {
            client.command(DesktopCommand::CloseViewer(id));
        }
        if retain_targets {
            for editor in preset.editors.values_mut() {
                if let Some(current) = self
                    .workspace
                    .editors
                    .values()
                    .find(|e| e.group == editor.group && e.contribution == editor.contribution)
                {
                    editor.navigation = current.navigation.clone();
                    editor.selection = current.selection.clone();
                    editor.mapped_navigation = current.mapped_navigation;
                }
            }
        }
        preset.project = client.workspace_project().unwrap_or_default();
        self.workspace = preset;
        self.focus = self.workspace.focused_editor;
        self.bootstrap = false;
        self.last_host_navigation = client.state().navigation_event;
        self.editors.clear();
        self.viewers.clear();
        self.transports.clear();
        self.navigation.clear();
        self.presentation.clear();
        self.sync_instances();
        self.rebuild_layout();
        if self.workspace.layout.is_empty() {
            self.reset_layout = true;
        } else {
            context.load_ini_settings(&self.workspace.layout);
        }
        self.saved.clear();
    }
    pub(super) fn preset_frame(
        &mut self,
        context: &mut dear_imgui_rs::Context,
        client: &mut dyn DesktopClient,
    ) {
        if self.restoring {
            return;
        }
        let Some(action) = self.workspace_menu.pending.take() else {
            return;
        };
        let result = (|| -> Result<(), String> {
            if matches!(action, Action::Default) {
                self.apply_preset(self.default_workspace.clone(), context, client, true);
                self.workspace_menu.active = None;
                self.workspace.preset_name = None;
                return Ok(());
            }
            let settings = self
                .settings
                .as_mut()
                .filter(|s| s.ready)
                .ok_or("Preferences are unavailable")?;
            match action {
                Action::Save(name) => {
                    self.workspace.layout.clear();
                    context.save_ini_settings(&mut self.workspace.layout);
                    settings.preferences.workspaces.insert(
                        name.clone(),
                        serde_json::to_value(self.workspace.preset()).map_err(|e| e.to_string())?,
                    );
                    settings.changed();
                    self.workspace_menu.active = Some(name);
                }
                Action::Load(name) => {
                    let bytes = serde_json::to_vec(
                        settings
                            .preferences
                            .workspaces
                            .get(&name)
                            .ok_or("Workspace preset is missing")?,
                    )
                    .map_err(|e| e.to_string())?;
                    let preset = Workspace::decode(&bytes, "")?.preset();
                    self.apply_preset(preset, context, client, true);
                    self.workspace_menu.active = Some(name);
                }
                Action::Rename(old, new) => {
                    if old != new && settings.preferences.workspaces.contains_key(&new) {
                        return Err("Workspace name is already in use".into());
                    }
                    let value = settings
                        .preferences
                        .workspaces
                        .remove(&old)
                        .ok_or("Workspace preset is missing")?;
                    settings.preferences.workspaces.insert(new.clone(), value);
                    settings.changed();
                    self.workspace_menu.active = Some(new);
                }
                Action::Delete(name) => {
                    settings.preferences.workspaces.remove(&name);
                    settings.changed();
                    self.workspace_menu.active = None;
                }
                Action::Default => unreachable!(),
            }
            self.workspace.preset_name = self.workspace_menu.active.clone();
            Ok(())
        })();
        if let Err(error) = result {
            client.command(DesktopCommand::Notify(format!("Workspace: {error}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_platform::desktop::{DesktopState, PreviewResult};
    #[derive(Default)]
    struct Host {
        state: DesktopState,
    }
    impl DesktopClient for Host {
        fn state(&self) -> &DesktopState {
            &self.state
        }
        fn poll(&mut self) {}
        fn command(&mut self, _: DesktopCommand) {}
        fn request_preview(&mut self, _: PreviewKey) {}
        fn cancel_preview(&mut self) {}
        fn take_preview(&mut self) -> Option<PreviewResult> {
            None
        }
    }
    #[test]
    fn preset_management_retains_current_targets_without_importing_saved_targets() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        let fonts = crate::sdk::typography::Typography::install(&mut context);
        let mut shell = Shell::new(vec![]);
        shell.settings = Some(crate::settings::Settings::new(fonts, None));
        let mut host = Host::default();
        let editor = shell
            .workspace
            .add_editor("unavailable.editor", "document.kind");
        let a = DocumentId::new();
        let b = DocumentId::new();
        shell
            .workspace
            .editors
            .get_mut(&editor)
            .unwrap()
            .bind(ViewLocation {
                document: a,
                time: Time::ZERO,
                label: "A".into(),
            });
        shell.workspace_menu.pending = Some(Action::Save("Editing".into()));
        shell.preset_frame(&mut context, &mut host);
        assert_eq!(shell.workspace.editors[&editor].document(), Some(a));
        shell
            .workspace
            .editors
            .get_mut(&editor)
            .unwrap()
            .bind(ViewLocation {
                document: b,
                time: Time::ZERO,
                label: "B".into(),
            });
        shell.workspace.add_viewer(Default::default());
        shell.workspace_menu.pending = Some(Action::Load("Editing".into()));
        shell.preset_frame(&mut context, &mut host);
        assert_eq!(shell.workspace.viewers.len(), 1);
        assert_eq!(shell.workspace.editors[&editor].document(), Some(b));
        shell.workspace_menu.pending = Some(Action::Rename("Editing".into(), "Custom".into()));
        shell.preset_frame(&mut context, &mut host);
        let presets = &shell.settings.as_ref().unwrap().preferences.workspaces;
        assert!(!presets.contains_key("Editing") && presets.contains_key("Custom"));
        // Closing/opening a project explicitly clears all old bindings.
        shell.apply_preset(shell.workspace.preset(), &mut context, &mut host, false);
        assert!(shell.workspace.editors[&editor].document().is_none());
        shell.workspace_menu.pending = Some(Action::Delete("Custom".into()));
        shell.preset_frame(&mut context, &mut host);
        assert!(
            shell
                .settings
                .as_ref()
                .unwrap()
                .preferences
                .workspaces
                .is_empty()
        );
        assert!(shell.workspace_menu.active.is_none());
        shell.workspace_menu.pending = Some(Action::Default);
        shell.preset_frame(&mut context, &mut host);
        assert!(shell.workspace.editors.is_empty());
        assert_eq!(shell.workspace.viewers.len(), 1);
    }
}
