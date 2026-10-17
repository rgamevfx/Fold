use super::overlay::{View, guides};
use super::state::State;
use fold_foundation::ObjectId;
use fold_motion_canvas::{
    editing::{self, Edit},
    model::*,
    tools,
};
use fold_ui::sdk::{
    ExtensionUi, ViewerRect,
    imgui::{Key, MouseButton, StyleColor},
    toolbar::{self, ToolbarIcon},
};

#[derive(Clone, Copy, Default, PartialEq)]
enum Tool {
    #[default]
    Select,
    Direct,
    Text,
    Gradient,
    Rectangle,
    Ellipse,
    Pen,
}
enum Gesture {
    Shape {
        id: ObjectId,
        start: [f64; 2],
        object: Object,
        ellipse: bool,
    },
    Pen {
        id: ObjectId,
        object: Object,
    },
}
#[derive(Default)]
pub(super) struct Viewport {
    tool: Tool,
    design: super::design_input::DesignInput,
    gradient: super::gradient_input::GradientInput,
    path: super::path_input::PathInput,
    gesture: Option<Gesture>,
    basis: Option<([f32; 2], [f32; 2])>,
    generation: u64,
    anchor_menu: Option<(fold_foundation::DocumentId, ObjectId, ObjectId)>,
}
impl Viewport {
    pub fn initialize(&mut self, context: &fold_ui::sdk::imgui::Context) {
        self.path.initialize(context);
    }
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.path.cancel();
        self.design.cancel();
        self.gradient.cancel();
        self.basis = None;
        self.anchor_menu = None;
    }
    pub fn draw(&mut self, context: ExtensionUi<'_>, rect: ViewerRect, state: &mut State) {
        let ExtensionUi { ui, host } = context;
        if self.gesture.is_some() && self.generation != state.generation {
            self.cancel();
        }
        if self.gesture.is_some()
            && (!state.editing || !rect.editable || self.basis != Some((rect.origin, rect.size)))
        {
            state.cancel(host);
            self.cancel();
        }
        let Some(canvas) = state.canvas().cloned() else {
            return;
        };
        if rect.size.iter().any(|v| *v <= 0.) {
            return;
        }
        let view = View {
            rect,
            dimensions: [canvas.width, canvas.height],
        };
        let save = ui.cursor_screen_pos();
        ui.set_cursor_screen_pos(rect.canvas_origin);
        let entries = [
            (
                Tool::Select,
                ToolbarIcon::Select,
                "Select (V): drag to move; corners resize; top handle rotates; Alt-drag the pivot",
            ),
            (
                Tool::Direct,
                ToolbarIcon::Move,
                "Direct selection (A): edit path points",
            ),
            (
                Tool::Text,
                ToolbarIcon::Text,
                "Text (T): click to create, double-click to edit",
            ),
            (Tool::Rectangle, ToolbarIcon::Rectangle, "Rectangle (R)"),
            (Tool::Ellipse, ToolbarIcon::Ellipse, "Ellipse (E)"),
            (Tool::Pen, ToolbarIcon::Pen, "Pen (P)"),
            (
                Tool::Gradient,
                ToolbarIcon::Curve,
                "Gradient (G): drag fill gradient handles",
            ),
        ];
        let mut chosen = None;
        let _disabled = ui.begin_disabled_with_cond(!rect.editable || state.busy());
        for (index, (tool, icon, label)) in entries.iter().enumerate() {
            if ui.cursor_screen_pos()[0] + 2. * (ui.frame_height() + 8.)
                > rect.canvas_origin[0] + rect.canvas_size[0]
            {
                if let Some(_menu) = toolbar::icon_menu(ui, "motion-tools", "More tools") {
                    for (tool, _, label) in &entries[index..] {
                        if ui.menu_item(label) {
                            chosen = Some(*tool);
                        }
                    }
                }
                break;
            }
            let _color = (*tool == self.tool).then(|| {
                ui.push_style_color(StyleColor::Button, ui.style_color(StyleColor::Header))
            });
            let tooltip = if *tool == Tool::Pen {
                "Pen (P): draw a path or add an anchor to a selected curve"
            } else {
                label
            };
            if toolbar::icon_button(ui, label, *icon, tooltip) {
                chosen = Some(*tool);
            }
            ui.same_line();
        }
        self.design.controls(ui);
        drop(_disabled);
        if let Some(tool) = chosen {
            state.cancel(host);
            self.cancel();
            self.tool = tool;
        }
        let top = rect.canvas_origin[1] + ui.frame_height() + ui.clone_style().item_spacing()[1];
        let origin = [rect.canvas_origin[0], top];
        let size = [
            rect.canvas_size[0],
            (rect.canvas_origin[1] + rect.canvas_size[1] - top).max(1.),
        ];
        ui.set_cursor_screen_pos(origin);
        let panning = ui.is_key_down(Key::Space);
        if !panning && !matches!(self.tool, Tool::Select | Tool::Direct) {
            ui.invisible_button("##motion-canvas", size);
        }
        let hovered = !panning
            && if matches!(self.tool, Tool::Select | Tool::Direct) {
                ui.is_window_hovered_with_flags(fold_ui::sdk::imgui::WindowHoveredFlags::ROOT_AND_CHILD_WINDOWS | fold_ui::sdk::imgui::WindowHoveredFlags::ALLOW_WHEN_BLOCKED_BY_ACTIVE_ITEM)
            } else {
                ui.is_item_hovered()
            }
            && ui.is_mouse_hovering_rect(origin, [origin[0] + size[0], origin[1] + size[1]]);
        let focused = ui.is_window_focused_with_flags(
            fold_ui::sdk::imgui::FocusedFlags::ROOT_AND_CHILD_WINDOWS,
        ) && !ui.io().want_text_input();
        if focused && ui.is_key_pressed(Key::Escape) {
            if self.gesture.is_none() && !state.editing && !state.busy() {
                state.point = None;
            }
            state.cancel(host);
            self.cancel();
        }
        if focused && self.gesture.is_none() {
            for (key, tool) in [
                (Key::V, Tool::Select),
                (Key::A, Tool::Direct),
                (Key::T, Tool::Text),
                (Key::R, Tool::Rectangle),
                (Key::E, Tool::Ellipse),
                (Key::P, Tool::Pen),
                (Key::G, Tool::Gradient),
            ] {
                if !ui.io().key_ctrl() && ui.is_key_pressed(key) && self.tool != tool {
                    state.cancel(host);
                    self.cancel();
                    self.tool = tool;
                }
            }
        }
        if focused
            && rect.editable
            && rect.image_current
            && !state.busy()
            && !state.editing
            && self.gesture.is_none()
            && ui.is_key_pressed(Key::Delete)
            && let Some((object, point)) = state.point
        {
            let result = state
                .preview(
                    host,
                    vec![Edit::RemovePoint {
                        object,
                        expected: Box::new(canvas.objects[&object].clone()),
                        point,
                    }],
                )
                .and_then(|_| state.commit(host));
            if let Err(error) = result {
                state.cancel(host);
                state.error = error;
            } else {
                state.point = None;
            }
        }
        let position = view.image(ui.io().mouse_pos());
        let click = hovered
            && !state.busy()
            && rect.editable
            && rect.image_current
            && ui.is_mouse_clicked(MouseButton::Left);
        if hovered
            && rect.editable
            && rect.image_current
            && !state.busy()
            && !state.editing
            && self.gesture.is_none()
            && ui.is_mouse_clicked(MouseButton::Right)
        {
            if let Ok(transforms) = tools::transforms(&canvas, state.time) {
                let mut closest = (9_f32, None);
                for id in &state.selected {
                    let Some(Object {
                        content: Content::Path { contours, .. },
                        ..
                    }) = canvas.objects.get(id)
                    else {
                        continue;
                    };
                    for contour in contours {
                        for point in &contour.points {
                            let Ok(anchor) = point.position_at(state.time) else {
                                continue;
                            };
                            let p = view.screen(transforms[id].point(anchor));
                            let mouse = ui.io().mouse_pos();
                            let distance = (p[0] - mouse[0]).hypot(p[1] - mouse[1]);
                            if distance < closest.0 {
                                closest = (distance, Some((*id, point.id)));
                            }
                        }
                    }
                }
                if let (Some(document), Some((object, point))) = (state.document, closest.1) {
                    self.anchor_menu = Some((document, object, point));
                    state.point = Some((object, point));
                    ui.open_popup("motion-anchor");
                }
            }
        }
        if let Some(_popup) = ui.begin_popup("motion-anchor") {
            let target = self.anchor_menu.filter(|(document, object, point)| {
                state.document == Some(*document)
                    && state.selected.contains(object)
                    && editing::editable(&canvas, *object).is_ok()
                    && matches!(&canvas.objects[object].content, Content::Path { contours, .. }
                        if contours.iter().flat_map(|c| &c.points).any(|p| p.id == *point))
            });
            let first_enabled = target.is_some_and(|(_, object, point)| {
                matches!(&canvas.objects[&object].content, Content::Path { contours, .. }
                    if contours.iter().any(|c| c.closed && c.points.iter().position(|p| p.id == point).is_some_and(|index| index > 0)))
            });
            let _disabled = ui.begin_disabled_with_cond(
                !rect.editable || state.busy() || state.editing || target.is_none(),
            );
            let first = ui.menu_item_enabled_selected_no_shortcut(
                "Make first anchor",
                false,
                first_enabled,
            );
            let remove = ui.menu_item("Remove anchor");
            if ui.is_item_hovered() {
                ui.tooltip_text("Remove this anchor and its animation. Keep the path object.");
            }
            if (first || remove)
                && let Some((document, object, point)) = self.anchor_menu.take()
                && state.document == Some(document)
            {
                let edits = if remove {
                    Ok(vec![Edit::RemovePoint {
                        object,
                        expected: Box::new(canvas.objects[&object].clone()),
                        point,
                    }])
                } else {
                    tools::first_anchor(&canvas, object, point)
                };
                let result = edits.and_then(|edits| {
                    if edits.is_empty() {
                        return Ok(());
                    }
                    state.preview(host, edits)?;
                    state.commit(host)
                });
                if let Err(error) = result {
                    state.cancel(host);
                    state.error = error;
                } else if remove {
                    state.point = None;
                }
            }
        }
        let path_owned = if self.tool == Tool::Direct && self.gesture.is_none() {
            match self
                .path
                .draw(ui, host, state, view, origin, size, !panning)
            {
                Ok(owned) => owned,
                Err(error) => {
                    state.cancel(host);
                    self.path.cancel();
                    state.error = error;
                    true
                }
            }
        } else {
            false
        };
        if self.tool == Tool::Gradient && rect.editable && !state.busy() {
            if let Err(error) = self.gradient.draw(ui, host, state, view, click) {
                state.cancel(host);
                self.gradient.cancel();
                state.error = error;
            }
        }
        self.design.text_tool = self.tool == Tool::Text;
        let design_owned = match self.design.draw(
            ui,
            host,
            state,
            view,
            matches!(self.tool, Tool::Select | Tool::Text) && !path_owned,
            hovered,
            click && !path_owned,
        ) {
            Ok(owned) => owned,
            Err(error) => {
                state.cancel(host);
                self.design.cancel();
                state.error = error;
                true
            }
        };
        let click = click && !path_owned && !design_owned;
        let mut finish = false;
        let mut select_after = None;
        let mut point_after = None;
        let result = (|| -> Result<(), String> {
            if let Some(Gesture::Pen { id, object }) = &mut self.gesture {
                let Content::Path { contours, .. } = &mut object.content else {
                    unreachable!()
                };
                if focused && ui.is_key_pressed(Key::Enter) {
                    finish = true;
                    select_after = Some(*id);
                }
                if click {
                    if contours[0].points.len() >= 2
                        && view.near(contours[0].points[0].position, position, 8.)
                    {
                        contours[0].closed = true;
                        finish = true;
                        select_after = Some(*id);
                    } else {
                        contours[0].points.push(point(position));
                    }
                }
                if ui.is_mouse_down(MouseButton::Left) && !finish {
                    let last = contours[0].points.last_mut().expect("pen anchor");
                    last.outgoing = [
                        position[0] - last.position[0],
                        position[1] - last.position[1],
                    ];
                    last.incoming = [-last.outgoing[0], -last.outgoing[1]];
                }
                if click || ui.is_mouse_down(MouseButton::Left) || finish {
                    state.preview(
                        host,
                        vec![Edit::Insert {
                            id: *id,
                            object: Box::new(object.clone()),
                            parent: None,
                            index: state.before.as_ref().expect("document").roots.len(),
                        }],
                    )?;
                }
            } else if click && self.gesture.is_none() {
                match self.tool {
                    Tool::Rectangle | Tool::Ellipse => {
                        let ellipse = self.tool == Tool::Ellipse;
                        let object = if ellipse {
                            tools::ellipse("Ellipse", position, position)
                        } else {
                            tools::rectangle("Rectangle", position, position)
                        };
                        self.gesture = Some(Gesture::Shape {
                            id: ObjectId::new(),
                            start: position,
                            object,
                            ellipse,
                        });
                    }
                    Tool::Pen => {
                        let transforms = tools::transforms(&canvas, state.time)?;
                        let segment = state.selected.iter().rev().find_map(|id| {
                            let Content::Path { contours, .. } = &canvas.objects.get(id)?.content
                            else {
                                return None;
                            };
                            let sampled = contours
                                .iter()
                                .map(|c| c.at(state.time))
                                .collect::<Result<Vec<_>, _>>()
                                .ok()?;
                            super::picking::segment(&sampled, transforms[id], position, view)
                                .map(|(contour, start, fraction)| (*id, contour, start, fraction))
                        });
                        if let Some((object, contour, start, fraction)) = segment {
                            let (point, edit) = tools::split_segment(
                                &canvas, object, contour, start, fraction, state.time,
                            )?;
                            state.preview(host, vec![edit])?;
                            finish = true;
                            select_after = Some(object);
                            point_after = Some((object, point));
                        } else {
                            let mut object = Object::new(
                                "Path",
                                Content::Path {
                                    contours: vec![Contour {
                                        id: ObjectId::new(),
                                        points: vec![point(position)],
                                        closed: false,
                                        extensions: Default::default(),
                                    }],
                                    extensions: Default::default(),
                                },
                            );
                            object.paint.fill = None;
                            object.paint.stroke = Some([0.8, 0.7, 1., 1.].into());
                            object.paint.stroke_width.value = 3.;
                            let id = ObjectId::new();
                            state.preview(
                                host,
                                vec![Edit::Insert {
                                    id,
                                    object: Box::new(object.clone()),
                                    parent: None,
                                    index: canvas.roots.len(),
                                }],
                            )?;
                            self.gesture = Some(Gesture::Pen { id, object });
                        }
                    }
                    Tool::Select | Tool::Text | Tool::Direct | Tool::Gradient => {}
                }
                self.basis = self.gesture.as_ref().map(|_| (rect.origin, rect.size));
                self.generation = state.generation;
            }
            if let Some(gesture) = &mut self.gesture {
                let before = state.before.as_ref().ok_or("No Motion document")?;
                match gesture {
                    Gesture::Shape {
                        id,
                        start,
                        object,
                        ellipse,
                    } => {
                        let position = if ui.io().key_shift() {
                            let d = [position[0] - start[0], position[1] - start[1]];
                            let side = d[0].abs().max(d[1].abs());
                            [
                                start[0] + side * d[0].signum(),
                                start[1] + side * d[1].signum(),
                            ]
                        } else {
                            position
                        };
                        let left = [start[0].min(position[0]), start[1].min(position[1])];
                        let dimensions = [
                            (position[0] - start[0]).abs(),
                            (position[1] - start[1]).abs(),
                        ];
                        if *ellipse {
                            object.transform.x.value = left[0] + dimensions[0] / 2.;
                            object.transform.y.value = left[1] + dimensions[1] / 2.;
                            if let Content::Ellipse {
                                radius_x, radius_y, ..
                            } = &mut object.content
                            {
                                radius_x.value = dimensions[0] / 2.;
                                radius_y.value = dimensions[1] / 2.;
                            }
                        } else {
                            object.transform.x.value = left[0];
                            object.transform.y.value = left[1];
                            object.transform.anchor_x.value = dimensions[0] / 2.;
                            object.transform.anchor_y.value = dimensions[1] / 2.;
                            if let Content::Rectangle { width, height, .. } = &mut object.content {
                                width.value = dimensions[0];
                                height.value = dimensions[1];
                            }
                        }
                        let index = before.roots.len();
                        state.preview(
                            host,
                            vec![Edit::Insert {
                                id: *id,
                                object: Box::new(object.clone()),
                                parent: None,
                                index,
                            }],
                        )?;
                        if ui.is_mouse_released(MouseButton::Left) {
                            finish = true;
                            select_after = Some(*id);
                        }
                    }
                    Gesture::Pen { .. } => {}
                }
            }
            if finish {
                if let Some(Gesture::Pen { id, object }) = &self.gesture
                    && let Content::Path { contours, .. } = &object.content
                    && let Some(point) = contours.last().and_then(|c| c.points.last())
                {
                    point_after = Some((*id, point.id));
                }
                state.commit(host)?;
                self.cancel();
                if let Some(id) = select_after {
                    state.select(host, vec![id]);
                }
                if let Some(point) = point_after {
                    state.point = Some(point);
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            state.cancel(host);
            self.cancel();
            state.error = error;
        }
        if let Some(canvas) = state.canvas() {
            let selected = match &self.gesture {
                Some(Gesture::Shape { id, .. } | Gesture::Pen { id, .. }) => vec![*id],
                _ => state.selected.clone(),
            };
            if let Ok(transforms) = tools::transforms(canvas, state.time) {
                guides(
                    ui,
                    canvas,
                    &transforms,
                    &selected,
                    state.point,
                    view,
                    state.time,
                    self.tool != Tool::Select || self.gesture.is_some(),
                );
            }
        }
        ui.set_cursor_screen_pos(save);
    }
}
fn point(position: [f64; 2]) -> Point {
    Point {
        position_track: None,
        incoming_track: None,
        outgoing_track: None,
        id: ObjectId::new(),
        position,
        incoming: [0.; 2],
        outgoing: [0.; 2],
        extensions: Default::default(),
    }
}
