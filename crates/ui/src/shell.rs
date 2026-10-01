//! Generic workspace shell. Creative panels arrive through the shared SDK;
//! this module knows no clips, tracks, nodes, or package command arguments.
use crate::sdk::{ExtensionUi, RegisteredPanel};
use dear_imgui_rs::{DockLayout, DockLayoutApply, DockSplit, Key, TextureId, Ui, WindowKey};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, PreviewKey},
    packages::PanelPlacement,
};

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
pub(crate) struct Shell {
    viewer: WindowKey,
    delivery: WindowKey,
    empty: WindowKey,
    panels: Vec<RegisteredPanel>,
    layout: DockLayout,
    reset_layout: bool,
    project_path: String,
    export_path: String,
    transport: crate::transport::Transport,
    divisor: u32,
    start: i32,
    end: i32,
    content: Option<String>,
    focused_document: Option<fold_foundation::DocumentId>,
    focus_workspace: Option<(String, u8)>,
    pending_workspace: Option<String>,
    focused_editor: Option<String>,
    visible_panels: Vec<usize>,
    image_rect: Option<crate::sdk::ViewerRect>,
}
impl Shell {
    pub fn new(panels: Vec<RegisteredPanel>) -> Self {
        let viewer = WindowKey::new("fold.viewer.main", "Viewer").unwrap();
        let delivery = WindowKey::new("fold.delivery.main", "Project / Delivery").unwrap();
        let empty = WindowKey::new("fold.editors.empty", "Editors").unwrap();
        let mut editors: Vec<_> = panels
            .iter()
            .filter(|p| p.descriptor.placement == PanelPlacement::Editor)
            .map(|p| &p.key)
            .collect();
        if editors.is_empty() {
            editors.push(&empty);
        }
        let mut inspectors: Vec<_> = panels
            .iter()
            .filter(|p| p.descriptor.placement == PanelPlacement::Inspector)
            .map(|p| &p.key)
            .collect();
        inspectors.push(&delivery);
        let layout = DockLayout::split(
            DockSplit::Right,
            0.27,
            DockLayout::tabs(inspectors),
            DockLayout::split(
                DockSplit::Down,
                0.52,
                DockLayout::tabs(editors),
                DockLayout::tabs([&viewer]),
            ),
        );
        Self {
            viewer,
            delivery,
            empty,
            panels,
            layout,
            reset_layout: false,
            project_path: "/tmp/fold-project.fold".into(),
            export_path: "/tmp/fold-export.mp4".into(),
            transport: Default::default(),
            divisor: 2,
            start: 0,
            end: 1,
            content: None,
            focused_document: None,
            focus_workspace: None,
            pending_workspace: None,
            focused_editor: None,
            visible_panels: Vec::new(),
            image_rect: None,
        }
    }
    pub fn accepts_background_pan(&self, position: [f32; 2]) -> bool {
        self.visible_panels
            .iter()
            .any(|&index| self.panels[index].panel.accepts_background_pan(position))
    }
    pub fn key(&self, client: &dyn DesktopClient) -> Option<PreviewKey> {
        client
            .state()
            .preview_key(client.state().frame, self.divisor)
    }
    pub fn controls(
        &mut self,
        ui: &Ui,
        client: &mut dyn DesktopClient,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = client.state().clone();
        if self.content != state.content {
            self.end = state.frames as i32;
            self.content = state.content.clone();
        }
        if !ui.io().want_text_input() && ui.io().key_ctrl() && ui.is_key_pressed(Key::Z) {
            client.command(if ui.io().key_shift() {
                DesktopCommand::Redo
            } else {
                DesktopCommand::Undo
            });
        }
        if self.focused_document != state.selection.document
            && !ui.is_mouse_down(dear_imgui_rs::MouseButton::Left)
        {
            self.focused_document = state.selection.document;
            self.image_rect = None;
            self.pending_workspace = client.snapshot().and_then(|s| {
                state
                    .selection
                    .document
                    .and_then(|id| s.state().documents.get(&id).map(|d| d.type_id.clone()))
            });
        }
        ui.main_menu_bar(|| {
            ui.text("Fold");
            for panel in self
                .panels
                .iter()
                .filter(|p| p.descriptor.placement == PanelPlacement::Editor)
            {
                ui.same_line();
                if ui.button(panel.descriptor.title) {
                    self.pending_workspace = panel.panel.document_type().map(str::to_owned);
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
            ui.same_line();
            if ui.button("Reset layout") {
                self.reset_layout = true;
            }
        });
        if let Some(kind) = self.pending_workspace.take() {
            client.command(DesktopCommand::ActivateWorkspace(kind.clone()));
            self.focus_workspace = Some((kind, 0));
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
        ui.window(&self.viewer).build(|| {
            if state.navigation.len() > 1 && ui.button("< Back to parent") {
                client.command(DesktopCommand::NavigateBack);
            }
            if !state.navigation.is_empty() {
                ui.same_line();
                ui.text(
                    state
                        .navigation
                        .iter()
                        .map(|v| v.label.as_str())
                        .collect::<Vec<_>>()
                        .join(" / "),
                );
                if let Some(location) = state.navigation.last() {
                    ui.text(format!(
                        "Local time {}/{} s",
                        location.time.numerator(),
                        location.time.denominator()
                    ));
                }
            }
            if state.transient {
                ui.text_colored([0.95, 0.75, 0.35, 1.0], "LIVE EDIT PREVIEW — not committed");
            }
        });
        let activate_focused_editor = !self.visible_panels.is_empty();
        self.visible_panels.clear();
        for (index, registered) in self.panels.iter_mut().enumerate() {
            let visible = ui
                .window(&registered.key)
                .focused(self.focus_workspace.as_ref().is_some_and(|(kind, stage)| {
                    registered.panel.supports_document_type(kind)
                        && matches!(
                            (stage, registered.descriptor.placement),
                            (1, PanelPlacement::Inspector) | (2, PanelPlacement::Editor)
                        )
                }))
                .build(|| {
                    // Direct dock-tab clicks bypass the workspace toolbar.
                    // React to a focus transition, not every frame: otherwise
                    // the old editor would undo Open Source while tabs settle.
                    if registered.descriptor.placement == PanelPlacement::Editor
                        && ui.is_window_focused()
                        && let Some(kind) = registered.panel.document_type()
                        && self.focused_editor.as_deref() != Some(kind)
                    {
                        self.focused_editor = Some(kind.into());
                        if activate_focused_editor && self.focus_workspace.is_none() {
                            client.command(DesktopCommand::ActivateWorkspace(kind.into()));
                        }
                    }
                    registered.panel.draw(ExtensionUi { ui, host: client });
                    if registered.descriptor.placement == PanelPlacement::Editor
                        && ui.is_window_focused()
                    {
                        crate::transport::shortcuts(ui, client);
                    }
                })
                .is_some();
            if visible {
                self.visible_panels.push(index);
            }
        }
        // Let newly docked windows appear first (otherwise Delivery's initial
        // focus wins), then select the inspector and editor on separate frames.
        if let Some((_, stage)) = &mut self.focus_workspace {
            if *stage == 2 {
                self.focus_workspace = None;
            } else {
                *stage += 1;
            }
        }
        if !self
            .panels
            .iter()
            .any(|p| p.descriptor.placement == PanelPlacement::Editor)
        {
            ui.window(&self.empty)
                .build(|| ui.text_wrapped("No editor contribution is installed."));
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
            ui.text(if state.busy {
                "Background job active"
            } else {
                "Worker idle"
            });
            ui.text_wrapped(&state.status);
            ui.text_disabled(
                "Export uses the committed project output, not a source-view or live edit.",
            );
        });
        Ok(())
    }
    pub fn viewer_overlays(&mut self, ui: &Ui, client: &mut dyn DesktopClient) {
        let Some(rect) = self.image_rect else {
            return;
        };
        let Some(snapshot) = client.snapshot() else {
            return;
        };
        let active = client.state().viewer_document;
        let Some(document) = active.and_then(|id| snapshot.state().documents.get(&id)) else {
            return;
        };
        ui.window(&self.viewer).build(|| {
            for registered in &mut self.panels {
                if registered.descriptor.placement == PanelPlacement::Editor
                    && registered.panel.document_type() == Some(document.type_id.as_str())
                {
                    registered
                        .panel
                        .draw_viewer_overlay(ExtensionUi { ui, host: client }, rect);
                }
            }
        });
    }
    pub fn viewer(
        &mut self,
        ui: &Ui,
        preview: &Preview,
        statistics: &str,
        client: &mut dyn DesktopClient,
    ) {
        self.image_rect = None;
        ui.window(&self.viewer).build(|| {
            if ui.is_window_focused() {
                crate::transport::shortcuts(ui, client);
            }
            // Quality and diagnostics stay in the context menu, not the transport.
            if let Some(_popup) = ui.begin_popup_context_window() {
                for (label, divisor) in [("Full", 1), ("Half", 2), ("Quarter", 4)] {
                    if ui.selectable(label) {
                        self.divisor = divisor;
                    }
                }
                ui.text_disabled(statistics);
            }
            let origin = ui.cursor_pos();
            let available = ui.content_region_avail();
            let image_height = (available[1] - crate::transport::Transport::height(ui)).max(0.);
            match preview {
                Preview::Pending => ui.text("Waiting for the current frame…"),
                Preview::Failed(error) => {
                    ui.text("Preview failed");
                    ui.text_wrapped(error);
                }
                Preview::Ready {
                    texture,
                    dimensions,
                } => {
                    let size = fitted_size(*dimensions, [available[0], image_height]);
                    if size[0] > 0.0 && size[1] > 0.0 {
                        self.image_rect = Some(crate::sdk::ViewerRect {
                            origin: ui.cursor_screen_pos(),
                            size,
                            dimensions: *dimensions,
                        });
                        ui.image(*texture, size);
                    }
                }
            }
            ui.set_cursor_pos([origin[0], origin[1] + image_height]);
            self.transport.draw(ui, client);
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use fold_platform::desktop::{DesktopState, PreviewResult};
    #[test]
    fn image_fit() {
        assert_eq!(fitted_size([640, 360], [800.0, 600.0]), [800.0, 450.0]);
        assert_eq!(fitted_size([640, 360], [-1.0, 90.0]), [0.0, 0.0]);
    }
    struct Client(DesktopState, Vec<DesktopCommand>);
    impl DesktopClient for Client {
        fn state(&self) -> &DesktopState {
            &self.0
        }
        fn poll(&mut self) {}
        fn command(&mut self, command: DesktopCommand) {
            self.1.push(command);
        }
        fn request_preview(&mut self, _: PreviewKey) {}
        fn cancel_preview(&mut self) {}
        fn take_preview(&mut self) -> Option<PreviewResult> {
            None
        }
    }
    #[test]
    fn initial_workspace_request_reveals_both_editor_and_inspector_tabs() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        use std::{cell::Cell, rc::Rc};
        struct Probe {
            id: &'static str,
            kind: &'static str,
            drawn: Rc<Cell<bool>>,
        }
        impl crate::sdk::Panel for Probe {
            fn id(&self) -> &'static str {
                self.id
            }
            fn document_type(&self) -> Option<&'static str> {
                Some(self.kind)
            }
            fn accepts_background_pan(&self, p: [f32; 2]) -> bool {
                self.id == "graph.editor"
                    && (50.0..100.0).contains(&p[0])
                    && (60.0..110.0).contains(&p[1])
            }
            fn draw(&mut self, context: ExtensionUi<'_>) {
                self.drawn.set(true);
                context.ui.text(self.id);
            }
        }
        let editor = Rc::new(Cell::new(false));
        let inspector = Rc::new(Cell::new(false));
        let mut panels = Vec::new();
        for (id, kind, placement, drawn) in [
            (
                "other.editor",
                "other",
                PanelPlacement::Editor,
                Rc::new(Cell::new(false)),
            ),
            (
                "other.inspector",
                "other",
                PanelPlacement::Inspector,
                Rc::new(Cell::new(false)),
            ),
            (
                "graph.editor",
                "graph",
                PanelPlacement::Editor,
                editor.clone(),
            ),
            (
                "graph.inspector",
                "graph",
                PanelPlacement::Inspector,
                inspector.clone(),
            ),
        ] {
            panels.push(RegisteredPanel {
                descriptor: fold_platform::packages::PanelDescriptor {
                    id,
                    title: id,
                    placement,
                },
                panel: Box::new(Probe { id, kind, drawn }),
                key: WindowKey::new(id, id).unwrap(),
            });
        }
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .io_mut()
            .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([1280.0, 800.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        let mut shell = Shell::new(panels);
        shell.pending_workspace = Some("graph".into());
        let mut client = Client(DesktopState::default(), vec![]);
        for _ in 0..12 {
            editor.set(false);
            inspector.set(false);
            let ui = context.frame();
            shell.controls(ui, &mut client).unwrap();
            shell.viewer(ui, &Preview::Pending, "test", &mut client);
            context.end_frame();
        }
        assert!(
            client
                .1
                .iter()
                .any(|c| matches!(c, DesktopCommand::ActivateWorkspace(kind) if kind == "graph")),
            "switching workspace must restore host playback context"
        );
        assert!(editor.get(), "requested graph tab must be visible");
        assert!(
            inspector.get(),
            "requested node inspector must not stay behind Delivery"
        );
        assert!(shell.accepts_background_pan([75.0, 75.0]));
        assert!(!shell.accepts_background_pan([10.0, 10.0]));
        shell.pending_workspace = Some("other".into());
        for _ in 0..12 {
            let ui = context.frame();
            shell.controls(ui, &mut client).unwrap();
            shell.viewer(ui, &Preview::Pending, "test", &mut client);
            context.end_frame();
        }
        assert!(
            !shell.accepts_background_pan([75.0, 75.0]),
            "hidden canvases cannot capture background drag"
        );
        assert!(
            client
                .1
                .iter()
                .any(|c| matches!(c, DesktopCommand::ActivateWorkspace(kind) if kind == "other"))
        );
        // A dock-tab focus change must notify the host too, without a toolbar
        // request. Merely drawing both registered editors must not switch back.
        client.1.clear();
        for frame in 0..5 {
            let ui = context.frame();
            shell.controls(ui, &mut client).unwrap();
            if frame == 0 {
                ui.window(&shell.panels[2].key).focused(true).build(|| {});
            }
            shell.viewer(ui, &Preview::Pending, "test", &mut client);
            context.end_frame();
        }
        assert_eq!(client.1.len(), 1);
        assert!(matches!(&client.1[0], DesktopCommand::ActivateWorkspace(kind) if kind == "graph"));
    }
    #[test]
    fn generic_shell_without_feature_panels_builds() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .io_mut()
            .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut shell = Shell::new(vec![]);
        let mut client = Client(DesktopState::default(), vec![]);
        for size in [[1280.0, 800.0], [640.0, 480.0], [1920.0, 1080.0]] {
            context.io_mut().set_display_size(size);
            context.io_mut().set_delta_time(1.0 / 60.0);
            let ui = context.frame();
            shell.controls(ui, &mut client).unwrap();
            shell.viewer(ui, &Preview::Pending, "cache test", &mut client);
            context.end_frame();
        }
    }
    #[test]
    fn viewer_reserves_both_transport_rows_when_resized() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_delta_time(1. / 60.);
        let mut shell = Shell::new(vec![]);
        let mut client = Client(DesktopState::default(), vec![]);
        for size in [[850., 400.], [400., 200.], [1920., 1080.]] {
            context.io_mut().set_display_size(size);
            for _ in 0..3 {
                let ui = context.frame();
                // Explicit floating Viewer dimensions isolate transport layout
                // from unrelated dock-node minimum sizes and tab visibility.
                ui.window(&shell.viewer)
                    .position([0.; 2], dear_imgui_rs::Condition::Always)
                    .size(size, dear_imgui_rs::Condition::Always)
                    .build(|| {});
                shell.viewer(
                    ui,
                    &Preview::Ready {
                        texture: TextureId::new(1),
                        dimensions: [640, 360],
                    },
                    "test",
                    &mut client,
                );
                let rect = shell.image_rect.expect("visible image");
                ui.window(&shell.viewer).build(|| {
                    let bottom = ui.cursor_screen_pos()[1];
                    assert!(
                        rect.origin[1] + rect.size[1]
                            <= bottom - crate::transport::Transport::height(ui) + 1.
                    );
                    assert!(bottom <= ui.window_pos()[1] + ui.window_size()[1]);
                });
                context.end_frame();
            }
        }
    }
}
