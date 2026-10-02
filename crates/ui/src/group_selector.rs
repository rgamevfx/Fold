//! The same small letter control for editors, viewers and inspectors.
use dear_imgui_rs::{StyleColor, Ui};
use fold_platform::workspace::LinkGroup;

pub(crate) fn draw(ui: &Ui, group: LinkGroup, source: bool) -> Option<LinkGroup> {
    let _source = source.then(|| {
        ui.push_style_color(
            StyleColor::FrameBg,
            ui.style_color(StyleColor::HeaderActive),
        )
    });
    ui.set_next_item_width(ui.calc_text_size("A")[0] + ui.frame_height() + 8.);
    let mut selected = None;
    if let Some(_combo) = ui.begin_combo("##panel-link-group", group.label()) {
        for choice in LinkGroup::ALL {
            if ui
                .selectable_config(choice.label())
                .selected(choice == group)
                .build()
            {
                selected = Some(choice);
            }
            if choice == group {
                ui.set_item_default_focus();
            }
        }
    }
    if ui.is_item_hovered() || ui.is_item_focused() {
        ui.tooltip_text("Panel link group");
    }
    selected
}

#[cfg(test)]
#[path = "group_selector_tests.rs"]
mod tests;
