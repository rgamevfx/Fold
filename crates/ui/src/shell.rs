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
    viewer: WindowKey,
    delivery: WindowKey,
    empty: WindowKey,
    panels: Vec<RegisteredPanel>,
    editors: BTreeMap<PanelInstanceId, RuntimeEditor>,
    viewers: BTreeMap<PanelInstanceId, WindowKey>,
    pub(crate) workspace: Workspace,
    layout: DockLayout,
    reset_layout: bool,
    project_path: String,
    export_path: String,
    transports: BTreeMap<PanelInstanceId, crate::transport::Transport>,
    start: i32,
    end: i32,
    delivery_output: Option<(DocumentId, u32)>,
    visible_panels: Vec<usize>,
    visible_editors: Vec<PanelInstanceId>,
    image_rects: BTreeMap<PanelInstanceId, crate::sdk::ViewerRect>,
    presentation: BTreeMap<PanelInstanceId, (Option<PreviewKey>, Option<String>)>,
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
            workspace.publish(first);
        }
        workspace.add_viewer(ViewerInstance::default());
        let viewer = WindowKey::new("fold.viewer.main", "Viewer").unwrap();
        let delivery = WindowKey::new("fold.delivery.main", "Delivery").unwrap();
        let empty = WindowKey::new("fold.editors.empty", "Editors").unwrap();
        let mut shell = Self {
            viewer,
            delivery,
            empty,
            panels,
            workspace,
            editors: Default::default(),
            viewers: Default::default(),
            layout: DockLayout::tabs(std::iter::empty::<&WindowKey>()),
            reset_layout: false,
            project_path: "/tmp/fold-project.fold".into(),
            export_path: "/tmp/fold-export.mp4".into(),
            transports: Default::default(),
            start: 0,
            end: 1,
            delivery_output: None,
            visible_panels: vec![],
            visible_editors: vec![],
            image_rects: Default::default(),
            presentation: Default::default(),
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
                    self.store.load(project);
                    self.bootstrap = true;
                    self.last_host_navigation = 0;
                    for editor in self.workspace.editors.values_mut() {
                        editor.navigation.clear();
                        editor.selection = Default::default();
                    }
                }
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
        self.sync_instances();
        for editor in self.editors.values_mut() {
            if !editor.initialized {
                editor.panels.editor.initialize(context);
                editor.panels.inspector.initialize(context);
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
    pub fn probe_full_quality(&mut self) {
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
                    let mut key = state.preview_key(state.frame, viewer.divisor)?;
                    key.output = output.output;
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
            self.workspace.publish(id);
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
        if !ui.io().want_text_input() && ui.io().key_ctrl() && ui.is_key_pressed(Key::Z) {
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
                    self.workspace.publish(editor);
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
        ui.main_menu_bar(|| {
            ui.text("Fold");
            if let Some(_menu) = ui.begin_menu("Panels") {
                for panel in self
                    .panels
                    .iter()
                    .filter(|p| p.descriptor.placement == PanelPlacement::Editor)
                {
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
                if ui.menu_item("Reset layout") {
                    self.reset_layout = true;
                }
            }
            ui.same_line();
            if ui.button("Undo") {
                client.command(DesktopCommand::Undo);
            }
            ui.same_line();
            if ui.button("Redo") {
                client.command(DesktopCommand::Redo);
            }
        });
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
                let editor = self.workspace.editors[&id].clone();
                let new = self
                    .workspace
                    .add_editor(&editor.contribution, &editor.document_type);
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
        for (&id, binding) in &mut self.workspace.editors {
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
                if let Some(group) = group_selector::draw(ui, binding.group, self.workspace.group_sources.get(&binding.group) == Some(&id)) { group_changes.push((id, group)); }
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
                    if ui.menu_item("Use as group source") { publish.push(id); }
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
            self.workspace.publish(id);
        }
        for (editor, time) in seeks {
            self.seek_from_editor(editor, time, client);
        }
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
                .build(|| {
                    if registered.descriptor.placement == PanelPlacement::Inspector {
                        if let Some(group) =
                            group_selector::draw(ui, self.workspace.inspector_group, false)
                        {
                            self.workspace.inspector_group = group;
                        }
                        let inspector_context = self.workspace.inspector_context();
                        if let Some(id) = self.workspace.inspector_editor()
                            && let Some(binding) = self.workspace.editors.get_mut(&id)
                            && registered
                                .panel
                                .supports_document_type(&binding.document_type)
                            && let Some(editor) = self.editors.get_mut(&id)
                        {
                            let scope = format!(
                                "editor-{}-viewer-{:?}",
                                id.0, self.workspace.inspector_viewer
                            );
                            let _scope = ui.push_id(&scope);
                            let mut scoped = binding.clone();
                            if let Some((owner, time)) = inspector_context
                                && owner == id
                                && let Some(location) = scoped.navigation.last_mut()
                            {
                                location.time = time;
                            }
                            let navigation = scoped.navigation.clone();
                            let mut context = PanelContext::new(client, &mut scoped).instance(id);
                            editor.panels.inspector.draw(ExtensionUi {
                                ui,
                                host: &mut context,
                            });
                            if let Some(time) = context.seek_request() {
                                seeks.push((id, time));
                            }
                            drop(context);
                            binding.selection = scoped.selection.clone();
                            if scoped.navigation != navigation {
                                binding.navigation = scoped.navigation;
                                binding.document_type = scoped.document_type;
                                binding.mapped_navigation = scoped.mapped_navigation;
                                publish.push(id);
                            }
                        } else {
                            ui.text_disabled("Choose a source editor for this group.");
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
            self.workspace.publish(id);
        }
        for (editor, time) in seeks {
            self.seek_from_editor(editor, time, client);
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
            ui.input_text("Project path", &mut self.project_path)
                .build();
            if ui.button("Save") {
                client.command(DesktopCommand::Save(self.project_path.clone().into()));
            }
            ui.same_line();
            if ui.button("Open") {
                client.command(DesktopCommand::Open(self.project_path.clone().into()));
            }
            ui.separator();
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
            Err(error) => client.command(DesktopCommand::Notify(error)),
        }
    }
    fn viewer_header(&mut self, ui: &Ui, id: PanelInstanceId, client: &mut dyn DesktopClient) {
        let group = self.workspace.viewers[&id].group;
        if let Some(group) = group_selector::draw(ui, group, false) {
            self.workspace.set_viewer_group(id, group);
            self.refresh_viewer_targets(client);
        }
        let viewer = &self.workspace.viewers[&id];
        let output = self.workspace.resolve(id);
        let labels = client
            .snapshot()
            .map(|s| workspace::document_labels(&s))
            .unwrap_or_default();
        let source = output
            .as_ref()
            .and_then(|o| labels.get(&o.document))
            .map(String::as_str)
            .unwrap_or("No source");
        let quality = match viewer.divisor {
            1 => "Full",
            2 => "Half",
            _ => "Quarter",
        };
        let pinned = matches!(viewer.binding, ViewerBinding::Pinned(_));
        let mode = client.viewer_transport(id).map_or_else(
            || self.playback_mode(id, client),
            |transport| transport.mode,
        );
        ui.same_line();
        ui.text_disabled(match mode {
            fold_platform::desktop::PlaybackMode::RealTime => "Real-time",
            fold_platform::desktop::PlaybackMode::EveryFrame => "Every-frame",
        });
        if self.workspace.monitored_viewer == Some(id)
            && mode == fold_platform::desktop::PlaybackMode::RealTime
        {
            ui.same_line();
            ui.text_disabled("Audio");
            crate::sdk::toolbar::tooltip(ui, "Audio monitor — change in the viewer context menu");
        }
        let image_time = self
            .presentation
            .get(&id)
            .and_then(|(key, _)| key.as_ref())
            .and_then(|key| key.target.map(|target| target.1))
            .unwrap_or(viewer.time);
        ui.same_line();
        ui.text_disabled(format!(
            "{}{} · Image {}/{}s · {source}",
            if pinned { "Pinned · " } else { "" },
            quality,
            image_time.numerator(),
            image_time.denominator()
        ));
        crate::sdk::toolbar::tooltip(
            ui,
            "Displayed image time; the transport below shows the requested playhead.",
        );
        if let Some(output) = &output
            && output.output != "video"
        {
            ui.same_line();
            ui.text_disabled(&output.output);
        }
        if let Some((_, Some(error))) = self.presentation.get(&id) {
            ui.text_wrapped(error);
        }
        if let Some(error) = client.viewer_audio_error(id) {
            ui.text_wrapped(error);
        }
        if let Some(output) = self.workspace.resolve(id) {
            let state = client.preview_state(&output, self.workspace.viewers[&id].time);
            if state.content.is_none() {
                ui.text_colored([0.95, 0.55, 0.35, 1.], &state.status);
            } else if state.transient {
                ui.text_colored([0.95, 0.75, 0.35, 1.], "Uncommitted edit preview");
            }
        }
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
        self.image_rects.remove(&id);
        let Some(key) = self.viewers.get(&id).cloned() else {
            return;
        };
        let mut close = false;
        ui.window(&key).build(|| {
            self.viewer_header(ui, id, client);
            if let Preview::Failed(error) = preview
                && self
                    .presentation
                    .get(&id)
                    .is_none_or(|(_, error)| error.is_none())
            {
                ui.text_wrapped(error);
            }
            if ui.is_window_focused() {
                self.workspace.inspector_viewer = Some(id);
                if let Some(editor) = self.workspace.editor_for_viewer(id) {
                    self.workspace.focused_editor = Some(editor);
                }
            }
            if let Some(_popup) = ui.begin_popup_context_window() {
                for (label, divisor) in [("Full", 1), ("Half", 2), ("Quarter", 4)] {
                    if ui.selectable(label) {
                        self.workspace.viewers.get_mut(&id).unwrap().divisor = divisor;
                    }
                }
                if let Some(output) = self.workspace.resolve(id) {
                    if ui.menu_item("Pin this output") {
                        let editor = self.workspace.editor_for_viewer(id);
                        let viewer = self.workspace.viewers.get_mut(&id).unwrap();
                        viewer.binding = ViewerBinding::Pinned(output.clone());
                        viewer.editor = editor;
                    }
                    if let Some(_menu) = ui.begin_menu("Output") {
                        for descriptor in client.outputs(output.document) {
                            let _disabled = ui.begin_disabled_with_cond(descriptor.info.is_err());
                            if ui.menu_item(descriptor.label) {
                                self.workspace.viewers.get_mut(&id).unwrap().binding =
                                    ViewerBinding::Pinned(descriptor.reference);
                                self.refresh_viewer_targets(client);
                            }
                        }
                    }
                }
                if self.workspace.viewers[&id].binding != ViewerBinding::Linked
                    && ui.menu_item("Follow group")
                {
                    let group = self.workspace.viewers[&id].group;
                    self.workspace.set_viewer_group(id, group);
                    self.refresh_viewer_targets(client);
                }
                if let Some(_menu) = ui.begin_menu("Playback mode") {
                    self.playback_mode_items(ui, id, client);
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
                ui.text_disabled(statistics);
            }
            let origin = ui.cursor_pos();
            let available = ui.content_region_avail();
            let image_height = (available[1] - crate::transport::Transport::height(ui)).max(0.);
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
            match preview {
                Preview::Pending => {}
                Preview::Failed(_) => {}
                Preview::Ready {
                    texture,
                    dimensions,
                } => {
                    let size = fitted_size(*dimensions, [available[0], image_height]);
                    if size[0] > 0. && size[1] > 0. {
                        self.image_rects.insert(
                            id,
                            crate::sdk::ViewerRect {
                                origin: ui.cursor_screen_pos(),
                                size,
                                dimensions: *dimensions,
                            },
                        );
                        ui.image(*texture, size);
                    }
                }
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
                if ui.is_window_focused() {
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
            self.sync_instances();
            self.rebuild_layout();
        }
    }
    pub fn viewer_overlays(&mut self, ui: &Ui, client: &mut dyn DesktopClient) {
        for (&id, rect) in &self.image_rects {
            let Some(viewer) = self.workspace.viewers.get(&id) else {
                continue;
            };
            let Some(editor_id) = self.workspace.editor_for_viewer(id) else {
                continue;
            };
            let Some(output) = self.workspace.resolve(id) else {
                continue;
            };
            if viewer.playing {
                continue;
            }
            if let Some((presented, _)) = self.presentation.get(&id) {
                let state = client.preview_state(&output, viewer.time);
                let expected = state
                    .preview_key(state.frame, viewer.divisor)
                    .map(|mut key| {
                        key.output = output.output.clone();
                        key
                    });
                if presented.is_none() || *presented != expected {
                    continue;
                }
            }
            let Some(binding) = self.workspace.editors.get(&editor_id) else {
                continue;
            };
            if binding.output().as_ref() != Some(&output) {
                continue;
            }
            let Some(editor) = self.editors.get_mut(&editor_id) else {
                continue;
            };
            let mut binding = binding.clone();
            if let Some(location) = binding.navigation.last_mut() {
                location.time = viewer.time;
            }
            ui.window(&self.viewers[&id]).build(|| {
                let mut context = PanelContext::new(client, &mut binding).instance(id);
                editor.panels.editor.draw_viewer_overlay(
                    ExtensionUi {
                        ui,
                        host: &mut context,
                    },
                    *rect,
                );
            });
            self.workspace
                .editors
                .get_mut(&editor_id)
                .unwrap()
                .selection = binding.selection;
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
