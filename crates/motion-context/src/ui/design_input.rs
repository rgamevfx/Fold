//! Selection gestures share one baseline and the Session preview/commit lifecycle.
use super::{
    overlay::{self, View},
    state::State,
};
use fold_foundation::ObjectId;
use fold_motion_canvas::{
    design::{self, Bounds},
    editing::Edit,
    model::Content,
    tools::{self, Transform},
};
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::{
    imgui::{Key, MouseButton, StyleColor, Ui},
    toolbar::{self, ToolbarIcon},
};

enum Gesture {
    Move {
        start: [f64; 2],
        objects: Vec<ObjectId>,
    },
    Scale {
        start: [f64; 2],
        pivot: [f64; 2],
        objects: Vec<ObjectId>,
    },
    Rotate {
        start: f64,
        pivot: [f64; 2],
        objects: Vec<ObjectId>,
    },
    Marquee {
        start: [f64; 2],
        selected: Vec<ObjectId>,
    },
    Pivot,
    TextBox {
        id: ObjectId,
        start: [f64; 2],
    },
}
struct TextEdit {
    generation: u64,
    id: ObjectId,
    text: String,
    focus: bool,
}
pub(super) struct DesignInput {
    gesture: Option<Gesture>,
    generation: u64,
    basis: Option<([f32; 2], [f32; 2])>,
    pivot: Option<[f64; 2]>,
    selection: Vec<ObjectId>,
    pub snap: bool,
    pub text_tool: bool,
    text: Option<TextEdit>,
}
impl Default for DesignInput {
    fn default() -> Self {
        Self {
            gesture: None,
            generation: 0,
            basis: None,
            pivot: None,
            selection: vec![],
            snap: true,
            text_tool: false,
            text: None,
        }
    }
}
impl DesignInput {
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.text = None;
        self.basis = None;
    }
    pub fn controls(&mut self, ui: &Ui) {
        let _color = self
            .snap
            .then(|| ui.push_style_color(StyleColor::Button, ui.style_color(StyleColor::Header)));
        if toolbar::icon_button(
            ui,
            "snap",
            ToolbarIcon::Snap,
            "Snap to artwork and canvas (hold Alt to bypass)",
        ) {
            self.snap = !self.snap;
        }
    }
    pub fn draw(
        &mut self,
        ui: &Ui,
        host: &mut dyn DesktopClient,
        state: &mut State,
        view: View,
        enabled: bool,
        hovered: bool,
        click: bool,
    ) -> Result<bool, String> {
        if self.selection != state.selected {
            self.pivot = None;
            self.selection = state.selected.clone();
        }
        let basis = (view.rect.origin, view.rect.size);
        if self.gesture.is_some()
            && (self.generation != state.generation
                || self.basis != Some(basis)
                || !view.rect.editable)
        {
            state.cancel(host);
            self.cancel();
        }
        let Some(canvas) = state.canvas().cloned() else {
            return Ok(false);
        };
        let focused = ui.is_window_focused_with_flags(
            fold_ui::sdk::imgui::FocusedFlags::ROOT_AND_CHILD_WINDOWS,
        );
        if focused && ui.is_key_pressed(Key::Escape) {
            state.cancel(host);
            self.cancel();
            return Ok(true);
        }
        if self.text.is_some() && state.busy() {
            return Ok(true);
        }
        if self
            .text
            .as_ref()
            .is_some_and(|edit| !edit.focus && edit.generation != state.generation)
        {
            self.text = None;
        }
        if let Some(edit) = &mut self.text {
            if !canvas.objects.contains_key(&edit.id) {
                self.text = None;
                return Ok(true);
            }
            let all = design::bounds(&canvas, state.time)?;
            let Some(bounds) = all.get(&edit.id) else {
                self.text = None;
                return Ok(true);
            };
            let a = view.screen(bounds.min);
            let b = view.screen(bounds.max);
            ui.set_cursor_screen_pos(a);
            if edit.focus {
                ui.set_keyboard_focus_here();
                edit.focus = false;
                edit.generation = state.generation;
            }
            let changed = ui
                .input_text_multiline(
                    "##canvas-text",
                    &mut edit.text,
                    [
                        (b[0] - a[0]).max(220.),
                        (b[1] - a[1]).max(ui.frame_height() * 3.),
                    ],
                )
                .build();
            let finish = ui.is_item_deactivated_after_edit()
                || (ui.io().key_ctrl() && ui.is_key_pressed(Key::Enter));
            if changed {
                let before = &state.before.as_ref().ok_or("No Motion document")?.objects[&edit.id];
                let mut object = before.clone();
                if let Content::Text { text, .. } = &mut object.content {
                    *text = edit.text.clone();
                }
                state.preview(
                    host,
                    vec![Edit::Replace {
                        id: edit.id,
                        expected: Box::new(before.clone()),
                        object: Box::new(object),
                    }],
                )?;
            }
            if finish {
                state.commit(host)?;
                self.text = None;
            }
            return Ok(true);
        }
        if !enabled || state.busy() {
            return Ok(false);
        }
        let position = view.image(ui.io().mouse_pos());
        let all = design::bounds(&canvas, state.time)?;
        let selected_bounds = state
            .selected
            .iter()
            .filter_map(|id| all.get(id).copied())
            .reduce(Bounds::union);
        let pivot = self.pivot.or_else(|| {
            if let [id] = state.selected.as_slice() {
                let object = &canvas.objects[id];
                tools::transforms(&canvas, state.time).ok().map(|world| {
                    world[id].point([
                        object.transform.anchor_x.at(state.time),
                        object.transform.anchor_y.at(state.time),
                    ])
                })
            } else {
                selected_bounds.map(Bounds::center)
            }
        });
        let mut handle = None;
        if let Some(b) = selected_bounds {
            let draw = ui.get_window_draw_list();
            let color = ui.style_color(StyleColor::PlotLinesHovered);
            let corners = b.corners().map(|p| view.screen(p));
            draw.add_polyline(corners.to_vec(), color)
                .closed(true)
                .thickness(1.5)
                .build();
            for (i, p) in b.corners().iter().enumerate() {
                let screen = view.screen(*p);
                draw.add_rect(
                    [screen[0] - 4., screen[1] - 4.],
                    [screen[0] + 4., screen[1] + 4.],
                    color,
                )
                .filled(true)
                .build();
                if view.near(position, *p, 8.) {
                    handle = Some(i);
                }
            }
            let top = view.screen([b.center()[0], b.min[1]]);
            let rotate = [top[0], top[1] - 24.];
            draw.add_line(top, rotate, color).build();
            draw.add_circle(rotate, 5., color).build();
            if view.near(position, view.image(rotate), 9.) {
                handle = Some(4);
            }
            if let Some(p) = pivot {
                let p = view.screen(p);
                draw.add_circle(p, 4., color).build();
                draw.add_line([p[0] - 7., p[1]], [p[0] + 7., p[1]], color)
                    .build();
                draw.add_line([p[0], p[1] - 7.], [p[0], p[1] + 7.], color)
                    .build();
                if ui.io().key_alt() && view.near(position, view.image(p), 10.) {
                    handle = Some(5);
                }
            }
        }
        if click && self.gesture.is_none() {
            let transforms = tools::transforms(&canvas, state.time)?;
            let mut hit = overlay::hit(&canvas, &transforms, position, state.time, view);
            // Ctrl-click drills into the selected group without flattening it.
            if ui.io().key_ctrl() {
                if let Some(id) = hit {
                    if let Content::Group { children, .. } = &canvas.objects[&id].content {
                        let mut local = canvas.clone();
                        local.roots = children.clone();
                        hit = overlay::hit(&local, &transforms, position, state.time, view);
                    }
                }
            }
            if self.text_tool || ui.is_mouse_double_clicked(MouseButton::Left) {
                if let Some(id) =
                    hit.filter(|id| matches!(canvas.objects[id].content, Content::Text { .. }))
                {
                    state.select(host, vec![id]);
                    let Content::Text { text, .. } = &canvas.objects[&id].content else {
                        unreachable!()
                    };
                    self.text = Some(TextEdit {
                        generation: state.generation,
                        id,
                        text: text.clone(),
                        focus: true,
                    });
                    return Ok(true);
                } else if self.text_tool {
                    self.gesture = Some(Gesture::TextBox {
                        id: ObjectId::new(),
                        start: position,
                    });
                    self.basis = Some(basis);
                    self.generation = state.generation;
                }
            }
            if self.gesture.is_none() {
                self.gesture = if let Some(index) = handle {
                    let b = selected_bounds.unwrap();
                    let objects = state.selected.clone();
                    Some(match index {
                        4 => {
                            let pivot = pivot.unwrap();
                            Gesture::Rotate {
                                start: (position[1] - pivot[1]).atan2(position[0] - pivot[0]),
                                pivot,
                                objects,
                            }
                        }
                        5 => Gesture::Pivot,
                        _ => Gesture::Scale {
                            start: b.corners()[index],
                            pivot: if ui.io().key_alt() {
                                pivot.unwrap()
                            } else {
                                b.corners()[(index + 2) % 4]
                            },
                            objects,
                        },
                    })
                } else if let Some(id) = hit {
                    let mut selection = state.selected.clone();
                    if ui.io().key_shift() {
                        if let Some(i) = selection.iter().position(|v| *v == id) {
                            selection.remove(i);
                        } else {
                            selection.push(id);
                        }
                        state.select(host, selection);
                    } else if !selection.contains(&id) {
                        state.select(host, vec![id]);
                    }
                    Some(Gesture::Move {
                        start: position,
                        objects: state.selected.clone(),
                    })
                } else {
                    Some(Gesture::Marquee {
                        start: position,
                        selected: if ui.io().key_shift() {
                            state.selected.clone()
                        } else {
                            vec![]
                        },
                    })
                };
                self.basis = Some(basis);
                self.generation = state.generation;
            }
        }
        if let Some(gesture) = &self.gesture {
            let before = state.before.as_ref().ok_or("No Motion document")?;
            let released = ui.is_mouse_released(MouseButton::Left);
            let edits = match gesture {
                Gesture::Move { start, objects } => {
                    let mut delta = [0, 1].map(|i| position[i] - start[i]);
                    if ui.io().key_shift() {
                        let axis = if delta[0].abs() > delta[1].abs() {
                            1
                        } else {
                            0
                        };
                        delta[axis] = 0.;
                    }
                    if self.snap && !ui.io().key_alt() {
                        if let Some(b) = design::selection_bounds(before, objects, state.time)? {
                            let all = design::bounds(before, state.time)?;
                            let threshold = [0, 1].map(|i| {
                                6. * f64::from(view.dimensions[i]) / f64::from(view.rect.size[i])
                            });
                            for axis in 0..2 {
                                let mut best = threshold[axis];
                                let mut correction = 0.;
                                let mut guide = None;
                                let canvas_edge = if axis == 0 {
                                    before.width
                                } else {
                                    before.height
                                } as f64;
                                let mut targets = vec![0., canvas_edge * 0.5, canvas_edge];
                                for (id, other) in &all {
                                    if !objects.contains(id)
                                        && !objects
                                            .iter()
                                            .any(|parent| is_descendant(before, *id, *parent))
                                    {
                                        targets.extend([
                                            other.min[axis],
                                            other.center()[axis],
                                            other.max[axis],
                                        ]);
                                    }
                                }
                                for target in targets {
                                    for own in [b.min[axis], b.center()[axis], b.max[axis]] {
                                        let d = target - own - delta[axis];
                                        if d.abs() < best {
                                            best = d.abs();
                                            correction = d;
                                            guide = Some(target);
                                        }
                                    }
                                }
                                delta[axis] += correction;
                                if let Some(value) = guide {
                                    let mut a = [0.; 2];
                                    let mut z = [before.width as f64, before.height as f64];
                                    a[axis] = value;
                                    z[axis] = value;
                                    ui.get_window_draw_list()
                                        .add_line(
                                            view.screen(a),
                                            view.screen(z),
                                            ui.style_color(StyleColor::PlotLinesHovered),
                                        )
                                        .build();
                                }
                            }
                        }
                    }
                    Some(tools::translate(
                        before,
                        objects,
                        state.time,
                        delta,
                        host.state().auto_key,
                    )?)
                }
                Gesture::Scale {
                    start,
                    pivot,
                    objects,
                } => {
                    let mut scale = [0, 1].map(|i| {
                        if (start[i] - pivot[i]).abs() > 1e-8 {
                            (position[i] - pivot[i]) / (start[i] - pivot[i])
                        } else {
                            1.
                        }
                    });
                    if ui.io().key_shift() {
                        let s = if (scale[0] - 1.).abs() > (scale[1] - 1.).abs() {
                            scale[0]
                        } else {
                            scale[1]
                        };
                        scale = [s; 2];
                    }
                    for v in &mut scale {
                        if v.abs() < 0.001 {
                            *v = 0.001;
                        }
                    }
                    Some(design::transform(
                        before,
                        objects,
                        state.time,
                        Transform([
                            scale[0],
                            0.,
                            0.,
                            scale[1],
                            pivot[0] * (1. - scale[0]),
                            pivot[1] * (1. - scale[1]),
                        ]),
                        host.state().auto_key,
                    )?)
                }
                Gesture::Rotate {
                    start,
                    pivot,
                    objects,
                } => {
                    let mut angle = (position[1] - pivot[1]).atan2(position[0] - pivot[0]) - start;
                    if ui.io().key_shift() {
                        let step = std::f64::consts::PI / 12.;
                        angle = (angle / step).round() * step;
                    }
                    let (s, c) = angle.sin_cos();
                    Some(design::transform(
                        before,
                        objects,
                        state.time,
                        Transform([
                            c,
                            s,
                            -s,
                            c,
                            pivot[0] - c * pivot[0] + s * pivot[1],
                            pivot[1] - s * pivot[0] - c * pivot[1],
                        ]),
                        host.state().auto_key,
                    )?)
                }
                Gesture::Marquee { start, selected } => {
                    let b = Bounds::new(*start, position);
                    let color = ui.style_color(StyleColor::PlotLinesHovered);
                    ui.get_window_draw_list()
                        .add_rect(view.screen(b.min), view.screen(b.max), color)
                        .build();
                    if released {
                        let mut selected = selected.clone();
                        for id in &before.roots {
                            if all.get(id).is_some_and(|other| b.intersects(*other))
                                && fold_motion_canvas::editing::editable(before, *id).is_ok()
                                && before.objects[id].visible
                                && !selected.contains(id)
                            {
                                selected.push(*id);
                            }
                        }
                        state.select(host, selected);
                    }
                    None
                }
                Gesture::TextBox { id, start } => {
                    let id = *id;
                    let b = Bounds::new(*start, position);
                    let mut object = tools::new_object("text")?;
                    object.transform.x.value = b.min[0];
                    object.transform.y.value = b.min[1];
                    if let Content::Text { layout, .. } = &mut object.content {
                        layout.width = if b.size()[0] > 16. { b.size()[0] } else { 0. };
                    }
                    let edits = vec![Edit::Insert {
                        id,
                        object: Box::new(object),
                        parent: None,
                        index: before.roots.len(),
                    }];
                    state.preview(host, edits)?;
                    if released {
                        state.commit(host)?;
                        state.select(host, vec![id]);
                        self.text = Some(TextEdit {
                            generation: state.generation,
                            id,
                            text: "Text".into(),
                            focus: true,
                        });
                    }
                    None
                }
                Gesture::Pivot => {
                    self.pivot = Some(position);
                    if let [id] = state.selected.as_slice() {
                        Some(vec![design::anchor(
                            before,
                            *id,
                            state.time,
                            position,
                            host.state().auto_key,
                        )?])
                    } else {
                        None
                    }
                }
            };
            if let Some(edits) = edits {
                state.preview(host, edits)?;
                if released {
                    state.commit(host)?;
                }
            }
            if released {
                self.gesture = None;
                self.basis = None;
            }
            return Ok(true);
        }
        if focused && hovered && !ui.io().want_text_input() && !state.editing {
            if ui.is_key_pressed(Key::Delete) && !state.selected.is_empty() {
                let edits = design::selection_roots(&canvas, &state.selected)?
                    .into_iter()
                    .map(|id| Edit::Delete {
                        id,
                        expected: Box::new(canvas.objects[&id].clone()),
                    })
                    .collect();
                state.preview(host, edits)?;
                state.commit(host)?;
                state.select(host, vec![]);
                return Ok(true);
            }
            if ui.io().key_ctrl() && ui.is_key_pressed(Key::G) {
                let (id, edits) = fold_motion_canvas::hierarchy::group(&canvas, &state.selected)?;
                state.preview(host, edits)?;
                state.commit(host)?;
                state.select(host, vec![id]);
                return Ok(true);
            }
            let mut delta = [0.; 2];
            let step = if ui.io().key_shift() { 10. } else { 1. };
            for (key, axis, sign) in [
                (Key::LeftArrow, 0, -1.),
                (Key::RightArrow, 0, 1.),
                (Key::UpArrow, 1, -1.),
                (Key::DownArrow, 1, 1.),
            ] {
                if ui.is_key_pressed(key) {
                    delta[axis] += sign * step;
                }
            }
            if delta != [0.; 2] && !state.selected.is_empty() {
                let edits = tools::translate(
                    &canvas,
                    &state.selected,
                    state.time,
                    delta,
                    host.state().auto_key,
                )?;
                state.preview(host, edits)?;
                state.commit(host)?;
                return Ok(true);
            }
        }
        Ok(click)
    }
}
fn is_descendant(
    canvas: &fold_motion_canvas::model::Canvas,
    mut id: ObjectId,
    parent: ObjectId,
) -> bool {
    while let Ok((Some(p), _)) = fold_motion_canvas::hierarchy::placement(canvas, id) {
        if p == parent {
            return true;
        }
        id = p;
    }
    false
}
