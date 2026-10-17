//! Shared format-dependent movie controls. Callers own persistence and transactions.
use crate::sdk::{
    EditResponse,
    imgui::Ui,
    property_rows::{check, combo, row},
};
use fold_media::encoding::{H264Preset, ProResProfile};
use fold_platform::delivery::{FileType, Settings};

pub fn file_type(ui: &Ui, settings: &mut Settings, movies_only: bool, response: &mut EditResponse) {
    let mut selected = settings.file_type;
    let choices: Vec<_> = FileType::ALL
        .into_iter()
        .filter(|f| !movies_only || *f != FileType::OpenExr)
        .map(|f| (f, f.label()))
        .collect();
    combo(ui, "File type", &mut selected, &choices, response);
    settings.set_file_type(selected);
}
pub fn movie_options(ui: &Ui, settings: &mut Settings, response: &mut EditResponse) {
    match settings.file_type {
        FileType::OpenExr => {}
        FileType::QuickTime => {
            row(ui, "Codec");
            ui.text("Apple ProRes");
            let choices: Vec<_> = ProResProfile::ALL
                .into_iter()
                .map(|p| (p, p.label()))
                .collect();
            combo(
                ui,
                "Profile",
                &mut settings.prores.profile,
                &choices,
                response,
            );
            if settings.prores.profile.supports_alpha() {
                check(ui, "Include alpha", &mut settings.prores.alpha, response);
            }
        }
        FileType::Mp4 => {
            row(ui, "Codec");
            ui.text("H.264");
            row(ui, "Quality (CRF)");
            let changed = crate::sdk::imgui::Drag::new("##crf")
                .range(0, 51)
                .build(ui, &mut settings.h264.crf);
            response.item(ui, changed);
            if ui.is_item_hovered() {
                ui.tooltip_text("Lower values give higher quality and larger files. 18 is a high-quality starting point.");
            }
            let choices: Vec<_> = H264Preset::ALL
                .into_iter()
                .map(|p| (p, p.label()))
                .collect();
            combo(ui, "Preset", &mut settings.h264.preset, &choices, response);
        }
    }
    if settings.movie().is_some_and(|m| !m.preserves_alpha()) {
        ui.text_disabled("RGB · black background");
    }
}
