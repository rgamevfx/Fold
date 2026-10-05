//! Floating application preferences. Live edits never enter project history.
use crate::{
    sdk::{appearance::Appearance, imgui, typography::Typography},
    settings_store::{Event, Store},
};
use std::time::{Duration, Instant};

pub(crate) struct Settings {
    pub(crate) open: bool,
    focus: bool,
    typography: Typography,
    store: Option<Store>,
    changed: Option<Instant>,
    edited: bool,
    error: String,
}
impl Settings {
    pub(crate) fn new(typography: Typography, store: Option<Store>) -> Self {
        Self {
            open: false,
            focus: false,
            typography,
            store,
            changed: None,
            edited: false,
            error: String::new(),
        }
    }
    pub(crate) fn show(&mut self) {
        self.open = true;
        self.focus = true;
    }
    pub(crate) fn prepare_frame(&mut self, context: &mut imgui::Context) {
        if let Some(store) = &mut self.store {
            while let Some(event) = store.poll() {
                match event {
                    Event::Loaded(Ok(value)) if !self.edited => {
                        let _ = self.typography.set_appearance(value);
                    }
                    Event::Loaded(Err(error)) | Event::Saved(Err(error)) => self.error = error,
                    Event::Saved(Ok(())) => self.error.clear(),
                    _ => {}
                }
            }
        }
        if self
            .changed
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(400))
        {
            self.save();
        }
        self.typography.appearance().apply(context.style_mut());
    }
    fn edit(&mut self, value: Appearance) {
        match self.typography.set_appearance(value) {
            Ok(()) => {
                self.edited = true;
                self.changed = Some(Instant::now());
            }
            Err(error) => self.error = error,
        }
    }
    fn save(&mut self) {
        if self.changed.take().is_some()
            && let Some(store) = &mut self.store
        {
            store.save(self.typography.appearance());
        }
    }
    pub(crate) fn draw(&mut self, ui: &imgui::Ui) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let mut close = false;
        let mut value = self.typography.appearance();
        let before = value;
        ui.window("Settings##fold.settings")
            .opened(&mut open)
            .focused(std::mem::take(&mut self.focus))
            .size([640., 560.], imgui::Condition::FirstUseEver)
            .size_constraints([440., 360.], [1000., 1000.])
            .flags(imgui::WindowFlags::NO_DOCKING | imgui::WindowFlags::NO_COLLAPSE | imgui::WindowFlags::NO_SAVED_SETTINGS)
            .build(|| {
                let unit = ui.current_font_size();
                let footer = ui.frame_height_with_spacing() + if self.error.is_empty() { 0. } else { unit * 4. };
                let height = (ui.content_region_avail()[1] - footer).max(80.);
                let rail = (ui.content_region_avail()[0] * 0.22).clamp(84., 140.);
                ui.child_window("settings-categories").size([rail, height]).build(ui, || {
                    ui.selectable_config("UI").selected(true).build();
                });
                ui.same_line();
                ui.child_window("settings-content").size([0., height]).build(ui, || {
                    ui.text("Text");
                    size_control(ui, "Application text", &mut value.application_text, 12., 24.);
                    ui.separator();
                    ui.text("Node graph");
                    size_control(ui, "Node titles", &mut value.graph.titles, 14., 32.);
                    size_control(ui, "Port labels", &mut value.graph.labels, 12., 28.);
                    size_control(ui, "Secondary text", &mut value.graph.secondary, 10., 24.);
                    size_control(ui, "Node spacing", &mut value.graph.spacing, 12., 28.);
                    ui.checkbox("Hide distant details", &mut value.graph.hide_details);
                    if ui.is_item_hovered() {
                        ui.tooltip_text("Hides port labels and summaries at distant zoom. Node titles stay visible.");
                    }
                    ui.separator();
                    ui.text("Colors");
                    for (label, color) in [
                        ("Text", &mut value.colors.text),
                        ("Secondary text", &mut value.colors.secondary),
                        ("Background", &mut value.colors.background),
                        ("Surfaces", &mut value.colors.surface),
                        ("Headers", &mut value.colors.header),
                        ("Borders / grid", &mut value.colors.border),
                        ("Accent / selection", &mut value.colors.accent),
                    ] {
                        let _id = ui.push_id(label);
                        ui.color_edit3_config(label, color)
                            .flags(imgui::ColorEditFlags::NO_INPUTS)
                            .build();
                    }
                });
                if ui.button("Restore defaults") { value = Appearance::default(); }
                ui.same_line();
                close = ui.button("Close");
                if ui.is_key_pressed(imgui::Key::Escape)
                    && ui.is_window_focused_with_flags(imgui::FocusedFlags::ROOT_AND_CHILD_WINDOWS)
                    && !ui.is_any_item_active() { close = true; }
                if !self.error.is_empty() { ui.text_wrapped(format!("UI preferences: {}", self.error)); }
            });
        if value != before {
            self.edit(value);
        }
        self.open = open && !close;
        if !self.open {
            self.save();
        }
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        self.save();
    }
}
fn size_control(ui: &imgui::Ui, label: &str, value: &mut f32, min: f32, max: f32) {
    let _id = ui.push_id(label);
    ui.text(label);
    ui.set_next_item_width(-1.);
    ui.slider_config("##size", min, max)
        .display_format(imgui::NumericFormat::new("%.0f px").expect("valid pixel format"))
        .flags(imgui::SliderFlags::ALWAYS_CLAMP)
        .build(value);
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
