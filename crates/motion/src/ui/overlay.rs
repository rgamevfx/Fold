//! Lightweight authored-base handles. Never evaluates text, fields, or media on UI.
use super::state::Shared;
use crate::{
    Motion,
    fields::Datum,
    geometry::{IDENTITY, transform},
    graph::{Input, Node},
};
use fold_foundation::ObjectId;
use fold_ui::sdk::{
    ExtensionUi, GRAPH_COLORS, ViewerRect,
    imgui::{self, MouseButton},
};
#[cfg(test)]
#[path = "overlay_tests.rs"]
mod tests;

#[derive(Clone)]
enum Target {
    Rotation,
    Scale,
    Vector(String),
    Radius,
    Size,
    Point(usize, usize),
}
#[derive(Clone)]
struct Handle {
    node: ObjectId,
    target: Target,
    point: [f64; 2],
    matrix: [f64; 6],
}
struct Gesture {
    handle: Handle,
    start: [f32; 2],
    base: Motion,
    generation: u64,
    viewer: Option<fold_platform::workspace::PanelInstanceId>,
    time: fold_foundation::Time,
}
#[derive(Default)]
pub struct Overlay {
    tools: super::tools::Tools,
    picking: super::picking::Picking,
    gesture: Option<Gesture>,
}
fn vector(n: &Node, key: &str) -> Option<[f64; 2]> {
    if let Some(Input::Value(Datum::Vector(v))) = n.inputs.get(key) {
        Some(*v)
    } else {
        None
    }
}
fn scalar(n: &Node, key: &str) -> Option<f64> {
    if let Some(Input::Value(Datum::Scalar(v))) = n.inputs.get(key) {
        Some(*v)
    } else {
        None
    }
}
fn matrix(n: &Node) -> Option<[f64; 6]> {
    Some(transform(
        vector(n, "translation")?,
        scalar(n, "rotation")?,
        vector(n, "scale")?,
        vector(n, "pivot")?,
    ))
}
fn project(m: [f64; 6], p: [f64; 2]) -> [f64; 2] {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}
/// Direct handles require an unambiguous downstream authored Transform. A
/// branched construction must select the desired Transform explicitly.
fn downstream(m: &Motion, selected: ObjectId) -> Option<&Node> {
    let mut pending = vec![selected];
    let mut seen = std::collections::BTreeSet::new();
    let mut found = Vec::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        for n in &m.graph.nodes {
            if n.inputs
                .values()
                .any(|v| matches!(v,Input::Link(l)if l.node==id))
            {
                if n.kind == "fold.motion.transform" {
                    found.push(n);
                } else {
                    pending.push(n.id);
                }
            }
        }
    }
    if found.len() == 1 {
        Some(found[0])
    } else {
        None
    }
}
fn handles(m: &Motion, selected: ObjectId) -> Vec<Handle> {
    let Ok(node) = m.graph.node(selected) else {
        return vec![];
    };
    let mut result = Vec::new();
    let downstream = m
        .scene
        .as_ref()
        .and_then(|s| s.objects.iter().find(|o| o.owns(selected)))
        .and_then(|o| o.transform)
        .and_then(|id| m.graph.node(id).ok())
        .or_else(|| downstream(m, selected));
    let outer = downstream.and_then(matrix).unwrap_or(IDENTITY);
    let base = if node.kind == "fold.motion.transform" {
        Some(node)
    } else {
        downstream
    };
    if let Some(base) = base
        && let Some(point) = vector(base, "translation")
    {
        result.push(Handle {
            node: base.id,
            target: Target::Vector("translation".into()),
            point,
            matrix: IDENTITY,
        });
    }
    match node.kind.as_str() {
        "fold.motion.rectangle" | "fold.motion.ellipse" => {
            if let (Some(w), Some(h)) = (scalar(node, "width"), scalar(node, "height")) {
                result.push(Handle {
                    node: node.id,
                    target: Target::Size,
                    point: [w, h],
                    matrix: outer,
                });
            }
        }
        "fold.motion.radial_falloff" => {
            if let Some(center) = vector(node, "center") {
                result.push(Handle {
                    node: node.id,
                    target: Target::Vector("center".into()),
                    point: center,
                    matrix: outer,
                });
                if let Some(radius) = scalar(node, "outer") {
                    result.push(Handle {
                        node: node.id,
                        target: Target::Radius,
                        point: [center[0] + radius, center[1]],
                        matrix: outer,
                    });
                }
            }
        }
        "fold.motion.path" => {
            if let Ok(path) = node.settings::<crate::nodes::content::path::Settings>() {
                for (i, segment) in path.segments.iter().enumerate() {
                    let points = match segment {
                        fold_render::vector::Segment::Move(p)
                        | fold_render::vector::Segment::Line(p) => vec![*p],
                        fold_render::vector::Segment::Quad(a, b) => vec![*a, *b],
                        fold_render::vector::Segment::Cubic(a, b, c) => vec![*a, *b, *c],
                        fold_render::vector::Segment::Close => vec![],
                    };
                    for (j, point) in points.into_iter().enumerate() {
                        result.push(Handle {
                            node: node.id,
                            target: Target::Point(i, j),
                            point,
                            matrix: outer,
                        });
                    }
                }
            }
        }
        _ => {}
    }
    result
}
impl Overlay {
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.tools.cancel();
    }
    pub fn draw(&mut self, state: &Shared, context: ExtensionUi<'_>, rect: ViewerRect) {
        let ExtensionUi { ui, host } = context;
        if self
            .gesture
            .as_ref()
            .is_some_and(|g| g.viewer != host.panel_instance())
        {
            return;
        }
        let mut state = state.borrow_mut();
        state.sync(host);
        if !rect.editable {
            self.cancel();
            if state.editing {
                state.cancel(host);
            }
        }
        if std::mem::take(&mut state.point_edit_requested) {
            self.tools.edit_points = true;
            self.tools.tool = super::tools::Tool::Select;
        }
        if state.motion.is_some() && self.tools.draw(ui, &mut state, host, rect) {
            return;
        }
        let time = host
            .state()
            .navigation
            .last()
            .map(|v| v.time)
            .unwrap_or(fold_foundation::Time::ZERO);
        if self.gesture.as_ref().is_some_and(|g| g.time != time) {
            self.gesture = None;
            state.cancel(host);
        }
        if self
            .gesture
            .as_ref()
            .is_some_and(|g| g.generation != state.generation)
        {
            self.gesture = None;
        }
        if !rect.image_current && self.gesture.is_none() {
            return;
        }
        if state.group.is_some() {
            return;
        }
        let Some(motion) = state.motion.clone() else {
            return;
        };
        let selected_object = state
            .selected
            .first()
            .and_then(|id| motion.scene.as_ref()?.objects.iter().find(|o| o.owns(*id)))
            .map(|o| o.id);
        let pick_key = super::picking::Key {
            revision: state.visual_revision,
            time,
            scope: state.scene_scope,
            selected: selected_object,
        };
        self.picking.update(&motion, pick_key);
        let Some(&selected) = state.selected.first().filter(|_| state.selected.len() == 1) else {
            self.pick(ui, &mut state, host, rect, &motion, time);
            return;
        };
        if motion.scene.as_ref().is_some_and(|s| {
            s.objects
                .iter()
                .any(|o| o.owns(selected) && (o.locked || !o.constraints.is_empty()))
        }) {
            self.pick(ui, &mut state, host, rect, &motion, time);
            return;
        }
        let scale = [
            rect.size[0] as f64 / motion.info.width as f64,
            rect.size[1] as f64 / motion.info.height as f64,
        ];
        let saved = ui.cursor_screen_pos();
        let mut controls = handles(&motion, selected);
        if !self.tools.edit_points {
            controls.retain(|h| !matches!(h.target, Target::Point(_, _)));
        }
        if let Some(handle) = controls.first_mut() {
            match self.tools.tool {
                super::tools::Tool::Rotate => {
                    handle.target = Target::Rotation;
                }
                super::tools::Tool::Scale => {
                    handle.target = Target::Scale;
                }
                _ => {}
            }
            if matches!(handle.target, Target::Rotation | Target::Scale)
                && let Ok(node) = motion.graph.node(handle.node)
                && let Some(pivot) = vector(node, "pivot")
            {
                handle.point[0] += pivot[0];
                handle.point[1] += pivot[1];
            }
        }
        if let Some(object) = motion
            .scene
            .as_ref()
            .and_then(|s| s.objects.iter().find(|o| o.owns(selected)))
        {
            let outer = self
                .picking
                .result
                .as_ref()
                .filter(|r| r.key.time == time)
                .and_then(|r| r.outlines.iter().find(|o| o.object == object.id))
                .map(|o| o.parent);
            let Some(outer) = outer else {
                self.pick(ui, &mut state, host, rect, &motion, time);
                return;
            };
            for handle in &mut controls {
                handle.matrix = crate::geometry::multiply(outer, handle.matrix);
            }
        }
        if let Some(handle) = controls
            .iter()
            .find(|h| matches!(h.target, Target::Point(..)))
            && let Ok(node) = motion.graph.node(handle.node)
            && let Ok(path) = node.settings::<crate::nodes::content::path::Settings>()
        {
            let screen = |p| {
                let p = project(handle.matrix, p);
                [
                    rect.origin[0] + (p[0] * scale[0]) as f32,
                    rect.origin[1] + (p[1] * scale[1]) as f32,
                ]
            };
            let draw = ui.get_window_draw_list();
            let inset = super::tools::insets(ui);
            draw.with_clip_rect(
                [
                    rect.canvas_origin[0] + inset[0],
                    rect.canvas_origin[1] + inset[1],
                ],
                [
                    rect.canvas_origin[0] + rect.canvas_size[0],
                    rect.canvas_origin[1] + rect.canvas_size[1],
                ],
                || {
                    let mut current = [0.; 2];
                    for segment in &path.segments {
                        use fold_render::vector::Segment::*;
                        match *segment {
                            Move(p) | Line(p) => current = p,
                            Cubic(a, b, c) => {
                                draw.add_line(screen(current), screen(a), GRAPH_COLORS.selected)
                                    .build();
                                draw.add_line(screen(b), screen(c), GRAPH_COLORS.selected)
                                    .build();
                                current = c;
                            }
                            Quad(a, b) => {
                                draw.add_line(screen(current), screen(a), GRAPH_COLORS.selected)
                                    .build();
                                draw.add_line(screen(a), screen(b), GRAPH_COLORS.selected)
                                    .build();
                                current = b;
                            }
                            Close => {}
                        }
                    }
                },
            );
        }
        for (i, handle) in controls.iter().enumerate() {
            let document = project(handle.matrix, handle.point);
            let center = [
                rect.origin[0] + (document[0] * scale[0]) as f32,
                rect.origin[1] + (document[1] * scale[1]) as f32,
            ];
            let p = if matches!(handle.target, Target::Rotation | Target::Scale) {
                [center[0] + 50., center[1]]
            } else {
                center
            };
            if matches!(handle.target, Target::Rotation) {
                ui.get_window_draw_list()
                    .add_circle(center, 50., GRAPH_COLORS.selected)
                    .build();
            } else if matches!(handle.target, Target::Scale) {
                ui.get_window_draw_list()
                    .add_line(center, p, GRAPH_COLORS.selected)
                    .build();
            }
            if !p.iter().all(|v| v.is_finite())
                || (0..2).any(|axis| {
                    p[axis] < rect.canvas_origin[axis] + super::tools::insets(ui)[axis]
                        || p[axis] > rect.canvas_origin[axis] + rect.canvas_size[axis]
                })
            {
                continue;
            }
            let label = format!("motion-base-handle-{:?}-{i}", handle.node);
            let _id = ui.push_id(&label);
            ui.set_cursor_screen_pos([p[0] - 7., p[1] - 7.]);
            ui.invisible_button("handle", [14., 14.]);
            ui.get_window_draw_list()
                .add_circle(p, 6., GRAPH_COLORS.selected)
                .filled(true)
                .build();
            if ui.is_item_hovered() {
                ui.tooltip_text(match handle.target {
                    Target::Rotation => "Rotate around pivot · Shift snaps 15° · Escape cancels",
                    Target::Scale => "Scale around pivot · Escape cancels",
                    Target::Point(_, _) => {
                        "Edit Bézier point · Alt breaks tangent coupling · Escape cancels"
                    }
                    _ => "Drag to edit · Escape cancels",
                });
            }
            if ui.is_item_activated() {
                self.gesture = Some(Gesture {
                    handle: handle.clone(),
                    start: ui.io().mouse_pos(),
                    base: motion.clone(),
                    generation: state.generation,
                    viewer: host.panel_instance(),
                    time,
                });
            }
        }
        ui.set_cursor_screen_pos(saved);
        // The shell appends overlays after the preview image. Restoring that
        // trailing cursor can extend the parent bounds (notably on resize), so
        // ImGui requires a submitted item to finish the layout operation.
        ui.dummy([0., 0.]);
        self.pick(ui, &mut state, host, rect, &motion, time);
        if ui.is_key_pressed(imgui::Key::Escape) && self.gesture.is_some() {
            self.gesture = None;
            state.cancel(host);
            return;
        }
        if let Some(gesture) = &self.gesture {
            if ui.is_mouse_down(MouseButton::Left) {
                let mouse = ui.io().mouse_pos();
                let delta = [
                    (mouse[0] - gesture.start[0]) as f64 / scale[0],
                    (mouse[1] - gesture.start[1]) as f64 / scale[1],
                ];
                let m = gesture.handle.matrix;
                let determinant = m[0] * m[3] - m[1] * m[2];
                if determinant.abs() < 1e-12 {
                    return;
                }
                let delta = [
                    (m[3] * delta[0] - m[2] * delta[1]) / determinant,
                    (-m[1] * delta[0] + m[0] * delta[1]) / determinant,
                ];
                let p = [
                    gesture.handle.point[0] + delta[0],
                    gesture.handle.point[1] + delta[1],
                ];
                let mut candidate = gesture.base.clone();
                if let Ok(node) = crate::authoring::node(&mut candidate, gesture.handle.node) {
                    match &gesture.handle.target {
                        Target::Rotation | Target::Scale => {
                            let origin = project(gesture.handle.matrix, gesture.handle.point);
                            let screen = [
                                rect.origin[0] + (origin[0] * scale[0]) as f32,
                                rect.origin[1] + (origin[1] * scale[1]) as f32,
                            ];
                            let local = |p: [f32; 2]| {
                                let x = (p[0] - screen[0]) as f64 / scale[0];
                                let y = (p[1] - screen[1]) as f64 / scale[1];
                                [
                                    (m[3] * x - m[2] * y) / determinant,
                                    (-m[1] * x + m[0] * y) / determinant,
                                ]
                            };
                            let from = local(gesture.start);
                            let to = local(mouse);
                            if matches!(gesture.handle.target, Target::Rotation) {
                                let angle = scalar(node, "rotation").unwrap_or(0.)
                                    + (to[1].atan2(to[0]) - from[1].atan2(from[0])).to_degrees();
                                node.set(
                                    "rotation",
                                    Datum::Scalar(if self.tools.snap || ui.io().key_shift() {
                                        (angle / 15.).round() * 15.
                                    } else {
                                        angle
                                    }),
                                );
                            } else {
                                let factor =
                                    (to[0].hypot(to[1]) / from[0].hypot(from[1]).max(1.)).max(0.01);
                                node.set(
                                    "scale",
                                    Datum::Vector(
                                        vector(node, "scale")
                                            .unwrap_or([1.; 2])
                                            .map(|v| v * factor),
                                    ),
                                );
                            }
                        }
                        Target::Vector(key) => node.set(
                            key,
                            Datum::Vector(if self.tools.snap {
                                p.map(|v| (v / self.tools.grid).round() * self.tools.grid)
                            } else {
                                p
                            }),
                        ),
                        Target::Radius => {
                            let center = vector(node, "center").unwrap_or([0.; 2]);
                            let inner = scalar(node, "inner").unwrap_or(0.);
                            node.set(
                                "outer",
                                Datum::Scalar(
                                    (p[0] - center[0]).hypot(p[1] - center[1]).max(inner + 0.01),
                                ),
                            );
                        }
                        Target::Size => {
                            node.set("width", Datum::Scalar(p[0].max(0.)));
                            node.set("height", Datum::Scalar(p[1].max(0.)));
                        }
                        Target::Point(i, j) => {
                            if let Ok(mut path) =
                                node.settings::<crate::nodes::content::path::Settings>()
                            {
                                crate::authoring::path::move_point(
                                    &mut path.segments,
                                    *i,
                                    *j,
                                    p,
                                    ui.io().key_alt(),
                                );
                                node.settings = serde_json::to_value(path).unwrap();
                            }
                        }
                    }
                }
                if state.motion.as_ref() != Some(&candidate) {
                    state.motion = Some(candidate);
                    state.preview(host);
                }
            } else if ui.is_mouse_released(MouseButton::Left) {
                state.commit(host);
                self.gesture = None;
            }
        }
    }
}

impl Overlay {
    fn pick(
        &mut self,
        ui: &imgui::Ui,
        state: &mut super::state::State,
        host: &mut dyn fold_platform::desktop::DesktopClient,
        rect: ViewerRect,
        m: &Motion,
        time: fold_foundation::Time,
    ) {
        let selected = state
            .selected
            .first()
            .and_then(|id| m.scene.as_ref()?.objects.iter().find(|o| o.owns(*id)))
            .map(|o| o.id);
        let key = super::picking::Key {
            revision: state.visual_revision,
            time,
            scope: state.scene_scope,
            selected,
        };
        let Some(result) = self.picking.current(key) else {
            return;
        };
        let scale = [
            rect.size[0] as f64 / m.info.width as f64,
            rect.size[1] as f64 / m.info.height as f64,
        ];
        let screen = |p: [f64; 2]| {
            [
                rect.origin[0] + (p[0] * scale[0]) as f32,
                rect.origin[1] + (p[1] * scale[1]) as f32,
            ]
        };
        for outline in &result.outlines {
            if Some(outline.object) == selected {
                let draw = ui.get_window_draw_list();
                let inset = super::tools::insets(ui);
                draw.with_clip_rect(
                    [
                        rect.canvas_origin[0] + inset[0],
                        rect.canvas_origin[1] + inset[1],
                    ],
                    [
                        rect.canvas_origin[0] + rect.canvas_size[0],
                        rect.canvas_origin[1] + rect.canvas_size[1],
                    ],
                    || {
                        for contour in &outline.contours {
                            for pair in contour.windows(2) {
                                draw.add_line(
                                    screen(pair[0]),
                                    screen(pair[1]),
                                    GRAPH_COLORS.selected,
                                )
                                .thickness(if outline.hidden { 2. } else { 1. })
                                .build();
                            }
                        }
                    },
                );
            }
        }
        if self.gesture.is_some()
            || ui.is_any_item_active()
            || !ui.is_window_hovered()
            || ui.is_key_down(imgui::Key::Space)
        {
            return;
        }
        let inset = super::tools::insets(ui);
        let start = [
            rect.canvas_origin[0] + inset[0],
            rect.canvas_origin[1] + inset[1],
        ];
        let end = [
            rect.canvas_origin[0] + rect.canvas_size[0],
            rect.canvas_origin[1] + rect.canvas_size[1],
        ];
        if !ui.is_mouse_hovering_rect(start, end) {
            return;
        }
        let mouse = ui.io().mouse_pos();
        let p = [
            (mouse[0] - rect.origin[0]) as f64 / scale[0],
            (mouse[1] - rect.origin[1]) as f64 / scale[1],
        ];
        let hit = result
            .outlines
            .iter()
            .find(|o| {
                state.reference_pick.as_ref().is_none_or(|(owner, port)| {
                    *owner != o.object
                        && (port != "path"
                            || m.graph
                                .node(o.object)
                                .is_ok_and(|n| n.kind == "fold.motion.path"))
                }) && o.hit(p, 4. / scale[0])
                    && !m
                        .scene
                        .as_ref()
                        .unwrap()
                        .objects
                        .iter()
                        .any(|obj| obj.id == o.object && obj.locked)
            })
            .map(|o| o.object);
        if let Some(id) = hit {
            let saved = ui.cursor_screen_pos();
            ui.set_cursor_screen_pos([mouse[0] - 2., mouse[1] - 2.]);
            ui.invisible_button("motion-object-hit", [4., 4.]);
            if ui.is_item_activated() {
                if let Some((instance, port)) = state.reference_pick.clone() {
                    state.change_scene(host, |m| {
                        crate::authoring::scene::assets::set_reference(
                            m,
                            instance,
                            &port,
                            id,
                            port == "source",
                        )?;
                        Ok(Some(instance))
                    });
                    if state.error.is_empty() {
                        state.reference_pick = None;
                    }
                    ui.set_cursor_screen_pos(saved);
                    ui.dummy([0., 0.]);
                    return;
                }
                state.cancel(host);
                super::tools::select(state, host, id);
                if !self.tools.edit_points
                    && let Some(object) = m
                        .scene
                        .as_ref()
                        .unwrap()
                        .objects
                        .iter()
                        .find(|o| o.id == id && o.constraints.is_empty())
                    && let Some(transform) = object.transform.and_then(|id| m.graph.node(id).ok())
                    && let Some(point) = vector(transform, "translation")
                {
                    let target = match self.tools.tool {
                        super::tools::Tool::Rotate => Target::Rotation,
                        super::tools::Tool::Scale => Target::Scale,
                        _ => Target::Vector("translation".into()),
                    };
                    let point = if matches!(target, Target::Rotation | Target::Scale) {
                        let pivot = vector(transform, "pivot").unwrap_or([0.; 2]);
                        [point[0] + pivot[0], point[1] + pivot[1]]
                    } else {
                        point
                    };
                    let matrix = result
                        .outlines
                        .iter()
                        .find(|o| o.object == id)
                        .unwrap()
                        .parent;
                    self.gesture = Some(Gesture {
                        handle: Handle {
                            node: transform.id,
                            target,
                            point,
                            matrix,
                        },
                        start: mouse,
                        base: m.clone(),
                        generation: state.generation,
                        viewer: host.panel_instance(),
                        time,
                    });
                }
            }
            if ui.is_mouse_double_clicked(MouseButton::Left) {
                self.gesture = None;
                if m.graph.node(id).is_ok_and(|n| n.kind == "fold.motion.text") {
                    self.tools.edit_text(id);
                }
                if m.graph
                    .node(id)
                    .is_ok_and(|n| n.kind == "fold.motion.scene_children")
                {
                    state.scene_scope = Some(id);
                } else if let Some(source) =
                    crate::authoring::scene::assets::reference(m, id, "source")
                {
                    state.source_history.push(id);
                    super::tools::select(state, host, source);
                    state.scene_scope = Some(source);
                }
            }
            ui.set_cursor_screen_pos(saved);
            ui.dummy([0., 0.]);
        }
    }
}
