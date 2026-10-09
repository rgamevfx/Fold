//! Opt-in integration harness: production shell, registered providers and native frames.
use super::*;
use fold_foundation::ObjectId;
use fold_platform::desktop::Selection;
#[path = "selection_probe_capture.rs"]
mod capture;

pub struct WorkspaceProbe {
    shell: Shell,
    context: dear_imgui_rs::Context,
    capture: Option<capture::Capture>,
}
impl WorkspaceProbe {
    pub fn new(panels: crate::sdk::PanelRegistry) -> Self {
        Self::create(panels, false)
    }
    pub fn new_gpu(panels: crate::sdk::PanelRegistry) -> Self {
        Self::create(panels, true)
    }
    fn create(panels: crate::sdk::PanelRegistry, gpu: bool) -> Self {
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .io_mut()
            .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
        let fonts = gpu.then(|| crate::sdk::typography::Typography::install(&mut context));
        let capture = if gpu {
            Some(capture::Capture::new(&mut context))
        } else {
            context
                .font_atlas()
                .try_claim_legacy_renderer()
                .unwrap()
                .build();
            None
        };
        context.io_mut().set_delta_time(1. / 60.);
        let mut shell = Shell::new(panels.finish().unwrap());
        shell.bootstrap = false;
        shell.typography = fonts;
        shell.layout = DockLayout::tabs(std::iter::empty::<&WindowKey>());
        Self {
            shell,
            context,
            capture,
        }
    }
    pub fn workspace(&self) -> &Workspace {
        &self.shell.workspace
    }
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.shell.workspace
    }
    pub fn restore(&mut self, workspace: Workspace) {
        self.shell.workspace = workspace;
        self.shell.editors.clear();
        self.shell.locked_inspector = None;
        self.shell.sync_instances();
    }
    pub fn bind(
        &mut self,
        kind: &str,
        document: DocumentId,
        group: workspace::LinkGroup,
    ) -> PanelInstanceId {
        let contribution = self
            .shell
            .panels
            .iter()
            .find(|p| p.panel.document_type() == Some(kind))
            .unwrap()
            .descriptor
            .id;
        let id = self
            .shell
            .workspace
            .editors
            .iter()
            .find(|(_, e)| e.document_type == kind && e.group == group)
            .map(|(&id, _)| id)
            .unwrap_or_else(|| self.shell.workspace.add_editor(contribution, kind));
        let binding = self.shell.workspace.editors.get_mut(&id).unwrap();
        binding.group = group;
        binding.bind(ViewLocation {
            document,
            time: Time::ZERO,
            label: "Linking probe".into(),
        });
        self.shell.sync_instances();
        self.shell.activate_editor(id, false);
        id
    }
    pub fn row(
        &self,
        parent: PanelInstanceId,
        scene: DocumentId,
        object: ObjectId,
        occurrence: Option<ObjectId>,
    ) -> ObjectId {
        self.shell.editors[&parent]
            .panels
            .editor
            .layer_for_graph(scene, Some(object), occurrence)
            .unwrap()
            .0
    }
    pub fn select(&mut self, editor: PanelInstanceId, objects: Vec<ObjectId>) {
        let selection = Selection {
            document: self.shell.workspace.editors[&editor].document(),
            objects,
        };
        self.shell
            .workspace
            .editors
            .get_mut(&editor)
            .unwrap()
            .selection = selection.clone();
        self.shell.selection_events.push((editor, selection));
    }
    pub fn seek(&mut self, editor: PanelInstanceId, time: Time, host: &mut dyn DesktopClient) {
        self.shell.seek_from_editor(editor, time, host);
    }
    pub fn capture(&self, path: &std::path::Path) {
        self.capture.as_ref().unwrap().save(path);
    }
    pub fn frame(&mut self, host: &mut dyn DesktopClient, width: f32) {
        self.context.io_mut().set_display_size([width, 1000.]);
        self.shell.refresh_viewer_targets(host);
        self.shell.prepare_frame(&mut self.context);
        let ui = self.context.frame();
        for panel in &self.shell.panels {
            let pos = match panel.descriptor.id {
                crate::sdk::composition::PANEL_ID => [0., 0.],
                crate::sdk::network::PANEL_ID => [width / 2., 0.],
                crate::sdk::outliner::PANEL_ID => [0., 500.],
                _ if panel.descriptor.placement == PanelPlacement::Inspector => [width / 2., 500.],
                _ => [width + 50., 0.],
            };
            ui.window(&panel.key)
                .position(pos, dear_imgui_rs::Condition::Always)
                .size([width / 2. - 10., 490.], dear_imgui_rs::Condition::Always)
                .build(|| {});
        }
        ui.window(&self.shell.delivery)
            .position([width + 50., 0.], dear_imgui_rs::Condition::Always)
            .size([300., 490.], dear_imgui_rs::Condition::Always)
            .build(|| {});
        self.shell.controls(ui, host).unwrap();
        if let Some(capture) = &mut self.capture {
            capture.draw(&mut self.context, width as u32, 1000);
        } else {
            let _ = self.context.render_legacy();
        }
    }
}
