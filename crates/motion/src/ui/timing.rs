//! Timing controls use the same curve gesture and edit transaction as Oscillator.
use crate::{graph::Node, nodes::timing::Settings};
use fold_animation::timing::{Behavior, Range};
use fold_foundation::Time;
use fold_ui::sdk::{EditResponse, imgui::Ui, toolbar::tooltip};

pub(super) fn draw(ui: &Ui, node: &mut Node, time: Time, response: &mut EditResponse) {
    if node.kind == "fold.motion.curve" {
        super::oscillator::draw_curve(ui, &mut node.settings, "Custom", f64::NAN, false, response);
        return;
    }
    if node.kind != "fold.motion.timing" {
        return;
    }
    let Ok(mut settings) = node.settings::<Settings>() else {
        return;
    };
    let mut explicit = settings.range.is_some();
    if ui.checkbox("Override duration", &mut explicit) {
        settings.range = explicit.then_some(Range {
            start: Time::ZERO,
            duration: Time::new(1, 1).unwrap(),
        });
        node.settings["range"] = serde_json::to_value(settings.range).unwrap();
        response.changed = true;
        response.finished = true;
    }
    tooltip(
        ui,
        "By default, use the owning instance or layer range. Override supplies an exact range without retiming media.",
    );
    if explicit {
        super::simulation::timing(
            ui,
            &mut node.settings["range"],
            "start",
            "Start time",
            true,
            response,
        );
        super::simulation::timing(
            ui,
            &mut node.settings["range"],
            "duration",
            "Duration",
            false,
            response,
        );
    }
    let label = match settings.behavior {
        Behavior::Clamp => "Clamp",
        Behavior::Repeat => "Repeat",
        Behavior::PingPong => "Ping-pong",
    };
    if let Some(_combo) = ui.begin_combo("Outside range", label) {
        for (label, behavior) in [
            ("Clamp", Behavior::Clamp),
            ("Repeat", Behavior::Repeat),
            ("Ping-pong", Behavior::PingPong),
        ] {
            if ui.selectable(label) {
                node.settings["behavior"] = serde_json::to_value(behavior).unwrap();
                response.changed = true;
                response.finished = true;
            }
        }
    }
    // An inherited range may come from another document's invocation. Do not
    // display a guessed cursor based on the editor's document duration.
    let cursor = node
        .settings::<Settings>()
        .ok()
        .and_then(|s| s.range.and_then(|r| r.sample(time, s.behavior).ok()))
        .map_or(f64::NAN, |s| s.progress);
    super::oscillator::draw_curve(ui, &mut node.settings, "Custom", cursor, false, response);
}
