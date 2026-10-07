//! Shared floating project/document settings. Drafts are pinned to a revision.
use crate::sdk::{
    imgui::{self, Ui},
    toolbar::{self, ToolbarIcon},
};
use fold_foundation::DocumentId;
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand},
    settings::*,
};
use fold_project::Revision;

pub(crate) fn button(ui: &Ui, host: &mut dyn DesktopClient, document: Option<DocumentId>) {
    let enabled = document.is_some();
    let _disabled = ui.begin_disabled_with_cond(!enabled);
    if toolbar::icon_button(
        ui,
        "document-settings",
        ToolbarIcon::Settings,
        "Item settings",
    ) {
        host.command(DesktopCommand::OpenSettings(document));
    }
}

pub(crate) struct Dialog {
    target: Option<DocumentId>,
    title: String,
    base: Revision,
    epoch: u64,
    defaults: ProjectDefaults,
    document: Option<DocumentSettings>,
    color_path: Option<String>,
    original_color: Option<String>,
    bundled: bool,
    error: String,
    pending: bool,
}
impl Dialog {
    pub fn new(host: &dyn DesktopClient, target: Option<DocumentId>) -> Result<Self, String> {
        let snapshot = host.snapshot().ok_or("No project is open")?;
        let document = target
            .map(|id| {
                host.document_settings(id)
                    .and_then(|v| v.ok_or("Settings unavailable".into()))
            })
            .transpose()?;
        let title = if let Some(id) = target {
            let name = fold_platform::browser::item_name(
                &snapshot,
                fold_project::ItemId::Document(id),
                "Document",
            );
            format!("{name} — Settings###fold.document-settings")
        } else {
            "Project Settings###fold.document-settings".into()
        };
        let color_path = fold_platform::color::project(&snapshot)?.map(|c| match c.config {
            fold_platform::color::ConfigSource::Bundled { .. } => String::new(),
            fold_platform::color::ConfigSource::External { path, .. } => {
                path.to_string_lossy().into_owned()
            }
        });
        Ok(Self {
            target,
            title,
            base: snapshot.revision(),
            epoch: host.workspace_epoch(),
            defaults: ProjectDefaults::read(&snapshot)?,
            document,
            bundled: color_path.as_ref().is_some_and(String::is_empty),
            original_color: color_path.clone(),
            color_path,
            error: String::new(),
            pending: false,
        })
    }
    /// Returns false when closed; Apply retains the window until a successful commit.
    pub fn draw(&mut self, ui: &Ui, host: &mut dyn DesktopClient) -> bool {
        let Some(snapshot) = host.snapshot() else {
            return false;
        };
        if host.workspace_epoch() != self.epoch {
            return false;
        }
        if self
            .target
            .is_some_and(|id| !snapshot.state().documents.contains_key(&id))
        {
            return false;
        }
        if self.pending && !host.state().busy {
            self.pending = false;
            let matches = if let Some(id) = self.target {
                host.document_settings(id)
                    .ok()
                    .flatten()
                    .is_some_and(|actual| {
                        self.document.as_ref().is_some_and(|draft| {
                            actual.format.width == draft.format.width
                                && actual.format.height == draft.format.height
                                && actual.format.rate == draft.format.rate
                                && actual.output_duration == draft.output_duration
                                && actual.timeline == draft.timeline
                                && (draft.timeline || actual.format.frames == draft.format.frames)
                        })
                    })
            } else {
                ProjectDefaults::read(&snapshot).is_ok_and(|actual| actual == self.defaults)
                    && fold_platform::color::project(&snapshot)
                        .ok()
                        .is_some_and(|color| {
                            color.map(|c| match c.config {
                                fold_platform::color::ConfigSource::Bundled { .. } => String::new(),
                                fold_platform::color::ConfigSource::External { path, .. } => {
                                    path.to_string_lossy().into_owned()
                                }
                            }) == self.color_path.as_ref().map(|p| {
                                if self.bundled {
                                    String::new()
                                } else {
                                    p.clone()
                                }
                            })
                        })
            };
            if matches {
                return false;
            }
            self.error = host.state().status.clone();
        }
        let stale = snapshot.revision() != self.base;
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let mut reload = false;
        ui.window(&self.title).opened(&mut open)
            .size(if self.target.is_none() { [440., 620.] } else { [400., 470.] }, imgui::Condition::FirstUseEver)
            .size_constraints([320., 300.], [760., 900.])
            .flags(imgui::WindowFlags::NO_DOCKING | imgui::WindowFlags::NO_COLLAPSE)
            .build(|| {
                let notice = if self.pending { "Validating settings…" }
                    else if !self.error.is_empty() { &self.error }
                    else if stale { "Project changed. Reload settings before applying." }
                    else { "" };
                let notice_height = if notice.is_empty() { 0. } else {
                    (ui.calc_text_size_with_opts(notice, false, ui.content_region_avail()[0] - 20.)[1] + 10.).min(100.)
                };
                let footer = ui.frame_height_with_spacing() + notice_height;
                ui.child_window("settings-fields").size([0., (ui.content_region_avail()[1] - footer).max(80.)]).build(ui, || {
                    let _disabled = ui.begin_disabled_with_cond(self.pending);
                    if let Some(settings) = &mut self.document {
                        ui.text("Format");
                        let old_rate = settings.format.rate;
                        format(ui, &mut settings.format, !settings.timeline);
                        if settings.timeline && settings.format.rate != old_rate {
                            if let Some(duration) = settings.output_duration {
                                if let Ok(frames) = duration.to_ticks(settings.format.rate[0], settings.format.rate[1], fold_foundation::Rounding::Ceil) {
                                    settings.format.frames = frames.clamp(1, u32::MAX as i64) as u32;
                                }
                            }
                        }
                        if settings.timeline {
                            ui.separator();
                            ui.text("Output range");
                            let mut automatic = settings.output_duration.is_none();
                            if ui.checkbox("Follow content end", &mut automatic) {
                                settings.output_duration = if automatic { None } else { settings.format.time(settings.format.frames).ok() };
                            }
                            if settings.output_duration.is_some() {
                                let old_frames = settings.format.frames;
                                duration(ui, &mut settings.format);
                                if settings.format.frames != old_frames { settings.output_duration = settings.format.time(settings.format.frames).ok(); }
                            }
                            ui.text_wrapped("The output starts at zero. Changing its duration keeps all clips and their timing intact.");
                        } else {
                            ui.text_wrapped("Format changes keep existing objects, layers and animation timing intact.");
                        }
                    } else {
                        ui.text("New item defaults");
                        format(ui, &mut self.defaults.format, true);
                        ui.text_wrapped("New standalone items use these defaults. Timelines use resolution and frame rate, with duration following the content end. Existing items keep their settings.");
                        ui.separator();
                        ui.text("Paths");
                        ui.text("Cache folder");
                        ui.set_next_item_width(-1.);
                        ui.input_text("##project-cache", &mut self.defaults.cache_folder).hint("Project-relative or absolute folder").build();
                        ui.text_wrapped("For future Cache node baked outputs · not yet used. Relative paths will resolve from the project folder.");
                        ui.separator();
                        ui.text("Color management");
                        if let Some(path) = &mut self.color_path {
                            ui.text("Working space: ACEScg");
                            ui.text("OCIO configuration");
                            ui.set_next_item_width(-1.);
                            if let Some(_combo) = ui.begin_combo("##config", if self.bundled { "Bundled ACES" } else { "External configuration" }) {
                                if ui.selectable("Bundled ACES") { self.bundled = true; }
                                if ui.selectable("External configuration") { self.bundled = false; }
                            }
                            if !self.bundled {
                                ui.set_next_item_width(-1.);
                                ui.input_text("##ocio-path", path).hint("Absolute path to config.ocio").build();
                            }
                            ui.text_wrapped("Working space is fixed. Footage input interpretation stays with each source; output transforms stay with Delivery.");
                        } else { ui.text_wrapped("Legacy color interpretation is retained for this project."); }
                        if let Some(error) = &host.state().color_choices.error { ui.text_wrapped(error); }
                    }
                });
                if notice_height > 0. { ui.child_window("settings-notice").size([0., notice_height]).build(ui, || ui.text_wrapped(notice)); }
                {
                    let _disabled = ui.begin_disabled_with_cond(stale || self.pending || host.state().busy || host.state().transient);
                    apply = ui.button("Apply");
                }
                ui.same_line();
                cancel = ui.button(if self.pending { "Close" } else { "Cancel" }) || ui.is_key_pressed(imgui::Key::Escape);
                if stale && !self.pending { ui.same_line(); reload = ui.button("Reload"); }
            });
        if cancel || !open {
            return false;
        }
        if reload {
            match Self::new(host, self.target) {
                Ok(value) => *self = value,
                Err(e) => self.error = e,
            }
        }
        if apply {
            match self.request() {
                Ok(request) => {
                    self.error.clear();
                    self.pending = true;
                    host.command(DesktopCommand::Extension(request));
                }
                Err(e) => self.error = e,
            }
        }
        true
    }
    fn request(&self) -> Result<fold_platform::packages::CommandRequest, String> {
        let (id, arguments) = if let Some(document) = self.target {
            let settings = self.document.clone().ok_or("Missing document settings")?;
            settings.format.validate()?;
            (
                DOCUMENT_COMMAND,
                serde_json::to_vec(&DocumentEdit { document, settings }),
            )
        } else {
            self.defaults.format.validate()?;
            let desired = self.color_path.as_ref().map(|p| {
                if self.bundled {
                    String::new()
                } else {
                    p.clone()
                }
            });
            if !self.bundled
                && desired
                    .as_ref()
                    .is_some_and(|p| !std::path::Path::new(p).is_absolute())
            {
                return Err("Choose an absolute OCIO configuration path".into());
            }
            (
                PROJECT_COMMAND,
                serde_json::to_vec(&ProjectEdit {
                    defaults: self.defaults.clone(),
                    color_config: if desired != self.original_color {
                        desired
                    } else {
                        None
                    },
                }),
            )
        };
        Ok(fold_platform::packages::CommandRequest {
            id: id.into(),
            base: self.base,
            arguments: arguments.map_err(|e| e.to_string())?,
        })
    }
}
fn positive(ui: &Ui, label: &str, value: &mut u32) {
    ui.text(label);
    ui.set_next_item_width(-1.);
    ui.input_scalar(format!("##{label}"), value).step(1).build();
}
fn duration(ui: &Ui, info: &mut fold_media::VideoInfo) {
    positive(ui, "Duration (frames)", &mut info.frames);
    if info.rate.iter().all(|v| *v > 0)
        && ui.collapsing_header("Timecode", imgui::TreeNodeFlags::empty())
    {
        let fps = (f64::from(info.rate[0]) / f64::from(info.rate[1]))
            .round()
            .max(1.) as u32;
        let mut parts = [
            (info.frames / fps / 3600) as i32,
            ((info.frames / fps / 60) % 60) as i32,
            ((info.frames / fps) % 60) as i32,
            (info.frames % fps) as i32,
        ];
        ui.set_next_item_width(-1.);
        if ui.input_int4("##timecode", &mut parts).build()
            && parts.iter().all(|v| *v >= 0)
            && parts[1] < 60
            && parts[2] < 60
            && (parts[3] as u32) < fps
        {
            let frames = (((parts[0] as u128 * 60 + parts[1] as u128) * 60 + parts[2] as u128)
                * u128::from(fps))
                + parts[3] as u128;
            if let Ok(frames) = u32::try_from(frames) {
                info.frames = frames;
            }
        }
        ui.text_disabled("Hours · minutes · seconds · frames (non-drop)");
    }
    if let Ok(time) = info.time(info.frames) {
        ui.text_disabled(format!(
            "{:.3} seconds",
            time.numerator() as f64 / time.denominator() as f64
        ));
    }
}
fn format(ui: &Ui, info: &mut fold_media::VideoInfo, show_duration: bool) {
    positive(ui, "Width (px)", &mut info.width);
    positive(ui, "Height (px)", &mut info.height);
    ui.text("Frame rate");
    let presets = [
        ("23.976", [24000, 1001]),
        ("24", [24, 1]),
        ("25", [25, 1]),
        ("29.97", [30000, 1001]),
        ("30", [30, 1]),
        ("50", [50, 1]),
        ("59.94", [60000, 1001]),
        ("60", [60, 1]),
    ];
    let label = presets
        .iter()
        .find(|(_, rate)| *rate == info.rate)
        .map(|(label, _)| format!("{label} fps"))
        .unwrap_or_else(|| format!("{}/{} fps", info.rate[0], info.rate[1]));
    ui.set_next_item_width(-1.);
    if let Some(_combo) = ui.begin_combo("##fps", label) {
        for (name, rate) in presets {
            if ui.selectable(name) {
                info.rate = rate;
            }
        }
    }
    if ui.collapsing_header("Exact frame rate", imgui::TreeNodeFlags::empty()) {
        positive(ui, "Numerator", &mut info.rate[0]);
        positive(ui, "Denominator", &mut info.rate[1]);
    }
    if show_duration {
        duration(ui, info);
    }
}

#[cfg(test)]
#[path = "document_settings_tests.rs"]
mod tests;
