//! Compact cycle preview and custom-wave editing over the actual group definition.
use crate::{
    graph::{Input, Node},
    nodes::{
        interface::{GroupDefinition, GroupSettings},
        oscillator::Settings,
    },
};
use fold_foundation::{ObjectId, Time};
use fold_ui::sdk::{EditResponse, imgui::Ui};
use std::collections::BTreeMap;

pub(super) fn is_oscillator(node: &Node) -> bool {
    node.kind == "fold.motion.oscillator"
        || node.settings.get("asset").and_then(|v| v.as_str()) == Some("fold.motion.oscillator")
}
pub(super) fn draw(
    ui: &Ui,
    node: &mut Node,
    groups: &mut BTreeMap<ObjectId, GroupDefinition>,
    time: Time,
    response: &mut EditResponse,
) {
    if !is_oscillator(node) {
        return;
    }
    let shape = match node.inputs.get("shape") {
        Some(Input::Value(crate::fields::Datum::Text(v))) => v.clone(),
        _ => {
            ui.text_disabled("Shape is driven");
            return;
        }
    };
    let scalar = |key: &str, fallback: f64| match node.inputs.get(key) {
        Some(Input::Value(v)) => node.sample_input(key, v, time).scalar().unwrap_or(fallback),
        _ => fallback,
    };
    let bpm = matches!(node.inputs.get("time_mode"), Some(Input::Value(crate::fields::Datum::Text(v))) if v == "BPM");
    let frequency = if bpm {
        scalar("bpm", 60.) / 60.
    } else {
        1. / scalar("duration", 2.)
    };
    let cursor = ((time.numerator() as f64 / time.denominator() as f64
        + scalar("time_offset", 0.))
        * scalar("time_scale", 1.)
        * frequency
        + scalar("phase", 0.))
    .rem_euclid(1.);
    let settings = if node.kind == "fold.motion.oscillator" {
        &mut node.settings
    } else {
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
            .get("oscillator_node")
            .and_then(|v| serde_json::from_value::<ObjectId>(v.clone()).ok())
        else {
            return;
        };
        let Some(inner) = group
            .graph
            .nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == "fold.motion.oscillator")
        else {
            return;
        };
        &mut inner.settings
    };
    draw_curve(ui, settings, &shape, cursor, true, response);
}

/// Preserve the oscillator's authored settings while sharing the curve widget.
pub(super) fn draw_curve(
    ui: &Ui,
    settings: &mut serde_json::Value,
    shape: &str,
    cursor: f64,
    periodic: bool,
    response: &mut EditResponse,
) {
    let Ok(mut cycle) = serde_json::from_value::<Settings>(settings.clone()) else {
        return;
    };
    let before = cycle.clone();
    let preview = |x: f64| before.sample(shape, x.min(1. - 1e-9)).unwrap_or(0.5);
    fold_ui::sdk::normalized_curve::draw(
        ui,
        &mut cycle.curve_data,
        if periodic && shape != "Custom" {
            Some(&preview)
        } else {
            None
        },
        cursor,
        response,
    );
    if cycle.curve_data != before.curve_data {
        settings["curve"] = serde_json::to_value(&cycle.curve).unwrap();
        settings["smooth"] = serde_json::json!(cycle.smooth);
    }
}
