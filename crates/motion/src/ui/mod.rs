//! UI-only presentation over transactional graph edits.
mod animation;
mod canvas;
mod gradient;
mod graph;
mod group_interface;
mod inspector;
mod overlay;
mod path_attributes;
mod picking;
mod relationships;
#[cfg(test)]
mod render_tests;
mod scene;
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
            canvas: Default::default(),
            overlay: Default::default(),
            animation: Default::default(),
            network_animation: Default::default(),
        },
    )?;
    registry.register_inspector(crate::PACKAGE, inspector::Inspector { state })
}

fn input_label(motion: &crate::Motion, node: &crate::graph::Node, key: &str) -> String {
    node.settings::<crate::nodes::interface::GroupSettings>()
        .ok()
        .and_then(|s| s.group)
        .and_then(|id| motion.groups.get(&id))
        .and_then(|g| g.inputs.iter().find(|p| p.id == key))
        .map(|p| inspector::property_label(&p.name))
        .unwrap_or_else(|| inspector::property_label(key))
}

mod network_scope;

mod oscillator;

mod color_ramp;
