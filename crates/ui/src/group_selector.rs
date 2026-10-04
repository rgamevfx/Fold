//! The same small letter control for editors, viewers and inspectors.
use dear_imgui_rs::Ui;
use fold_platform::workspace::LinkGroup;

pub(crate) fn draw(ui: &Ui, group: LinkGroup) -> Option<LinkGroup> {
    draw_choices(ui, group, |_| true)
}

pub(crate) fn draw_choices(
    ui: &Ui,
    group: LinkGroup,
    enabled: impl Fn(LinkGroup) -> bool,
) -> Option<LinkGroup> {
    let origin = ui.cursor_screen_pos();
    let unlinked = group == LinkGroup::Unlinked;
    ui.set_next_item_width(ui.calc_text_size("A")[0] + ui.frame_height() + 8.);
    let mut selected = None;
    if let Some(_combo) = ui.begin_combo(
        "##panel-link-group",
        if unlinked { " " } else { group.label() },
    ) {
        for choice in LinkGroup::ALL.into_iter().chain([LinkGroup::Unlinked]) {
            let available = enabled(choice);
            let _disabled = ui.begin_disabled_with_cond(!available);
            let label = if available {
                choice.label().to_owned()
            } else {
                format!("{} (in use)", choice.label())
            };
            if ui
                .selectable_config(&label)
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
    if unlinked {
        crate::sdk::toolbar::draw_icon(
            ui,
            crate::sdk::toolbar::ToolbarIcon::Unlink,
            origin,
            ui.frame_height(),
        );
    }
    crate::sdk::toolbar::tooltip(
        ui,
        if unlinked {
            "Unlinked"
        } else {
            "Panel link group"
        },
    );
    selected
}

#[cfg(test)]
#[path = "group_selector_tests.rs"]
mod tests;
