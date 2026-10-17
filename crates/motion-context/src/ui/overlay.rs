//! Viewer coordinates, selection picking and native control guides.
//! These guides do not render artwork; artwork uses the fframes image output.
use fold_foundation::ObjectId;
use fold_motion_canvas::{model::*, tools};
use fold_ui::sdk::{ViewerRect, imgui::StyleColor};

#[derive(Clone, Copy)]
pub(super) struct View {
    pub rect: ViewerRect,
    pub dimensions: [u32; 2],
}
impl View {
    pub fn screen(self, p: [f64; 2]) -> [f32; 2] {
        std::array::from_fn(|i| {
            self.rect.origin[i] + (p[i] / f64::from(self.dimensions[i])) as f32 * self.rect.size[i]
        })
    }
    pub fn image(self, p: [f32; 2]) -> [f64; 2] {
        std::array::from_fn(|i| {
            f64::from(p[i] - self.rect.origin[i]) / f64::from(self.rect.size[i])
                * f64::from(self.dimensions[i])
        })
    }
    pub fn near(self, a: [f64; 2], b: [f64; 2], radius: f32) -> bool {
        let (a, b) = (self.screen(a), self.screen(b));
        (a[0] - b[0]).hypot(a[1] - b[1]) <= radius
    }
}
pub(super) fn hit(
    canvas: &Canvas,
    transforms: &std::collections::BTreeMap<ObjectId, tools::Transform>,
    position: [f64; 2],
    time: fold_foundation::Time,
    view: View,
) -> Option<ObjectId> {
    fn visit(
        canvas: &Canvas,
        transforms: &std::collections::BTreeMap<ObjectId, tools::Transform>,
        ids: &[ObjectId],
        p: [f64; 2],
        time: fold_foundation::Time,
        view: View,
    ) -> Option<ObjectId> {
        for id in ids.iter().rev() {
            let object = &canvas.objects[id];
            if object.opacity(time) <= 0. || object.protected || object.locked || !object.visible {
                continue;
            }
            if let Content::Group { children, .. } = &object.content {
                if visit(canvas, transforms, children, p, time, view).is_some() {
                    return Some(*id);
                }
            }
            let Ok(inverse) = transforms[id].inverse() else {
                continue;
            };
            let [x, y] = inverse.point(p);
            let hit = match &object.content {
                Content::Path { contours, .. } => {
                    let sampled = contours
                        .iter()
                        .map(|c| c.at(time))
                        .collect::<Result<Vec<_>, _>>()
                        .ok()?;
                    super::picking::path(&sampled, &object.paint, transforms[id], p, view, time)
                }
                Content::Rectangle { width, height, .. } | Content::Image { width, height, .. } => {
                    x >= 0. && y >= 0. && x <= width.at(time) && y <= height.at(time)
                }
                Content::Ellipse {
                    radius_x, radius_y, ..
                } => {
                    let (rx, ry) = (radius_x.at(time), radius_y.at(time));
                    rx > 0. && ry > 0. && (x / rx).powi(2) + (y / ry).powi(2) <= 1.
                }
                Content::Text { .. } => fold_motion_canvas::design::local_bounds(object, time)
                    .is_some_and(|b| b.contains([x, y])),
                _ => false,
            };
            if hit {
                return Some(*id);
            }
        }
        None
    }
    visit(canvas, transforms, &canvas.roots, position, time, view)
}
pub(super) fn guides(
    ui: &fold_ui::sdk::imgui::Ui,
    canvas: &Canvas,
    transforms: &std::collections::BTreeMap<ObjectId, tools::Transform>,
    selected: &[ObjectId],
    point: Option<(ObjectId, ObjectId)>,
    view: View,
    time: fold_foundation::Time,
    paths: bool,
) {
    let draw = ui.get_window_draw_list();
    let color = ui.style_color(StyleColor::PlotLinesHovered);
    for id in selected {
        let Some(object) = canvas.objects.get(id) else {
            continue;
        };
        let transform = transforms[id];
        let bounds = match &object.content {
            Content::Rectangle { width, height, .. } | Content::Image { width, height, .. } => {
                Some([0., 0., width.at(time), height.at(time)])
            }
            Content::Ellipse {
                radius_x, radius_y, ..
            } => {
                let (rx, ry) = (radius_x.at(time), radius_y.at(time));
                Some([-rx, -ry, rx, ry])
            }
            _ => None,
        };
        if let Some([x, y, right, bottom]) = bounds {
            let corners = [[x, y], [right, y], [right, bottom], [x, bottom]]
                .map(|p| view.screen(transform.point(p)));
            draw.add_polyline(corners.to_vec(), color)
                .closed(true)
                .thickness(1.)
                .build();
        }
        if paths && let Content::Path { contours, .. } = &object.content {
            for contour in contours {
                for (index, a) in contour.points.iter().enumerate() {
                    let (Ok(position), Ok(incoming), Ok(outgoing)) = (
                        a.position_at(time),
                        a.incoming_at(time),
                        a.outgoing_at(time),
                    ) else {
                        continue;
                    };
                    let b = contour
                        .points
                        .get(index + 1)
                        .or_else(|| contour.closed.then(|| contour.points.first()).flatten());
                    if let Some(b) = b
                        && let (Ok(end), Ok(handle)) = (b.position_at(time), b.incoming_at(time))
                    {
                        let p = [
                            position,
                            [position[0] + outgoing[0], position[1] + outgoing[1]],
                            [end[0] + handle[0], end[1] + handle[1]],
                            end,
                        ]
                        .map(|p| view.screen(transform.point(p)));
                        draw.add_bezier_curve(p[0], p[1], p[2], p[3], color)
                            .thickness(1.)
                            .build();
                    }
                    let anchor = view.screen(transform.point(position));
                    if point == Some((*id, a.id)) {
                        draw.add_rect(
                            [anchor[0] - 5., anchor[1] - 5.],
                            [anchor[0] + 5., anchor[1] + 5.],
                            color,
                        )
                        .thickness(2.)
                        .build();
                    }
                    if index == 0 && contour.closed {
                        draw.add_circle(anchor, 6., color).build();
                    }
                    for offset in [incoming, outgoing] {
                        if point != Some((*id, a.id)) || offset == [0.; 2] {
                            continue;
                        }
                        let handle = view.screen(
                            transform.point([position[0] + offset[0], position[1] + offset[1]]),
                        );
                        draw.add_line(anchor, handle, color).build();
                        draw.add_circle(handle, 3., color).filled(true).build();
                    }
                    draw.add_rect(
                        [anchor[0] - 3., anchor[1] - 3.],
                        [anchor[0] + 3., anchor[1] + 3.],
                        color,
                    )
                    .filled(true)
                    .build();
                }
            }
        }
    }
}
