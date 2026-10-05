//! Motion-only viewport tools. Geometry evaluation stays off the interaction path.
use super::state::State;
#[path = "tool_shelf.rs"]
mod shelf;
use crate::{
    Motion,
    authoring::{node, scene},
    fields::Datum,
};
use fold_foundation::ObjectId;
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_render::vector::Segment;
use fold_ui::sdk::{
    ViewerRect,
    imgui::{self, Key, MouseButton, Ui},
    toolbar::{self, ToolbarIcon},
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Select,
    Move,
    Rotate,
    Scale,
    Text,
    Rectangle,
    Ellipse,
    Pen,
}
impl Tool {
    fn name(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Text => "Text",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Pen => "Bézier Pen",
        }
    }
    fn creates(self) -> bool {
        matches!(
            self,
            Self::Text | Self::Rectangle | Self::Ellipse | Self::Pen
        )
    }
}
#[derive(Clone, Copy)]
struct Anchor {
    point: [f64; 2],
    tangent: [f64; 2],
}
struct Creation {
    id: ObjectId,
    base: Motion,
    start: [f64; 2],
    anchors: Vec<Anchor>,
    closed: bool,
    dragging: bool,
    viewer: Option<fold_platform::workspace::PanelInstanceId>,
    generation: u64,
    time: fold_foundation::Time,
}
pub(super) struct Tools {
    pub tool: Tool,
    pub snap: bool,
    pub grid: f64,
    pub edit_points: bool,
    pub fill: [f64; 4],
    pub stroke: f64,
    pub radius: f64,
    pub text_size: f64,
    creation: Option<Creation>,
    text_editing: bool,
    text_target: Option<ObjectId>,
    focus_text: bool,
}
impl Default for Tools {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            snap: false,
            grid: 10.,
            edit_points: false,
            fill: [0.75, 0.85, 1., 1.],
            stroke: 2.,
            radius: 8.,
            text_size: 36.,
            creation: None,
            text_editing: false,
            text_target: None,
            focus_text: false,
        }
    }
}
pub(super) fn insets(ui: &Ui) -> [f32; 2] {
    [ui.frame_height() + 16., ui.frame_height() + 12.]
}
pub(super) fn select(state: &mut State, host: &mut dyn DesktopClient, id: ObjectId) {
    state.selected = vec![id];
    state.group = None;
    state.parents.clear();
    state.network_object = Some(id);
    host.command(DesktopCommand::Select(Selection {
        document: state.document,
        objects: vec![id],
    }));
}
fn position(rect: ViewerRect, m: &Motion, p: [f32; 2]) -> [f64; 2] {
    [
        (p[0] - rect.origin[0]) as f64 / rect.size[0] as f64 * m.info.width as f64,
        (p[1] - rect.origin[1]) as f64 / rect.size[1] as f64 * m.info.height as f64,
    ]
}
pub(super) fn path_segments(points: &[([f64; 2], [f64; 2])], closed: bool) -> Vec<Segment> {
    let Some(first) = points.first() else {
        return vec![];
    };
    let mut result = vec![Segment::Move(first.0)];
    let mut pairs: Vec<_> = points.windows(2).map(|p| (p[0], p[1])).collect();
    if closed && points.len() > 1 {
        pairs.push((*points.last().unwrap(), *first));
    }
    for ((a, ta), (b, tb)) in pairs {
        result.push(Segment::Cubic(
            [a[0] + ta[0], a[1] + ta[1]],
            [b[0] - tb[0], b[1] - tb[1]],
            b,
        ));
    }
    if closed {
        result.push(Segment::Close);
    }
    result
}
impl Tools {
    pub fn cancel(&mut self) {
        self.creation = None;
        self.text_target = None;
        self.text_editing = false;
    }
    pub fn edit_text(&mut self, id: ObjectId) {
        self.text_target = Some(id);
        self.focus_text = true;
    }
    pub fn busy(&self) -> bool {
        self.creation.is_some()
    }
    pub fn draw(
        &mut self,
        ui: &Ui,
        state: &mut State,
        host: &mut dyn DesktopClient,
        rect: ViewerRect,
    ) -> bool {
        if self
            .creation
            .as_ref()
            .is_some_and(|g| g.viewer != host.panel_instance())
        {
            return true;
        }
        let time = host
            .state()
            .navigation
            .last()
            .map(|v| v.time)
            .unwrap_or(fold_foundation::Time::ZERO);
        if self
            .creation
            .as_ref()
            .is_some_and(|g| g.generation != state.generation || g.time != time)
        {
            self.creation = None;
            if state.editing {
                state.cancel(host);
            }
        }
        if self.text_target.is_some_and(|id| {
            !state
                .motion
                .as_ref()
                .is_some_and(|m| m.graph.node(id).is_ok())
        }) {
            self.text_target = None;
            self.text_editing = false;
        }
        let saved = ui.cursor_screen_pos();
        let inset = insets(ui);
        self.shelf(ui, state, host, rect);
        if !rect.editable {
            ui.set_cursor_screen_pos(saved);
            ui.dummy([0., 0.]);
            return true;
        }
        if self.busy() && ui.is_key_pressed(Key::Escape) {
            self.creation = None;
            state.cancel(host);
        }
        if self.tool == Tool::Pen && self.busy() && ui.is_key_pressed(Key::Enter) {
            self.finish(state, host);
        }
        let start = [
            rect.canvas_origin[0] + inset[0],
            rect.canvas_origin[1] + inset[1],
        ];
        let end = [
            rect.canvas_origin[0] + rect.canvas_size[0],
            rect.canvas_origin[1] + rect.canvas_size[1],
        ];
        let hovered = ui.is_window_hovered() && ui.is_mouse_hovering_rect(start, end);
        let Some(m) = state.motion.as_ref() else {
            ui.set_cursor_screen_pos(saved);
            ui.dummy([0., 0.]);
            return false;
        };
        let mut p = position(rect, m, ui.io().mouse_pos());
        if self.snap {
            p = p.map(|v| (v / self.grid).round() * self.grid);
        }
        let clicked =
            hovered && ui.is_mouse_clicked(MouseButton::Left) && !ui.is_key_down(Key::Space);
        if self.tool.creates() && (hovered || self.busy()) && !ui.is_key_down(Key::Space) {
            // Own left gestures so the shared viewer never pans while drawing.
            ui.set_cursor_screen_pos(start);
            ui.invisible_button(
                "motion-drawing-surface",
                [end[0] - start[0], end[1] - start[1]],
            );
            if clicked {
                self.begin(p, state, host);
            }
            if let Some(g) = &mut self.creation {
                if self.tool == Tool::Pen && g.dragging && ui.is_mouse_down(MouseButton::Left) {
                    if let Some(anchor) = g.anchors.last_mut() {
                        anchor.tangent = [p[0] - anchor.point[0], p[1] - anchor.point[1]];
                    }
                }
                let mut candidate = g.base.clone();
                if self.tool == Tool::Pen {
                    let points: Vec<_> = g.anchors.iter().map(|a| (a.point, a.tangent)).collect();
                    let mut segments = path_segments(&points, g.closed);
                    if segments.len() == 1 {
                        segments.push(Segment::Line(points[0].0));
                    }
                    node(&mut candidate, g.id).unwrap().settings["segments"] =
                        serde_json::to_value(segments).unwrap();
                } else {
                    let mut delta = [p[0] - g.start[0], p[1] - g.start[1]];
                    if ui.io().key_shift() {
                        let side = delta[0].abs().max(delta[1].abs());
                        delta = [side * delta[0].signum(), side * delta[1].signum()];
                    }
                    let xy = [g.start[0] + delta[0].min(0.), g.start[1] + delta[1].min(0.)];
                    let tr = candidate
                        .scene
                        .as_ref()
                        .unwrap()
                        .objects
                        .iter()
                        .find(|o| o.id == g.id)
                        .unwrap()
                        .transform
                        .unwrap();
                    node(&mut candidate, tr)
                        .unwrap()
                        .set("translation", Datum::Vector(xy));
                    node(&mut candidate, g.id)
                        .unwrap()
                        .set("width", Datum::Scalar(delta[0].abs().max(1.)));
                    node(&mut candidate, g.id)
                        .unwrap()
                        .set("height", Datum::Scalar(delta[1].abs().max(1.)));
                }
                if state.motion.as_ref() != Some(&candidate) {
                    state.motion = Some(candidate);
                    state.preview(host);
                }
                if ui.is_mouse_released(MouseButton::Left) {
                    if self.tool == Tool::Pen {
                        g.dragging = false;
                    } else {
                        self.finish(state, host);
                    }
                }
            }
        }
        if let Some(g) = &self.creation
            && self.tool == Tool::Pen
        {
            let m = &g.base;
            let screen = |p: [f64; 2]| {
                [
                    rect.origin[0] + (p[0] / m.info.width as f64) as f32 * rect.size[0],
                    rect.origin[1] + (p[1] / m.info.height as f64) as f32 * rect.size[1],
                ]
            };
            let draw = ui.get_window_draw_list();
            let color = ui.style_color(imgui::StyleColor::PlotLinesHovered);
            draw.with_clip_rect(start, end, || {
                for anchor in &g.anchors {
                    let a = screen([
                        anchor.point[0] - anchor.tangent[0],
                        anchor.point[1] - anchor.tangent[1],
                    ]);
                    let b = screen([
                        anchor.point[0] + anchor.tangent[0],
                        anchor.point[1] + anchor.tangent[1],
                    ]);
                    draw.add_line(a, b, color).build();
                    draw.add_circle(screen(anchor.point), 4., color)
                        .filled(true)
                        .build();
                    for p in [a, b] {
                        draw.add_circle(p, 3., color).build();
                    }
                }
            });
        }
        ui.set_cursor_screen_pos(saved);
        ui.dummy([0., 0.]);
        self.tool.creates()
    }
    fn switch(&mut self, tool: Tool, state: &mut State, host: &mut dyn DesktopClient) {
        if self.tool != tool {
            if self.text_editing {
                state.commit(host);
            }
            self.text_editing = false;
            self.text_target = None;
            if self.busy() {
                self.creation = None;
                state.cancel(host);
            }
            self.tool = tool;
        }
    }
    fn begin(&mut self, p: [f64; 2], state: &mut State, host: &mut dyn DesktopClient) {
        if let Some(g) = &mut self.creation {
            if self.tool == Tool::Pen {
                if g.anchors.len() > 2 && (p[0] - g.start[0]).hypot(p[1] - g.start[1]) < 8. {
                    g.closed = true;
                } else if g.anchors.len() < 1024 {
                    g.anchors.push(Anchor {
                        point: p,
                        tangent: [0.; 2],
                    });
                }
                g.dragging = true;
            }
            return;
        }
        state.cancel(host);
        let Some(mut m) = state.motion.clone() else {
            return;
        };
        let kind = match self.tool {
            Tool::Text => "fold.motion.text",
            Tool::Rectangle => "fold.motion.rectangle",
            Tool::Ellipse => "fold.motion.ellipse",
            Tool::Pen => "fold.motion.path",
            _ => return,
        };
        let result = (|| -> Result<ObjectId, String> {
            let id = scene::create_object(&mut m, kind)?;
            let object = m
                .scene
                .as_ref()
                .unwrap()
                .objects
                .iter()
                .find(|o| o.id == id)
                .unwrap()
                .clone();
            node(&mut m, object.transform.unwrap())?.set(
                "translation",
                Datum::Vector(if self.tool == Tool::Pen { [0.; 2] } else { p }),
            );
            node(&mut m, object.appearance.unwrap())?.set("color", Datum::Color(self.fill));
            if self.tool == Tool::Text {
                node(&mut m, id)?.set("size", Datum::Scalar(self.text_size));
                node(&mut m, id)?.set("text", Datum::Text("Text".into()));
            }
            if self.tool == Tool::Rectangle {
                node(&mut m, id)?.set("radius", Datum::Scalar(self.radius));
            }
            if self.tool == Tool::Pen {
                node(&mut m, object.appearance.unwrap())?.set("color", Datum::Color([0.; 4]));
                let mut stroke = crate::graph::Node::new("fold.motion.stroke")?;
                stroke.set("width", Datum::Scalar(self.stroke));
                stroke.set("color", Datum::Color(self.fill));
                stroke.connect("content", object.appearance.unwrap(), "content");
                node(&mut m, object.transform.unwrap())?.connect("content", stroke.id, "content");
                m.graph.nodes.push(stroke);
            }
            Ok(id)
        })();
        match result {
            Ok(id) => {
                if self.tool == Tool::Text {
                    state.motion = Some(m);
                    state.commit(host);
                    select(state, host, id);
                    self.tool = Tool::Select;
                    self.edit_text(id);
                } else {
                    self.creation = Some(Creation {
                        id,
                        base: m,
                        start: p,
                        anchors: vec![Anchor {
                            point: p,
                            tangent: [0.; 2],
                        }],
                        closed: false,
                        dragging: true,
                        viewer: host.panel_instance(),
                        generation: state.generation,
                        time: host
                            .state()
                            .navigation
                            .last()
                            .map(|v| v.time)
                            .unwrap_or(fold_foundation::Time::ZERO),
                    });
                    state.selected = vec![id];
                }
            }
            Err(e) => state.error = e,
        }
    }
    fn finish(&mut self, state: &mut State, host: &mut dyn DesktopClient) {
        if let Some(g) = self.creation.take() {
            if self.tool == Tool::Pen && g.anchors.len() < 2 {
                state.cancel(host);
                return;
            }
            state.commit(host);
            select(state, host, g.id);
            self.tool = Tool::Select;
        }
    }
}

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;
