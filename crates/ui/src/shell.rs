use dear_imgui_rs::{DockLayout, DockLayoutApply, DockSplit, TextureId, Ui, WindowKey};
use fold_platform::desktop::{DesktopClient, DesktopCommand, PreviewKey};

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
    inspector: WindowKey,
    timeline: WindowKey,
    layout: DockLayout,
    reset_layout: bool,
    first: String,
    second: String,
    project_path: String,
    export_path: String,
    frame: i32,
    divisor: u32,
    start: i32,
    end: i32,
    opacity: f32,
    content: Option<String>,
}
impl Shell {
    pub fn new() -> Self {
        let viewer = WindowKey::new("fold.viewer.main", "Viewer").unwrap();
        let inspector = WindowKey::new("fold.inspector.main", "Media / Delivery").unwrap();
        let timeline = WindowKey::new("fold.timeline.main", "Transport").unwrap();
        let layout = DockLayout::split(
            DockSplit::Down,
            0.28,
            DockLayout::tabs([&timeline]),
            DockLayout::split(
                DockSplit::Right,
                0.36,
                DockLayout::tabs([&inspector]),
                DockLayout::tabs([&viewer]),
            ),
        );
        Self {
            viewer,
            inspector,
            timeline,
            layout,
            reset_layout: false,
            first: String::new(),
            second: String::new(),
            project_path: "/tmp/fold-project.fold".into(),
            export_path: "/tmp/fold-export.mp4".into(),
            frame: 0,
            divisor: 2,
            start: 0,
            end: 1,
            opacity: 0.5,
            content: None,
        }
    }
    pub fn key(&self, client: &dyn DesktopClient) -> Option<PreviewKey> {
        client
            .state()
            .preview_key(self.frame.max(0) as u32, self.divisor)
    }
    pub fn controls(
        &mut self,
        ui: &Ui,
        client: &mut dyn DesktopClient,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = client.state().clone();
        if self.content != state.content {
            self.frame = self.frame.clamp(0, state.frames as i32 - 1);
            self.end = state.frames as i32;
            self.content = state.content.clone();
            self.opacity = state.foreground_opacity;
        }
        ui.main_menu_bar(|| {
            ui.text("Fold | Media | Video only");
            ui.same_line();
            if ui.button("Reset layout") {
                self.reset_layout = true;
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
        ui.window(&self.inspector).build(|| {
            ui.text_wrapped("MP4/H.264 | 8-bit SDR BT.709 limited range | CFR | square pixels. Audio is ignored.");
            ui.input_text("Background path", &mut self.first).build();
            ui.input_text("Foreground (optional)", &mut self.second).build();
            if ui.button("Import layers") {
                let mut paths = vec![self.first.clone().into()];
                if !self.second.is_empty() { paths.push(self.second.clone().into()); }
                client.command(DesktopCommand::Import(paths));
            }
            ui.text_wrapped("Two layers must have matching dimensions, rate and frame count.");
            ui.slider("Foreground opacity", 0.0f32, 1.0f32, &mut self.opacity);
            if ui.button("Apply opacity") { client.command(DesktopCommand::Opacity(self.opacity)); }
            ui.separator();
            ui.input_text("Project path", &mut self.project_path).build();
            if ui.button("Save project") { client.command(DesktopCommand::Save(self.project_path.clone().into())); }
            ui.same_line(); if ui.button("Open project") { client.command(DesktopCommand::Open(self.project_path.clone().into())); }
            ui.separator();
            ui.input_text("New MP4 output", &mut self.export_path).build();
            ui.text("Export range [start, end): end is exclusive");
            ui.input_int("Start frame", &mut self.start);
            ui.input_int("End frame", &mut self.end);
            if ui.button("Export range") && self.start >= 0 && self.end > self.start {
                client.command(DesktopCommand::Export { path: self.export_path.clone().into(), start: self.start as u32, end: self.end as u32 });
            }
            ui.same_line(); if ui.button("Cancel job") { client.command(DesktopCommand::Cancel); }
            ui.text(if state.busy { "Background job active" } else { "Background worker idle" });
            ui.text_wrapped(&state.status);
        });
        ui.window(&self.timeline).build(|| {
            ui.text(format!(
                "Frame {} / {} | {} / {} fps | exact frame-based scrubbing",
                self.frame,
                state.frames.saturating_sub(1),
                state.rate[0],
                state.rate[1]
            ));
            ui.slider("Seek", 0, (state.frames as i32 - 1).max(0), &mut self.frame);
            ui.input_int("Frame", &mut self.frame);
            self.frame = self.frame.clamp(0, state.frames as i32 - 1);
            for (label, divisor) in [("Full", 1), ("Half", 2), ("Quarter", 4)] {
                if ui.button(label) {
                    self.divisor = divisor;
                }
                ui.same_line();
            }
            ui.text(format!(
                "Preview 1/{} | No playback/audio yet",
                self.divisor
            ));
        });
        Ok(())
    }
    pub fn viewer(&self, ui: &Ui, preview: &Preview, statistics: &str) {
        ui.window(&self.viewer).build(|| {
            ui.text(format!(
                "Frame {} | SDR sRGB v1 | preview 1/{}",
                self.frame, self.divisor
            ));
            ui.text(statistics);
            ui.separator();
            match preview {
                Preview::Pending => {
                    ui.text("No current image: import media or wait for rendering…")
                }
                Preview::Failed(error) => {
                    ui.text("Preview failed");
                    ui.text_wrapped(error);
                }
                Preview::Ready {
                    texture,
                    dimensions,
                } => {
                    ui.text(format!(
                        "{} x {} display pixels",
                        dimensions[0], dimensions[1]
                    ));
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
    fn docked_controls_and_viewer_build() {
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
        let mut shell = Shell::new();
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
