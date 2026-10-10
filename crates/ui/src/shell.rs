//! Generic instance-aware workspace shell. Feature models stay in providers.
#[cfg(feature = "native-probe")]
#[path = "selection_probe.rs"]
mod selection_probe;
#[cfg(feature = "native-probe")]
pub use selection_probe::WorkspaceProbe;
#[path = "shell_linking.rs"]
mod linking;
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
#[path = "shell_delivery.rs"]
mod delivery;
#[path = "shell_runtime.rs"]
mod runtime;
#[path = "shell_viewer.rs"]
mod viewer_drawing;
use runtime::RuntimeViewer;

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
    pub(crate) cached_previews: Vec<PreviewKey>,
    pub(crate) settings: Option<crate::settings::Settings>,
    document_settings: Option<crate::document_settings::Dialog>,
    pub(crate) typography: Option<crate::sdk::typography::Typography>,
    delivery: WindowKey,
    empty: WindowKey,
    panels: Vec<RegisteredPanel>,
    editors: BTreeMap<PanelInstanceId, RuntimeEditor>,
    viewers: BTreeMap<PanelInstanceId, RuntimeViewer>,
    pub(crate) workspace: Workspace,
    layout: DockLayout,
    reset_layout: bool,
    pub(crate) project_menu: crate::project_menu::ProjectMenu,
    workspace_menu: crate::workspace_presets::Menu,
    default_workspace: Workspace,
    delivery_controls: delivery::Delivery,
    visible_panels: Vec<usize>,
    visible_editors: Vec<PanelInstanceId>,
    #[cfg(feature = "native-probe")]
    probe_full_resolution: bool,
    review_actions: Vec<(PanelInstanceId, crate::review::Action)>,
    selection_events: Vec<(PanelInstanceId, fold_platform::desktop::Selection)>,
    locked_inspector: Option<linking::LockedInspector>,
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
        let delivery = WindowKey::new("fold.delivery.main", "Delivery").unwrap();
        let empty = WindowKey::new("fold.editors.empty", "Editors").unwrap();
        let mut shell = Self {
            settings: None,
            document_settings: None,
            typography: None,
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
            delivery_controls: Default::default(),
            visible_panels: vec![],
            visible_editors: vec![],
            #[cfg(feature = "native-probe")]
            probe_full_resolution: false,
            cached_previews: vec![],
            review_actions: vec![],
            selection_events: vec![],
            locked_inspector: None,
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
            "Drop media onto Project, a timeline, a composition, or a node graph".into(),
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
        if !self.viewers.contains_key(&id) || !self.workspace.viewers.contains_key(&id) {
            return;
        }
        if let Some(transport) = client.viewer_transport(id)
            && let Some(viewer) = self.workspace.viewers.get_mut(&id)
        {
            if viewer.last_output.as_ref() != Some(&transport.output) {
                // An external retarget is scoped to this viewer, not its linked editor.
                // Adopt it before the next demand instead of requesting the old source
                // against a clock that now accepts only the new source's frames.
                viewer.binding = ViewerBinding::Pinned(transport.output.clone());
                viewer.last_output = Some(transport.output.clone());
                viewer.last_editor = None;
                viewer.editor = None;
                viewer.range = transport.range;
            }
            viewer.time = transport.time;
            viewer.playing = transport.playing;
        }
        let output = self.workspace.resolve(id);
        let runtime = self.viewers.get_mut(&id).unwrap();
        runtime.bind(output);
        runtime.presentation = (key, error);
    }
    pub fn take_review_actions(&mut self) -> Vec<(PanelInstanceId, crate::review::Action)> {
        std::mem::take(&mut self.review_actions)
    }
    pub fn review_state(&mut self, id: PanelInstanceId, state: (Option<String>, bool)) {
        if let Some(runtime) = self.viewers.get_mut(&id) {
            runtime.review = state;
        }
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
        let runtime = self.viewers.get(&id)?;
        let navigation = &runtime.navigation;
        let (dimensions, region) = navigation.demand(
            key.dimensions,
            runtime.viewport_pixels.unwrap_or([1.; 2]),
            runtime.viewport_scale,
            divisor,
        )?;
        key.dimensions = dimensions;
        key.region = region;
        Some(key)
    }
    pub fn keys(&self, client: &dyn DesktopClient) -> Vec<(PanelInstanceId, Option<PreviewKey>)> {
        let keys: Vec<_> = self
            .workspace
            .viewers
            .iter()
            .map(|(&id, viewer)| {
                let key = self.workspace.resolve(id).and_then(|output| {
                    let time = client
                        .viewer_request(id)
                        .map_or(viewer.time, |request| request.1);
                    let state = client.preview_state(&output, time);
                    self.viewers.get(&id)?.viewport_pixels?;
                    let mut key =
                        self.spatial_key(id, state.preview_key(state.frame, 1)?, viewer.divisor)?;
                    key.channels = viewer.channels.clone();
                    Some(key)
                });
                (id, key)
            })
            .collect();
        keys
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
        self.delivery_controls.refresh(client);
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
                    .or_else(|| self.delivery_controls.output.map(|(document, _)| document));
                let editor = target.and_then(|document| {
                    self.workspace
                        .editors
                        .iter()
                        .find(|(_, e)| e.document() == Some(document))
                        .map(|(&id, _)| id)
                });
                if let Some(editor) = editor {
                    if self
                        .editors
                        .get(&editor)
                        .is_some_and(|e| e.panels.editor.supports_composition())
                    {
                        self.workspace.composition_editor = Some(editor);
                    }
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
        let mut reveal_composition = false;
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
                    if panel.descriptor.id == crate::sdk::composition::PANEL_ID {
                        reveal_composition |= ui.menu_item("Composition");
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
                if ui.menu_item("Project Settings…") {
                    client.command(DesktopCommand::OpenSettings(None));
                }
                if ui.menu_item("Settings…")
                    && let Some(settings) = &mut self.settings
                {
                    settings.show();
                }
            }
        });
        if let Some(target) = client.take_settings_request() {
            match crate::document_settings::Dialog::new(client, target) {
                Ok(dialog) => self.document_settings = Some(dialog),
                Err(error) => client.command(DesktopCommand::Notify(error)),
            }
        }
        if let Some(dialog) = &mut self.document_settings {
            if !dialog.draw(ui, client) {
                self.document_settings = None;
            }
        }
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
        self.refresh_link_times(client);
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
        let cache_references: BTreeMap<_, _> = self.keys(client).into_iter().collect();
        let editor_cache: BTreeMap<_, _> = self
            .workspace
            .editors
            .keys()
            .map(|&id| {
                (
                    id,
                    self.workspace
                        .viewer_for_editor(id)
                        .ok()
                        .and_then(|v| cache_references.get(&v).cloned().flatten()),
                )
            })
            .collect();
        let mut seeks = Vec::new();
        let mut layer_graphs = Vec::new();
        let mut navigations = Vec::new();
        let mut activations = Vec::new();
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
            if self.editors.get(&id).is_some_and(|e| {
                e.panels.editor.supports_composition() || e.panels.editor.is_layer_graph()
            }) {
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
                // Immediately after Begin, the last item is this window's dock tab/title.
                if ui.is_item_clicked() { activations.push(id); }
                if let Some(_popup) = ui.begin_popup_context_item_with_label(Some("editor-tab-menu")) {
                    if ui.menu_item("Close editor") { close_editor = Some(id); }
                }
                if ui.is_window_focused() { self.workspace.focused_editor = Some(id); }
                if let Some(group) = group_selector::draw_choices(ui, binding.group, |group| group == workspace::LinkGroup::Unlinked || !memberships.iter().any(|(other, occupied, contribution)| *other != id && *occupied == group && *contribution == binding.contribution)) { group_changes.push((id, group)); }
                ui.same_line();
                crate::document_settings::button(ui, client, binding.document());
                ui.same_line();
                if let Some(action) = target_selector::draw(ui, &label, binding.navigation.len() > 1, &choices) {
                    client.command(DesktopCommand::CancelPreviewEdit);
                    let mut target = binding.clone();
                    match action {
                        Action::Choose(document) => target.bind(ViewLocation { document, time: Time::ZERO, label: String::new() }),
                        Action::Back => {
                            target.back();
                            if let Some(snapshot) = client.snapshot() && let Some(document) = target.document().and_then(|d| snapshot.state().documents.get(&d)) { target.document_type = document.type_id.clone(); }
                        }
                    }
                    navigations.push((id, target));
                }
                if let Some(editor) = self.editors.get_mut(&id)
                    && editor.panels.editor.document_type() == Some(binding.document_type.as_str())
                {
                    let mut scoped = binding.clone();
                    if let Some(time) = edit_times.get(&id) && let Some(location) = scoped.navigation.last_mut() { location.time = *time; }
                    let navigation = scoped.navigation.clone();
                    let mut context = PanelContext::new(client, &mut scoped).instance(id).with_cache(&self.cached_previews).with_cache_reference(editor_cache.get(&id).cloned().flatten());
                    editor.panels.editor.draw(ExtensionUi { ui, host: &mut context });
                    if let Some(time) = context.seek_request() { seeks.push((id, time)); }
                    if context.selection_requested() { publish.push((id, context.state().selection.clone())); }
                    if let Some(graph) = context.take_layer_graph() { layer_graphs.push((id, graph)); }
                    drop(context);
                    if scoped.navigation != navigation { navigations.push((id, scoped)); }
                    else { binding.selection = scoped.selection; }
                } else { ui.text_wrapped("This editor contribution is unavailable. Its binding has been retained."); }
            }).is_some();
            if visible {
                self.visible_editors.push(id);
            }
        }
        for (id, group) in group_changes {
            self.workspace.set_editor_group(id, group);
        }
        self.selection_events.extend(publish);
        for id in activations.drain(..) {
            self.activate_editor(id, false);
        }
        for (id, target) in navigations.drain(..) {
            self.navigate_editor(id, target);
        }
        for (editor, time) in seeks {
            self.seek_from_editor(editor, time, client);
        }
        if let Some(id) = self.focus
            && self
                .editors
                .get(&id)
                .is_some_and(|e| e.panels.editor.supports_composition())
        {
            self.workspace.composition_editor = Some(id);
        }
        reveal_composition |= self
            .focus
            .and_then(|id| self.editors.get(&id))
            .is_some_and(|e| e.panels.editor.supports_composition());
        reveal_network |= self
            .focus
            .and_then(|id| self.editors.get(&id))
            .is_some_and(|e| e.panels.editor.is_layer_graph());
        reveal_network |= self
            .editors
            .values_mut()
            .any(|e| e.panels.editor.take_network_request());
        if let Some(id) = self.focus
            && self
                .editors
                .get(&id)
                .is_some_and(|e| e.panels.editor.is_layer_graph())
        {
            self.workspace.follow_network(id);
        }
        self.focus = None;
        self.refresh_viewer_targets(client);
        self.visible_panels.clear();
        let layer_graph_types: Vec<_> = self
            .panels
            .iter()
            .filter(|p| p.panel.is_layer_graph())
            .filter_map(|p| p.panel.document_type().map(str::to_owned))
            .collect();
        let mut publish = Vec::new();
        let mut seeks = Vec::new();
        let graph_layers: BTreeMap<_, _> = self
            .workspace
            .editors
            .keys()
            .filter_map(|&id| self.linked_layer(id, None).map(|layer| (id, layer)))
            .collect();
        for (index, registered) in self.panels.iter_mut().enumerate() {
            if registered.descriptor.placement == PanelPlacement::Editor
                && registered.panel.document_type().is_some()
            {
                continue;
            }
            let visible = ui
                .window(&registered.key)
                .focused(
                    (reveal_composition
                        && registered.descriptor.id == crate::sdk::composition::PANEL_ID)
                        || (reveal_network
                            && registered.descriptor.id == crate::sdk::network::PANEL_ID),
                )
                .build(|| {
                    if registered.descriptor.id == crate::sdk::composition::PANEL_ID
                        && ui.is_item_clicked()
                        && let Some(id) = self.workspace.composition_editor
                    {
                        activations.push(id);
                    }
                    if registered.descriptor.id == crate::sdk::composition::PANEL_ID
                        && let Some(_menu) =
                            ui.begin_popup_context_item_with_label(Some("composition-tab-menu"))
                        && ui.menu_item("Close composition context")
                    {
                        close_editor = self.workspace.composition_editor;
                    }
                    if registered.descriptor.id == crate::sdk::outliner::PANEL_ID
                        || registered.descriptor.placement == PanelPlacement::Inspector
                        || registered.descriptor.id == crate::sdk::composition::PANEL_ID
                        || registered.descriptor.id == crate::sdk::network::PANEL_ID
                    {
                        let composition =
                            registered.descriptor.id == crate::sdk::composition::PANEL_ID;
                        let mut target = if composition {
                            self.workspace.composition_editor
                        } else {
                            self.workspace.inspector_editor()
                        };
                        if composition {
                            if let Some(id) = target {
                                let group = self.workspace.editors[&id].group;
                                if let Some(group) =
                                    group_selector::draw_choices(ui, group, |group| {
                                        self.workspace.can_join_group(id, group)
                                    })
                                {
                                    self.workspace.set_editor_group(id, group);
                                    publish
                                        .push((id, self.workspace.editors[&id].selection.clone()));
                                }
                            }
                        } else {
                            let group = if registered.descriptor.id == crate::sdk::network::PANEL_ID
                            {
                                &mut self.workspace.network_group
                            } else if registered.descriptor.id == crate::sdk::outliner::PANEL_ID {
                                &mut self.workspace.outliner_group
                            } else {
                                &mut self.workspace.inspector_group
                            };
                            if let Some(value) = group_selector::draw(ui, *group) {
                                *group = value;
                                if registered.descriptor.placement == PanelPlacement::Inspector {
                                    self.workspace.inspector_lock = None;
                                    client.command(DesktopCommand::CancelPreviewEdit);
                                }
                            }
                            if registered.descriptor.placement == PanelPlacement::Inspector {
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
                                    client.command(DesktopCommand::CancelPreviewEdit);
                                }
                            }
                        }
                        if registered.descriptor.id == crate::sdk::network::PANEL_ID {
                            target = self.workspace.network_target(self.workspace.network_group);
                        } else if registered.descriptor.id == crate::sdk::outliner::PANEL_ID {
                            target = self.workspace.network_target(self.workspace.outliner_group);
                        }
                        if registered.descriptor.id == crate::sdk::network::PANEL_ID
                            || registered.descriptor.id == crate::sdk::composition::PANEL_ID
                        {
                            ui.same_line();
                            let current = target;
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
                            let label = self
                                .workspace
                                .inspector_lock
                                .as_ref()
                                .filter(|_| {
                                    registered.descriptor.placement == PanelPlacement::Inspector
                                })
                                .map(|(_, b)| b)
                                .or_else(|| current.and_then(|id| self.workspace.editors.get(&id)))
                                .map(|binding| {
                                    let edit_context = if composition
                                        || registered.descriptor.id == crate::sdk::network::PANEL_ID
                                    {
                                        current.and_then(|id| self.workspace.editor_context(id))
                                    } else {
                                        self.workspace.inspector_context()
                                    };
                                    if let Some((_, time)) = edit_context {
                                        breadcrumb_at(binding, &labels, time)
                                    } else {
                                        context_name(binding)
                                    }
                                })
                                .unwrap_or("Choose context".into());
                            let settings_target = self
                                .workspace
                                .inspector_lock
                                .as_ref()
                                .filter(|_| {
                                    registered.descriptor.placement == PanelPlacement::Inspector
                                })
                                .map(|(_, b)| b)
                                .or_else(|| current.and_then(|id| self.workspace.editors.get(&id)))
                                .and_then(|b| b.document());
                            crate::document_settings::button(ui, client, settings_target);
                            ui.same_line();
                            let mut choose = None;
                            ui.set_next_item_width(
                                (ui.content_region_avail()[0] - ui.frame_height() - 12.).max(32.),
                            );
                            if let Some(_combo) = ui.begin_combo("##authoring-context", &label) {
                                for (&id, binding) in &self.workspace.editors {
                                    if self.editors.get(&id).is_some_and(|e| {
                                        if registered.descriptor.id
                                            == crate::sdk::composition::PANEL_ID
                                        {
                                            e.panels.editor.supports_composition()
                                        } else {
                                            e.panels.editor.supports_network()
                                        }
                                    }) {
                                        let title = context_name(binding);
                                        if ui.selectable(format!("{title} · {}", id.0)) {
                                            choose = Some(id);
                                        }
                                    }
                                }
                            }
                            if let Some(id) = choose {
                                if self
                                    .editors
                                    .get(&id)
                                    .is_some_and(|e| e.panels.editor.supports_composition())
                                {
                                    self.workspace.composition_editor = Some(id);
                                }
                                target = Some(id);
                                if registered.descriptor.id == crate::sdk::network::PANEL_ID {
                                    self.workspace.follow_network(id);
                                }
                                if !composition {
                                    self.workspace.network_group =
                                        self.workspace.editors[&id].group;
                                }
                                self.workspace.focused_editor = Some(id);
                                activations.push(id);
                            }
                        }
                        if (registered.descriptor.id == crate::sdk::network::PANEL_ID
                            || registered.descriptor.id == crate::sdk::composition::PANEL_ID)
                            && let Some(id) = target
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
                                let mut target = binding.clone();
                                target.back();
                                if let Some(snapshot) = client.snapshot()
                                    && let Some(document) = target
                                        .document()
                                        .and_then(|id| snapshot.state().documents.get(&id))
                                {
                                    target.document_type = document.type_id.clone();
                                }
                                navigations.push((id, target));
                            }
                        }
                        let locked_binding = self
                            .workspace
                            .inspector_lock
                            .as_ref()
                            .filter(|_| {
                                registered.descriptor.placement == PanelPlacement::Inspector
                            })
                            .map(|(_, binding)| binding.clone());
                        let inspector_context =
                            if registered.descriptor.placement != PanelPlacement::Inspector {
                                target.and_then(|id| self.workspace.editor_context(id))
                            } else {
                                self.workspace.inspector_context()
                            };
                        let graph_layer = target.and_then(|id| graph_layers.get(&id).copied());
                        if let Some(id) = target
                            && let Some(binding) = self.workspace.editors.get_mut(&id)
                            && registered.panel.supports_document_type(
                                &locked_binding.as_ref().unwrap_or(binding).document_type,
                            )
                            && let Some(editor) = self.editors.get_mut(&id)
                        {
                            if editor.panels.editor.supports_composition()
                                && locked_binding
                                    .as_ref()
                                    .unwrap_or(binding)
                                    .document()
                                    .is_some()
                                && inspector_context.is_none()
                                && registered.descriptor.id != crate::sdk::network::PANEL_ID
                                && registered.descriptor.id != crate::sdk::outliner::PANEL_ID
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
                            // Property widgets belong to the inspected identities, not
                            // merely to the reusable editor slot displaying them.
                            let selection_identity = format!("selection:{:?}", scoped.selection);
                            let _selection_scope = (registered.descriptor.placement
                                == PanelPlacement::Inspector)
                                .then(|| ui.push_id(&selection_identity));
                            let mut context = PanelContext::new(client, &mut scoped)
                                .instance(id)
                                .with_cache(&self.cached_previews)
                                .with_cache_reference(editor_cache.get(&id).cloned().flatten());
                            let _layer_lock = ui.begin_disabled_with_cond(
                                graph_layer.is_some_and(|(_, locked)| locked),
                            );
                            if registered.descriptor.id == crate::sdk::network::PANEL_ID {
                                if locked_binding.is_none()
                                    && ui.is_window_focused()
                                    && editor.panels.editor.supports_composition()
                                {
                                    self.workspace.composition_editor = Some(id);
                                }
                                self.visible_editors.push(id);
                                if ui.is_window_focused() {
                                    self.workspace.focused_editor = Some(id);
                                }
                                editor.panels.editor.draw_network(ExtensionUi {
                                    ui,
                                    host: &mut context,
                                });
                            } else if registered.descriptor.id == crate::sdk::composition::PANEL_ID
                            {
                                if !editor.panels.editor.supports_composition() {
                                    ui.text_disabled("Choose a composition context.");
                                    return;
                                }
                                self.visible_editors.push(id);
                                if ui.is_window_focused() {
                                    self.workspace.focused_editor = Some(id);
                                }
                                editor.panels.editor.draw(ExtensionUi {
                                    ui,
                                    host: &mut context,
                                });
                            } else if registered.descriptor.id == crate::sdk::outliner::PANEL_ID {
                                registered.panel.draw(ExtensionUi {
                                    ui,
                                    host: &mut context,
                                });
                            } else if locked_binding.is_some() {
                                if let Some(locked) = &mut self.locked_inspector {
                                    locked.panel.draw(ExtensionUi {
                                        ui,
                                        host: &mut context,
                                    });
                                }
                            } else {
                                editor.panels.inspector.draw(ExtensionUi {
                                    ui,
                                    host: &mut context,
                                });
                            }
                            if let Some(time) = context.seek_request() {
                                seeks.push((id, time, locked_binding.is_some()));
                            }
                            if context.selection_requested() && locked_binding.is_none() {
                                publish.push((id, context.state().selection.clone()));
                            }
                            if let Some(graph) = context.take_layer_graph() {
                                layer_graphs.push((id, graph));
                            }
                            drop(context);
                            if locked_binding.is_some() {
                                self.workspace.inspector_lock = Some((id, scoped.clone()));
                            } else if scoped.navigation == navigation {
                                binding.selection = scoped.selection.clone();
                            }
                            if locked_binding.is_none() && scoped.navigation != navigation {
                                if editor.panels.editor.supports_composition()
                                    && layer_graph_types.contains(&scoped.document_type)
                                {
                                    // Keep the parent stack open while its layer graph gets
                                    // its own editor context and exact local-time breadcrumb.
                                    layer_graphs.push((id, scoped));
                                } else {
                                    navigations.push((id, scoped));
                                }
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
        self.selection_events.extend(publish);
        for id in activations {
            self.activate_editor(id, false);
        }
        for (id, target) in navigations {
            self.navigate_editor(id, target);
        }
        self.flush_selections(client);
        for (parent, graph) in layer_graphs {
            self.open_layer_graph(parent, graph);
        }
        // Navigation may have introduced a document not present in the previous
        // provider snapshot. Resolve it before matching queued editor seeks.
        self.refresh_viewer_targets(client);
        for (editor, time, locked_context) in seeks {
            if locked_context
                && let Some((owner, locked)) = &mut self.workspace.inspector_lock
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
        if let Some(id) = close_editor {
            self.close_editor(id, client);
        }
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
        let viewed = self
            .workspace
            .viewers
            .keys()
            .next()
            .and_then(|&id| self.workspace.resolve(id));
        self.delivery_controls
            .draw(ui, &self.delivery, viewed, &labels, client);
        Ok(())
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
    format!(
        "{path} · {:.2} s",
        time.numerator() as f64 / f64::from(time.denominator())
    )
}
#[cfg(test)]
#[path = "docking_tests.rs"]
mod docking_tests;
#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;

#[path = "shell_workspaces.rs"]
mod presets;
