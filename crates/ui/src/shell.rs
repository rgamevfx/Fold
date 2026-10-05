//! Generic instance-aware workspace shell. Feature models stay in providers.
use crate::{
    group_selector,
    sdk::{EditorPanels, ExtensionUi, RegisteredPanel},
    target_selector::{self, Action, Choice},
    workspace_store::WorkspaceStore,
};
use dear_imgui_rs::{DockLayout, DockLayoutApply, DockSplit, Key, TextureId, Ui, WindowKey};
use fold_foundation::{DocumentId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, PreviewKey, ViewLocation},
    packages::PanelPlacement,
    panel_context::PanelContext,
    workspace::{self, EditorInstance, PanelInstanceId, ViewerBinding, ViewerInstance, Workspace},
};
use std::collections::BTreeMap;

pub(crate) enum Preview {
    Pending,
    Failed(String),
    Ready {
        texture: TextureId,
        dimensions: [u32; 2],
        uv_max: [f32; 2],
    },
}
fn fitted_size(dimensions: [u32; 2], available: [f32; 2]) -> [f32; 2] {
    let [width, height] = dimensions.map(|v| v as f32);
    let scale = (available[0].max(0.0) / width).min(available[1].max(0.0) / height);
    [width * scale, height * scale]
}
struct RuntimeEditor {
    contribution: String,
    key: WindowKey,
    panels: EditorPanels,
    initialized: bool,
}
pub(crate) struct Shell {
    pub(crate) settings: Option<crate::settings::Settings>,
    pub(crate) typography: Option<crate::sdk::typography::Typography>,
    viewer: WindowKey,
    delivery: WindowKey,
    empty: WindowKey,
    panels: Vec<RegisteredPanel>,
    editors: BTreeMap<PanelInstanceId, RuntimeEditor>,
    viewers: BTreeMap<PanelInstanceId, WindowKey>,
    pub(crate) workspace: Workspace,
    layout: DockLayout,
    reset_layout: bool,
    pub(crate) project_menu: crate::project_menu::ProjectMenu,
    workspace_menu: crate::workspace_presets::Menu,
    default_workspace: Workspace,
    export_path: String,
    transports: BTreeMap<PanelInstanceId, crate::transport::Transport>,
    start: i32,
    end: i32,
    delivery_output: Option<(DocumentId, u32)>,
    visible_panels: Vec<usize>,
    visible_editors: Vec<PanelInstanceId>,
    navigation: BTreeMap<PanelInstanceId, crate::viewer_navigation::Navigation>,
    viewport_scale: BTreeMap<PanelInstanceId, [f32; 2]>,
    viewport_pixels: BTreeMap<PanelInstanceId, [f32; 2]>,
    #[cfg(feature = "native-probe")]
    probe_full_resolution: bool,
    presentation: BTreeMap<PanelInstanceId, (Option<PreviewKey>, Option<String>)>,
    review_actions: Vec<(PanelInstanceId, crate::review::Action)>,
    review_states: BTreeMap<PanelInstanceId, (Option<String>, bool)>,
    last_host_navigation: u64,
    bootstrap: bool,
    focus: Option<PanelInstanceId>,
    store: WorkspaceStore,
    epoch: u64,
    restoring: bool,
    saved: String,
    last_save: std::time::Instant,
}
impl Shell {
    pub fn new(panels: Vec<RegisteredPanel>) -> Self {
        let mut workspace = Workspace::default();
        for panel in &panels {
            if panel.descriptor.placement == PanelPlacement::Editor
                && let Some(kind) = panel.panel.document_type()
            {
                workspace.add_editor(panel.descriptor.id, kind);
            }
        }
        let first = workspace.editors.keys().next().copied();
        if let Some(first) = first {
            workspace.record_selection(first);
        }
        workspace.add_viewer(ViewerInstance::default());
        let viewer = WindowKey::new("fold.viewer.main", "Viewer").unwrap();
        let delivery = WindowKey::new("fold.delivery.main", "Delivery").unwrap();
        let empty = WindowKey::new("fold.editors.empty", "Editors").unwrap();
        let mut shell = Self {
            settings: None,
            typography: None,
            viewer,
            delivery,
            empty,
            panels,
            workspace,
            editors: Default::default(),
            viewers: Default::default(),
            layout: DockLayout::tabs(std::iter::empty::<&WindowKey>()),
            reset_layout: false,
            project_menu: Default::default(),
            workspace_menu: Default::default(),
            default_workspace: Workspace::default(),
            export_path: "/tmp/fold-export.mp4".into(),
            transports: Default::default(),
            start: 0,
            end: 1,
            delivery_output: None,
            visible_panels: vec![],
            visible_editors: vec![],
            navigation: Default::default(),
            viewport_scale: Default::default(),
            viewport_pixels: Default::default(),
            #[cfg(feature = "native-probe")]
            probe_full_resolution: false,
            presentation: Default::default(),
            review_actions: vec![],
            review_states: Default::default(),
            last_host_navigation: 0,
            bootstrap: true,
            focus: None,
            store: WorkspaceStore::new(),
            epoch: 0,
            restoring: false,
            saved: String::new(),
            last_save: std::time::Instant::now(),
        };
        shell.sync_instances();
        shell.rebuild_layout();
        shell.default_workspace = shell.workspace.preset();
        shell
    }
    fn sync_instances(&mut self) {
        for panel in &mut self.panels {
            if panel.descriptor.placement == PanelPlacement::Editor
                && panel.panel.document_type().is_some()
            {
                continue;
            }
            let id = self.workspace.panel(panel.descriptor.id);
            let name = format!("fold.panel.{}", id.0);
            self.workspace.layout = self.workspace.layout.replace(
                &format!("[Window][{}]", panel.descriptor.id),
                &format!("[Window][{name}]"),
            );
            panel.key = WindowKey::new(name, panel.descriptor.title).unwrap();
        }
        let delivery = self.workspace.panel("fold.ui.delivery");
        let empty = self.workspace.panel("fold.ui.empty-editors");
        let delivery_name = format!("fold.panel.{}", delivery.0);
        let empty_name = format!("fold.panel.{}", empty.0);
        self.workspace.layout = self.workspace.layout.replace(
            "[Window][fold.delivery.main]",
            &format!("[Window][{delivery_name}]"),
        );
        self.workspace.layout = self.workspace.layout.replace(
            "[Window][fold.editors.empty]",
            &format!("[Window][{empty_name}]"),
        );
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
        self.viewers
            .retain(|id, _| self.workspace.viewers.contains_key(id));
        self.transports
            .retain(|id, _| self.workspace.viewers.contains_key(id));
        for &id in self.workspace.viewers.keys() {
            self.viewers.entry(id).or_insert_with(|| {
                WindowKey::new(format!("fold.viewer.{}", id.0), format!("Viewer {}", id.0)).unwrap()
            });
        }
        if let Some(key) = self.viewers.values().next() {
            self.viewer = key.clone();
        }
    }
    fn rebuild_layout(&mut self) {
        let keys: Vec<_> = self
            .workspace
            .editors
            .keys()
            .filter(|id| {
                !self
                    .editors
                    .get(id)
                    .is_some_and(|e| e.panels.editor.network_primary())
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
        let layout = DockLayout::split(
            DockSplit::Right,
            0.27,
            DockLayout::tabs(inspectors),
            DockLayout::split(
                DockSplit::Down,
                0.52,
                DockLayout::tabs(editors),
                DockLayout::tabs(self.viewers.values()),
            ),
        );
        let browsers: Vec<_> = self
            .panels
            .iter()
            .filter(|p| p.descriptor.placement == PanelPlacement::Browser)
            .map(|p| &p.key)
            .collect();
        self.layout = if browsers.is_empty() {
            layout
        } else {
            DockLayout::split(DockSplit::Left, 0.23, DockLayout::tabs(browsers), layout)
        };
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
        while let Some((project, result)) = self.store.take() {
            if project != self.workspace.project {
                continue;
            }
            match result {
                Ok(Some(workspace)) => {
                    for &id in self.workspace.viewers.keys() {
                        client.command(DesktopCommand::CloseViewer(id));
                    }
                    self.workspace = workspace;
                    self.workspace_menu.active = self.workspace.preset_name.clone();
                    self.bootstrap = false;
                    self.last_host_navigation = client.state().navigation_event;
                    self.editors.clear();
                    self.viewers.clear();
                    self.sync_instances();
                    self.rebuild_layout();
                    context.load_ini_settings(&self.workspace.layout);
                }
                Ok(None) => {}
                Err(error) => client.command(DesktopCommand::Notify(format!("Workspace: {error}"))),
            }
            self.restoring = false;
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
    pub fn external_drag(&mut self, position: Option<[f32; 2]>) {
        for panel in &mut self.panels {
            panel.panel.external_drag(position);
        }
        for editor in self.editors.values_mut() {
            editor.panels.editor.external_drag(position);
        }
    }
    pub fn files_dropped(
        &mut self,
        position: [f32; 2],
        paths: &[std::path::PathBuf],
        host: &mut dyn DesktopClient,
    ) {
        for &index in &self.visible_panels {
            let panel = &mut self.panels[index];
            let id = self
                .workspace
                .panels
                .iter()
                .find(|(_, contribution)| contribution.as_str() == panel.descriptor.id)
                .map(|(&id, _)| id)
                .unwrap();
            let mut context = PanelContext::for_panel(host, id);
            if panel.panel.files_dropped(position, paths, &mut context) {
                return;
            }
        }
        for &id in &self.visible_editors {
            if let Some(editor) = self.editors.get_mut(&id) {
                let mut context =
                    PanelContext::new(host, self.workspace.editors.get_mut(&id).unwrap())
                        .instance(id);
                if editor
                    .panels
                    .editor
                    .files_dropped(position, paths, &mut context)
                {
                    return;
                }
            }
        }
        host.command(DesktopCommand::Notify(
            "Import files into a Project bin before placing them in an editor".into(),
        ));
    }
    pub fn accepts_background_pan(&self, position: [f32; 2]) -> bool {
        self.visible_panels
            .iter()
            .any(|&i| self.panels[i].panel.accepts_background_pan(position))
            || self.visible_editors.iter().any(|id| {
                self.editors
                    .get(id)
                    .is_some_and(|e| e.panels.editor.accepts_background_pan(position))
            })
    }
    #[cfg(feature = "native-probe")]
    pub fn probe_workspace_ready(&self) -> bool {
        !self.restoring
    }
    #[cfg(feature = "native-probe")]
    pub fn probe_add_viewer(
        &mut self,
        output: fold_platform::workspace::DocumentRef,
    ) -> PanelInstanceId {
        let id = self.workspace.add_viewer(ViewerInstance {
            binding: ViewerBinding::Pinned(output.clone()),
            last_output: Some(output),
            divisor: 1,
            ..Default::default()
        });
        self.sync_instances();
        self.rebuild_layout();
        id
    }
    #[cfg(feature = "native-probe")]
    pub fn probe_full_quality(&mut self) {
        // Diagnostic only: sustained 1080p checkpoints must not accidentally
        // measure a smaller viewport after automatic preview sizing.
        self.probe_full_resolution = true;
        for viewer in self.workspace.viewers.values_mut() {
            viewer.divisor = 1;
        }
    }
    #[cfg(feature = "native-probe")]
    pub fn probe_time(&mut self, client: &mut dyn DesktopClient) {
        if self.workspace.viewers.len() == 1
            && let Some((&id, viewer)) = self.workspace.viewers.iter_mut().next()
        {
            let time = Time::new(
                i64::from(client.state().frame) * i64::from(client.state().rate[1]),
                client.state().rate[0],
            )
            .unwrap_or(Time::ZERO);
            if viewer.time != time {
                viewer.time = time;
                self.submit_viewer(id, client);
            }
        }
    }
    pub fn presentation(
        &mut self,
        id: PanelInstanceId,
        key: Option<PreviewKey>,
        error: Option<String>,
        client: &dyn DesktopClient,
    ) {
        self.presentation
            .retain(|id, _| self.workspace.viewers.contains_key(id));
        self.presentation.insert(id, (key, error));
        if let Some(transport) = client.viewer_transport(id)
            && let Some(viewer) = self.workspace.viewers.get_mut(&id)
        {
            viewer.time = transport.time;
            viewer.playing = transport.playing;
        }
    }
    pub fn take_review_actions(&mut self) -> Vec<(PanelInstanceId, crate::review::Action)> {
        std::mem::take(&mut self.review_actions)
    }
    pub fn review_state(&mut self, id: PanelInstanceId, state: (Option<String>, bool)) {
        self.review_states
            .retain(|id, _| self.workspace.viewers.contains_key(id));
        self.review_states.insert(id, state);
    }
    fn spatial_key(
        &self,
        id: PanelInstanceId,
        mut key: PreviewKey,
        divisor: u32,
    ) -> Option<PreviewKey> {
        #[cfg(feature = "native-probe")]
        if self.probe_full_resolution {
            return Some(key);
        }
        let default = crate::viewer_navigation::Navigation::default();
        let navigation = self.navigation.get(&id).unwrap_or(&default);
        let (dimensions, region) = navigation.demand(
            key.dimensions,
            self.viewport_pixels.get(&id).copied().unwrap_or([1.; 2]),
            self.viewport_scale.get(&id).copied().unwrap_or([1.; 2]),
            divisor,
        )?;
        key.dimensions = dimensions;
        key.region = region;
        Some(key)
    }
    pub fn keys(&self, client: &dyn DesktopClient) -> Vec<(PanelInstanceId, Option<PreviewKey>)> {
        self.workspace
            .viewers
            .iter()
            .map(|(&id, viewer)| {
                let key = self.workspace.resolve(id).and_then(|output| {
                    let time = client
                        .viewer_request(id)
                        .map_or(viewer.time, |request| request.1);
                    let state = client.preview_state(&output, time);
                    self.viewport_pixels.get(&id)?;
                    let mut key =
                        self.spatial_key(id, state.preview_key(state.frame, 1)?, viewer.divisor)?;
                    key.output = output.output;
                    key.channels = viewer.channels.clone();
                    Some(key)
                });
                (id, key)
            })
            .collect()
    }
    fn open_host_target(&mut self, client: &dyn DesktopClient) {
        let state = client.state();
        let target = state
            .navigation
            .last()
            .map(|v| v.document)
            .or(state.selection.document);
        if state.navigation_event == self.last_host_navigation || target.is_none() {
            return;
        }
        self.last_host_navigation = state.navigation_event;
        let Some(snapshot) = client.snapshot() else {
            return;
        };
        let Some(document) = target.and_then(|id| snapshot.state().documents.get(&id)) else {
            return;
        };
        let instance = self
            .workspace
            .focused_editor
            .filter(|id| {
                self.workspace
                    .editors
                    .get(id)
                    .is_some_and(|e| e.document_type == document.type_id)
            })
            .or_else(|| {
                self.workspace
                    .editors
                    .iter()
                    .find(|(_, e)| e.document_type == document.type_id)
                    .map(|(&id, _)| id)
            });
        if let Some(id) = instance {
            let editor = self.workspace.editors.get_mut(&id).unwrap();
            editor.bind(state.navigation.last().cloned().unwrap_or(ViewLocation {
                document: document.id,
                time: Time::ZERO,
                label: String::new(),
            }));
            editor.selection = state.selection.clone();
            editor.selection.document = Some(document.id);
            self.focus = Some(id);
            self.workspace.focused_editor = Some(id);
            self.workspace.record_selection(id);
        }
    }
    pub fn controls(
        &mut self,
        ui: &Ui,
        client: &mut dyn DesktopClient,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = client.state().clone();
        for (&id, viewer) in &mut self.workspace.viewers {
            if let Some(transport) = client.viewer_transport(id)
                && viewer.last_output.as_ref() == Some(&transport.output)
            {
                viewer.time = transport.time;
                viewer.playing = transport.playing;
            }
        }
        client.command(DesktopCommand::MonitorViewer(
            self.workspace.monitored_viewer,
        ));
        let output = client
            .output_info()
            .ok()
            .map(|(id, info)| (id, info.frames));
        if self.delivery_output != output {
            self.start = 0;
            self.end = output.map_or(0, |(_, frames)| frames as i32);
            self.delivery_output = output;
        }
        if !ui.io().want_text_input()
            && !ui.is_popup_open_with_flags("", dear_imgui_rs::PopupQueryFlags::ANY_POPUP)
            && !state.transient
            && ui.io().key_ctrl()
            && ui.is_key_pressed(Key::Z)
            && if ui.io().key_shift() {
                state.can_redo
            } else {
                state.can_undo
            }
        {
            client.command(if ui.io().key_shift() {
                DesktopCommand::Redo
            } else {
                DesktopCommand::Undo
            });
        }
        let initial = self.bootstrap && !self.restoring;
        if !self.restoring {
            // The only automatic first-document boundary is initial/legacy workspace
            // creation. Once bound, deletion never picks a replacement document.
            if self.bootstrap
                && let Some(snapshot) = client.snapshot()
                && !snapshot.state().documents.is_empty()
            {
                for editor in self.workspace.editors.values_mut() {
                    if editor.navigation.is_empty()
                        && let Some(document) = snapshot
                            .state()
                            .documents
                            .values()
                            .find(|d| d.type_id == editor.document_type)
                    {
                        editor.bind(ViewLocation {
                            document: document.id,
                            time: Time::ZERO,
                            label: String::new(),
                        });
                    }
                }
                self.bootstrap = false;
            }
            self.open_host_target(client);
            if initial && !self.bootstrap {
                let target = state
                    .navigation
                    .last()
                    .map(|v| v.document)
                    .or_else(|| self.delivery_output.map(|(document, _)| document));
                let editor = target.and_then(|document| {
                    self.workspace
                        .editors
                        .iter()
                        .find(|(_, e)| e.document() == Some(document))
                        .map(|(&id, _)| id)
                });
                if let Some(editor) = editor {
                    self.workspace.record_selection(editor);
                    if let Some(&viewer) = self.workspace.viewers.keys().next() {
                        self.workspace.follow_source(viewer, editor);
                    }
                } else if let Some(document) = target
                    && let Some(viewer) = self.workspace.viewers.values_mut().next()
                {
                    viewer.binding = ViewerBinding::Pinned(workspace::DocumentRef {
                        document,
                        output: "video".into(),
                        extensions: Default::default(),
                    });
                    viewer.editor = None;
                }
            }
        }
        let mut duplicate = None;
        let mut new_editor = None;
        let mut add_viewer = false;
        let mut reveal_animation = false;
        let mut reveal_network = false;
        ui.main_menu_bar(|| {
            let recent = self
                .settings
                .as_ref()
                .map(|s| s.preferences.recent.as_slice())
                .unwrap_or(&[]);
            self.project_menu.menus(ui, client, recent);
            self.workspace_menu.draw(ui, self.settings.as_ref());
            if let Some(_menu) = ui.begin_menu("Panels") {
                for panel in self
                    .panels
                    .iter()
                    .filter(|p| p.descriptor.placement == PanelPlacement::Editor)
                {
                    if panel.descriptor.id == crate::sdk::network::PANEL_ID {
                        reveal_network |= ui.menu_item("Network");
                        continue;
                    }
                    if panel.descriptor.id == crate::sdk::animation_editor::PANEL_ID {
                        reveal_animation |= ui.menu_item("Animation");
                        continue;
                    }
                    if ui.menu_item(format!("New {} editor", panel.descriptor.title)) {
                        new_editor = panel
                            .panel
                            .document_type()
                            .map(|kind| (panel.descriptor.id, kind));
                    }
                }
                for (&id, editor) in &self.workspace.editors {
                    let title = self
                        .panels
                        .iter()
                        .find(|p| p.descriptor.id == editor.contribution)
                        .map(|p| p.descriptor.title)
                        .unwrap_or("Unavailable editor");
                    if ui.menu_item(format!("Duplicate {title} ({})", id.0)) {
                        duplicate = Some(id);
                    }
                }
                if ui.menu_item("New viewer") {
                    add_viewer = true;
                }
            }
            if let Some(_menu) = ui.begin_menu("Fold") {
                if ui.menu_item("Settings…")
                    && let Some(settings) = &mut self.settings
                {
                    settings.show();
                }
            }
        });
        self.project_menu.confirmation(ui, client);
        self.workspace_menu.dialog(ui, self.settings.as_ref());
        if let Some(settings) = &mut self.settings {
            if ui.is_key_down(Key::ModCtrl) && ui.is_key_pressed(Key::Comma) {
                settings.show();
            }
            settings.draw(ui);
        }
        if self.workspace.panels.len() + self.workspace.editors.len() + self.workspace.viewers.len()
            < 64
        {
            if let Some((contribution, kind)) = new_editor {
                let id = self.workspace.add_editor(contribution, kind);
                self.focus = Some(id);
                self.sync_instances();
                self.rebuild_layout();
            }
            if let Some(id) = duplicate {
                let mut editor = self.workspace.editors[&id].clone();
                let new = self
                    .workspace
                    .add_editor(&editor.contribution, &editor.document_type);
                editor.group = self.workspace.editors[&new].group;
                self.workspace.editors.insert(new, editor);
                self.focus = Some(new);
                self.sync_instances();
                self.rebuild_layout();
            }
            if add_viewer {
                self.workspace.add_viewer(ViewerInstance::default());
                self.sync_instances();
                self.rebuild_layout();
            }
        }
        ui.dockspace()
            .layout(
                &self.layout,
                if self.reset_layout {
                    DockLayoutApply::Replace
                } else {
                    DockLayoutApply::IfMissing
                },
            )
            .build()?;
        self.reset_layout = false;
        let labels = client
            .snapshot()
            .map(|s| workspace::document_labels(&s))
            .unwrap_or_default();
        self.visible_editors.clear();
        let edit_times: BTreeMap<_, _> = self
            .workspace
            .editors
            .keys()
            .filter_map(|&id| {
                self.workspace
                    .viewer_for_editor(id)
                    .ok()
                    .map(|viewer| (id, self.workspace.viewers[&viewer].time))
            })
            .collect();
        let mut seeks = Vec::new();
        let mut close_editor = None;
        let mut group_changes = Vec::new();
        let mut publish = Vec::new();
        let memberships: Vec<_> = self
            .workspace
            .editors
            .iter()
            .map(|(&id, e)| (id, e.group, e.contribution.clone()))
            .collect();
        for (&id, binding) in &mut self.workspace.editors {
            if self
                .editors
                .get(&id)
                .is_some_and(|e| e.panels.editor.network_primary())
            {
                continue;
            }
            let choices = client
                .snapshot()
                .map(|s| {
                    s.state()
                        .documents
                        .values()
                        .filter(|d| d.type_id == binding.document_type)
                        .map(|d| Choice {
                            target: d.id,
                            label: labels[&d.id].clone(),
                            error: (!client.supports_document(d))
                                .then(|| "Unavailable provider".into()),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let label = breadcrumb_at(
                binding,
                &labels,
                edit_times
                    .get(&id)
                    .copied()
                    .or_else(|| binding.navigation.last().map(|v| v.time))
                    .unwrap_or(Time::ZERO),
            );
            let key = self
                .editors
                .get(&id)
                .map(|e| e.key.clone())
                .unwrap_or_else(|| {
                    WindowKey::new(format!("fold.editor.{}", id.0), "Unavailable editor").unwrap()
                });
            let visible = ui.window(&key).focused(self.focus == Some(id)).build(|| {
                if ui.is_window_focused() { self.workspace.focused_editor = Some(id); }
                if let Some(group) = group_selector::draw_choices(ui, binding.group, |group| group == workspace::LinkGroup::Unlinked || !memberships.iter().any(|(other, occupied, contribution)| *other != id && *occupied == group && *contribution == binding.contribution)) { group_changes.push((id, group)); }
                ui.same_line();
                if let Some(action) = target_selector::draw(ui, &label, binding.navigation.len() > 1, &choices) {
                    client.command(DesktopCommand::CancelPreviewEdit);
                    publish.push(id);
                    match action {
                        Action::Choose(document) => binding.bind(ViewLocation { document, time: Time::ZERO, label: String::new() }),
                        Action::Back => {
                            binding.back();
                            if let Some(snapshot) = client.snapshot() && let Some(document) = binding.document().and_then(|d| snapshot.state().documents.get(&d)) { binding.document_type = document.type_id.clone(); }
                        }
                    }
                }
                if let Some(_popup) = ui.begin_popup_context_window() {
                    if ui.menu_item("Close editor") { close_editor = Some(id); }
                }
                if let Some(editor) = self.editors.get_mut(&id)
                    && editor.panels.editor.document_type() == Some(binding.document_type.as_str())
                {
                    let mut scoped = binding.clone();
                    if let Some(time) = edit_times.get(&id) && let Some(location) = scoped.navigation.last_mut() { location.time = *time; }
                    let navigation = scoped.navigation.clone();
                    let mut context = PanelContext::new(client, &mut scoped).instance(id);
                    editor.panels.editor.draw(ExtensionUi { ui, host: &mut context });
                    if let Some(time) = context.seek_request() { seeks.push((id, time)); }
                    if context.selection_requested() { publish.push(id); }
                    drop(context);
                    binding.selection = scoped.selection.clone();
                    if scoped.navigation != navigation { publish.push(id); binding.navigation = scoped.navigation; binding.document_type = scoped.document_type; binding.mapped_navigation = scoped.mapped_navigation; }
                } else { ui.text_wrapped("This editor contribution is unavailable. Its binding has been retained."); }
            }).is_some();
            if visible {
                self.visible_editors.push(id);
            }
        }
        for (id, group) in group_changes {
            self.workspace.set_editor_group(id, group);
        }
        for id in publish {
            self.workspace.record_selection(id);
        }
        for (editor, time) in seeks {
            self.seek_from_editor(editor, time, client);
        }
        reveal_network |= self
            .focus
            .and_then(|id| self.editors.get(&id))
            .is_some_and(|e| e.panels.editor.network_primary());
        reveal_network |= self
            .editors
            .values_mut()
            .any(|e| e.panels.editor.take_network_request());
        self.focus = None;
        if let Some(id) = close_editor {
            if let Some(editor) = self.editors.get_mut(&id) {
                let mut context =
                    PanelContext::new(client, self.workspace.editors.get_mut(&id).unwrap())
                        .instance(id);
                editor.panels.editor.cancel_interaction(&mut context);
            }
            self.workspace.close_editor(id);
            self.sync_instances();
            self.rebuild_layout();
        }
        self.refresh_viewer_targets(client);
        self.visible_panels.clear();
        let mut publish = Vec::new();
        let mut seeks = Vec::new();
        for (index, registered) in self.panels.iter_mut().enumerate() {
            if registered.descriptor.placement == PanelPlacement::Editor
                && registered.panel.document_type().is_some()
            {
                continue;
            }
            let visible = ui
                .window(&registered.key)
                .focused(
                    (reveal_animation
                        && registered.descriptor.id == crate::sdk::animation_editor::PANEL_ID)
                        || (reveal_network
                            && registered.descriptor.id == crate::sdk::network::PANEL_ID),
                )
                .build(|| {
                    if registered.descriptor.placement == PanelPlacement::Inspector
                        || registered.descriptor.id == crate::sdk::animation_editor::PANEL_ID
                        || registered.descriptor.id == crate::sdk::network::PANEL_ID
                    {
                        if let Some(group) =
                            group_selector::draw(ui, self.workspace.inspector_group)
                        {
                            self.workspace.inspector_group = group;
                            self.workspace.inspector_lock = None;
                        }
                        ui.same_line();
                        let locked = self.workspace.inspector_lock.is_some();
                        if crate::sdk::toolbar::icon_button(
                            ui,
                            "inspector-lock",
                            if locked {
                                crate::sdk::toolbar::ToolbarIcon::Lock
                            } else {
                                crate::sdk::toolbar::ToolbarIcon::Unlock
                            },
                            if locked {
                                "Unlock inspected selection"
                            } else {
                                "Lock inspected selection"
                            },
                        ) {
                            self.workspace.toggle_inspector_lock();
                        }
                        if registered.descriptor.id == crate::sdk::network::PANEL_ID {
                            ui.same_line();
                            let current = self.workspace.inspector_editor();
                            let context_name = |binding: &EditorInstance| {
                                binding
                                    .document()
                                    .and_then(|id| labels.get(&id))
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        binding
                                            .document_type
                                            .rsplit('.')
                                            .next()
                                            .unwrap_or("Network")
                                            .to_owned()
                                    })
                            };
                            let label = current
                                .and_then(|id| self.workspace.editors.get(&id))
                                .map(&context_name)
                                .unwrap_or("Choose context".into());
                            let mut choose = None;
                            ui.set_next_item_width(
                                (ui.content_region_avail()[0] - ui.frame_height() - 12.).max(32.),
                            );
                            if let Some(_combo) = ui.begin_combo("##network-context", &label) {
                                for (&id, binding) in &self.workspace.editors {
                                    if self
                                        .editors
                                        .get(&id)
                                        .is_some_and(|e| e.panels.editor.supports_network())
                                    {
                                        let title = context_name(binding);
                                        if ui.selectable(format!("{title}##network-{}", id.0)) {
                                            choose = Some(id);
                                        }
                                    }
                                }
                            }
                            if let Some(id) = choose {
                                self.workspace.inspector_lock = None;
                                self.workspace.inspector_group = self.workspace.editors[&id].group;
                                self.workspace.focused_editor = Some(id);
                                self.workspace.record_selection(id);
                            }
                        }
                        if registered.descriptor.id == crate::sdk::network::PANEL_ID
                            && self.workspace.inspector_lock.is_none()
                            && let Some(id) = self.workspace.inspector_editor()
                            && let Some(binding) = self.workspace.editors.get_mut(&id)
                            && binding.navigation.len() > 1
                        {
                            ui.same_line();
                            if crate::sdk::toolbar::icon_button(
                                ui,
                                "network-back",
                                crate::sdk::toolbar::ToolbarIcon::Back,
                                "Return to parent document",
                            ) {
                                binding.back();
                                if let Some(snapshot) = client.snapshot()
                                    && let Some(document) = binding
                                        .document()
                                        .and_then(|id| snapshot.state().documents.get(&id))
                                {
                                    binding.document_type = document.type_id.clone();
                                }
                                publish.push(id);
                            }
                        }
                        let locked_binding = self
                            .workspace
                            .inspector_lock
                            .as_ref()
                            .map(|(_, binding)| binding.clone());
                        let inspector_context = self.workspace.inspector_context();
                        if let Some(id) = self.workspace.inspector_editor()
                            && let Some(binding) = self.workspace.editors.get_mut(&id)
                            && registered.panel.supports_document_type(
                                &locked_binding.as_ref().unwrap_or(binding).document_type,
                            )
                            && let Some(editor) = self.editors.get_mut(&id)
                        {
                            if editor.panels.editor.supports_animation()
                                && inspector_context.is_none()
                                && registered.descriptor.id != crate::sdk::network::PANEL_ID
                            {
                                ui.text_wrapped(
                                    "Select a linked viewer to choose the animation edit time.",
                                );
                                return;
                            }
                            let scope = format!(
                                "editor-{}-viewer-{:?}",
                                id.0, self.workspace.inspector_viewer
                            );
                            let _scope = ui.push_id(&scope);
                            let mut scoped =
                                locked_binding.clone().unwrap_or_else(|| binding.clone());
                            if let Some((owner, time)) = inspector_context
                                && owner == id
                                && let Some(location) = scoped.navigation.last_mut()
                            {
                                location.time = time;
                            }
                            let navigation = scoped.navigation.clone();
                            let mut context = PanelContext::new(client, &mut scoped).instance(id);
                            if registered.descriptor.id == crate::sdk::network::PANEL_ID {
                                self.visible_editors.push(id);
                                if ui.is_window_focused() {
                                    self.workspace.focused_editor = Some(id);
                                }
                                editor.panels.editor.draw_network(ExtensionUi {
                                    ui,
                                    host: &mut context,
                                });
                            } else if registered.descriptor.id
                                == crate::sdk::animation_editor::PANEL_ID
                            {
                                editor.panels.editor.draw_animation(ExtensionUi {
                                    ui,
                                    host: &mut context,
                                });
                            } else {
                                editor.panels.inspector.draw(ExtensionUi {
                                    ui,
                                    host: &mut context,
                                });
                            }
                            if let Some(time) = context.seek_request() {
                                seeks.push((id, time));
                            }
                            if context.selection_requested() {
                                publish.push(id);
                            }
                            drop(context);
                            if locked_binding.is_some() {
                                self.workspace.inspector_lock = Some((id, scoped.clone()));
                            } else {
                                binding.selection = scoped.selection.clone();
                            }
                            if locked_binding.is_none() && scoped.navigation != navigation {
                                binding.navigation = scoped.navigation;
                                binding.document_type = scoped.document_type;
                                binding.mapped_navigation = scoped.mapped_navigation;
                                publish.push(id);
                            }
                        } else {
                            ui.text_disabled("Select an object in an editor in this group.");
                        }
                    } else {
                        let id = self
                            .workspace
                            .panels
                            .iter()
                            .find(|(_, contribution)| {
                                contribution.as_str() == registered.descriptor.id
                            })
                            .map(|(&id, _)| id)
                            .unwrap();
                        let mut context = PanelContext::for_panel(client, id);
                        registered.panel.draw(ExtensionUi {
                            ui,
                            host: &mut context,
                        });
                    }
                })
                .is_some();
            if visible {
                self.visible_panels.push(index);
            }
        }
        for id in publish {
            self.workspace.record_selection(id);
        }
        for (editor, time) in seeks {
            if let Some((owner, locked)) = &mut self.workspace.inspector_lock
                && *owner == editor
                && self
                    .workspace
                    .editors
                    .get(&editor)
                    .and_then(EditorInstance::document)
                    != locked.document()
            {
                if let Some(location) = locked.navigation.last_mut() {
                    location.time = time;
                }
            } else {
                self.seek_from_editor(editor, time, client);
            }
        }
        self.refresh_viewer_targets(client);
        for (&id, contribution) in &self.workspace.panels {
            if matches!(
                contribution.as_str(),
                "fold.ui.delivery" | "fold.ui.empty-editors"
            ) || self.panels.iter().any(|p| p.descriptor.id == contribution)
            {
                continue;
            }
            let key = WindowKey::new(format!("fold.panel.{}", id.0), "Unavailable panel").unwrap();
            ui.window(&key).build(|| {
                ui.text_wrapped("This panel contribution is unavailable. Its workspace identity has been retained.");
                if ui.is_window_hovered() { ui.tooltip_text(contribution); }
            });
        }
        if self.workspace.editors.is_empty() {
            ui.window(&self.empty)
                .build(|| ui.text_wrapped("Open or create a document in Project."));
        }
        ui.window(&self.delivery).build(|| {
            if let Some((document, frames)) = self.delivery_output {
                ui.text_wrapped(format!(
                    "Delivery: {} · {frames} frames",
                    labels
                        .get(&document)
                        .map(String::as_str)
                        .unwrap_or("Missing output")
                ));
            } else {
                ui.text_disabled("No valid delivery output selected");
            }
            let viewed = self
                .workspace
                .viewers
                .keys()
                .next()
                .and_then(|&id| self.workspace.resolve(id));
            if let Some(output) = viewed
                && output.output == "video"
                && ui.button("Use first viewer as delivery output")
            {
                client.command(DesktopCommand::SetOutput(output.document));
            }
            if let (Some(snapshot), Some((document, _))) = (client.snapshot(), self.delivery_output)
                && fold_platform::color::project(&snapshot)
                    .ok()
                    .flatten()
                    .is_some()
            {
                match fold_platform::color::output(&snapshot, document) {
                    Ok(mut transform) => {
                        if crate::sdk::color_controls::output(
                            ui,
                            &state.color_choices,
                            &mut transform,
                        ) {
                            client.command(DesktopCommand::SetOutputColor {
                                document,
                                transform,
                            });
                        }
                    }
                    Err(error) => ui.text_wrapped(error),
                }
            }
            ui.input_text("New MP4 output", &mut self.export_path)
                .build();
            ui.input_int("Start frame", &mut self.start);
            ui.input_int("End (exclusive)", &mut self.end);
            if ui.button("Export range") && self.start >= 0 && self.end > self.start {
                client.command(DesktopCommand::Export {
                    path: self.export_path.clone().into(),
                    start: self.start as u32,
                    end: self.end as u32,
                });
            }
            ui.same_line();
            if ui.button("Cancel job") {
                client.command(DesktopCommand::Cancel);
            }
            ui.text_wrapped(&state.status);
        });
        Ok(())
    }
    fn refresh_viewer_targets(&mut self, client: &mut dyn DesktopClient) {
        let targets: Vec<_> = self
            .workspace
            .viewers
            .keys()
            .map(|&id| {
                (
                    id,
                    self.workspace.resolve(id),
                    self.workspace.editor_for_viewer(id),
                )
            })
            .collect();
        for (id, output, source_editor) in targets {
            let viewer = self.workspace.viewers.get_mut(&id).unwrap();
            let same_editor = source_editor.is_some() && source_editor == viewer.last_editor;
            viewer.last_editor = source_editor;
            if output == viewer.last_output {
                if output.is_some() && client.viewer_transport(id).is_none() {
                    self.submit_viewer(id, client);
                }
                continue;
            }
            viewer.playing = false;
            viewer.range = Default::default();
            if same_editor
                && viewer.binding == ViewerBinding::Linked
                && let Some(location) = source_editor
                    .and_then(|id| self.workspace.editors.get(&id))
                    .filter(|e| e.mapped_navigation)
                    .and_then(|e| e.navigation.last())
            {
                viewer.time = location.time;
            }
            if let Some(output) = &output
                && let Some(info) = client
                    .outputs(output.document)
                    .into_iter()
                    .find(|o| o.reference.output == output.output)
                    .and_then(|o| o.info.ok())
                && viewer.time >= info.time(info.frames).unwrap_or(Time::ZERO)
            {
                viewer.time = info
                    .time(info.frames.saturating_sub(1))
                    .unwrap_or(Time::ZERO);
            }
            viewer.last_output = output;
            self.submit_viewer(id, client);
        }
        self.workspace.reconcile();
    }
    fn playback_mode(
        &self,
        id: PanelInstanceId,
        client: &dyn DesktopClient,
    ) -> fold_platform::desktop::PlaybackMode {
        self.workspace.viewers[&id]
            .playback_mode
            .unwrap_or_else(|| {
                self.workspace
                    .resolve(id)
                    .and_then(|output| {
                        client
                            .outputs(output.document)
                            .into_iter()
                            .find(|descriptor| descriptor.reference.output == output.output)
                            .map(|descriptor| descriptor.playback_mode)
                    })
                    .unwrap_or_default()
            })
    }
    fn playback_mode_items(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        client: &mut dyn DesktopClient,
    ) {
        use fold_platform::desktop::PlaybackMode;
        let selected = self.workspace.viewers[&id].playback_mode;
        for (label, mode) in [
            ("Use source default", None),
            ("Real-time", Some(PlaybackMode::RealTime)),
            ("Every-frame (muted)", Some(PlaybackMode::EveryFrame)),
        ] {
            if ui.menu_item_enabled_selected_no_shortcut(label, selected == mode, true) {
                self.workspace.viewers.get_mut(&id).unwrap().playback_mode = mode;
                self.submit_viewer(id, client);
            }
        }
    }
    fn review_menu_items(&mut self, ui: &Ui, id: PanelInstanceId, client: &dyn DesktopClient) {
        let viewer = &self.workspace.viewers[&id];
        if ui.menu_item_enabled_selected_no_shortcut("Prepare marked range", false, !viewer.playing)
        {
            self.review_actions
                .push((id, crate::review::Action::Prepare));
        }
        let ready = self.review_states.get(&id).is_some_and(|state| state.1);
        let realtime =
            self.playback_mode(id, client) == fold_platform::desktop::PlaybackMode::RealTime;
        if ui.menu_item_enabled_selected_no_shortcut(
            "Play cached Real-time preview",
            false,
            ready && realtime,
        ) {
            self.review_actions
                .push((id, crate::review::Action::PlayCached));
        }
        if !realtime {
            ui.text_disabled("Choose Real-time playback to replay cached frames");
        }
        if ui.menu_item("Cancel preparation") {
            self.review_actions
                .push((id, crate::review::Action::Cancel));
        }
    }
    fn submit_viewer(&self, id: PanelInstanceId, client: &mut dyn DesktopClient) {
        if let Some(output) = self.workspace.resolve(id) {
            let viewer = &self.workspace.viewers[&id];
            client.command(DesktopCommand::ViewerTransport {
                viewer: id,
                transport: fold_platform::desktop::ViewerTransport {
                    mode: self.playback_mode(id, client),
                    output,
                    time: viewer.time,
                    range: viewer.range,
                    looping: viewer.looping,
                    playing: viewer.playing,
                },
            });
        } else {
            client.command(DesktopCommand::CloseViewer(id));
        }
    }
    fn seek_from_editor(
        &mut self,
        editor: PanelInstanceId,
        time: Time,
        client: &mut dyn DesktopClient,
    ) {
        match self.workspace.viewer_for_editor(editor) {
            Ok(viewer) => {
                let binding = self.workspace.viewers.get_mut(&viewer).unwrap();
                binding.playing = false;
                binding.time = time;
                self.submit_viewer(viewer, client);
            }
            Err(error) => {
                let has_viewer = self
                    .workspace
                    .viewers
                    .keys()
                    .any(|id| self.workspace.editor_for_viewer(*id) == Some(editor));
                if !has_viewer
                    && let Some(location) = self
                        .workspace
                        .editors
                        .get_mut(&editor)
                        .and_then(|e| e.navigation.last_mut())
                {
                    location.time = time;
                } else {
                    client.command(DesktopCommand::Notify(error));
                }
            }
        }
    }
    fn viewer_channel_selector(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        client: &dyn DesktopClient,
        compact: bool,
    ) {
        use fold_platform::desktop::{ChannelView, DisplayRange};
        let Some(output) = self.workspace.resolve(id) else {
            return;
        };
        ui.same_line();
        ui.set_next_item_width(if compact { 80. } else { 130. });
        let current = self.workspace.viewers[&id].channels.clone();
        if let Some(_combo) = ui.begin_combo("##channels", current.label()) {
            match client.channels(&output) {
                Ok(channels) => {
                    let layers: std::collections::BTreeSet<_> =
                        channels.iter().map(|name| name.layer()).collect();
                    for layer in layers {
                        if ["red", "green", "blue"].iter().all(|component| {
                            channels
                                .iter()
                                .any(|c| c.as_str() == format!("{layer}.{component}"))
                        }) && ui.selectable(format!("{layer} · RGB"))
                        {
                            self.workspace.viewers.get_mut(&id).unwrap().channels =
                                ChannelView::Rgb {
                                    layer: layer.to_owned(),
                                };
                        }
                    }
                    ui.separator();
                    for name in channels {
                        if ui.selectable(name.as_str()) {
                            self.workspace.viewers.get_mut(&id).unwrap().channels =
                                ChannelView::Channel {
                                    name,
                                    range: DisplayRange::default(),
                                };
                        }
                    }
                }
                Err(error) => ui.text_wrapped(error),
            }
            if let ChannelView::Channel { range, .. } =
                &mut self.workspace.viewers.get_mut(&id).unwrap().channels
            {
                ui.separator();
                let mut values = range.values();
                let black = crate::sdk::imgui::Drag::new("Black")
                    .speed(0.01)
                    .build(ui, &mut values[0]);
                let white = crate::sdk::imgui::Drag::new("White")
                    .speed(0.01)
                    .build(ui, &mut values[1]);
                if (black || white)
                    && let Ok(value) = DisplayRange::new(values[0], values[1])
                {
                    *range = value;
                }
            }
        }
        crate::sdk::toolbar::tooltip(
            ui,
            &format!(
                "{} — viewer layer/channel; data uses the selected grayscale range",
                current.label()
            ),
        );
    }
    fn viewer_header(&mut self, ui: &Ui, id: PanelInstanceId, client: &mut dyn DesktopClient) {
        let width = ui.content_region_avail()[0];
        let compact = width < 540.;
        let labels = client
            .snapshot()
            .map(|s| workspace::document_labels(&s))
            .unwrap_or_default();
        let fit_width = ui.calc_text_size("Fit")[0]
            + ui.clone_style().frame_padding()[0] * 2.
            + ui.clone_style().item_spacing()[0];
        let reserved = fit_width
            + if compact {
                (ui.frame_height() + 4.)
                    .max(ui.calc_text_size("1/2")[0] + ui.clone_style().frame_padding()[0] * 2.)
                    + ui.clone_style().item_spacing()[0]
            } else {
                190.
            };
        let source_changed = crate::viewer_source::draw(
            ui,
            &mut self.workspace,
            id,
            &labels,
            reserved,
            |document| client.outputs(document),
        );
        if compact {
            ui.same_line();
            let divisor = self.workspace.viewers[&id].divisor;
            let popup = if divisor > 1 {
                let label = if divisor == 2 {
                    "1/2##viewer-options"
                } else {
                    "1/4##viewer-options"
                };
                if ui.button(label) {
                    ui.open_popup("viewer-options");
                }
                crate::sdk::toolbar::tooltip(
                    ui,
                    "Preview resolution — channels and playback options",
                );
                ui.begin_popup("viewer-options")
            } else {
                crate::sdk::toolbar::icon_menu(
                    ui,
                    "viewer-options",
                    "Full resolution — channels and playback options",
                )
            };
            if let Some(_popup) = popup {
                ui.text_disabled("Channels");
                self.viewer_channel_selector(ui, id, client, false);
                ui.separator();
                self.playback_mode_items(ui, id, client);
            }
        } else {
            self.viewer_channel_selector(ui, id, client, false);
            if self.workspace.viewers[&id].divisor > 1 {
                ui.same_line();
                ui.text_disabled(if self.workspace.viewers[&id].divisor == 2 {
                    "1/2"
                } else {
                    "1/4"
                });
                crate::sdk::toolbar::tooltip(ui, "Preview resolution");
            }
        }
        if source_changed {
            self.refresh_viewer_targets(client);
        }
        ui.same_line();
        if ui.button("Fit") {
            self.navigation.entry(id).or_default().fit();
        }
        crate::sdk::toolbar::tooltip(
            ui,
            "Fit image — F. Wheel to zoom; left or middle drag to pan.",
        );
    }
    #[cfg(test)]
    pub fn viewer(
        &mut self,
        ui: &Ui,
        preview: &Preview,
        statistics: &str,
        client: &mut dyn DesktopClient,
    ) {
        if let Some(&id) = self.workspace.viewers.keys().next() {
            self.viewer_instance(ui, id, preview, statistics, client);
        }
    }
    pub fn viewer_instance(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        preview: &Preview,
        statistics: &str,
        client: &mut dyn DesktopClient,
    ) {
        let Some(key) = self.viewers.get(&id).cloned() else {
            return;
        };
        let mut close = false;
        ui.window(&key).flags(dear_imgui_rs::WindowFlags::NO_SCROLLBAR | dear_imgui_rs::WindowFlags::NO_SCROLL_WITH_MOUSE).build(|| {
            self.viewer_header(ui, id, client);
            if ui.is_window_focused_with_flags(dear_imgui_rs::FocusedFlags::ROOT_AND_CHILD_WINDOWS) {
                self.workspace.inspector_viewer = Some(id);
                if let Some(editor) = self.workspace.editor_for_viewer(id) {
                    self.workspace.focused_editor = Some(editor);
                }
            }
            if ui.is_window_hovered_with_flags(dear_imgui_rs::WindowHoveredFlags::CHILD_WINDOWS)
                && ui.is_mouse_released(dear_imgui_rs::MouseButton::Right) {
                ui.open_popup("viewer-context");
            }
            if let Some(_popup) = ui.begin_popup("viewer-context") {
                if let Some(_menu) = ui.begin_menu("Display sampling") {
                    let pixel_exact = &mut self.workspace.viewers.get_mut(&id).unwrap().pixel_exact;
                    if ui.menu_item_enabled_selected_no_shortcut("Smooth", !*pixel_exact, true) { *pixel_exact = false; }
                    if ui.menu_item_enabled_selected_no_shortcut("Pixel exact", *pixel_exact, true) { *pixel_exact = true; }
                }
                for (label, divisor) in [("Full", 1), ("Half", 2), ("Quarter", 4)] {
                    if ui.selectable(label) {
                        self.workspace.viewers.get_mut(&id).unwrap().divisor = divisor;
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text("Resolution at the current viewer zoom in physical pixels, capped at document resolution.");
                    }
                }
                if let Some(output) = self.workspace.resolve(id) {
                    if let Some(_menu) = ui.begin_menu("Output") {
                        for descriptor in client.outputs(output.document) {
                            let _disabled = ui.begin_disabled_with_cond(descriptor.info.is_err());
                            if ui.menu_item(descriptor.label) {
                                self.workspace.pin_output(id, descriptor.reference);
                                self.refresh_viewer_targets(client);
                            }
                        }
                    }
                }
                if let Some(_menu) = ui.begin_menu("Playback mode") {
                    self.playback_mode_items(ui, id, client);
                }
                if let Some(_menu) = ui.begin_menu("Review cache") {
                    self.review_menu_items(ui, id, client);
                }
                let viewer = self.workspace.viewers.get_mut(&id).unwrap();
                if ui.checkbox("Loop playback", &mut viewer.looping) {
                    self.submit_viewer(id, client);
                }
                let mut monitored = self.workspace.monitored_viewer == Some(id);
                if ui.checkbox("Monitor audio", &mut monitored) {
                    self.workspace.monitored_viewer = monitored.then_some(id);
                    client.command(DesktopCommand::MonitorViewer(
                        self.workspace.monitored_viewer,
                    ));
                }
                if ui.menu_item("Close viewer") {
                    close = true;
                }
                if let Some(_menu) = ui.begin_menu("Diagnostics") {
                    ui.text_disabled(statistics);
                    ui.text_wrapped(client.resource_statistics());
                }
            }
            let origin = ui.cursor_pos();
            let available = ui.content_region_avail();
            let review_status = self.review_states.get(&id).and_then(|s| s.0.clone());
            let image_height = (available[1] - crate::transport::Transport::height(ui)).max(0.);
            let scale = ui.io().display_framebuffer_scale();
            self.viewport_scale.insert(id, scale);
            self.viewport_pixels.insert(
                id,
                [
                    available[0].max(1.) * scale[0],
                    image_height.max(1.) * scale[1],
                ],
            );
            let output = self.workspace.resolve(id);
            let wrong_source = self
                .presentation
                .get(&id)
                .and_then(|(key, _)| key.as_ref())
                .is_some_and(|key| {
                    output.as_ref().is_none_or(|output| {
                        key.target.map(|t| t.0) != Some(output.document)
                            || key.output != output.output
                    })
                });
            let preview = if output.is_none() || wrong_source {
                &Preview::Pending
            } else {
                preview
            };
            let mut messages = Vec::new();
            if let Some((_, Some(error))) = self.presentation.get(&id) {
                messages.push(format!("Preview failed — displayed image may be stale: {error}"));
            } else if let Preview::Failed(error) = preview {
                messages.push(format!("Preview failed: {error}"));
            }
            if let Some(error) = client.viewer_audio_error(id) {
                messages.push(format!("Audio: {error}"));
            }
            if let Some(output) = &output {
                let state = client.preview_state(output, self.workspace.viewers[&id].time);
                if state.content.is_none() && !state.status.is_empty() {
                    messages.push(state.status.clone());
                }
            }
            if let Some(status) = review_status { messages.push(status); }
            if image_height > 0. && available[0] > 0. {
                ui.child_window("viewer-canvas")
                    .size([available[0], image_height])
                    .flags(dear_imgui_rs::WindowFlags::NO_SCROLLBAR | dear_imgui_rs::WindowFlags::NO_SCROLL_WITH_MOUSE)
                    .build(ui, || {
                        let canvas_origin = ui.cursor_screen_pos();
                        let canvas_size = ui.content_region_avail();
                        let insets = self.workspace.editor_for_viewer(id)
                            .filter(|editor| self.workspace.editors.get(editor).and_then(|b| b.output()).as_ref() == output.as_ref())
                            .and_then(|editor| self.editors.get(&editor))
                            .map(|e| e.panels.editor.viewer_tool_insets(ui)).unwrap_or([0.; 2]);
                        let canvas = [canvas_origin[0] + insets[0], canvas_origin[1] + insets[1]];
                        let area = [canvas_size[0] - insets[0], canvas_size[1] - insets[1]];
                        if area[0] <= 0. || area[1] <= 0. { return; }
                        self.viewport_pixels.insert(id, [area[0]*scale[0], area[1]*scale[1]]);
                        let dimensions = output.as_ref().map(|output| client.preview_state(output, self.workspace.viewers[&id].time).dimensions)
                            .unwrap_or([1, 1]);
                        let fitted = fitted_size(dimensions, area);
                        let (image_origin, size) = self.navigation.entry(id).or_default().rect(canvas, area, fitted);
                        let rect = crate::sdk::ViewerRect { editable: true, image_current: matches!(preview, Preview::Ready { .. }), canvas_origin, canvas_size, origin: image_origin, size, dimensions };
                        if let Preview::Ready { texture, dimensions: tile_dimensions, uv_max } = preview {
                            debug_assert!(tile_dimensions.iter().all(|n| *n > 0));
                            // A held tile remains at its original image-space location
                            // while a new demand renders; never stretch it over the frame.
                            let presented = self.presentation.get(&id).and_then(|p| p.0.as_ref());
                            let mut start = image_origin;
                            let mut end = [image_origin[0] + size[0], image_origin[1] + size[1]];
                            if let Some(key) = presented && let Some(region) = key.region {
                                for i in 0..2 {
                                    let low = [region.x, region.y][i] as f32 / key.dimensions[i] as f32;
                                    let high = ([region.x, region.y][i] + region.dimensions()[i]) as f32 / key.dimensions[i] as f32;
                                    start[i] = image_origin[i] + low * size[i];
                                    end[i] = image_origin[i] + high * size[i];
                                }
                            }
                            let draw = ui.get_window_draw_list();
                            if self.workspace.viewers[&id].pixel_exact { draw.set_sampler_nearest(); }
                            draw.add_image(*texture, start, end, [0., 0.], *uv_max, [1.; 4]);
                            if self.workspace.viewers[&id].pixel_exact { draw.set_sampler_linear(); }
                            // Feature overlays acquire their own draw-list wrapper.
                            drop(draw);
                        }
                        self.viewer_overlay(ui, id, rect, client);
                        self.navigation.entry(id).or_default().input(ui, canvas, area, fitted);
                        if let Some(presented) = self.presentation.get(&id).and_then(|p| p.0.as_ref()) {
                            let mut expected = presented.clone(); expected.dimensions = dimensions; expected.region = None;
                            if self.spatial_key(id, expected, self.workspace.viewers[&id].divisor)
                                .is_some_and(|key| key.dimensions != presented.dimensions || key.region != presented.region) {
                                ui.get_window_draw_list().add_text([canvas[0] + 8., canvas[1] + area[1] - ui.text_line_height() - 8.],
                                    ui.style_color(dear_imgui_rs::StyleColor::Text), "Updating preview…");
                            }
                        }
                        if !messages.is_empty() {
                            let width = (area[0] - 16.).max(1.);
                            let message = messages.join("\n");
                            let height = ui.calc_text_size_with_opts(&message, false, width)[1];
                            let draw = ui.get_window_draw_list();
                            draw.add_rect(canvas, [canvas[0] + area[0], canvas[1] + height + 16.],
                                ui.style_color(dear_imgui_rs::StyleColor::WindowBg)).filled(true).build();
                            draw.add_text_with_font(ui.current_font(), ui.current_font_size(),
                                [canvas[0] + 8., canvas[1] + 8.], ui.style_color(dear_imgui_rs::StyleColor::Text),
                                message, width, None);
                        }
                    });
            }
            ui.set_cursor_pos([origin[0], origin[1] + image_height]);
            if let Some(output) = self.workspace.resolve(id) {
                let viewer = &self.workspace.viewers[&id];
                let mut binding = self
                    .workspace
                    .editor_for_viewer(id)
                    .and_then(|e| self.workspace.editors.get(&e).cloned())
                    .unwrap_or(EditorInstance {
                        group: viewer.group,
                        contribution: String::new(),
                        document_type: String::new(),
                        navigation: vec![],
                        selection: Default::default(),
                        mapped_navigation: false,
                    });
                binding.navigation = vec![ViewLocation {
                    document: output.document,
                    time: viewer.time,
                    label: String::new(),
                }];
                binding.selection.document = Some(output.document);
                let mut context = PanelContext::new(client, &mut binding)
                    .instance(id)
                    .with_output(&output);
                let before_time = viewer.time;
                let before_range = viewer.range;
                context.transport_context(viewer.playing, viewer.range);
                if ui.is_window_focused_with_flags(dear_imgui_rs::FocusedFlags::ROOT_AND_CHILD_WINDOWS) {
                    crate::transport::shortcuts(ui, &mut context);
                }
                self.transports
                    .entry(id)
                    .or_default()
                    .draw(ui, &mut context);
                let requested = context.playback_requested();
                let range = context.state().playback_range;
                drop(context);
                let viewer = self.workspace.viewers.get_mut(&id).unwrap();
                viewer.time = binding.navigation[0].time;
                viewer.range = range;
                if let Some(playing) = requested {
                    viewer.playing = playing;
                }
                if requested.is_some() || viewer.time != before_time || range != before_range {
                    self.submit_viewer(id, client);
                }
            } else {
                ui.dummy([0., 0.]);
            }
        });
        if close {
            if let Some(editor_id) = self.workspace.editor_for_viewer(id)
                && let Some(editor) = self.editors.get_mut(&editor_id)
                && let Some(binding) = self.workspace.editors.get_mut(&editor_id)
            {
                let mut context = PanelContext::new(client, binding).instance(id);
                editor.panels.editor.cancel_interaction(&mut context);
            }
            client.command(DesktopCommand::CloseViewer(id));
            if self.workspace.monitored_viewer == Some(id) {
                self.workspace.monitored_viewer = None;
            }
            self.workspace.viewers.remove(&id);
            self.viewport_pixels.remove(&id);
            self.viewport_scale.remove(&id);
            self.navigation.remove(&id);
            self.sync_instances();
            self.rebuild_layout();
        }
    }
    fn viewer_overlay(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        mut rect: crate::sdk::ViewerRect,
        client: &mut dyn DesktopClient,
    ) {
        let Some(viewer) = self.workspace.viewers.get(&id) else {
            return;
        };
        let Some(editor_id) = self.workspace.editor_for_viewer(id) else {
            return;
        };
        let Some(output) = self.workspace.resolve(id) else {
            return;
        };
        rect.editable = !viewer.playing;
        if let Some((presented, _)) = self.presentation.get(&id) {
            let state = client.preview_state(&output, viewer.time);
            let expected = state
                .preview_key(state.frame, 1)
                .and_then(|key| self.spatial_key(id, key, viewer.divisor))
                .map(|mut key| {
                    key.output = output.output.clone();
                    key.channels = viewer.channels.clone();
                    key
                });
            if presented.is_none() || *presented != expected {
                rect.image_current = false;
            }
        }
        let Some(binding) = self.workspace.editors.get(&editor_id) else {
            return;
        };
        if binding.output().as_ref() != Some(&output) {
            return;
        }
        let Some(editor) = self.editors.get_mut(&editor_id) else {
            return;
        };
        let mut binding = binding.clone();
        if let Some(location) = binding.navigation.last_mut() {
            location.time = viewer.time;
        }
        let selected = {
            let mut context = PanelContext::new(client, &mut binding).instance(id);
            editor.panels.editor.draw_viewer_overlay(
                ExtensionUi {
                    ui,
                    host: &mut context,
                },
                rect,
            );
            context.selection_requested()
        };
        self.workspace
            .editors
            .get_mut(&editor_id)
            .unwrap()
            .selection = binding.selection;
        if selected {
            self.workspace.record_selection(editor_id);
        }
    }
}
fn breadcrumb_at(
    editor: &EditorInstance,
    labels: &BTreeMap<DocumentId, String>,
    time: Time,
) -> String {
    if editor.navigation.is_empty() {
        return "Choose a document".into();
    }
    let path = editor
        .navigation
        .iter()
        .map(|v| {
            labels
                .get(&v.document)
                .map(String::as_str)
                .unwrap_or("Missing document")
        })
        .collect::<Vec<_>>()
        .join(" › ");
    format!("{path} · {}/{} s", time.numerator(), time.denominator())
}
#[cfg(test)]
#[path = "docking_tests.rs"]
mod docking_tests;
#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;

#[path = "shell_workspaces.rs"]
mod presets;
