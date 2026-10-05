//! Compact color-ramp editing over the path's actual named color attribute.
use crate::{
    fields::Datum,
    geometry::path::Stop,
    nodes::color_ramp::{Interpolation, sample},
};
use fold_ui::sdk::{
    EditResponse,
    imgui::{MouseButton, Ui},
};
pub(super) fn draw(
    ui: &Ui,
    settings: &mut serde_json::Value,
    aces: bool,
    response: &mut EditResponse,
) {
    let Some(value) = settings.get("attributes").and_then(|v| v.get("color")) else {
        return;
    };
    let Ok(mut stops) = serde_json::from_value::<Vec<Stop>>(value.clone()) else {
        return;
    };
    if stops.is_empty() || stops.iter().any(|s| !matches!(s.value, Datum::Color(_))) {
        return;
    }
    if edit(ui, &mut stops, aces, Interpolation::Linear, response) {
        settings["attributes"]["color"] = serde_json::to_value(stops).unwrap();
    }
}

/// Shared ramp editor. Paths retain linear interpolation; driver ramps choose a mode.
pub(super) fn edit(
    ui: &Ui,
    stops: &mut Vec<Stop>,
    aces: bool,
    mode: Interpolation,
    response: &mut EditResponse,
) -> bool {
    let origin = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0].max(30.);
    let height = ui.frame_height();
    {
        let draw = ui.get_window_draw_list();
        let tile = height / 2.;
        for row in 0..2 {
            for col in 0..(width / tile).ceil() as usize {
                let shade = if (row + col) % 2 == 0 { 0.22 } else { 0.4 };
                draw.add_rect(
                    [origin[0] + col as f32 * tile, origin[1] + row as f32 * tile],
                    [
                        origin[0] + ((col + 1) as f32 * tile).min(width),
                        origin[1] + (row + 1) as f32 * tile,
                    ],
                    [shade, shade, shade, 1.],
                )
                .filled(true)
                .build();
            }
        }
        for i in 0..64 {
            let u = i as f64 / 63.;
            let color = sample(stops, mode, u)
                .and_then(|v| v.color())
                .unwrap_or([0.; 4]);
            let color =
                fold_platform::color::to_picker(color, aces).map(|v| v.clamp(0., 1.) as f32);
            draw.add_rect(
                [origin[0] + width * i as f32 / 64., origin[1]],
                [origin[0] + width * (i + 1) as f32 / 64., origin[1] + height],
                color,
            )
            .filled(true)
            .build();
        }
    }
    ui.invisible_button("gradient", [width, height]);
    fold_ui::sdk::toolbar::tooltip(
        ui,
        "Double-click to add a stop. Drag to move; click or right-click to edit color, opacity and position.",
    );
    let mut changed = false;
    if ui.is_item_hovered() && ui.is_mouse_double_clicked(MouseButton::Left) && stops.len() < 1024 {
        let u = ((ui.io().mouse_pos()[0] - origin[0]) / width).clamp(0., 1.) as f64;
        if stops.iter().all(|s| (s.position - u).abs() > 1e-6) {
            let value = sample(stops, mode, u).unwrap();
            stops.push(Stop {
                position: u,
                value,
                extensions: Default::default(),
            });
            stops.sort_by(|a, b| a.position.total_cmp(&b.position));
            changed = true;
            response.finished = true;
        }
    }
    let positions: Vec<_> = stops.iter().map(|s| s.position).collect();
    let mut remove = None;
    for (i, stop) in stops.iter_mut().enumerate() {
        let _id = ui.push_id(i as i32);
        let x = origin[0] + (stop.position as f32 * width).clamp(5., width - 5.);
        ui.set_cursor_screen_pos([x - 5., origin[1] + height]);
        let clicked = ui.invisible_button_flags(
            "stop",
            [10., 12.],
            fold_ui::sdk::imgui::ButtonFlags::ENABLE_NAV,
        );
        let low = if i == 0 { 0. } else { positions[i - 1] + 1e-6 };
        let high = positions.get(i + 1).map(|v| v - 1e-6).unwrap_or(1.);
        let mut moved = false;
        if ui.is_item_active() && ui.is_mouse_dragging(MouseButton::Left) && low <= high {
            let next = ((ui.io().mouse_pos()[0] - origin[0]) / width) as f64;
            let next = next.clamp(low, high);
            moved = next != stop.position;
            stop.position = next;
        }
        response.item(ui, moved);
        changed |= moved;
        if (clicked && ui.mouse_drag_delta(MouseButton::Left)[0].abs() < 3.)
            || (ui.is_item_hovered() && ui.is_mouse_clicked(MouseButton::Right))
        {
            ui.open_popup("edit-stop");
        }
        let color = fold_platform::color::to_picker(stop.value.color().unwrap(), aces)
            .map(|v| v.clamp(0., 1.) as f32);
        let draw = ui.get_window_draw_list();
        draw.add_triangle(
            [x, origin[1] + height],
            [x - 5., origin[1] + height + 10.],
            [x + 5., origin[1] + height + 10.],
            color,
        )
        .filled(true)
        .build();
        drop(draw);
        if let Some(_popup) = ui.begin_popup("edit-stop") {
            if let Datum::Color(color) = &mut stop.value {
                let mut edit = EditResponse::default();
                fold_ui::sdk::color_controls::authored(ui, color, aces, &mut edit);
                changed |= edit.changed;
                response.finished |= edit.finished;
            }
            let before = stop.position;
            let low = if i == 0 { 0. } else { positions[i - 1] + 1e-6 };
            let high = positions.get(i + 1).map(|v| v - 1e-6).unwrap_or(1.);
            if low <= high {
                let edit = fold_ui::sdk::imgui::Drag::new("Position")
                    .speed(0.002)
                    .range(low, high)
                    .flags(fold_ui::sdk::imgui::DragFlags::ALWAYS_CLAMP)
                    .build(ui, &mut stop.position);
                changed |= edit && stop.position != before;
                response.finished |= ui.is_item_deactivated_after_edit();
            }
            if positions.len() > 1 && ui.small_button("Remove stop") {
                remove = Some(i);
                ui.close_current_popup();
            }
        }
    }
    if let Some(i) = remove {
        stops.remove(i);
        changed = true;
        response.finished = true;
    }
    ui.set_cursor_screen_pos([origin[0], origin[1] + height + 15.]);
    ui.dummy([0., 0.]);
    if changed {
        response.changed = true;
    }
    changed
}
