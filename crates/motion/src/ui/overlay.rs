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
#[derive(Clone)]
enum Target {
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
}
#[derive(Default)]
pub struct Overlay {
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
    let downstream = downstream(m, selected);
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
    pub fn draw(&mut self, state: &Shared, context: ExtensionUi<'_>, rect: ViewerRect) {
        let ExtensionUi { ui, host } = context;
        let mut state = state.borrow_mut();
        state.sync(host);
        if self
            .gesture
            .as_ref()
            .is_some_and(|g| g.generation != state.generation)
        {
            self.gesture = None;
        }
        if state.group.is_some() {
            return;
        }
        let Some(motion) = state.motion.clone() else {
            return;
        };
        let Some(&selected) = state.selected.first().filter(|_| state.selected.len() == 1) else {
            return;
        };
        let scale = [
            rect.size[0] as f64 / motion.info.width as f64,
            rect.size[1] as f64 / motion.info.height as f64,
        ];
        let saved = ui.cursor_screen_pos();
        let controls = handles(&motion, selected);
        for (i, handle) in controls.iter().enumerate() {
            let document = project(handle.matrix, handle.point);
            let p = [
                rect.origin[0] + (document[0] * scale[0]) as f32,
                rect.origin[1] + (document[1] * scale[1]) as f32,
            ];
            if !p.iter().all(|v| v.is_finite())
                || (0..2).any(|axis| {
                    p[axis] < rect.origin[axis] || p[axis] > rect.origin[axis] + rect.size[axis]
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
                ui.tooltip_text("Authored base — drag to edit; Escape cancels");
            }
            if ui.is_item_activated() {
                self.gesture = Some(Gesture {
                    handle: handle.clone(),
                    start: ui.io().mouse_pos(),
                    base: motion.clone(),
                    generation: state.generation,
                });
            }
        }
        ui.set_cursor_screen_pos(saved);
        // The shell appends overlays after the preview image. Restoring that
        // trailing cursor can extend the parent bounds (notably on resize), so
        // ImGui requires a submitted item to finish the layout operation.
        ui.dummy([0., 0.]);
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
                        Target::Vector(key) => node.set(key, Datum::Vector(p)),
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
                                if let Some(segment) = path.segments.get_mut(*i) {
                                    use fold_render::vector::Segment as S;
                                    let target = match segment {
                                        S::Move(a) | S::Line(a) => Some(a),
                                        S::Quad(a, b) => Some(if *j == 0 { a } else { b }),
                                        S::Cubic(a, b, c) => Some(match *j {
                                            0 => a,
                                            1 => b,
                                            _ => c,
                                        }),
                                        S::Close => None,
                                    };
                                    if let Some(target) = target {
                                        *target = p;
                                        node.settings = serde_json::to_value(path).unwrap();
                                    }
                                }
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
