//! Compact exact timing controls; editing uses the inspector's normal transaction.
use crate::{fields::Kind, graph::Node};
use fold_foundation::Time;
use fold_ui::sdk::{
    EditResponse,
    imgui::{Drag, Ui},
};

pub(super) fn draw(ui: &Ui, node: &mut Node, response: &mut EditResponse) -> bool {
    match node.kind.as_str() {
        "fold.motion.solver" => {
            timing(
                ui,
                &mut node.settings,
                "start",
                "Start time",
                true,
                response,
            );
            timing(
                ui,
                &mut node.settings,
                "step",
                "Step duration",
                false,
                response,
            );
            ui.text_disabled("Fixed-step replay • hold between steps");
            fold_ui::sdk::toolbar::tooltip(
                ui,
                "Seeking replays from initial geometry or a matching checkpoint. Edit Graph changes the update operation.",
            );
            true
        }
        "fold.motion.history" => {
            timing(
                ui,
                &mut node.settings,
                "start",
                "Start time",
                true,
                response,
            );
            timing(
                ui,
                &mut node.settings,
                "interval",
                "Sample interval",
                false,
                response,
            );
            true
        }
        "fold.motion.read_attribute" | "fold.motion.store_attribute" => {
            choice(
                ui,
                &mut node.settings,
                "geometry_kind",
                "Geometry",
                &[Kind::Content, Kind::Points],
                response,
            );
            choice(
                ui,
                &mut node.settings,
                "value_type",
                "Attribute type",
                &[Kind::Scalar, Kind::Vector, Kind::Color, Kind::Bool],
                response,
            );
            // Retype only literal defaults. Existing links remain explicit and
            // validation reports incompatible wiring instead of deleting it.
            if response.changed
                && let Ok((inputs, _)) =
                    crate::graph::registry::find(&node.kind).and_then(|d| d.signature(node))
            {
                for input in inputs {
                    if matches!(node.inputs.get(&input.id), Some(crate::graph::Input::Value(value)) if value.kind() != input.kind)
                        && let Some(default) = input.default
                    {
                        node.set(&input.id, default);
                    }
                }
            }
            true
        }
        "fold.motion.solver_state" => {
            choice(
                ui,
                &mut node.settings,
                "value_type",
                "State type",
                &[Kind::Content, Kind::Points],
                response,
            );
            true
        }
        _ => false,
    }
}
fn choice(
    ui: &Ui,
    settings: &mut serde_json::Value,
    key: &str,
    label: &str,
    kinds: &[Kind],
    response: &mut EditResponse,
) {
    let current = settings[key].as_str().unwrap_or("Content").to_owned();
    if let Some(_combo) = ui.begin_combo(label, &current) {
        for kind in kinds {
            let label = format!("{kind:?}");
            if ui.selectable(&label) {
                settings[key] = serde_json::to_value(kind).unwrap();
                response.changed = true;
                response.finished = true;
            }
        }
    }
}
fn timing(
    ui: &Ui,
    settings: &mut serde_json::Value,
    key: &str,
    label: &str,
    zero: bool,
    response: &mut EditResponse,
) {
    let Ok(time) = serde_json::from_value::<Time>(settings[key].clone()) else {
        return;
    };
    let _id = ui.push_id(key);
    ui.text(label);
    let mut numerator = time.numerator();
    let mut denominator = time.denominator();
    ui.set_next_item_width((ui.content_region_avail()[0] * 0.45).max(45.));
    let changed = Drag::new("##numerator").speed(1.).build(ui, &mut numerator);
    response.item(ui, changed);
    ui.same_line();
    ui.text("/");
    ui.same_line();
    ui.set_next_item_width((ui.content_region_avail()[0] - 25.).max(45.));
    let denominator_changed = Drag::new("##denominator")
        .speed(1.)
        .range(1, u32::MAX)
        .build(ui, &mut denominator);
    response.item(ui, denominator_changed);
    ui.same_line();
    ui.text("s");
    if (changed || denominator_changed)
        && let Ok(next) = Time::new(numerator, denominator)
        && (zero || next >= Time::new(1, 1000).unwrap())
    {
        settings[key] = serde_json::to_value(next).unwrap();
    }
}
