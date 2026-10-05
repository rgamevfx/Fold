//! Compact cycle preview and custom-wave editing over the actual group definition.
use crate::{
    graph::{Input, Node},
    nodes::{
        interface::{GroupDefinition, GroupSettings},
        oscillator::Settings,
    },
};
use fold_foundation::{ObjectId, Time};
use fold_ui::sdk::{
    EditResponse, EditorColors,
    imgui::{Drag, MouseButton, Ui},
    toolbar::tooltip,
};
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
    let Ok(mut cycle) = serde_json::from_value::<Settings>(settings.clone()) else {
        return;
    };
    if cycle.validate().is_err() {
        return;
    }
    let before = serde_json::to_value(&cycle).unwrap();
    let origin = ui.cursor_screen_pos();
    let size = [
        ui.content_region_avail()[0].max(40.),
        ui.frame_height() * 4.,
    ];
    let point = |p: [f64; 2]| {
        [
            origin[0] + p[0] as f32 * size[0],
            origin[1] + (1. - p[1] as f32) * size[1],
        ]
    };
    let colors = EditorColors::from_ui(ui);
    {
        let draw = ui.get_window_draw_list();
        draw.add_rect(
            origin,
            [origin[0] + size[0], origin[1] + size[1]],
            colors.muted,
        )
        .build();
        draw.add_line(point([0., 0.5]), point([1., 0.5]), colors.muted)
            .build();
        let points = (0..=128)
            .map(|i| {
                let x = i as f64 / 128.;
                point([x, cycle.sample(&shape, x.min(1. - 1e-9)).unwrap_or(0.5)])
            })
            .collect::<Vec<_>>();
        draw.add_polyline(points, colors.selected)
            .thickness(2.)
            .build();
        if cursor.is_finite() {
            draw.add_line(point([cursor, 0.]), point([cursor, 1.]), colors.playhead)
                .build();
        }
    }
    ui.invisible_button("cycle", size);
    tooltip(
        ui,
        if shape == "Custom" {
            "One cycle. Double-click to add a point; drag points or right-click for numeric controls. Curve edits affect this group definition."
        } else {
            "One normalized cycle. The marker shows the current phase of the first channel."
        },
    );
    if shape == "Custom" {
        if ui.is_item_hovered()
            && ui.is_mouse_double_clicked(MouseButton::Left)
            && cycle.curve.len() < 64
        {
            let mouse = ui.io().mouse_pos();
            let p = [
                ((mouse[0] - origin[0]) / size[0]).clamp(0., 1.) as f64,
                (1. - (mouse[1] - origin[1]) / size[1]).clamp(0., 1.) as f64,
            ];
            if cycle.curve.iter().all(|v| (v[0] - p[0]).abs() > 0.001) {
                cycle.curve.push(p);
                cycle.curve.sort_by(|a, b| a[0].total_cmp(&b[0]));
                response.finished = true;
            }
        }
        let positions = cycle.curve.clone();
        let mut remove = None;
        for (i, p) in cycle.curve.iter_mut().enumerate() {
            let _id = ui.push_id(i as i32);
            let xy = point(*p);
            ui.set_cursor_screen_pos([xy[0] - 6., xy[1] - 6.]);
            ui.invisible_button("point", [12., 12.]);
            let mut changed = false;
            let edge = i == 0 || i + 1 == positions.len();
            let range = if edge {
                [p[0], p[0]]
            } else {
                [positions[i - 1][0] + 0.0001, positions[i + 1][0] - 0.0001]
            };
            if ui.is_item_active() && ui.is_mouse_dragging(MouseButton::Left) {
                let mouse = ui.io().mouse_pos();
                if !edge {
                    p[0] = ((mouse[0] - origin[0]) / size[0]) as f64;
                    p[0] = p[0].clamp(range[0], range[1]);
                }
                p[1] = (1. - (mouse[1] - origin[1]) / size[1]).clamp(0., 1.) as f64;
                changed = true;
            }
            response.item(ui, changed);
            ui.get_window_draw_list()
                .add_circle(xy, 4., colors.playhead)
                .filled(true)
                .build();
            if let Some(_popup) = ui.begin_popup_context_item() {
                if !edge {
                    let changed = Drag::new("Phase")
                        .speed(0.001)
                        .range(range[0], range[1])
                        .build(ui, &mut p[0]);
                    p[0] = p[0].clamp(range[0], range[1]);
                    response.item(ui, changed);
                }
                let changed = Drag::new("Value")
                    .speed(0.01)
                    .range(0., 1.)
                    .build(ui, &mut p[1]);
                p[1] = p[1].clamp(0., 1.);
                response.item(ui, changed);
                if !edge && ui.menu_item("Remove point") {
                    remove = Some(i);
                    response.finished = true;
                }
            }
        }
        if let Some(i) = remove {
            cycle.curve.remove(i);
        }
        ui.set_cursor_screen_pos([
            origin[0],
            origin[1] + size[1] + ui.clone_style().item_spacing()[1],
        ]);
        let changed = ui.checkbox("Smooth curve", &mut cycle.smooth);
        response.item(ui, changed);
        tooltip(
            ui,
            "Smooth interpolation between points; disable for straight segments.",
        );
    }
    let after = serde_json::to_value(&cycle).unwrap();
    if before != after {
        settings["curve"] = after["curve"].clone();
        settings["smooth"] = after["smooth"].clone();
        response.changed = true;
    }
}
