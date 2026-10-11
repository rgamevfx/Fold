//! Screen-space observation and picking; no document mutation.
use super::{Editor, transform as tx};
use crate::roto::{Layer, Roto, Spline, add, map, mul};
use fold_foundation::{ObjectId, Time};
use fold_ui::sdk::{
    ViewerRect,
    imgui::{StyleColor, Ui},
};
#[derive(Clone, Copy)]
pub(super) struct View {
    pub rect: ViewerRect,
    pub dimensions: [u32; 2],
}
impl View {
    pub fn screen(self, p: [f64; 2]) -> [f32; 2] {
        std::array::from_fn(|i| {
            self.rect.origin[i] + (p[i] / self.dimensions[i] as f64) as f32 * self.rect.size[i]
        })
    }
    pub fn image(self, p: [f32; 2]) -> [f64; 2] {
        std::array::from_fn(|i| {
            f64::from(p[i] - self.rect.origin[i]) / f64::from(self.rect.size[i])
                * self.dimensions[i] as f64
        })
    }
    pub fn near(self, a: [f64; 2], b: [f64; 2], radius: f32) -> bool {
        let a = self.screen(a);
        let b = self.screen(b);
        (a[0] - b[0]).hypot(a[1] - b[1]) <= radius
    }
}
pub(super) fn curve(c: crate::roto::Cubic, t: f64) -> [f64; 2] {
    let u = 1. - t;
    add(
        add(mul(c[0], u * u * u), mul(c[1], 3. * u * u * t)),
        add(mul(c[2], 3. * u * t * t), mul(c[3], t * t * t)),
    )
}
pub(super) fn shape_hit(layer: &Layer, roto: &Roto, view: View, p: [f64; 2]) -> Option<ObjectId> {
    if !layer.enabled || !roto.root.editable(layer.id) {
        return None;
    }
    for child in layer.children.iter().rev() {
        if let Some(id) = shape_hit(child, roto, view, p) {
            return Some(id);
        }
    }
    if !layer.enabled || !roto.root.editable(layer.id) {
        return None;
    }
    let shape = layer.shape.as_ref()?;
    let m = roto.transform_for(layer.id)?;
    let mouse = view.screen(p).map(f64::from);
    let mut inside = false;
    for c in shape.cubics(false) {
        let mut a = view.screen(map(m, c[0])).map(f64::from);
        for i in 1..=32 {
            let b = view.screen(map(m, curve(c, i as f64 / 32.))).map(f64::from);
            if tx::distance(mouse, a, b) < 7. {
                return Some(layer.id);
            }
            if (a[1] > mouse[1]) != (b[1] > mouse[1])
                && mouse[0] < (b[0] - a[0]) * (mouse[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
            a = b;
        }
    }
    (inside && shape.closed).then_some(layer.id)
}
pub(super) fn draw_layer(
    ui: &Ui,
    layer: &Layer,
    roto: &Roto,
    view: View,
    editor: &Editor,
    time: Time,
) {
    if !layer.enabled || layer.lifetime.is_some_and(|[a, b]| time < a || time >= b) {
        return;
    }
    let draw = ui.get_window_draw_list();
    let selected = editor.layer == Some(layer.id);
    let color = ui.style_color(if selected {
        StyleColor::PlotLinesHovered
    } else {
        StyleColor::TextDisabled
    });
    if let Some(shape) = &layer.shape {
        let m = roto.transform_for(layer.id).unwrap();
        for feather in [false, true] {
            if feather && (!selected || !editor.show_feather) {
                continue;
            }
            for c in shape.cubics(feather) {
                let c = c.map(|p| view.screen(map(m, p)));
                draw.add_bezier_curve(
                    c[0],
                    c[1],
                    c[2],
                    c[3],
                    if feather {
                        ui.style_color(StyleColor::PlotLines)
                    } else {
                        color
                    },
                )
                .thickness(if selected { 1.5 } else { 1. })
                .build();
            }
        }
        if selected {
            let feather = shape.feather_positions();
            for (i, p) in shape.points.iter().enumerate() {
                let q = view.screen(map(m, p.position));
                let chosen = editor.points.contains(&p.id);
                let color = ui.style_color(if chosen {
                    StyleColor::PlotLinesHovered
                } else {
                    StyleColor::Text
                });
                draw.add_rect([q[0] - 3.5, q[1] - 3.5], [q[0] + 3.5, q[1] + 3.5], color)
                    .filled(chosen)
                    .build();
                if chosen && shape.spline == Spline::Bezier {
                    for handle in [p.incoming, p.outgoing] {
                        let h = view.screen(map(m, add(p.position, handle)));
                        draw.add_line(q, h, color).build();
                        draw.add_circle(h, 3.5, color).filled(true).build();
                    }
                }
                if editor.show_feather {
                    let f = view.screen(map(m, feather[i]));
                    draw.add_line(q, f, ui.style_color(StyleColor::PlotLines))
                        .build();
                    draw.add_circle(f, 4., ui.style_color(StyleColor::PlotLines))
                        .build();
                }
            }
        }
    }
    drop(draw);
    for child in &layer.children {
        draw_layer(ui, child, roto, view, editor, time);
    }
}
pub(super) fn selected_bounds(roto: &Roto, editor: &Editor) -> Option<[f64; 4]> {
    let id = editor.layer?;
    let m = roto.transform_for(id)?;
    let shape = roto.root.find(id)?.shape.as_ref()?;
    tx::bounds(
        shape
            .points
            .iter()
            .filter(|p| editor.points.contains(&p.id))
            .map(|p| map(m, p.position)),
    )
}
pub(super) fn handle_bounds(view: View, b: [f64; 4]) -> [f64; 4] {
    let pad = std::array::from_fn::<_, 2, _>(|i| {
        12. * view.dimensions[i] as f64 / view.rect.size[i] as f64
    });
    [b[0] - pad[0], b[1] - pad[1], b[2] + pad[0], b[3] + pad[1]]
}
pub(super) fn rotation_handle(view: View, b: [f64; 4]) -> [f64; 2] {
    let mut p = view.screen([(b[0] + b[2]) * 0.5, b[1]]);
    p[1] -= 24.;
    view.image(p)
}
pub(super) fn draw_box(ui: &Ui, view: View, b: [f64; 4]) {
    let b = handle_bounds(view, b);
    let draw = ui.get_window_draw_list();
    let color = ui.style_color(StyleColor::PlotLinesHovered);
    let c = tx::corners(b).map(|p| view.screen(p));
    for i in 0..4 {
        draw.add_line(c[i], c[(i + 1) % 4], color).build();
        draw.add_rect(
            [c[i][0] - 4., c[i][1] - 4.],
            [c[i][0] + 4., c[i][1] + 4.],
            color,
        )
        .filled(true)
        .build();
    }
    let r = view.screen(rotation_handle(view, b));
    draw.add_line(view.screen([(b[0] + b[2]) * 0.5, b[1]]), r, color)
        .build();
    draw.add_circle(r, 5., color).build();
}

/// The pen draft is screen-space interaction state, independent of rendered
/// frames, selection and animation. Even one point must have a visible guide.
pub(super) fn draw_draft(
    ui: &Ui,
    layer: &Layer,
    view: View,
    cursor: Option<[f64; 2]>,
    dragging: bool,
) {
    let Some(shape) = &layer.shape else {
        return;
    };
    let Some(first) = shape.points.first() else {
        return;
    };
    let closing =
        cursor.is_some_and(|p| shape.points.len() >= 3 && view.near(p, first.position, 10.));
    let draw = ui.get_window_draw_list();
    let color = ui.style_color(StyleColor::PlotLinesHovered);
    let border = ui.style_color(StyleColor::WindowBg);
    let line = |a, b, color| {
        draw.add_line(a, b, border).thickness(4.).build();
        draw.add_line(a, b, color).thickness(1.5).build();
    };
    let mut preview = shape.clone();
    if closing {
        preview.closed = true;
    } else if let Some(p) = cursor.filter(|_| !dragging) {
        let mut cursor_point = first.clone();
        cursor_point.position = p;
        cursor_point.incoming = [0.; 2];
        cursor_point.outgoing = [0.; 2];
        preview.points.push(cursor_point);
    }
    for c in preview.cubics(false) {
        let c = c.map(|p| view.screen(p));
        draw.add_bezier_curve(c[0], c[1], c[2], c[3], border)
            .thickness(4.)
            .build();
        draw.add_bezier_curve(c[0], c[1], c[2], c[3], color)
            .thickness(2.)
            .build();
    }
    // B-spline control points do not lie on the curve. Keep the control polygon
    // visible so the artist can understand what each click will change.
    if shape.spline == Spline::BSpline {
        for pair in preview.points.windows(2) {
            line(
                view.screen(pair[0].position),
                view.screen(pair[1].position),
                ui.style_color(StyleColor::Text),
            );
        }
    }
    for point in &shape.points {
        let p = view.screen(point.position);
        if shape.spline == Spline::Bezier {
            for handle in [point.incoming, point.outgoing]
                .into_iter()
                .filter(|h| *h != [0.; 2])
            {
                let h = view.screen(add(point.position, handle));
                line(p, h, ui.style_color(StyleColor::Text));
                draw.add_circle(h, 4.5, border).filled(true).build();
                draw.add_circle(h, 3., ui.style_color(StyleColor::Text))
                    .filled(true)
                    .build();
            }
        }
        draw.add_rect([p[0] - 5., p[1] - 5.], [p[0] + 5., p[1] + 5.], border)
            .filled(true)
            .build();
        draw.add_rect([p[0] - 3.5, p[1] - 3.5], [p[0] + 3.5, p[1] + 3.5], color)
            .filled(true)
            .build();
    }
    if shape.points.len() >= 3 {
        let p = view.screen(first.position);
        draw.add_circle(p, if closing { 10. } else { 7. }, border)
            .thickness(4.)
            .build();
        draw.add_circle(p, if closing { 10. } else { 7. }, color)
            .thickness(2.)
            .build();
        if closing {
            draw.add_text(
                [p[0] + 13., p[1] - 15.],
                ui.style_color(StyleColor::Text),
                "Close",
            );
        }
    }
}
