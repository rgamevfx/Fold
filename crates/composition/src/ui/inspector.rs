use super::state::{self, Shared};
use fold_ui::sdk::{
    self, EditResponse, ExtensionUi, Panel, animated_property::AnimatedProperty, imgui,
};
pub(super) struct Inspector(pub Shared);
impl Panel for Inspector {
    fn id(&self) -> &'static str {
        crate::package::INSPECTOR
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::COMPOSITION)
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = self.0.borrow_mut();
        state.sync(host);
        let Some(id) = state.selected.first().copied() else {
            ui.text_disabled("Select a layer.");
            return;
        };
        let time = state::time(host);
        let auto_key = host.state().auto_key;
        let c = state.composition.as_mut().unwrap();
        let locked = c.locked(id);
        let Ok(layer) = c.layer_mut(id) else {
            return;
        };
        let mut edit = EditResponse::default();
        let disabled = ui.begin_disabled_with_cond(locked);
        ui.set_next_item_width(ui.content_region_avail()[0]);
        if ui.input_text("##layer-name", &mut layer.name).build() {
            edit.changed = true;
        }
        edit.finished |= ui.is_item_deactivated_after_edit();
        if ui.checkbox("Visible", &mut layer.state.visible) {
            edit.changed = true;
            edit.finished = true;
        }
        drop(disabled);
        ui.same_line();
        if ui.checkbox("Locked", &mut layer.state.locked) {
            edit.changed = true;
            edit.finished = true;
        }
        let disabled = ui.begin_disabled_with_cond(locked);
        AnimatedProperty {
            id: "scene.opacity",
            label: "Opacity",
            components: &[""],
            unit: "",
            range: Some([0., 1.]),
            color: None,
        }
        .draw(
            ui,
            std::slice::from_mut(&mut layer.state.opacity),
            &mut layer.state.animation,
            time,
            auto_key,
            &mut edit,
        );
        // Matrix coefficients retain exact existing affine semantics. Translation is
        // exposed first; advanced axes live under a disclosure, with the same keys.
        let mut translation = [layer.offset[4], layer.offset[5]];
        let mut channels = std::collections::BTreeMap::new();
        for i in 0..2 {
            if let Some(curve) = layer.animation.get(&format!("transform.{}", i + 4)) {
                channels.insert(format!("position.{i}"), curve.clone());
            }
        }
        AnimatedProperty {
            id: "position",
            label: "Position",
            components: &["X", "Y"],
            unit: "px",
            range: None,
            color: None,
        }
        .draw(
            ui,
            &mut translation,
            &mut channels,
            time,
            auto_key,
            &mut edit,
        );
        layer.offset[4..6].copy_from_slice(&translation);
        for i in 0..2 {
            let path = format!("transform.{}", i + 4);
            if let Some(curve) = channels.remove(&format!("position.{i}")) {
                layer.animation.insert(path, curve);
            } else {
                layer.animation.remove(&path);
            }
        }
        if ui.collapsing_header("Transform axes", imgui::TreeNodeFlags::empty()) {
            let state = &mut layer.state;
            AnimatedProperty {
                id: "transform",
                label: "Axes",
                components: &["XX", "YX", "XY", "YY"],
                unit: "",
                range: None,
                color: None,
            }
            .draw(
                ui,
                &mut state.offset[..4],
                &mut state.animation,
                time,
                auto_key,
                &mut edit,
            );
        }
        if layer.source.is_some() {
            let rate = match &layer.source {
                Some(fold_platform::image_source::Source::Output(reference)) => host
                    .video_info(reference.document)
                    .map(|info| info.rate)
                    .unwrap_or(layer.info.rate),
                _ => layer.info.rate,
            };
            let mut source_in = sdk::time_view::frame(layer.source_start, rate) as i32;
            ui.text("Source in (frames)");
            ui.set_next_item_width(ui.content_region_avail()[0]);
            if ui.input_int("##source-in", &mut source_in) && source_in >= 0 {
                if let Ok(value) = sdk::time_view::time(i64::from(source_in), rate) {
                    layer.source_start = value;
                    edit.changed = true;
                }
            }
            edit.finished |= ui.is_item_deactivated_after_edit();
        }
        drop(disabled);
        if let Some(fold_platform::image_source::Source::Output(source)) = &layer.source {
            let name = host
                .snapshot()
                .map(|s| {
                    fold_platform::browser::item_name(
                        &s,
                        fold_project::ItemId::Document(source.document),
                        &layer.name,
                    )
                })
                .unwrap_or_else(|| "Graph".into());
            ui.text_wrapped(format!("Source: {name}"));
            if ui.button("Edit layer graph") {
                if let Err(e) = state.open(host, id) {
                    state.error = e;
                }
            }
        }
        if ui.is_key_pressed(imgui::Key::Escape) && state.editing {
            state.cancel(host);
        } else {
            if edit.changed {
                state.preview(host);
            }
            if edit.finished {
                state.commit(host);
            }
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}
