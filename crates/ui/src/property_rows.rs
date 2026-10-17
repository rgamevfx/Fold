//! Compact inspector rows. Labels and controls share the animated-property grid.
use crate::sdk::EditResponse;
use crate::sdk::imgui::Ui;

pub fn row(ui: &Ui, label: &str) -> f32 {
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    let label_width = (width * 0.30).clamp(ui.frame_height(), 96.);
    ui.align_text_to_frame_padding();
    ui.text(label);
    ui.same_line();
    ui.set_cursor_screen_pos([
        start[0] + label_width + ui.clone_style().item_spacing()[0],
        start[1],
    ]);
    let remaining = (width - label_width - ui.clone_style().item_spacing()[0]).max(20.);
    ui.set_next_item_width(remaining);
    remaining
}
pub fn combo<T: Copy + PartialEq>(
    ui: &Ui,
    label: &str,
    value: &mut T,
    choices: &[(T, &str)],
    response: &mut EditResponse,
) {
    let _id = ui.push_id(label);
    row(ui, label);
    let preview = choices
        .iter()
        .find(|(v, _)| v == value)
        .map(|(_, label)| *label)
        .unwrap_or("Custom");
    if let Some(_combo) = ui.begin_combo("##value", preview) {
        for &(choice, label) in choices {
            if ui
                .selectable_config(label)
                .selected(choice == *value)
                .build()
            {
                *value = choice;
                response.changed = true;
                response.finished = true;
            }
        }
    }
}
pub fn check(ui: &Ui, label: &str, value: &mut bool, response: &mut EditResponse) {
    if ui.checkbox(label, value) {
        response.changed = true;
        response.finished = true;
    }
}
