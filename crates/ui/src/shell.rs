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
    frame: u32,
    divisor: u32,
    start: i32,
    end: i32,
    content: Option<String>,
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
            DockSplit::Down,
            0.46,
            DockLayout::tabs(editors),
            DockLayout::split(
                DockSplit::Right,
                0.29,
                DockLayout::tabs(inspectors),
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
            frame: 0,
            divisor: 2,
            start: 0,
            end: 1,
            content: None,
        }
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
        self.frame = state.frame;
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
        ui.main_menu_bar(|| {
            ui.text("Fold  |  Editing");
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
        for registered in &mut self.panels {
            ui.window(&registered.key)
                .build(|| registered.panel.draw(ExtensionUi { ui, host: client }));
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
        });
        Ok(())
    }
    pub fn viewer(&mut self, ui: &Ui, preview: &Preview, statistics: &str) {
        ui.window(&self.viewer).build(|| {
            ui.text(format!(
                "Frame {}  |  SDR sRGB  |  Preview 1/{}",
                self.frame, self.divisor
            ));
            for (label, divisor) in [("Full", 1), ("Half", 2), ("Quarter", 4)] {
                ui.same_line();
                if ui.button(label) {
                    self.divisor = divisor;
                }
            }
            ui.text(statistics);
            ui.separator();
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
                    let size = fitted_size(*dimensions, ui.content_region_avail());
                    if size[0] > 0.0 && size[1] > 0.0 {
                        ui.image(*texture, size);
                    }
                }
            }
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
    #[test]
    fn generic_shell_without_feature_panels_builds() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        struct Client(DesktopState);
        impl DesktopClient for Client {
            fn state(&self) -> &DesktopState {
                &self.0
            }
            fn poll(&mut self) {}
            fn command(&mut self, _: DesktopCommand) {}
            fn request_preview(&mut self, _: PreviewKey) {}
            fn cancel_preview(&mut self) {}
            fn take_preview(&mut self) -> Option<PreviewResult> {
                None
            }
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
        let mut shell = Shell::new(vec![]);
        let mut client = Client(DesktopState::default());
        for size in [[1280.0, 800.0], [640.0, 480.0], [1920.0, 1080.0]] {
            context.io_mut().set_display_size(size);
            context.io_mut().set_delta_time(1.0 / 60.0);
            let ui = context.frame();
            shell.controls(ui, &mut client).unwrap();
            shell.viewer(ui, &Preview::Pending, "cache test");
            context.end_frame();
        }
    }
}
