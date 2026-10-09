//! Shared parameter actions. Feature adapters own resolution and transactions.
use crate::sdk::imgui::Ui;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Copy,
    PasteRelative,
    EditExpression,
    GoToSource,
    RemoveReference,
}
pub fn items(ui: &Ui, driven: bool, editable: bool) -> Option<Action> {
    let mut action = None;
    for (label, value, enabled) in [
        ("Copy Parameter", Action::Copy, true),
        ("Paste Relative Reference", Action::PasteRelative, editable),
        ("Edit Expression", Action::EditExpression, editable),
        ("Go to Source", Action::GoToSource, driven),
        (
            "Remove Reference",
            Action::RemoveReference,
            driven && editable,
        ),
    ] {
        if ui.menu_item_enabled_selected_no_shortcut(label, false, enabled) {
            action = Some(value);
        }
    }
    action
}
pub fn context(ui: &Ui, driven: bool, editable: bool) -> Option<Action> {
    let _popup = ui.begin_popup_context_item_with_label(Some("parameter-reference"))?;
    items(ui, driven, editable)
}

thread_local! {
    // A parameter clipboard carries typed identity data, not a copied display value.
    static COPIED_PARAMETER: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
pub fn copy_parameter(value: String) {
    COPIED_PARAMETER.with(|clipboard| *clipboard.borrow_mut() = Some(value));
}
pub fn copied_parameter() -> Option<String> {
    COPIED_PARAMETER.with(|clipboard| clipboard.borrow().clone())
}
