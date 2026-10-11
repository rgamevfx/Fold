//! Viewport input owns one frozen gesture and one transaction at a time.
use super::{
    Editor, Tool,
    gesture::{Action, Gesture, gesture_roto, insert_layer},
    overlay::*,
    transform as tx,
};
use crate::roto::{Layer, Point, Roto, Spline, add, map, mul, sub};
use fold_foundation::{ObjectId, Time};
use fold_ui::sdk::{
    ExtensionUi, ViewerRect,
    imgui::{Key, MouseButton, StyleColor},
};
impl Editor {
    pub(in crate::ui) fn draw(
        &mut self,
        context: ExtensionUi<'_>,
        rect: ViewerRect,
        state: &mut super::super::state::State,
    ) {
        let ExtensionUi { ui, host } = context;
        let time = host
            .state()
            .navigation
            .last()
            .map_or(Time::ZERO, |v| v.time);

        let Some(id) = state
            .selected
            .first()
            .copied()
            .filter(|_| state.selected.len() == 1)
        else {
            if self.active() {
                state.cancel(host);
                self.reset_gesture();
            }
            return;
        };
        let Some(graph) = state.graph.as_ref() else {
            return;
        };
        let Some(_) = graph
            .node(id)
            .ok()
            .filter(|n| n.operation.operation == "Roto")
        else {
            if self.active() {
                state.cancel(host);
                self.reset_gesture();
            }
            self.target = None;
            return;
        };
        if self.target != Some(id) && self.active() {
            state.cancel(host);
            self.reset_gesture();
        }
        self.select(id);
        let auto_key = self.auto_key;
        if self
            .gesture
            .as_ref()
            .is_some_and(|g| g.time != time || g.generation != state.generation)
            || !rect.editable
        {
            if self.active() {
                state.cancel(host);
                self.reset_gesture();
            }
            if !rect.editable {
                return;
            }
        }
        let Ok(roto) = state.roto_at(id, time) else {
            return;
        };
        let Some(graph) = state.graph.as_ref() else {
            return;
        };
        if rect.size.iter().any(|v| *v <= 0.) {
            return;
        }
        let view = View {
            rect,
            dimensions: [graph.info.width, graph.info.height],
        };
        self.layer = self
            .layer
            .filter(|id| roto.root.find(*id).is_some())
            .or(Some(roto.root.id));
        let save = ui.cursor_screen_pos();
        ui.set_cursor_screen_pos(rect.canvas_origin);
        let old_tool = self.tool;
        {
            let _disabled = ui.begin_disabled_with_cond(self.gesture.is_some());
            super::inspector::toolbar(
                ui,
                self,
                ui.calc_text_size("Feather")[0]
                    + ui.frame_height()
                    + 3. * ui.clone_style().item_spacing()[0],
            );
        }
        ui.same_line();
        ui.checkbox("Feather", &mut self.show_feather);
        if old_tool != self.tool {
            if state.editing && !state.property_editing {
                state.cancel(host);
            }
            self.reset_gesture();
        }
        let top = rect.canvas_origin[1] + ui.frame_height() + ui.clone_style().item_spacing()[1];
        let origin = [rect.canvas_origin[0], top];
        let size = [
            rect.canvas_size[0],
            (rect.canvas_origin[1] + rect.canvas_size[1] - top).max(1.),
        ];
        ui.set_cursor_screen_pos(origin);
        if !ui.is_key_down(Key::Space) {
            ui.invisible_button("##roto-canvas", size);
        }
        // Document guides remain interactive while the image worker catches up.
        let hovered = ui.is_item_hovered()
            && ui.is_mouse_hovering_rect(origin, [origin[0] + size[0], origin[1] + size[1]]);
        let focused = ui.is_window_focused() && !ui.io().want_text_input();
        if focused && ui.is_key_pressed(Key::Escape) {
            if state.editing && !state.property_editing {
                state.cancel(host);
            }
            self.reset_gesture();
            ui.set_cursor_screen_pos(save);
            return;
        }
        if focused && !self.active() && !ui.is_key_down(Key::Space) {
            for (key, tool) in [
                (Key::V, Tool::Select),
                (Key::R, Tool::Rectangle),
                (Key::E, Tool::Ellipse),
                (Key::B, Tool::Bezier),
                (Key::S, Tool::BSpline),
            ] {
                if ui.is_key_pressed(key) {
                    self.tool = tool;
                }
            }
        }
        let p = view.image(ui.io().mouse_pos());
        let mut result: Result<(), String> = Ok(());
        if self.tool == Tool::Select && hovered && ui.is_mouse_double_clicked(MouseButton::Left) {
            if let Some(layer) = self
                .layer
                .filter(|id| roto.root.editable(*id))
                .and_then(|id| roto.root.find(id))
            {
                if let Some(shape) = &layer.shape {
                    let m = roto.transform_for(layer.id).unwrap();
                    if !shape
                        .points
                        .iter()
                        .any(|q| view.near(p, map(m, q.position), 10.))
                    {
                        let mut best = (f64::INFINITY, 0, 0.5);
                        for (segment, c) in shape.cubics(false).into_iter().enumerate() {
                            for i in 1..64 {
                                let t = i as f64 / 64.;
                                let q = view.screen(map(m, curve(c, t)));
                                let mouse = ui.io().mouse_pos();
                                let distance = (q[0] - mouse[0]).hypot(q[1] - mouse[1]) as f64;
                                if distance < best.0 {
                                    best = (distance, segment, t);
                                }
                            }
                        }
                        if best.0 < 8. {
                            let mut after = roto.clone();
                            result = after
                                .root
                                .find_mut(layer.id)
                                .unwrap()
                                .shape
                                .as_mut()
                                .unwrap()
                                .insert_point(best.1, best.2)
                                .and_then(|point| {
                                    super::edit(
                                        state.graph.as_mut().unwrap().node_mut(id)?,
                                        &roto,
                                        after,
                                        time,
                                        auto_key,
                                    )?;
                                    state.commit(host);
                                    self.gesture = None;
                                    self.points = [point].into();
                                    Ok(())
                                });
                            if let Err(e) = result {
                                state.error = e;
                            }
                            ui.set_cursor_screen_pos(save);
                            return;
                        }
                    }
                }
            }
        }
        if self.tool == Tool::Bezier || self.tool == Tool::BSpline {
            if hovered && ui.is_mouse_clicked(MouseButton::Left) && !ui.is_key_down(Key::Space) {
                let close = self
                    .drawing
                    .as_ref()
                    .and_then(|l| l.shape.as_ref())
                    .is_some_and(|s| {
                        s.points.len() >= 3 && view.near(p, s.points[0].position, 10.)
                    });
                if close {
                    result = self.finish_drawing(state, host, &roto, id, time, auto_key);
                } else {
                    let layer = self.drawing.get_or_insert_with(|| {
                        Layer::curve(
                            if self.tool == Tool::Bezier {
                                Spline::Bezier
                            } else {
                                Spline::BSpline
                            },
                            vec![],
                        )
                    });
                    let shape = layer.shape.as_mut().unwrap();
                    shape.closed = false;
                    shape.points.push(Point::new(p));
                    self.pen_anchor = Some(p);
                }
            }
            if let (Some(layer), Some(anchor)) = (&mut self.drawing, self.pen_anchor) {
                if ui.is_mouse_down(MouseButton::Left) {
                    let s = layer.shape.as_mut().unwrap();
                    if s.spline == Spline::Bezier {
                        let last = s.points.last_mut().unwrap();
                        last.outgoing = sub(p, anchor);
                        last.incoming = mul(last.outgoing, -1.);
                    }
                } else {
                    self.pen_anchor = None;
                }
            }
            if focused && ui.is_key_pressed(Key::Enter) {
                result = self.finish_drawing(state, host, &roto, id, time, auto_key);
            }
        } else if hovered
            && ui.is_mouse_clicked(MouseButton::Left)
            && !ui.is_key_down(Key::Space)
            && !state.property_editing
        {
            let mut action = None;
            let mut created = None;
            if matches!(self.tool, Tool::Rectangle | Tool::Ellipse) {
                let layer = Layer::rectangle(p, p, self.tool == Tool::Ellipse);
                created = Some(layer);
                action = Some(Action::Create(self.tool == Tool::Ellipse));
            } else if let Some(layer) = self.layer.and_then(|id| roto.root.find(id)) {
                if let Some(shape) = &layer.shape {
                    let m = roto.transform_for(layer.id).unwrap();
                    if roto.root.editable(layer.id) {
                        // Tangents win over the selection box; points win over
                        // collapsed handles. All hit targets are screen-sized.
                        if self.show_feather {
                            for (i, q) in shape.feather_positions().into_iter().enumerate() {
                                if view.near(p, map(m, q), 8.)
                                    && !view.near(map(m, q), map(m, shape.points[i].position), 1.)
                                {
                                    action = Some(Action::Feather(shape.points[i].id));
                                    break;
                                }
                            }
                        }
                        if action.is_none() {
                            for point in &shape.points {
                                if self.points.contains(&point.id) {
                                    for (incoming, h) in
                                        [(true, point.incoming), (false, point.outgoing)]
                                    {
                                        if h != [0.; 2]
                                            && view.near(p, map(m, add(point.position, h)), 8.)
                                        {
                                            action = Some(Action::Handle(point.id, incoming));
                                        }
                                    }
                                }
                            }
                        }
                        if action.is_none() {
                            for point in &shape.points {
                                if view.near(p, map(m, point.position), 8.) {
                                    if ui.io().key_shift() {
                                        if !self.points.insert(point.id) {
                                            self.points.remove(&point.id);
                                        }
                                    } else if !self.points.contains(&point.id) {
                                        self.points.clear();
                                        self.points.insert(point.id);
                                    }
                                    action = Some(Action::Move);
                                    break;
                                }
                            }
                        }
                        if action.is_none() {
                            if let Some(b) = selected_bounds(&roto, self) {
                                let b = handle_bounds(view, b);
                                if view.near(p, rotation_handle(view, b), 9.) {
                                    action = Some(Action::Rotate);
                                }
                                for (i, c) in tx::corners(b).into_iter().enumerate() {
                                    if view.near(p, c, 9.) {
                                        action = Some(if ui.io().key_ctrl() {
                                            Action::Pin(i)
                                        } else {
                                            Action::Scale(i)
                                        });
                                    }
                                }
                                if action.is_none()
                                    && !ui.io().key_shift()
                                    && p[0] >= b[0]
                                    && p[0] <= b[2]
                                    && p[1] >= b[1]
                                    && p[1] <= b[3]
                                {
                                    action = Some(Action::Move);
                                }
                            }
                        }
                    }
                }
            }
            if action.is_none() {
                if let Some(hit) = shape_hit(&roto.root, &roto, view, p) {
                    self.layer = Some(hit);
                    self.points = roto
                        .root
                        .find(hit)
                        .unwrap()
                        .shape
                        .as_ref()
                        .unwrap()
                        .points
                        .iter()
                        .map(|p| p.id)
                        .collect();
                    action = Some(Action::Move);
                } else {
                    if !ui.io().key_shift() {
                        self.points.clear();
                    }
                    action = Some(Action::Marquee);
                }
            }
            if let Some(action) = action {
                self.gesture = Some(Gesture {
                    before: roto.clone(),
                    node: state.graph.as_ref().unwrap().node(id).unwrap().clone(),
                    time,
                    generation: state.generation,
                    start: p,
                    action,
                    layer: self.layer.unwrap(),
                    points: self.points.clone(),
                    bounds: selected_bounds(&roto, self),
                    created,
                });
            }
        }
        if let Some(g) = &self.gesture {
            if matches!(g.action, Action::Marquee) {
                let a = view.screen(g.start);
                let b = view.screen(p);
                ui.get_window_draw_list()
                    .add_rect(a, b, ui.style_color(StyleColor::PlotLinesHovered))
                    .build();
                if let Some(shape) = roto.root.find(g.layer).and_then(|l| l.shape.as_ref()) {
                    let m = roto.transform_for(g.layer).unwrap();
                    let b = [
                        g.start[0].min(p[0]),
                        g.start[1].min(p[1]),
                        g.start[0].max(p[0]),
                        g.start[1].max(p[1]),
                    ];
                    self.points = g.points.clone();
                    for point in &shape.points {
                        let q = map(m, point.position);
                        if q[0] >= b[0] && q[0] <= b[2] && q[1] >= b[1] && q[1] <= b[3] {
                            self.points.insert(point.id);
                        }
                    }
                }
            } else if (ui.is_mouse_down(MouseButton::Left)
                || ui.is_mouse_released(MouseButton::Left))
                && ui.io().mouse_delta() != [0.; 2]
            {
                result =
                    gesture_roto(g, p, ui.io().key_shift(), ui.io().key_alt()).and_then(|after| {
                        let graph = state.graph.as_mut().ok_or("Missing composite")?;
                        let node = graph.node_mut(id)?;
                        *node = g.node.clone();
                        super::edit(node, &g.before, after, time, auto_key)
                    });
                if result.is_ok() {
                    state.preview(host);
                }
            }
            if !ui.is_mouse_down(MouseButton::Left) {
                if let Some(created) = &g.created {
                    self.layer = Some(created.id);
                    self.points = created
                        .shape
                        .as_ref()
                        .unwrap()
                        .points
                        .iter()
                        .map(|p| p.id)
                        .collect();
                    self.tool = Tool::Select;
                }
                self.gesture = None;
                if state.editing && !state.property_editing {
                    state.commit(host);
                }
            }
        }
        if focused && self.tool == Tool::Select && self.gesture.is_none() {
            if ui.io().key_ctrl() && ui.is_key_pressed(Key::A) {
                if let Some(s) = self
                    .layer
                    .and_then(|id| roto.root.find(id))
                    .and_then(|l| l.shape.as_ref())
                {
                    self.points = s.points.iter().map(|p| p.id).collect();
                }
            }
            if ui.is_key_pressed(Key::Delete) || ui.is_key_pressed(Key::Backspace) {
                let mut after = roto.clone();
                if let Some(id) = self.layer.filter(|id| after.root.editable(*id)) {
                    if let Some(s) = after.root.find_mut(id).and_then(|l| l.shape.as_mut()) {
                        s.points.retain(|p| !self.points.contains(&p.id));
                        result = state
                            .graph
                            .as_mut()
                            .unwrap()
                            .node_mut(id)
                            .and_then(|n| super::edit(n, &roto, after, time, auto_key));
                        if result.is_ok() {
                            state.commit(host);
                            self.points.clear();
                        }
                    }
                }
            }
            if ui.is_key_pressed(Key::L) {
                let mut after = roto.clone();
                if let Some(id) = self.layer.filter(|id| after.root.editable(*id)) {
                    if let Some(s) = after.root.find_mut(id).and_then(|l| l.shape.as_mut()) {
                        for p in &mut s.points {
                            if self.points.contains(&p.id) {
                                p.linked = !p.linked;
                            }
                        }
                        result = state
                            .graph
                            .as_mut()
                            .unwrap()
                            .node_mut(id)
                            .and_then(|n| super::edit(n, &roto, after, time, auto_key));
                        if result.is_ok() {
                            state.commit(host);
                        }
                    }
                }
            }
        }
        if let Err(error) = result {
            state.error = error;
        }
        // Observe the local draft immediately; rendered pixels update asynchronously.
        let visible = state.roto_at(id, time).unwrap_or(roto);
        draw_layer(ui, &visible.root, &visible, view, self, time);
        if let Some(b) = selected_bounds(&visible, self) {
            if self.points.len() > 1 {
                draw_box(ui, view, b);
            }
        }
        if let Some(layer) = &self.drawing {
            draw_draft(
                ui,
                layer,
                view,
                hovered.then_some(p),
                self.pen_anchor.is_some(),
            );
        }
        if !state.error.is_empty() {
            ui.get_window_draw_list().add_text(
                [origin[0] + 8., origin[1] + 8.],
                ui.style_color(StyleColor::Text),
                &state.error,
            );
        }
        ui.set_cursor_screen_pos(save);
    }
    fn finish_drawing(
        &mut self,
        state: &mut super::super::state::State,
        host: &mut dyn fold_platform::desktop::DesktopClient,
        before: &Roto,
        id: ObjectId,
        time: Time,
        auto: bool,
    ) -> Result<(), String> {
        let Some(mut layer) = self.drawing.clone() else {
            return Ok(());
        };
        if layer.shape.as_ref().unwrap().points.len() < 3 {
            return Err("A closed shape needs at least three points".into());
        }
        layer.shape.as_mut().unwrap().closed = true;
        let selected = layer.id;
        let points = layer
            .shape
            .as_ref()
            .unwrap()
            .points
            .iter()
            .map(|p| p.id)
            .collect();
        let mut after = before.clone();
        insert_layer(&mut after, layer, self.layer)?;
        super::edit(
            state.graph.as_mut().unwrap().node_mut(id)?,
            before,
            after,
            time,
            auto,
        )?;
        state.commit(host);
        if !state.error.is_empty() {
            return Err(state.error.clone());
        }
        self.drawing = None;
        self.layer = Some(selected);
        self.points = points;
        self.tool = Tool::Select;
        self.pen_anchor = None;
        Ok(())
    }
}
