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
        self.store.invalidate_load();
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
        self.reset_runtime();
        self.sync_instances();
        self.rebuild_layout();
        self.restore_layout(context);
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

impl Shell {
    pub(super) fn sync_instances(&mut self) {
        // The shared Composition surface always needs a context, including when
        // the restored layout does not contain this registered contribution.
        for panel in &self.panels {
            if panel.panel.supports_composition()
                && let Some(kind) = panel.panel.document_type()
                && !self
                    .workspace
                    .editors
                    .values()
                    .any(|e| e.document_type == kind)
            {
                self.workspace.add_editor(panel.descriptor.id, kind);
            }
        }
        for panel in &mut self.panels {
            if panel.descriptor.placement == PanelPlacement::Editor
                && panel.panel.document_type().is_some()
            {
                continue;
            }
            let id = self.workspace.panel(panel.descriptor.id);
            let name = format!("fold.panel.{}", id.0);
            panel.key = WindowKey::new(name, panel.descriptor.title).unwrap();
        }
        let delivery = self.workspace.panel("fold.ui.delivery");
        let empty = self.workspace.panel("fold.ui.empty-editors");
        let delivery_name = format!("fold.panel.{}", delivery.0);
        let empty_name = format!("fold.panel.{}", empty.0);
        self.delivery = WindowKey::new(delivery_name, "Delivery").unwrap();
        self.empty = WindowKey::new(empty_name, "Editors").unwrap();
        self.editors
            .retain(|id, _| self.workspace.editors.contains_key(id));
        for (&id, editor) in &mut self.workspace.editors {
            let original = self
                .panels
                .iter()
                .find(|p| p.descriptor.id == editor.contribution);
            let template = original.and_then(|p| {
                if p.panel.document_type() == Some(editor.document_type.as_str()) {
                    Some(p)
                } else {
                    self.panels.iter().find(|p| {
                        p.descriptor.placement == PanelPlacement::Editor
                            && p.panel.document_type() == Some(editor.document_type.as_str())
                    })
                }
            });
            // Unavailable contributions are retained as placeholders, not rebound.
            let Some(template) = template else {
                self.editors.remove(&id);
                continue;
            };
            if self
                .editors
                .get(&id)
                .is_some_and(|p| p.contribution == template.descriptor.id)
            {
                continue;
            }
            if let Some(panels) = template.panel.new_instance() {
                editor.contribution = template.descriptor.id.into();
                self.editors.insert(
                    id,
                    RuntimeEditor {
                        contribution: editor.contribution.clone(),
                        key: WindowKey::new(
                            format!("fold.editor.{}", id.0),
                            format!("{} {}", template.descriptor.title, id.0),
                        )
                        .unwrap(),
                        panels,
                        initialized: false,
                    },
                );
            }
        }
        if !self.workspace.composition_editor.is_some_and(|id| {
            self.editors
                .get(&id)
                .is_some_and(|e| e.panels.editor.supports_composition())
        }) {
            self.workspace.composition_editor = self
                .workspace
                .focused_editor
                .filter(|id| {
                    self.editors
                        .get(id)
                        .is_some_and(|e| e.panels.editor.supports_composition())
                })
                .or_else(|| {
                    self.editors
                        .iter()
                        .find(|(_, e)| e.panels.editor.supports_composition())
                        .map(|(&id, _)| id)
                });
        }
        self.viewers
            .retain(|id, _| self.workspace.viewers.contains_key(id));
        for &id in self.workspace.viewers.keys() {
            self.viewers
                .entry(id)
                .or_insert_with(|| RuntimeViewer::new(id));
        }
    }
    pub(super) fn rebuild_layout(&mut self) {
        let keys: Vec<_> = self
            .workspace
            .editors
            .keys()
            .filter(|id| {
                !self.editors.get(id).is_some_and(|e| {
                    e.panels.editor.supports_composition() || e.panels.editor.is_layer_graph()
                })
            })
            .map(|&id| {
                self.editors
                    .get(&id)
                    .map(|e| e.key.clone())
                    .unwrap_or_else(|| {
                        WindowKey::new(format!("fold.editor.{}", id.0), "Unavailable editor")
                            .unwrap()
                    })
            })
            .collect();
        let mut editors: Vec<_> = keys.iter().collect();
        editors.extend(
            self.panels
                .iter()
                .filter(|p| {
                    p.descriptor.placement == PanelPlacement::Editor
                        && p.panel.document_type().is_none()
                })
                .map(|p| &p.key),
        );
        if editors.is_empty() {
            editors.push(&self.empty);
        }
        let mut inspectors: Vec<_> = self
            .panels
            .iter()
            .filter(|p| p.descriptor.placement == PanelPlacement::Inspector)
            .map(|p| &p.key)
            .collect();
        inspectors.push(&self.delivery);
        inspectors.extend(
            self.panels
                .iter()
                .filter(|p| p.descriptor.placement == PanelPlacement::Browser)
                .map(|p| &p.key),
        );
        let network: Vec<_> = self
            .panels
            .iter()
            .filter(|p| p.descriptor.id == crate::sdk::network::PANEL_ID)
            .map(|p| &p.key)
            .collect();
        editors.retain(|key| !network.contains(key));
        self.layout = DockLayout::split(
            DockSplit::Right,
            0.34,
            DockLayout::split(
                DockSplit::Down,
                0.52,
                DockLayout::tabs(network),
                DockLayout::tabs(inspectors),
            ),
            DockLayout::split(
                DockSplit::Down,
                0.52,
                DockLayout::tabs(editors),
                DockLayout::tabs(self.viewers.values().map(|viewer| &viewer.key)),
            ),
        );
    }
    fn restore_layout(&mut self, context: &mut dear_imgui_rs::Context) {
        self.reset_layout = self.workspace.layout.is_empty();
        // ImGui interprets a zero byte count as strlen(), whereas an empty Rust
        // String may point to address 1. An absent layout means default docking.
        if !self.reset_layout {
            context.load_ini_settings(&self.workspace.layout);
        }
    }
    pub fn save_workspace(&mut self, context: &mut dear_imgui_rs::Context) {
        if self.restoring || self.workspace.project.is_empty() {
            return;
        }
        self.workspace.layout.clear();
        context.save_ini_settings(&mut self.workspace.layout);
        self.store.save(self.workspace.clone());
    }
    pub fn workspace_frame(
        &mut self,
        context: &mut dear_imgui_rs::Context,
        client: &mut dyn DesktopClient,
    ) {
        // Bind saved editor contexts to the document types in the current snapshot.
        if let Some(snapshot) = client.snapshot() {
            self.workspace.refresh_document_types(&snapshot);
        }

        if let (Some(settings), Some(path)) = (&mut self.settings, &client.state().project_path) {
            if settings.ready && settings.preferences.recent.first() != Some(path) {
                settings.preferences.recent.retain(|p| p != path);
                settings.preferences.recent.insert(0, path.clone());
                settings.preferences.recent.truncate(20);
                settings.changed();
            }
        }
        if let Some(settings) = &self.settings {
            if settings.ready {
                self.workspace_menu.active = self
                    .workspace
                    .preset_name
                    .clone()
                    .filter(|name| settings.preferences.workspaces.contains_key(name));
            }
        }
        if self.epoch != client.workspace_epoch() {
            self.epoch = client.workspace_epoch();
            if let Some(project) = client.workspace_project() {
                if !self.workspace.project.is_empty() && self.workspace.project != project {
                    self.store.save(self.workspace.clone());
                }
                self.workspace.project = project.clone();
                self.saved.clear();
                self.restoring = client.workspace_restore();
                if self.restoring {
                    self.apply_preset(self.workspace.preset(), context, client, false);
                    self.workspace_menu.active = None;
                    self.store.load(project);
                    self.bootstrap = true;
                    self.last_host_navigation = 0;
                }
            } else {
                if !self.workspace.project.is_empty() {
                    self.store.save(self.workspace.clone());
                }
                let preset = self.workspace.preset();
                self.apply_preset(preset, context, client, false);
                self.workspace.project.clear();
                self.workspace_menu.active = None;
                self.restoring = false;
                self.bootstrap = true;
                self.last_host_navigation = 0;
            }
        }
        while let Some(completion) = self.store.take() {
            let loaded = completion.load_ticket.is_some();
            if completion.project != self.workspace.project {
                continue;
            }
            match completion.value {
                Ok(Some(workspace)) => {
                    for &id in self.workspace.viewers.keys() {
                        client.command(DesktopCommand::CloseViewer(id));
                    }
                    self.workspace = workspace;
                    if let Some(snapshot) = client.snapshot() {
                        self.workspace.refresh_document_types(&snapshot);
                    }
                    self.workspace_menu.active = self.workspace.preset_name.clone();
                    self.bootstrap = false;
                    self.last_host_navigation = client.state().navigation_event;
                    self.reset_runtime();
                    self.sync_instances();
                    self.rebuild_layout();
                    self.restore_layout(context);
                }
                Ok(None) => {}
                Err(error) => {
                    if loaded && self.restoring {
                        self.apply_preset(self.default_workspace.clone(), context, client, false);
                    }
                    client.command(DesktopCommand::Notify(format!("Workspace: {error}")));
                }
            }
            if loaded {
                self.restoring = false;
            }
        }
        self.preset_frame(context, client);
        if !self.restoring
            && !self.workspace.project.is_empty()
            && self.last_save.elapsed().as_millis() >= 500
        {
            self.last_save = std::time::Instant::now();
            self.workspace.layout.clear();
            context.save_ini_settings(&mut self.workspace.layout);
            if let Ok(value) = serde_json::to_string(&self.workspace)
                && value != self.saved
            {
                self.saved = value;
                self.store.save(self.workspace.clone());
            }
        }
    }
    pub fn prepare_frame(&mut self, context: &mut dear_imgui_rs::Context) {
        if let Some(settings) = &mut self.settings {
            settings.prepare_frame(context);
        }
        self.sync_instances();
        self.prepare_locked_inspector(context);
        for editor in self.editors.values_mut() {
            if !editor.initialized {
                editor
                    .panels
                    .editor
                    .initialize(context, self.typography.as_ref());
                editor
                    .panels
                    .inspector
                    .initialize(context, self.typography.as_ref());
                editor.initialized = true;
            }
            editor.panels.editor.prepare_frame(context);
            editor.panels.inspector.prepare_frame(context);
        }
        for panel in &mut self.panels {
            panel.panel.prepare_frame(context);
        }
    }
}

impl Shell {
    /// A replacement may reuse panel IDs. No runtime state crosses that lifetime.
    pub(super) fn reset_runtime(&mut self) {
        self.editors.clear();
        self.viewers.clear();
        self.selection_events.clear();
        self.review_actions.clear();
        self.locked_inspector = None;
        self.document_settings = None;
        self.delivery_controls = Default::default();
        self.visible_editors.clear();
        self.visible_panels.clear();
        self.cached_previews.clear();
        self.focus = self.workspace.focused_editor;
    }
}

impl Shell {
    pub(super) fn close_viewer(&mut self, id: PanelInstanceId, client: &mut dyn DesktopClient) {
        if !self.workspace.viewers.contains_key(&id) {
            return;
        }
        if let Some(editor_id) = self.workspace.editor_for_viewer(id)
            && let Some(editor) = self.editors.get_mut(&editor_id)
            && let Some(binding) = self.workspace.editors.get_mut(&editor_id)
        {
            let mut context = PanelContext::new(client, binding)
                .instance(id)
                .with_cache(&self.cached_previews);
            editor.panels.editor.cancel_interaction(&mut context);
        }
        client.command(DesktopCommand::CloseViewer(id));
        if self.workspace.monitored_viewer == Some(id) {
            self.workspace.monitored_viewer = None;
        }
        self.workspace.viewers.remove(&id);
        self.viewers.remove(&id);
        self.review_actions.retain(|(viewer, _)| *viewer != id);
        if self.workspace.inspector_viewer == Some(id) {
            self.workspace.inspector_viewer = None;
        }
        self.sync_instances();
        self.rebuild_layout();
    }
}

impl Shell {
    pub(super) fn close_editor(&mut self, id: PanelInstanceId, client: &mut dyn DesktopClient) {
        if let Some(editor) = self.editors.get_mut(&id)
            && let Some(binding) = self.workspace.editors.get_mut(&id)
        {
            let mut context = PanelContext::new(client, binding).instance(id);
            editor.panels.editor.cancel_interaction(&mut context);
        }
        self.workspace.close_editor(id);
        self.selection_events.retain(|(owner, _)| *owner != id);
        if self.focus == Some(id) {
            self.focus = None;
        }
        if self.workspace.inspector_lock.is_none() {
            self.locked_inspector = None;
        }
        self.sync_instances();
        self.rebuild_layout();
        self.refresh_viewer_targets(client);
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

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn saved_layout_supersedes_a_pending_default_reset() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        let mut shell = Shell::new(vec![]);
        shell.reset_layout = true;
        shell.workspace.layout =
            "[Window][Restored panel]\nPos=24,36\nSize=320,240\nCollapsed=0\n".into();
        shell.restore_layout(&mut context);
        assert!(!shell.reset_layout);
        let mut saved = String::new();
        context.save_ini_settings(&mut saved);
        assert!(saved.contains("[Window][Restored panel]"));
        assert!(saved.contains("Pos=24,36"));
    }

    #[test]
    fn empty_sidecar_layout_restores_default_docking_without_ffi_read() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        let mut shell = Shell::new(vec![]);
        assert!(shell.workspace.layout.is_empty());
        shell.restore_layout(&mut context);
        assert!(shell.reset_layout);
        shell.layout.validate().unwrap();
    }
}
