//! Compact source collection controls. Choices use Project names and capability
//! metadata; they never probe or decode files on the UI thread.
use crate::nodes::image::{Entry, Settings};
use fold_platform::{
    desktop::DesktopClient,
    image_source::{Outside, Source},
};
use fold_ui::sdk::{EditResponse, imgui::Ui};
pub fn draw(
    ui: &Ui,
    host: &dyn DesktopClient,
    node: &mut crate::graph::Node,
    info: &fold_media::VideoInfo,
    response: &mut EditResponse,
) -> bool {
    if matches!(
        node.kind.as_str(),
        "fold.motion.image_card" | "fold.motion.image_cards"
    ) {
        let current = node.settings["fit"]
            .as_str()
            .unwrap_or("Contain")
            .to_owned();
        if let Some(_combo) = ui.begin_combo("Fit", &current) {
            for choice in ["Contain", "Cover", "Stretch"] {
                if ui.selectable(choice) {
                    node.settings["fit"] = choice.into();
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
        return true;
    }
    if node.kind == "fold.motion.sample_image" {
        let current = node.settings["channel"]
            .as_str()
            .unwrap_or("Luminance")
            .to_owned();
        if let Some(_combo) = ui.begin_combo("Channel", &current) {
            for channel in ["Luminance", "Red", "Green", "Blue", "Alpha"] {
                if ui.selectable(channel) {
                    node.settings["channel"] = channel.into();
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
        return true;
    }
    if node.kind != "fold.motion.image" {
        return false;
    }
    let Ok(mut settings) = node.settings::<Settings>() else {
        ui.text_wrapped("Invalid image source settings");
        return true;
    };
    let Some(snapshot) = host.snapshot() else {
        ui.text_wrapped("Open a project to choose image sources.");
        return true;
    };
    let mut choices = Vec::new();
    for item in &snapshot.state().organization.items {
        let entry = match item.id {
            fold_project::ItemId::Asset(id) => {
                let Ok(source_info) = fold_platform::image_source::asset_info(&snapshot, id, info)
                else {
                    continue;
                };
                Entry {
                    source: Source::Asset(id),
                    info: source_info,
                    offset: fold_foundation::Time::ZERO,
                    extensions: Default::default(),
                }
            }
            fold_project::ItemId::Document(id) => {
                let Ok(info) = host.video_info(id) else {
                    continue;
                };
                Entry {
                    source: Source::Output(fold_project::DocumentRef {
                        document: id,
                        output: "video".into(),
                        extensions: Default::default(),
                    }),
                    info,
                    offset: fold_foundation::Time::ZERO,
                    extensions: Default::default(),
                }
            }
        };
        choices.push((item.name.clone(), entry));
    }
    let mut changed = false;
    let mut remove = None;
    for (i, entry) in settings.sources.iter_mut().enumerate() {
        let _id = ui.push_id(&i.to_string());
        let name = choices
            .iter()
            .find(|(_, v)| v.source == entry.source)
            .map(|(n, _)| n.as_str())
            .unwrap_or("Missing source");
        ui.set_next_item_width((ui.content_region_avail()[0] - 30.).max(70.));
        if let Some(_combo) = ui.begin_combo("##source", name) {
            for (name, value) in &choices {
                if ui.selectable(name) {
                    let offset = entry.offset;
                    let extensions = std::mem::take(&mut entry.extensions);
                    *entry = value.clone();
                    entry.offset = offset;
                    entry.extensions = extensions;
                    changed = true;
                }
            }
        }
        ui.same_line();
        if ui.small_button("×") {
            remove = Some(i);
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Remove source from collection");
        }
        let mut frame = entry
            .offset
            .to_ticks(
                entry.info.rate[0],
                entry.info.rate[1],
                fold_foundation::Rounding::Nearest,
            )
            .unwrap_or(0) as i32;
        ui.set_next_item_width((ui.content_region_avail()[0] - 110.).max(50.));
        if ui.input_int("Offset (frames)", &mut frame) {
            if let Ok(time) = fold_foundation::Time::new(
                i64::from(frame) * i64::from(entry.info.rate[1]),
                entry.info.rate[0],
            ) {
                entry.offset = time;
                changed = true;
            }
        }
    }
    if let Some(i) = remove {
        settings.sources.remove(i);
        changed = true;
    }
    if settings.sources.len() < 64 {
        if let Some(_combo) = ui.begin_combo("Add source", "Choose from Project") {
            for (name, value) in &choices {
                if ui.selectable(name) {
                    settings.sources.push(value.clone());
                    changed = true;
                }
            }
        }
    }
    if settings.sources.is_empty() {
        ui.text_wrapped("Choose an image, video or composition from Project.");
    }
    let name = match settings.outside {
        Outside::Hold => "Hold",
        Outside::Loop => "Loop",
        Outside::Black => "Transparent",
        Outside::Error => "Error",
    };
    if let Some(_combo) = ui.begin_combo("Outside range", name) {
        for (name, value) in [
            ("Hold", Outside::Hold),
            ("Loop", Outside::Loop),
            ("Transparent", Outside::Black),
            ("Error", Outside::Error),
        ] {
            if ui.selectable(name) {
                settings.outside = value;
                changed = true;
            }
        }
    }
    if changed {
        node.settings["sources"] = serde_json::to_value(settings.sources).unwrap();
        node.settings["outside"] = serde_json::to_value(settings.outside).unwrap();
        response.changed = true;
        response.finished = true;
    }
    true
}
