//! One quiet target control shared by editors and viewers.
use crate::sdk::imgui::Ui;
#[cfg(test)]
#[path = "target_selector_tests.rs"]
mod tests;

pub(crate) struct Choice<T> {
    pub target: T,
    pub label: String,
    pub error: Option<String>,
}
pub(crate) enum Action<T> {
    Choose(T),
    Back,
}
pub(crate) fn draw<T: Clone>(
    ui: &Ui,
    label: &str,
    back: bool,
    choices: &[Choice<T>],
) -> Option<Action<T>> {
    let mut action = None;
    if back {
        if ui.button("‹##target-back") {
            action = Some(Action::Back);
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Back to parent at its saved local time");
        }
        ui.same_line();
    }
    ui.set_next_item_width(ui.content_region_avail()[0].max(40.));
    if let Some(_combo) = ui.begin_combo("##target", label) {
        for (index, choice) in choices.iter().enumerate() {
            let _id = ui.push_id(index as i32);
            let _disabled = ui.begin_disabled_with_cond(choice.error.is_some());
            if ui.selectable(&choice.label) {
                action = Some(Action::Choose(choice.target.clone()));
            }
            if let Some(error) = &choice.error {
                ui.same_line();
                ui.text_disabled(error);
            }
        }
        if choices.is_empty() {
            ui.text_disabled("No compatible project targets");
        }
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(label);
    }
    action
}
