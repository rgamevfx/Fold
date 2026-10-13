//! Selection and transform handles in the shared viewer. Numeric transforms
//! remain in the inspector; movement uses the same binding edit and transaction.
use super::state::State;
use crate::interaction::{View, add, mul, sub};
use fold_foundation::{DocumentId, ObjectId, Time, affine::Affine};
use fold_platform::desktop::{DesktopCommand, Selection};
use fold_project::Revision;
use fold_ui::sdk::{
    self, ExtensionUi, ViewerRect,
    imgui::{Key, MouseButton, StyleColor, Ui},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Mode {
    #[default]
    Move,
    Rotate,
    Scale,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
        }
    }
}
struct Gesture {
    id: ObjectId,
    document: DocumentId,
    revision: Revision,
    time: Time,
    rect: ([f32; 2], [f32; 2]),
    view: View,
    origin: [f64; 3],
    axis: Option<[f64; 3]>,
    start: [f64; 3],
    matrix: Affine,
    pointer: [f32; 2],
    direction: [f64; 2],
    mode: Mode,
}
#[derive(Default)]
pub(super) struct Viewport {
    gesture: Option<Gesture>,
    pub(super) mode: Mode,
}
fn screen(rect: ViewerRect, p: [f64; 2]) -> [f32; 2] {
    std::array::from_fn(|i| rect.origin[i] + rect.size[i] * p[i] as f32)
}
fn normalized(rect: ViewerRect, p: [f32; 2]) -> [f64; 2] {
    std::array::from_fn(|i| ((p[i] - rect.origin[i]) / rect.size[i]) as f64)
}
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
fn handle_point(
    view: View,
    pointer: [f64; 2],
    origin: [f64; 3],
    axis: Option<[f64; 3]>,
) -> Option<[f64; 3]> {
    match axis {
        Some(axis) => view
            .axis(pointer, origin, axis)
            .map(|t| add(origin, mul(axis, t))),
        None => view.plane(pointer, origin),
    }
}
impl Viewport {
    pub fn toolbar(&mut self, ui: &Ui) {
        ui.same_line();
        if let Some(_menu) =
            sdk::toolbar::menu_button(ui, self.mode.label(), "Viewer transform tool")
        {
            for mode in [Mode::Move, Mode::Rotate, Mode::Scale] {
                if ui.menu_item(mode.label()) {
                    self.mode = mode;
                }
            }
        }
    }

    pub fn cancel(&mut self) {
        self.gesture = None;
    }
    pub fn draw(&mut self, context: ExtensionUi<'_>, rect: ViewerRect, state: &mut State) {
        let ExtensionUi { ui, host } = context;
        if rect.size.iter().any(|v| *v <= 0.) {
            return;
        }
        if self.gesture.as_ref().is_some_and(|g| {
            Some(g.document) != state.document
                || g.revision != state.revision()
                || g.time != state.time
                || state.selected != [g.id]
                || g.rect != (rect.origin, rect.size)
                || !rect.editable
                || state.busy()
                || !state.editing()
        }) || self.gesture.is_some() && ui.is_key_pressed(Key::Escape)
        {
            state.cancel(host);
            self.cancel();
        }
        let Some(scene) = state.current() else {
            return;
        };
        let Ok(view) = View::new(scene, state.time, rect.dimensions) else {
            return;
        };
        let pointer = ui.io().mouse_pos();
        let normalized = normalized(rect, pointer);
        let saved = ui.cursor_screen_pos();
        ui.set_cursor_screen_pos(rect.canvas_origin);
        let panning = ui.is_key_down(Key::Space);
        if !panning {
            ui.invisible_button("##3d-viewer", rect.canvas_size);
        }
        let hovered =
            !panning && ui.is_item_hovered() && normalized.iter().all(|v| (0. ..=1.).contains(v));
        let mut handle = None;
        let mut selected_origin = None;
        let mut selected_matrix = Affine::IDENTITY;
        if let [id] = state.selected.as_slice()
            && let Ok(matrix) = crate::interaction::world(scene, *id, state.time)
            && let Some(center) = view.project(matrix.point([0.; 3]))
        {
            let origin = matrix.point([0.; 3]);
            selected_origin = Some((*id, origin));
            selected_matrix = matrix;
            let center = screen(rect, center);
            let draw = ui.get_window_draw_list();
            let color = ui.style_color(StyleColor::PlotLinesHovered);
            draw.with_clip_rect(
                rect.canvas_origin,
                std::array::from_fn(|i| rect.canvas_origin[i] + rect.canvas_size[i]),
                || {
                    if self.mode == Mode::Rotate {
                        let length = view.units_per_height(origin) * 60. / rect.size[1] as f64;
                        for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                            let a = (axis + 1) % 3;
                            let b = (axis + 2) % 3;
                            let basis = |index| {
                                matrix.vector(std::array::from_fn(
                                    |i| if i == index { 1. } else { 0. },
                                ))
                            };
                            let norm = |v: [f64; 3]| v.iter().map(|v| v * v).sum::<f64>().sqrt();
                            let radius = length / norm(basis(a)).max(norm(basis(b)));
                            let mut previous = None;
                            for step in 0..=64 {
                                let angle = std::f64::consts::TAU * step as f64 / 64.;
                                let mut local = [0.; 3];
                                local[a] = radius * angle.cos();
                                local[b] = radius * angle.sin();
                                let point =
                                    view.project(matrix.point(local)).map(|p| screen(rect, p));
                                if let (Some(from), Some(to)) = (previous, point) {
                                    draw.add_line(from, to, color).thickness(1.5).build();
                                    if handle.is_none()
                                        && segment_distance(pointer, from, to) <= 6.
                                        && view.local_plane(normalized, matrix, axis).is_some()
                                    {
                                        handle = Some(Some(std::array::from_fn(|i| {
                                            if i == axis { 1. } else { 0. }
                                        })));
                                    }
                                }
                                if step == 8
                                    && let Some(point) = point
                                {
                                    draw.add_text(point, color, label);
                                }
                                previous = point;
                            }
                        }
                    } else {
                        draw.add_circle(center, 5., color).thickness(2.).build();
                        if distance(pointer, center) <= 8. {
                            handle = Some(None);
                        }
                        let length = view.units_per_height(origin) * 60. / rect.size[1] as f64;
                        for (index, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                            let axis = std::array::from_fn(|i| if i == index { 1. } else { 0. });
                            let direction = if self.mode == Mode::Scale {
                                matrix.vector(axis)
                            } else {
                                axis
                            };
                            let norm = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
                            let direction = mul(direction, norm.recip());
                            let Some(endpoint) = view.project(add(origin, mul(direction, length)))
                            else {
                                continue;
                            };
                            let endpoint = screen(rect, endpoint);
                            let total = distance(center, endpoint);
                            if total < 18. {
                                continue;
                            }
                            draw.add_line(center, endpoint, color).thickness(2.).build();
                            if self.mode == Mode::Scale {
                                draw.add_rect(
                                    [endpoint[0] - 4., endpoint[1] - 4.],
                                    [endpoint[0] + 4., endpoint[1] + 4.],
                                    color,
                                )
                                .filled(true)
                                .build();
                            } else {
                                draw.add_circle(endpoint, 4., color).filled(true).build();
                            }
                            draw.add_text([endpoint[0] + 5., endpoint[1] + 2.], color, label);
                            let line = [endpoint[0] - center[0], endpoint[1] - center[1]];
                            let t = ((pointer[0] - center[0]) * line[0]
                                + (pointer[1] - center[1]) * line[1])
                                / (total * total);
                            let closest = [center[0] + t * line[0], center[1] + t * line[1]];
                            if handle.is_none()
                                && (0.2..=1.1).contains(&t)
                                && distance(pointer, closest) <= 6.
                            {
                                handle = Some(Some(axis));
                            }
                        }
                    }
                },
            );
            if hovered && handle.is_some() && self.gesture.is_none() {
                ui.tooltip_text(match self.mode {
                    Mode::Move => "Move in world axes. Center: move in view plane. Escape: cancel.",
                    Mode::Rotate => "Rotate around a local axis. Escape: cancel.",
                    Mode::Scale => {
                        "Scale on a local axis. Center: drag up to scale all axes. Escape: cancel."
                    }
                });
            }
        }
        if hovered
            && rect.editable
            && rect.image_current
            && !state.busy()
            && !state.editing()
            && self.gesture.is_none()
            && ui.is_mouse_clicked(MouseButton::Left)
        {
            if let (Some(axis), Some((id, origin))) = (handle, selected_origin) {
                let start = match self.mode {
                    Mode::Move => handle_point(view, normalized, origin, axis),
                    Mode::Rotate => axis.and_then(|axis| {
                        view.local_plane(normalized, selected_matrix, axis_index(axis))
                    }),
                    Mode::Scale => Some([0.; 3]),
                };
                if let Some(start) = start {
                    let initial = match self.mode {
                        Mode::Move => crate::tools::translate_world(
                            scene,
                            id,
                            state.time,
                            [0.; 3],
                            host.state().auto_key,
                        ),
                        Mode::Rotate => crate::tools::rotate_local(
                            scene,
                            id,
                            state.time,
                            axis_index(axis.unwrap()),
                            0.,
                            host.state().auto_key,
                        ),
                        Mode::Scale => crate::tools::scale_local(
                            scene,
                            id,
                            state.time,
                            [1.; 3],
                            host.state().auto_key,
                        ),
                    };
                    let direction = axis
                        .and_then(|axis| {
                            let from = screen(rect, view.project(origin)?);
                            let axis = selected_matrix.vector(axis);
                            let norm = axis.iter().map(|v| v * v).sum::<f64>().sqrt();
                            let length = view.units_per_height(origin) * 60. / rect.size[1] as f64;
                            let to =
                                screen(rect, view.project(add(origin, mul(axis, length / norm)))?);
                            let length = distance(from, to) as f64;
                            (length > 1e-6)
                                .then(|| std::array::from_fn(|i| (to[i] - from[i]) as f64 / length))
                        })
                        .unwrap_or([0., -1.]);
                    let result = initial.and_then(|edits| state.preview(host, edits));
                    match result {
                        Ok(()) => {
                            self.gesture = Some(Gesture {
                                id,
                                document: state.document.unwrap(),
                                revision: state.revision(),
                                time: state.time,
                                rect: (rect.origin, rect.size),
                                view,
                                origin,
                                axis,
                                start,
                                matrix: selected_matrix,
                                pointer,
                                direction,
                                mode: self.mode,
                            })
                        }
                        Err(error) => state.error = error,
                    }
                }
            } else {
                match view.pick(scene, state.time, normalized) {
                    Ok(hit) => {
                        let mut objects = if ui.io().key_ctrl() {
                            state.selected.clone()
                        } else {
                            vec![]
                        };
                        if let Some(id) = hit {
                            if let Some(index) = objects.iter().position(|v| *v == id) {
                                objects.remove(index);
                            } else {
                                objects.push(id);
                            }
                        }
                        host.command(DesktopCommand::Select(Selection {
                            document: state.document,
                            objects,
                        }));
                    }
                    Err(error) => state.error = error,
                }
            }
        }
        if let Some(gesture) = &self.gesture {
            if ui.is_mouse_down(MouseButton::Left) {
                let scene = state.scene.as_ref().unwrap();
                let edits = match gesture.mode {
                    Mode::Move => {
                        handle_point(gesture.view, normalized, gesture.origin, gesture.axis).map(
                            |point| {
                                crate::tools::translate_world(
                                    scene,
                                    gesture.id,
                                    gesture.time,
                                    sub(point, gesture.start),
                                    host.state().auto_key,
                                )
                            },
                        )
                    }
                    Mode::Rotate => {
                        let axis = axis_index(gesture.axis.unwrap());
                        gesture
                            .view
                            .local_plane(normalized, gesture.matrix, axis)
                            .map(|point| {
                                let a = (axis + 1) % 3;
                                let b = (axis + 2) % 3;
                                let angle = (gesture.start[a] * point[b]
                                    - gesture.start[b] * point[a])
                                    .atan2(
                                        gesture.start[a] * point[a] + gesture.start[b] * point[b],
                                    );
                                crate::tools::rotate_local(
                                    scene,
                                    gesture.id,
                                    gesture.time,
                                    axis,
                                    angle,
                                    host.state().auto_key,
                                )
                            })
                    }
                    Mode::Scale => {
                        let amount = (0..2)
                            .map(|i| {
                                (pointer[i] - gesture.pointer[i]) as f64 * gesture.direction[i]
                            })
                            .sum::<f64>();
                        let factor = (amount / 80.).clamp(-12., 12.).exp();
                        let factors = std::array::from_fn(|i| {
                            if gesture.axis.is_none_or(|axis| axis_index(axis) == i) {
                                factor
                            } else {
                                1.
                            }
                        });
                        Some(crate::tools::scale_local(
                            scene,
                            gesture.id,
                            gesture.time,
                            factors,
                            host.state().auto_key,
                        ))
                    }
                };
                if let Some(edits) = edits
                    && let Err(error) = edits.and_then(|edits| state.preview(host, edits))
                {
                    state.cancel(host);
                    state.error = error;
                    self.cancel();
                }
            } else {
                if let Err(error) = state.commit(host) {
                    state.cancel(host);
                    state.error = error;
                }
                self.cancel();
            }
        }
        ui.set_cursor_screen_pos(saved);
    }
}

fn axis_index(axis: [f64; 3]) -> usize {
    axis.iter().position(|v| *v == 1.).expect("gizmo unit axis")
}
fn segment_distance(point: [f32; 2], from: [f32; 2], to: [f32; 2]) -> f32 {
    let delta = [to[0] - from[0], to[1] - from[1]];
    let length = delta[0] * delta[0] + delta[1] * delta[1];
    if length <= 1e-6 {
        return distance(point, from);
    }
    let t = (((point[0] - from[0]) * delta[0] + (point[1] - from[1]) * delta[1]) / length)
        .clamp(0., 1.);
    distance(point, [from[0] + t * delta[0], from[1] + t * delta[1]])
}
