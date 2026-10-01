//! Spatial graph authoring through the shared UI SDK. Canvas and inspector
//! share UI-only gesture state; all persistent changes are host transactions.
mod canvas;
mod graph;
mod inspector;
mod state;
#[cfg(test)]
mod tests;
use fold_ui::sdk::PanelRegistry;
use std::{cell::RefCell, rc::Rc};
pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    let state = Rc::new(RefCell::new(state::State::default()));
    registry.register(crate::PACKAGE, canvas::Canvas::new(state.clone()))?;
    registry.register_node_inspector(crate::PACKAGE, inspector::Inspector(state))
}
