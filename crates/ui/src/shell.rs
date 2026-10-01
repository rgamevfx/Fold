use dear_imgui_rs::{DockLayout, DockLayoutApply, DockSplit, Ui, WindowKey};

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

    pub(crate) fn draw(&mut self, ui: &Ui) -> Result<(), Box<dyn std::error::Error>> {
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
        ui.window(&self.viewer).build(|| {
            ui.text("No preview — viewer placeholder");
            ui.separator();
            ui.text_wrapped(
                "Engine-rendered images arrive in phase 4. No frame is being evaluated.",
            );
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
            shell.draw(context.frame()).unwrap();
            context.end_frame();
            assert!(!shell.reset_layout);
        }
    }
}
