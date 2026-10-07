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
        drop(disabled);
        let open_network =
            layer.source.as_ref().is_some_and(|source| {
                matches!(source, fold_platform::image_source::Source::Output(_))
            }) && ui.small_button("Open Network");
        let disabled = ui.begin_disabled_with_cond(locked);
        if !layer.adjustment {
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
            let (mut scale, mut rotation, shear) = crate::layer::components(layer.offset);
            let original = (scale, rotation);
            let state = &mut layer.state;
            AnimatedProperty {
                id: "scene.scale",
                label: "Scale",
                components: &["X", "Y"],
                unit: "",
                range: None,
                color: None,
            }
            .draw(
                ui,
                &mut scale,
                &mut state.animation,
                time,
                auto_key,
                &mut edit,
            );
            AnimatedProperty {
                id: "scene.rotation",
                label: "Rotation",
                components: &[""],
                unit: "°",
                range: None,
                color: None,
            }
            .draw(
                ui,
                std::slice::from_mut(&mut rotation),
                &mut state.animation,
                time,
                auto_key,
                &mut edit,
            );
            if original != (scale, rotation) {
                crate::layer::set_components(&mut state.offset, scale, rotation, shear);
            }
        }
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
        let effect_navigation = sdk::effects::stack(ui, host, &mut layer.effects, time, &mut edit);
        drop(disabled);
        if open_network {
            if let Err(e) = state.open(host, id) {
                state.error = e;
            }
        }
        if let Some(source) = effect_navigation {
            state.cancel(host);
            host.command(fold_platform::desktop::DesktopCommand::Navigate(
                fold_platform::desktop::ViewLocation {
                    document: source.document,
                    time,
                    label: "Effect".into(),
                },
            ));
            return;
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
