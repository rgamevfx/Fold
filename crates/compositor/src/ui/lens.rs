//! Viewer center handle, sharing the compositor preview/commit/cancel lifecycle.
use crate::{Node, OperationConfig as P};
use fold_foundation::{ObjectId, Time};
use fold_ui::sdk::{
    ExtensionUi, ViewerRect,
    imgui::{Key, MouseButton, StyleColor},
};
#[derive(Default)]
pub(super) struct Editor {
    gesture: Option<Gesture>,
}
struct Gesture {
    id: ObjectId,
    node: Node,
    time: Time,
    generation: u64,
    start: [f32; 2],
    center: [f64; 2],
}
impl Editor {
    pub fn reset(&mut self) {
        self.gesture = None;
    }
    pub fn draw(
        &mut self,
        context: ExtensionUi<'_>,
        rect: ViewerRect,
        state: &mut super::state::State,
    ) {
        let ExtensionUi { ui, host } = context;
        let time = host
            .state()
            .navigation
            .last()
            .map_or(Time::ZERO, |v| v.time);
        let selected = state
            .selected
            .first()
            .copied()
            .filter(|_| state.selected.len() == 1);
        if self.gesture.as_ref().is_some_and(|g| {
            Some(g.id) != selected
                || g.time != time
                || g.generation != state.generation
                || host.state().playing
                || !rect.editable
                || ui.is_key_pressed(Key::Escape)
        }) {
            state.cancel(host);
            self.reset();
            return;
        }
        let Some(id) = selected else {
            return;
        };
        let Some(graph) = state.graph.as_ref() else {
            return;
        };
        let Ok(config) = graph
            .prepare()
            .and_then(|g| g.evaluated_parameters(id, time))
        else {
            return;
        };
        if rect.size.iter().any(|v| *v <= 0.) {
            return;
        }
        let center = match config {
            P::LensDistortion { settings } => [settings.center_x, settings.center_y],
            P::ChromaticAberration { settings } => [settings.center_x, settings.center_y],
            _ => return,
        };
        let p =
            std::array::from_fn::<_, 2, _>(|i| rect.origin[i] + center[i] as f32 * rect.size[i]);
        let color = ui.style_color(StyleColor::PlotLinesHovered);
        let draw = ui.get_window_draw_list();
        let end = [rect.origin[0] + rect.size[0], rect.origin[1] + rect.size[1]];
        draw.with_clip_rect(rect.origin, end, || {
            if state.lens_grid {
                let mut grid = ui.style_color(StyleColor::TextDisabled);
                grid[3] = 0.45;
                for i in 1..10 {
                    let t = i as f32 / 10.;
                    let x = rect.origin[0] + rect.size[0] * t;
                    let y = rect.origin[1] + rect.size[1] * t;
                    draw.add_line([x, rect.origin[1]], [x, end[1]], grid)
                        .build();
                    draw.add_line([rect.origin[0], y], [end[0], y], grid)
                        .build();
                }
            }
            draw.add_circle(p, 7., color).thickness(1.5).build();
            draw.add_line([p[0] - 11., p[1]], [p[0] + 11., p[1]], color)
                .build();
            draw.add_line([p[0], p[1] - 11.], [p[0], p[1] + 11.], color)
                .build();
        });
        if !rect.editable || host.state().playing || state.property_editing {
            return;
        }
        let mouse = ui.io().mouse_pos();
        let hovered = ui.is_mouse_hovering_rect(rect.origin, end)
            && (mouse[0] - p[0]).hypot(mouse[1] - p[1]) < 13.;
        if self.gesture.is_none() && hovered && !ui.is_key_down(Key::Space) {
            let node = graph.node(id).unwrap();
            let driven = node.animation.driven("center_x.0") || node.animation.driven("center_y.0");
            ui.tooltip_text(if driven {
                "Optical center is linked or expression-driven"
            } else {
                "Drag optical center • Escape cancels"
            });
            if !driven && ui.is_mouse_clicked(MouseButton::Left) {
                self.gesture = Some(Gesture {
                    id,
                    node: node.clone(),
                    time,
                    generation: state.generation,
                    start: mouse,
                    center,
                });
            }
        }
        let Some(g) = &self.gesture else {
            return;
        };
        let mut next = g.node.clone();
        let result = (|| -> Result<(), String> {
            for (i, property) in ["center_x", "center_y"].iter().enumerate() {
                let value = (g.center[i] + f64::from((mouse[i] - g.start[i]) / rect.size[i]))
                    .clamp(-2., 3.);
                let path = format!("{property}.0");
                let curve = next.animation.curve(&path)?;
                if curve.is_some() || host.state().auto_key {
                    let mut curve = curve.unwrap_or_default();
                    curve.insert(time, value);
                    next.animation.insert(path, curve)?;
                } else {
                    next.set_property(property, &[value])?;
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            state.cancel(host);
            state.error = e;
            self.reset();
            return;
        }
        if let Some(graph) = state.graph.as_mut() {
            if graph.node(id).ok() != Some(&next) {
                *graph.node_mut(id).unwrap() = next;
                state.preview(host);
            }
        }
        if !ui.is_mouse_down(MouseButton::Left) {
            if state.editing {
                state.commit(host);
            }
            self.reset();
        }
    }
}
