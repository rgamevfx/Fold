use dear_imgui_rs::{DockLayout, DockLayoutApply, DockSplit, TextureId, Ui, WindowKey};

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
}

impl Shell {
    pub(crate) fn new() -> Self {
        let viewer = WindowKey::new("fold.viewer.main", "Viewer").unwrap();
        let inspector = WindowKey::new("fold.inspector.main", "Inspector").unwrap();
        let timeline = WindowKey::new("fold.timeline.main", "Timeline").unwrap();
        let layout = DockLayout::split(
            DockSplit::Down,
            0.28,
            DockLayout::tabs([&timeline]),
            DockLayout::split(
                DockSplit::Right,
                0.25,
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
        }
    }

    pub(crate) fn draw(
        &mut self,
        ui: &Ui,
        preview: &Preview,
    ) -> Result<(), Box<dyn std::error::Error>> {
        ui.main_menu_bar(|| {
            ui.text("Fold | Editing shell");
            ui.same_line();
            if ui.button("Reset layout") {
                self.reset_layout = true;
            }
        });
        let policy = if self.reset_layout {
            DockLayoutApply::Replace
        } else {
            DockLayoutApply::IfMissing
        };
        ui.dockspace().layout(&self.layout, policy).build()?;
        self.reset_layout = false;
        ui.window(&self.viewer).build(|| match preview {
            Preview::Pending => ui.text("Rendering generated demo…"),
            Preview::Failed(error) => {
                ui.text("Preview failed");
                ui.text_wrapped(error);
            }
            Preview::Ready {
                texture,
                dimensions,
            } => {
                ui.text(format!(
                    "Generated demo | {}×{} | t = 0 | SDR sRGB",
                    dimensions[0], dimensions[1]
                ));
                ui.separator();
                let size = fitted_size(*dimensions, ui.content_region_avail());
                if size[0] > 0.0 && size[1] > 0.0 {
                    ui.image(*texture, size);
                }
            }
        });
        ui.window(&self.inspector).build(|| {
            ui.text("No selection");
            ui.separator();
            ui.text_wrapped("Inspector placeholder. Property edits are not available yet.");
        });
        ui.window(&self.timeline).build(|| {
            ui.text("Timeline placeholder | Transport unavailable");
            ui.separator();
            ui.text_wrapped(
                "No sequence loaded. Clip editing and playback arrive in later phases.",
            );
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dear_imgui_rs::{ConfigFlags, Context};

    #[test]
    fn image_fit_preserves_aspect_and_handles_collapsed_panels() {
        assert_eq!(fitted_size([640, 360], [800.0, 600.0]), [800.0, 450.0]);
        assert_eq!(fitted_size([640, 360], [320.0, 90.0]), [160.0, 90.0]);
        assert_eq!(fitted_size([640, 360], [-1.0, 90.0]), [0.0, 0.0]);
    }

    #[test]
    fn docked_shell_builds_frames_and_resets_at_multiple_sizes() {
        let mut shell = Shell::new();
        shell.layout.validate().unwrap();
        let mut context = Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .io_mut()
            .set_config_flags(ConfigFlags::DOCKING_ENABLE | ConfigFlags::NAV_ENABLE_KEYBOARD);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        for size in [[1280.0, 800.0], [640.0, 480.0], [1920.0, 1080.0]] {
            context.io_mut().set_display_size(size);
            context.io_mut().set_delta_time(1.0 / 60.0);
            shell.reset_layout = true;
            for preview in [
                Preview::Pending,
                Preview::Failed("test failure".into()),
                Preview::Ready {
                    texture: TextureId::new(1),
                    dimensions: [640, 360],
                },
            ] {
                shell.draw(context.frame(), &preview).unwrap();
                context.end_frame();
            }
            assert!(!shell.reset_layout);
        }
    }
}
