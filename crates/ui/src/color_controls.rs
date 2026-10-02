//! Compact I/O transform pickers and the shared authored-color editor.
#[cfg(test)]
#[path = "color_controls_tests.rs"]
mod tests;
use crate::sdk::{EditResponse, NumericProperty, imgui::Ui};
use fold_platform::color::{Choices, OutputTransform};

pub fn input(ui: &Ui, choices: &Choices, value: &mut String) -> bool {
    let mut changed = false;
    ui.set_next_item_width(
        (ui.content_region_avail()[0] - ui.calc_text_size("Input")[0] - 12.).max(40.),
    );
    if let Some(_combo) = ui.begin_combo("Input", value.as_str()) {
        for name in &choices.inputs {
            if ui.selectable_config(name).selected(name == value).build() && name != value {
                *value = name.clone();
                changed = true;
            }
        }
    }
    if let Some(error) = &choices.error {
        ui.text_wrapped(error);
    }
    changed
}
pub fn output(ui: &Ui, choices: &Choices, value: &mut OutputTransform) -> bool {
    let mut changed = false;
    ui.set_next_item_width(
        (ui.content_region_avail()[0] - ui.calc_text_size("Output")[0] - 12.).max(40.),
    );
    if let Some(_combo) = ui.begin_combo("Output", value.label()) {
        for choice in &choices.outputs {
            let selected = choice.display == value.display
                && choice.view == value.view
                && choice.look == value.look;
            if ui
                .selectable_config(choice.label())
                .selected(selected)
                .build()
                && !selected
            {
                let extensions = value.extensions.clone();
                *value = choice.clone();
                value.extensions = extensions;
                changed = true;
            }
        }
    }
    if let Some(error) = &choices.error {
        ui.text_wrapped(error);
    }
    changed
}
pub fn authored(ui: &Ui, value: &mut [f64; 4], aces: bool, response: &mut EditResponse) {
    // Clamp only the SDR picker presentation, never the stored working values.
    let mut picker = fold_platform::color::to_picker(*value, aces).map(|v| {
        if v.is_finite() {
            v.clamp(0., 1.) as f32
        } else {
            0.
        }
    });
    let original_picker = picker;
    let changed = ui
        .color_edit4_config("Color", &mut picker)
        .flags(crate::sdk::imgui::ColorEditFlags::NO_OPTIONS)
        .build();
    response.item(ui, changed);
    if changed {
        if picker[..3] == original_picker[..3] {
            value[3] = f64::from(picker[3]);
        } else {
            *value = fold_platform::color::from_picker(picker.map(f64::from), aces);
        }
    }
    crate::sdk::toolbar::tooltip(ui, "sRGB color picker. Right-click for working values.");
    if let Some(_popup) = ui.begin_popup_context_item_with_label(Some("working-color")) {
        for (i, label) in ["Red", "Green", "Blue", "Alpha"].iter().enumerate() {
            NumericProperty {
                id: label,
                label,
                unit: "",
                speed: 0.005,
                range: if i == 3 || !aces {
                    Some([0., 1.])
                } else {
                    None
                },
                default: None,
            }
            .draw(ui, &mut value[i], response);
        }
    }
}
