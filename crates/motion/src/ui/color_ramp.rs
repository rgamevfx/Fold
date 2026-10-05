//! Ramp controls edit the same primitive settings exposed inside its group.
use crate::{
    graph::Node,
    nodes::{
        color_ramp::{Interpolation, Settings},
        interface::{GroupDefinition, GroupSettings},
    },
};
use fold_foundation::ObjectId;
use fold_ui::sdk::{EditResponse, imgui::Ui};
use std::collections::BTreeMap;
pub(super) fn draw(
    ui: &Ui,
    node: &mut Node,
    groups: &mut BTreeMap<ObjectId, GroupDefinition>,
    aces: bool,
    response: &mut EditResponse,
) {
    let settings = if node.kind == "fold.motion.color_ramp" {
        &mut node.settings
    } else {
        if node.settings.get("asset").and_then(|v| v.as_str()) != Some("fold.motion.color_ramp") {
            return;
        }
        let Some(group) = node
            .settings::<GroupSettings>()
            .ok()
            .and_then(|s| s.group)
            .and_then(|id| groups.get_mut(&id))
        else {
            return;
        };
        let Some(id) = node
            .settings
            .get("ramp_node")
            .and_then(|v| serde_json::from_value::<ObjectId>(v.clone()).ok())
        else {
            return;
        };
        let Some(inner) = group
            .graph
            .nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == "fold.motion.color_ramp")
        else {
            return;
        };
        &mut inner.settings
    };
    let Ok(mut ramp) = serde_json::from_value::<Settings>(settings.clone()) else {
        return;
    };
    if ramp.validate().is_err() {
        return;
    }
    let mut changed = false;
    if let Some(_combo) = ui.begin_combo("Interpolation", format!("{:?}", ramp.interpolation)) {
        for (name, mode) in [
            ("Linear", Interpolation::Linear),
            ("Smooth", Interpolation::Smooth),
            ("Stepped", Interpolation::Stepped),
        ] {
            if ui.selectable(name) {
                ramp.interpolation = mode;
                changed = true;
                response.finished = true;
            }
        }
    }
    changed |= super::gradient::edit(ui, &mut ramp.stops, aces, ramp.interpolation, response);
    if changed {
        settings["stops"] = serde_json::to_value(ramp.stops).unwrap();
        settings["interpolation"] = serde_json::to_value(ramp.interpolation).unwrap();
        response.changed = true;
    }
}
