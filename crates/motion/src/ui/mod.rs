//! UI-only presentation over transactional graph edits.
mod canvas;
mod curves;
mod graph;
mod inspector;
mod overlay;
mod state;
#[cfg(test)]
mod tests;
#[cfg(test)]
static IMGUI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
use fold_ui::sdk::PanelRegistry;
pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    let state = std::rc::Rc::new(std::cell::RefCell::new(state::State::default()));
    registry.register(
        crate::PACKAGE,
        canvas::Canvas {
            state: state.clone(),
            canvas: Default::default(),
            overlay: Default::default(),
        },
    )?;
    registry.register_node_inspector(
        crate::PACKAGE,
        inspector::Inspector {
            state,
            curves: Default::default(),
        },
    )
}
