//! Floating application preferences. Live edits never enter project history.
use crate::{
    sdk::{appearance::Appearance, imgui, typography::Typography},
    settings_store::{Event, Preferences, Store},
};
use std::time::{Duration, Instant};

pub(crate) struct Settings {
    pub(crate) open: bool,
    pub(crate) preferences: Preferences,
    pub(crate) ready: bool,
    pub(crate) clear_cache: bool,
    pub(crate) cache_usage: (usize, usize),
    pub(crate) retention_usage: [(u64, u64); 3],
    pub(crate) retention_error: Option<String>,
    pub(crate) clear_retention: Vec<crate::preview::Tier>,
    pub(crate) adapter: String,
    pub(crate) active_application: fold_platform::application::Application,
    pub(crate) application_category: bool,
    pub(crate) application_draft: fold_platform::application::Application,
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
            preferences: Preferences::default(),
            ready: store.is_none(),
            clear_cache: false,
            cache_usage: (0, 256 * 1024 * 1024),
            retention_usage: [(0, 0); 3],
            retention_error: None,
            clear_retention: vec![],
            adapter: String::new(),
            active_application: Default::default(),
            application_category: false,
            application_draft: Default::default(),
            focus: false,
            typography,
            store,
            changed: None,
            edited: false,
            error: String::new(),
        }
    }
    pub(crate) fn report_error(&mut self, error: String) {
        self.error = error;
        self.show();
    }
    pub(crate) fn show(&mut self) {
        self.open = true;
        self.focus = true;
    }
    pub(crate) fn prepare_frame(&mut self, context: &mut imgui::Context) {
        if let Some(store) = &mut self.store {
            while let Some(event) = store.poll() {
                match event {
                    Event::Loaded(Ok(mut value)) => {
                        if self.edited {
                            value.ui = self.typography.appearance();
                        }
                        let _ = self.typography.set_appearance(value.ui);
                        self.application_draft = value.application.clone();
                        self.preferences = value;
                        self.ready = true;
                    }
                    Event::Loaded(Err(error)) | Event::Saved(Err(error)) => {
                        self.error = error;
                        self.open = true;
                        self.focus = true;
                    }
                    Event::Saved(Ok(())) => self.error.clear(),
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
        if !self.ready {
            return;
        }
        if self.changed.take().is_some()
            && let Some(store) = &mut self.store
        {
            self.preferences.ui = self.typography.appearance();
            store.save(self.preferences.clone());
        }
    }
    pub(crate) fn changed(&mut self) {
        self.changed = Some(Instant::now());
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
                    if ui.selectable_config("Application").selected(self.application_category).build() { self.application_category = true; }
                    if ui.selectable_config("UI").selected(!self.application_category).build() { self.application_category = false; }
                });
                ui.same_line();
                ui.child_window("settings-content").size([0., height]).build(ui, || {
                    if self.application_category {
                        self.draw_application(ui);
                        return;
                    }
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
                if ui.button("Restore defaults") {
                    if self.application_category && self.ready { self.application_draft = Default::default(); }
                    else if !self.application_category { value = Appearance::default(); }
                }
                ui.same_line();
                close = ui.button("Close");
                if ui.is_key_pressed(imgui::Key::Escape)
                    && ui.is_window_focused_with_flags(imgui::FocusedFlags::ROOT_AND_CHILD_WINDOWS)
                    && !ui.is_any_item_active() { close = true; }
                if !self.error.is_empty() { ui.text_wrapped(format!("Preferences: {}", self.error)); }
            });
        if value != before {
            self.edit(value);
        }
        self.open = open && !close;
        if !self.open {
            self.save();
        }
    }
    fn draw_application(&mut self, ui: &imgui::Ui) {
        let _disabled = ui.begin_disabled_with_cond(!self.ready);
        let app = &mut self.application_draft;
        ui.text("Performance");
        if !self.adapter.is_empty() {
            ui.text_wrapped(format!("GPU: {}", self.adapter));
        }
        ui.checkbox("Automatic budgets", &mut app.automatic);
        if app.automatic {
            ui.text_wrapped("Conservative defaults: 1024 MiB GPU, 256 MiB GPU playback cache, 768 MiB CPU image processing.");
        } else {
            memory_control(ui, "GPU memory budget (MiB)", &mut app.gpu_mib, 128, 65536);
            app.viewer_mib = app.viewer_mib.min(app.gpu_mib / 2);
            memory_control(
                ui,
                "GPU playback cache (MiB)",
                &mut app.viewer_mib,
                32,
                (app.gpu_mib / 2).min(32768),
            );
            memory_control(
                ui,
                "CPU image processing (MiB)",
                &mut app.ram_mib,
                128,
                262144,
            );
        }
        ui.text_wrapped("GPU playback cache is included in the GPU budget. CPU image processing covers decoded and working images.");
        ui.separator();
        ui.text("Playback retention");
        memory_control(
            ui,
            "Viewer RAM cache (MiB)",
            &mut app.viewer_ram_mib,
            0,
            262144,
        );
        ui.text_wrapped("0 disables retention. BC7 stores lossy playback pixels. Cache writes run while playback is paused; rendering and export retain their original precision.");
        for (label, limit, folder, tier, index) in [
            (
                "Timeline",
                &mut app.timeline_disk_gib,
                &mut app.timeline_cache_folder,
                crate::preview::Tier::Timeline,
                1,
            ),
            (
                "Composition / source",
                &mut app.composition_disk_gib,
                &mut app.composition_cache_folder,
                crate::preview::Tier::Composition,
                2,
            ),
        ] {
            let _id = ui.push_id(label);
            memory_control(ui, &format!("{label} disk cache (GiB)"), limit, 0, 1048576);
            ui.text(format!("{label} cache folder"));
            ui.set_next_item_width(-1.);
            ui.input_text("##folder", folder)
                .hint("Default cache folder")
                .build();
            let (used, budget) = self.retention_usage[index];
            ui.text(format!(
                "{:.2} / {:.2} GiB",
                used as f64 / 1073741824.,
                budget as f64 / 1073741824.
            ));
            if ui.button("Clear disk cache") {
                self.clear_retention.push(tier);
            }
        }
        let (used, budget) = self.retention_usage[0];
        ui.text(format!(
            "Viewer RAM: {:.0} / {:.0} MiB",
            used as f64 / 1048576.,
            budget as f64 / 1048576.
        ));
        if ui.button("Clear viewer RAM cache") {
            self.clear_retention.push(crate::preview::Tier::Ram);
        }
        if let Some(error) = &self.retention_error {
            ui.text_wrapped(format!("Playback retention: {error}"));
        }
        ui.separator();
        ui.text("Media scratch");
        ui.text("Media cache folder");
        ui.set_next_item_width(-1.);
        ui.input_text("##cache-folder", &mut app.cache_folder)
            .hint("Default cache folder")
            .build();
        ui.text_wrapped(format!("Folder: {}", app.cache_path().display()));
        ui.text_wrapped("Temporary media files are removed when released. Playback caches use separate versioned subfolders.");
        ui.text(format!(
            "GPU playback cache: {:.0} / {:.0} MiB",
            self.cache_usage.0 as f64 / 1048576.,
            self.cache_usage.1 as f64 / 1048576.
        ));
        if ui.button("Clear unused GPU cache") {
            self.clear_cache = true;
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Stops cache preparation and cached replay, then frees unused frames. Displayed and in-flight frames remain protected.");
        }
        if self.preferences.application != self.active_application {
            ui.text_wrapped("Restart Fold to apply budgets and cache folders.");
        }
        if *app != self.preferences.application && ui.button("Apply application settings") {
            match app.validate() {
                Ok(()) => {
                    self.preferences.application = app.clone();
                    self.error.clear();
                    self.changed();
                }
                Err(error) => self.error = error,
            }
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

fn memory_control(ui: &imgui::Ui, label: &str, value: &mut u32, min: u32, max: u32) {
    ui.text(label);
    ui.set_next_item_width(-1.);
    if ui.input_scalar(format!("##{label}"), value).step(1).build() {
        *value = (*value).clamp(min, max);
    }
}
