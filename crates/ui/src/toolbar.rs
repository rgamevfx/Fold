//! Compact editor actions shared by creative panels. Icons are drawn rather
//! than relying on optional font glyphs; labels remain available as tooltips.
use super::imgui::{PopupToken, StyleColor, Ui};

#[derive(Clone, Copy)]
pub enum ToolbarIcon {
    Select,
    Move,
    Rotate,
    Scale,
    Text,
    Rectangle,
    Ellipse,
    Pen,
    Add,
    FrameAll,
    More,
    Help,
    Back,
    View,
    Grid,
    List,
    Curve,
    DopeSheet,
    Unlink,
    Lock,
    Unlock,
}

pub fn tooltip(ui: &Ui, text: &str) {
    if ui.is_item_hovered() || ui.is_item_focused() {
        ui.tooltip_text(text);
    }
}

pub fn icon_button(ui: &Ui, id: &str, icon: ToolbarIcon, tip: &str) -> bool {
    let _id = ui.push_id(id);
    let p = ui.cursor_screen_pos();
    let size = ui.frame_height();
    let clicked = ui.button_with_size("##action", [size + 4., size]);
    tooltip(ui, tip);
    draw_icon(ui, icon, p, size);
    clicked
}

pub fn draw_icon(ui: &Ui, icon: ToolbarIcon, p: [f32; 2], size: f32) {
    let draw = ui.get_window_draw_list();
    let color = ui.style_color(StyleColor::Text);
    draw_icon_on(ui, &draw, icon, p, size, color);
}

/// Reuse a canvas draw-list borrow when rendering inline controls.
pub fn draw_icon_on(
    ui: &Ui,
    draw: &super::imgui::DrawListMut<'_>,
    icon: ToolbarIcon,
    p: [f32; 2],
    size: f32,
    color: [f32; 4],
) {
    let center = [p[0] + (size + 4.) / 2., p[1] + size / 2.];
    let unit = size * 0.23;
    let point = |x: f32, y: f32| [center[0] + x * unit, center[1] + y * unit];
    let line = |a: [f32; 2], b: [f32; 2]| {
        draw.add_line(point(a[0], a[1]), point(b[0], b[1]), color)
            .thickness((size / 14.).max(1.))
            .build();
    };
    match icon {
        ToolbarIcon::Select => {
            line([-0.7, -1.], [-0.7, 1.]);
            line([-0.7, -1.], [0.9, 0.3]);
            line([-0.7, 1.], [0., 0.3]);
            line([0., 0.3], [0.9, 0.3]);
        }
        ToolbarIcon::Move => {
            line([-1., 0.], [1., 0.]);
            line([0., -1.], [0., 1.]);
            for (x, y) in [(-1., 0.), (1., 0.), (0., -1.), (0., 1.)] {
                line([x, y], [x * 0.6 - y * 0.3, y * 0.6 + x * 0.3]);
                line([x, y], [x * 0.6 + y * 0.3, y * 0.6 - x * 0.3]);
            }
        }
        ToolbarIcon::Rotate => {
            draw.add_circle(center, unit, color).build();
            line([0.5, -0.8], [1., -0.8]);
            line([1., -0.8], [1., -0.2]);
        }
        ToolbarIcon::Scale => {
            draw.add_rect(point(-0.9, -0.9), point(0.9, 0.9), color)
                .build();
            line([-0.5, 0.5], [1., -1.]);
        }
        ToolbarIcon::Text => {
            line([-1., -0.9], [1., -0.9]);
            line([0., -0.9], [0., 1.]);
            line([-0.5, 1.], [0.5, 1.]);
        }
        ToolbarIcon::Rectangle => {
            draw.add_rect(point(-1., -0.8), point(1., 0.8), color)
                .rounding(2.)
                .build();
        }
        ToolbarIcon::Ellipse => {
            draw.add_circle(center, unit, color).build();
        }
        ToolbarIcon::Pen => {
            line([0., -1.], [-0.8, 0.4]);
            line([-0.8, 0.4], [0., 1.]);
            line([0., 1.], [0.8, 0.4]);
            line([0.8, 0.4], [0., -1.]);
            line([0., -1.], [0., 0.3]);
        }
        ToolbarIcon::Unlink => {
            line([-1., 0.2], [-1., -0.7]);
            line([-1., -0.7], [-0.2, -0.7]);
            line([1., -0.2], [1., 0.7]);
            line([1., 0.7], [0.2, 0.7]);
            line([-0.8, 1.], [0.8, -1.]);
        }
        ToolbarIcon::Lock | ToolbarIcon::Unlock => {
            draw.add_rect(point(-0.8, -0.1), point(0.8, 1.), color)
                .build();
            line([-0.5, -0.1], [-0.5, -0.9]);
            line([-0.5, -0.9], [0.5, -0.9]);
            if matches!(icon, ToolbarIcon::Lock) {
                line([0.5, -0.9], [0.5, -0.1]);
            }
        }
        ToolbarIcon::Curve => {
            for (a, b) in [
                ([-1., 0.8], [-0.5, 0.6]),
                ([-0.5, 0.6], [0., -0.4]),
                ([0., -0.4], [1., -0.8]),
            ] {
                line(a, b);
            }
        }
        ToolbarIcon::DopeSheet => {
            for (x, y) in [(-0.5, -0.6), (0.4, 0.6)] {
                line([-1., y], [1., y]);
                draw.add_polyline(
                    vec![
                        point(x, y - 0.3),
                        point(x + 0.3, y),
                        point(x, y + 0.3),
                        point(x - 0.3, y),
                    ],
                    color,
                )
                .closed(true)
                .filled(true)
                .build();
            }
        }
        ToolbarIcon::Add => {
            line([-1., 0.], [1., 0.]);
            line([0., -1.], [0., 1.]);
        }
        ToolbarIcon::FrameAll => {
            for x in [-1., 1.] {
                for y in [-1., 1.] {
                    line([x, y * 0.4], [x, y]);
                    line([x, y], [x * 0.4, y]);
                }
            }
        }
        ToolbarIcon::More => {
            for x in [-1., 0., 1.] {
                draw.add_circle(point(x, 0.), (unit * 0.2).max(1.), color)
                    .filled(true)
                    .build();
            }
        }
        ToolbarIcon::Help => {
            let text = "?";
            let extent = ui.calc_text_size(text);
            draw.add_text(
                [center[0] - extent[0] / 2., center[1] - extent[1] / 2.],
                color,
                text,
            );
        }
        ToolbarIcon::Back => {
            line([-1., 0.], [1., 0.]);
            line([-1., 0.], [0., -1.]);
            line([-1., 0.], [0., 1.]);
        }
        ToolbarIcon::Grid => {
            for x in [-1., 0.3] {
                for y in [-1., 0.3] {
                    draw.add_rect(point(x, y), point(x + 0.7, y + 0.7), color)
                        .build();
                }
            }
        }
        ToolbarIcon::List => {
            for y in [-0.8, 0., 0.8] {
                line([-1., y], [1., y]);
            }
        }
        ToolbarIcon::View => {
            draw.add_rect(point(-1.2, -0.8), point(1.2, 0.8), color)
                .build();
            line([0., 0.8], [0., 1.3]);
            line([-0.6, 1.3], [0.6, 1.3]);
        }
    }
}

pub fn menu_button<'ui>(ui: &'ui Ui, label: &str, tip: &str) -> Option<PopupToken<'ui>> {
    if ui.button(label) {
        ui.open_popup(label);
    }
    tooltip(ui, tip);
    ui.begin_popup(label)
}

pub fn icon_menu<'ui>(ui: &'ui Ui, id: &str, tip: &str) -> Option<PopupToken<'ui>> {
    if icon_button(ui, id, ToolbarIcon::More, tip) {
        ui.open_popup(id);
    }
    ui.begin_popup(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::imgui::{Condition, Context, MouseButton};

    #[test]
    fn compact_menu_opens_and_activates_once() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([400., 300.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut button = [0.; 2];
        let mut action = [0.; 2];
        let mut calls = 0;
        let mut frame = |context: &mut Context| {
            let mut open = false;
            let ui = context.frame();
            ui.window("Toolbar")
                .position([0.; 2], Condition::Always)
                .size([320., 250.], Condition::Always)
                .build(|| {
                    if let Some(_menu) = menu_button(ui, "Edit", "Editing actions") {
                        open = true;
                        if ui.menu_item("Split") {
                            calls += 1;
                        }
                        let min = ui.item_rect_min();
                        let max = ui.item_rect_max();
                        action = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
                    } else {
                        let min = ui.item_rect_min();
                        let max = ui.item_rect_max();
                        button = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
                    }
                    ui.same_line();
                    icon_button(ui, "frame", ToolbarIcon::FrameAll, "Frame all (F)");
                });
            assert!(context.render_legacy().total_vtx_count() > 0);
            (open, button, action, calls)
        };
        for _ in 0..3 {
            frame(&mut context);
        }
        let (_, button, _, _) = frame(&mut context);
        context.io_mut().add_mouse_pos_event(button);
        frame(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        frame(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        frame(&mut context);
        let (open, _, action, calls) = frame(&mut context);
        assert!(open);
        assert_eq!(calls, 0);
        context.io_mut().add_mouse_pos_event(action);
        frame(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        frame(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        frame(&mut context);
        let (open, _, _, calls) = frame(&mut context);
        assert!(!open);
        assert_eq!(calls, 1);
    }
}
