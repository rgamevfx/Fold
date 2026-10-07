//! Published controls edit the retained definition; instances migrate explicitly.
use crate::{Composite, reusable::KEY};
use fold_ui::sdk::imgui::Ui;
pub(super) fn draw(ui: &Ui, graph: &mut Composite) -> Result<bool, String> {
    if !graph.extensions.contains_key(KEY) {
        return Ok(false);
    }
    let Some(_tree) = ui.tree_node("Published controls") else {
        return Ok(false);
    };
    let mut definition = graph.reusable()?;
    ui.text_disabled(format!(
        "Interface version {}",
        definition.interface.version
    ));
    let mut remove = None;
    for control in &definition.interface.controls {
        let _id = ui.push_id(&control.id);
        ui.text_wrapped(&control.label);
        if ui.small_button("Unpublish control") {
            remove = Some(control.id.clone());
        }
    }
    let Some(id) = remove else {
        return Ok(false);
    };
    definition.interface.controls.retain(|c| c.id != id);
    definition.bindings.retain(|b| b.control != id);
    definition.interface.version = definition
        .interface
        .version
        .checked_add(1)
        .ok_or("Interface version overflow")?;
    graph.extensions.insert(
        KEY.into(),
        serde_json::to_value(definition).map_err(|e| e.to_string())?,
    );
    Ok(true)
}
