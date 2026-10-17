//! Gradient handles edit native paint coordinates, including its own transform.
use super::{overlay::View, state::State};
use fold_foundation::ObjectId;
use fold_motion_canvas::{
    editing::{self, Edit},
    model::{PaintSource, Spread, Stop},
    tools,
};
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::imgui::{MouseButton, StyleColor, Ui};
#[derive(Default)]
pub(super) struct GradientInput {
    active: Option<(ObjectId, bool, u64)>,
}
impl GradientInput {
    pub fn cancel(&mut self) {
        self.active = None;
    }
    pub fn draw(
        &mut self,
        ui: &Ui,
        host: &mut dyn DesktopClient,
        state: &mut State,
        view: View,
        click: bool,
    ) -> Result<(), String> {
        if self
            .active
            .is_some_and(|(_, _, generation)| generation != state.generation)
        {
            self.cancel();
        }
        let [id] = state.selected.as_slice() else {
            return Ok(());
        };
        let id = *id;
        let canvas = state.canvas().ok_or("No Motion document")?;
        editing::editable(canvas, id)?;
        let transforms = tools::transforms(canvas, state.time)?;
        let object = canvas.objects[&id].clone();
        let mouse = view.image(ui.io().mouse_pos());
        let world = transforms[&id];
        let mut paint = object
            .paint
            .fill
            .clone()
            .unwrap_or_else(|| [0., 0., 0., 1.].into());
        let mut endpoints = None;
        match &paint {
            PaintSource::Linear {
                start,
                end,
                transform,
                ..
            } => {
                let t = world.compose(tools::Transform(*transform));
                endpoints = Some([t.point(*start), t.point(*end)]);
            }
            PaintSource::Radial {
                center,
                radius,
                transform,
                ..
            } => {
                let t = world.compose(tools::Transform(*transform));
                endpoints = Some([t.point(*center), t.point([center[0] + radius, center[1]])]);
            }
            _ => {}
        }
        if let Some(points) = endpoints {
            let color = ui.style_color(StyleColor::PlotLinesHovered);
            let draw = ui.get_window_draw_list();
            draw.add_line(view.screen(points[0]), view.screen(points[1]), color)
                .thickness(2.)
                .build();
            for point in points {
                draw.add_circle(view.screen(point), 6., color)
                    .filled(true)
                    .build();
            }
        }
        if click && !state.busy() {
            if let Some(points) = endpoints {
                if let Some(index) = points.iter().position(|p| view.near(mouse, *p, 10.)) {
                    self.active = Some((id, index == 0, state.generation));
                }
            } else {
                let local = world.inverse()?.point(mouse);
                let color = match paint {
                    PaintSource::Solid { color, .. } => color,
                    _ => [0., 0., 0., 1.],
                };
                paint = PaintSource::Linear {
                    start: local,
                    end: [local[0] + 1., local[1]],
                    transform: [1., 0., 0., 1., 0., 0.],
                    spread: Spread::Pad,
                    stops: vec![
                        Stop {
                            id: ObjectId::new(),
                            offset: 0.,
                            color,
                            extensions: Default::default(),
                        },
                        Stop {
                            id: ObjectId::new(),
                            offset: 1.,
                            color: [1.; 4],
                            extensions: Default::default(),
                        },
                    ],
                    extensions: Default::default(),
                };
                self.active = Some((id, false, state.generation));
            }
        }
        if let Some((target, start, _)) = self.active {
            if id != target {
                self.cancel();
                return Ok(());
            }
            match &mut paint {
                PaintSource::Linear {
                    start: a,
                    end: b,
                    transform,
                    ..
                } => {
                    let p = world
                        .compose(tools::Transform(*transform))
                        .inverse()?
                        .point(mouse);
                    if start {
                        if p != *b {
                            *a = p;
                        }
                    } else if p != *a {
                        *b = p;
                    }
                }
                PaintSource::Radial {
                    center,
                    focus,
                    radius,
                    transform,
                    ..
                } => {
                    let p = world
                        .compose(tools::Transform(*transform))
                        .inverse()?
                        .point(mouse);
                    if start {
                        let offset = [p[0] - center[0], p[1] - center[1]];
                        *center = p;
                        *focus = [focus[0] + offset[0], focus[1] + offset[1]];
                    } else {
                        *radius = (p[0] - center[0]).hypot(p[1] - center[1]).max(0.01);
                    }
                }
                _ => return Ok(()),
            }
            let expected = state.before.as_ref().ok_or("No Motion document")?.objects[&id].clone();
            let mut object = expected.clone();
            object.paint.fill = Some(paint);
            state.preview(
                host,
                vec![Edit::Replace {
                    id,
                    expected: Box::new(expected),
                    object: Box::new(object),
                }],
            )?;
            if ui.is_mouse_released(MouseButton::Left) {
                state.commit(host)?;
                self.cancel();
            }
        }
        Ok(())
    }
}
