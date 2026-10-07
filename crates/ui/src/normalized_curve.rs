//! Normalized curve preview and interaction shared by Timing, Oscillator and transitions.
use crate::sdk::{
    EditResponse, EditorColors,
    imgui::{Drag, MouseButton, Ui},
    toolbar::tooltip,
};
pub fn draw(
    ui: &Ui,
    curve: &mut fold_animation::timing::Curve,
    preview: Option<&dyn Fn(f64) -> f64>,
    cursor: f64,
    response: &mut EditResponse,
) {
    if curve.validate().is_err() {
        return;
    }
    let before = curve.clone();
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
                point([
                    x,
                    preview.map_or_else(|| curve.sample(x), |sample| sample(x)),
                ])
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
        if preview.is_none() {
            "Normalized curve. Double-click to add a point; drag points or right-click for numeric controls."
        } else {
            "One normalized cycle. The marker shows the current phase of the first channel."
        },
    );
    if preview.is_none() {
        if ui.is_item_hovered()
            && ui.is_mouse_double_clicked(MouseButton::Left)
            && curve.curve.len() < 64
        {
            let mouse = ui.io().mouse_pos();
            let p = [
                ((mouse[0] - origin[0]) / size[0]).clamp(0., 1.) as f64,
                (1. - (mouse[1] - origin[1]) / size[1]).clamp(0., 1.) as f64,
            ];
            if curve.curve.iter().all(|v| (v[0] - p[0]).abs() > 0.001) {
                curve.curve.push(p);
                curve.curve.sort_by(|a, b| a[0].total_cmp(&b[0]));
                response.finished = true;
            }
        }
        let positions = curve.curve.clone();
        let mut remove = None;
        for (i, p) in curve.curve.iter_mut().enumerate() {
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
                    let changed = Drag::new("Progress")
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
            curve.curve.remove(i);
        }
        ui.set_cursor_screen_pos([
            origin[0],
            origin[1] + size[1] + ui.clone_style().item_spacing()[1],
        ]);
        let changed = ui.checkbox("Smooth curve", &mut curve.smooth);
        response.item(ui, changed);
        tooltip(
            ui,
            "Smooth interpolation between points; disable for straight segments.",
        );
    }
    response.changed |= before != *curve;
}
