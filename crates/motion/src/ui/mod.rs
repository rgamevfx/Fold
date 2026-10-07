//! UI-only presentation over transactional graph edits.
mod asset_transfer;
mod canvas;
mod composition;
mod gradient;
mod graph;
mod group_interface;
mod inspector;
mod overlay;
mod path_attributes;
mod picking;
mod program;
mod relationships;
#[cfg(test)]
mod render_tests;
mod scene_inspector;
mod state;
#[cfg(test)]
mod tests;
mod tool_actions;
mod tools;
#[cfg(test)]
static IMGUI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
use fold_ui::sdk::PanelRegistry;
pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    let state = std::rc::Rc::new(std::cell::RefCell::new(state::State::default()));
    registry.register(
        crate::PACKAGE,
        canvas::Canvas {
            state: state.clone(),
            files: Default::default(),
            canvas: Default::default(),
            overlay: Default::default(),
        },
    )?;
    registry.register_inspector(crate::PACKAGE, inspector::Inspector { state })
}

use crate::keyed_parameters::input_label;

mod oscillator;
mod timing;

mod color_ramp;

mod simulation;

mod wrangle;

mod images;

mod image_bake;
