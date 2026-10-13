//! ImPlot owns point hit testing, capture and canvas coordinate conversion.
//! Fold owns the draft and commits one document transaction on release.
use super::{overlay::View, state::State};
use fold_foundation::ObjectId;
use fold_motion_canvas::{
    editing,
    model::Content,
    tools::{self, Part},
};
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::{
    imgui::{self, MouseButton, StyleColor, Ui},
    implot::{
        self, AxisFlags, DragToolFlags, DragToolId, Plot, PlotColorElement, PlotCond, PlotContext,
        StyleVar, XAxis, YAxis,
    },
};

#[derive(Clone, Copy)]
struct Control {
    object: ObjectId,
    point: ObjectId,
    part: Part,
    position: [f64; 2],
}
#[derive(Default)]
pub(super) struct PathInput {
    plot: Option<PlotContext>,
    active: Option<(Control, u64)>,
    cancelled: bool,
}
impl PathInput {
    pub fn initialize(&mut self, context: &imgui::Context) {
        self.plot = Some(PlotContext::create(context));
    }
    pub fn cancel(&mut self) {
        self.active = None;
        self.cancelled = true;
    }
    pub fn draw(
        &mut self,
        ui: &Ui,
        host: &mut dyn DesktopClient,
        state: &mut State,
        view: View,
        origin: [f32; 2],
        size: [f32; 2],
        enabled: bool,
    ) -> Result<bool, String> {
        if self
            .active
            .is_some_and(|(_, generation)| generation != state.generation || !view.rect.editable)
        {
            if state.editing {
                state.cancel(host);
            }
            self.active = None;
            self.cancelled = true;
        }
        if !ui.is_mouse_down(MouseButton::Left) {
            self.cancelled = false;
        }
        let Some(context) = &self.plot else {
            return Ok(false);
        };
        let plot = context.get_plot_ui(ui);
        let _minimum = plot.push_style_var_vec2(StyleVar::PlotMinSize, [1.; 2]);
        let _padding = plot.push_style_var_vec2(StyleVar::PlotPadding, [0.; 2]);
        let _frame = plot.push_style_color(PlotColorElement::FrameBg, [0.; 4]);
        let _background = plot.push_style_color(PlotColorElement::PlotBg, [0.; 4]);
        let _border = plot.push_style_color(PlotColorElement::PlotBorder, [0.; 4]);
        let _coordinates = plot.push_style_color(PlotColorElement::InlayText, [0.; 4]);
        ui.set_cursor_screen_pos(origin);
        let Some(_token) = plot.begin_plot_with_size("##path-controls", size) else {
            return Ok(false);
        };
        let flags = AxisFlags::NO_LABEL
            | AxisFlags::NO_GRID_LINES
            | AxisFlags::NO_TICK_MARKS
            | AxisFlags::NO_TICK_LABELS
            | AxisFlags::NO_MENUS
            | AxisFlags::LOCK_MIN
            | AxisFlags::LOCK_MAX;
        plot.setup_x_axis(XAxis::X1, None, flags);
        plot.setup_y_axis(YAxis::Y1, None, flags | AxisFlags::INVERT);
        let low = view.image(origin);
        let high = view.image([origin[0] + size[0], origin[1] + size[1]]);
        plot.setup_axes_limits(low[0], high[0], low[1], high[1], PlotCond::Always);
        plot.setup_finish();
        let canvas = state.canvas().ok_or("No Motion document")?;
        let transforms = tools::transforms(canvas, state.time)?;
        let mut controls = Vec::new();
        for (id, object) in &canvas.objects {
            if editing::editable(canvas, *id).is_err() {
                continue;
            }
            let Content::Path { contours, .. } = &object.content else {
                continue;
            };
            for point in contours.iter().flat_map(|c| &c.points) {
                let anchor = point.position_at(state.time)?;
                for (part, offset) in [
                    (Part::Anchor, [0.; 2]),
                    (Part::Incoming, point.incoming_at(state.time)?),
                    (Part::Outgoing, point.outgoing_at(state.time)?),
                ] {
                    if part != Part::Anchor
                        && (state.point != Some((*id, point.id)) || offset == [0.; 2])
                    {
                        continue;
                    }
                    controls.push(Control {
                        object: *id,
                        point: point.id,
                        part,
                        position: transforms[id]
                            .point([anchor[0] + offset[0], anchor[1] + offset[1]]),
                    });
                }
            }
        }
        let color = ui.style_color(StyleColor::PlotLinesHovered);
        let input = !self.cancelled
            && enabled
            && view.rect.editable
            && !state.busy()
            && !host.state().playing;
        let mut owned = self.active.is_some();
        for control in controls {
            // UUIDs and part provide stable ImGui identities when selection exposes handles.
            let _object = ui.push_id(&control.object.to_string());
            let _point = ui.push_id(&control.point.to_string());
            let [mut x, mut y] = control.position;
            let result = plot.drag_point(
                DragToolId::new(control.part as i32),
                &mut x,
                &mut y,
                color,
                if control.part == Part::Anchor { 4. } else { 5. },
                DragToolFlags::NO_FIT
                    | if input {
                        DragToolFlags::NONE
                    } else {
                        DragToolFlags::NO_INPUTS
                    },
            );
            owned |= result.clicked || result.held;
            if result.held && input && self.active.is_none() {
                if !state.selected.contains(&control.object) {
                    state.select(host, vec![control.object]);
                }
                state.point = Some((control.object, control.point));
                self.active = Some((control, state.generation));
            }
            if result.changed && input && self.active.is_some() {
                let edits = point_edit(state, host, control, [x, y], ui.io().key_alt())?;
                state.preview(host, edits)?;
            }
        }
        if self.active.is_some() && ui.is_mouse_released(MouseButton::Left) {
            state.commit(host)?;
            self.active = None;
        }
        if let Some(canvas) = state.canvas() {
            draw_guides(&plot, canvas, state, color)?;
        }
        Ok(owned)
    }
}

fn point_edit(
    state: &State,
    host: &dyn DesktopClient,
    c: Control,
    position: [f64; 2],
    free: bool,
) -> Result<Vec<editing::Edit>, String> {
    let before = state.before.as_ref().ok_or("No Motion document")?;
    let edit = tools::move_point(
        before,
        c.object,
        c.point,
        c.part,
        position,
        state.time,
        false,
        host.state().auto_key,
    )?;
    let mut edits = vec![edit];
    // Smooth handles rotate together, retaining the other length.
    // Corner handles stay independent. Alt temporarily breaks linkage.
    if c.part != Part::Anchor && !free {
        let Content::Path { contours, .. } = &before.objects[&c.object].content else {
            unreachable!()
        };
        let point = contours
            .iter()
            .flat_map(|c| &c.points)
            .find(|p| p.id == c.point)
            .ok_or("Missing point")?;
        let a = point.incoming_at(state.time)?;
        let b = point.outgoing_at(state.time)?;
        let la = a[0].hypot(a[1]);
        let lb = b[0].hypot(b[1]);
        if la > 1e-9
            && lb > 1e-9
            && (a[0] * b[1] - a[1] * b[0]).abs() <= la * lb * 1e-6
            && a[0] * b[0] + a[1] * b[1] < 0.
        {
            let transform = tools::transforms(before, state.time)?[&c.object];
            let anchor = point.position_at(state.time)?;
            let local = transform.inverse()?.point(position);
            let offset = [local[0] - anchor[0], local[1] - anchor[1]];
            let length = offset[0].hypot(offset[1]);
            if length > 1e-9 {
                let other_length = if c.part == Part::Incoming { lb } else { la };
                let other = transform.point(std::array::from_fn(|i| {
                    anchor[i] - offset[i] * other_length / length
                }));
                let draft = editing::apply(before, &edits)?;
                edits.push(tools::move_point(
                    &draft,
                    c.object,
                    c.point,
                    if c.part == Part::Incoming {
                        Part::Outgoing
                    } else {
                        Part::Incoming
                    },
                    other,
                    state.time,
                    false,
                    host.state().auto_key,
                )?);
            }
        }
    }
    Ok(edits)
}
fn draw_guides(
    plot: &implot::PlotUi<'_>,
    canvas: &fold_motion_canvas::model::Canvas,
    state: &State,
    color: [f32; 4],
) -> Result<(), String> {
    let transforms = tools::transforms(canvas, state.time)?;
    for id in &state.selected {
        let Some(object) = canvas.objects.get(id) else {
            continue;
        };
        let Content::Path { contours, .. } = &object.content else {
            continue;
        };
        for contour in contours {
            for (index, point) in contour.points.iter().enumerate() {
                let anchor = point.position_at(state.time)?;
                let outgoing = point.outgoing_at(state.time)?;
                if let Some(next) = contour
                    .points
                    .get(index + 1)
                    .or_else(|| contour.closed.then(|| &contour.points[0]))
                {
                    let end = next.position_at(state.time)?;
                    let incoming = next.incoming_at(state.time)?;
                    let p = [
                        anchor,
                        [anchor[0] + outgoing[0], anchor[1] + outgoing[1]],
                        [end[0] + incoming[0], end[1] + incoming[1]],
                        end,
                    ];
                    let mut xs = Vec::with_capacity(65);
                    let mut ys = Vec::with_capacity(65);
                    for step in 0..=64 {
                        let t = step as f64 / 64.;
                        let u = 1. - t;
                        let p = transforms[id].point(std::array::from_fn(|axis| {
                            u * u * u * p[0][axis]
                                + 3. * u * u * t * p[1][axis]
                                + 3. * u * t * t * p[2][axis]
                                + t * t * t * p[3][axis]
                        }));
                        xs.push(p[0]);
                        ys.push(p[1]);
                    }
                    implot::LinePlot::new("##curve", &xs, &ys)
                        .with_line_color(color)
                        .with_line_weight(1.)
                        .plot(plot);
                }
                if state.point == Some((*id, point.id)) {
                    for offset in [point.incoming_at(state.time)?, outgoing] {
                        let a = transforms[id].point(anchor);
                        let b =
                            transforms[id].point([anchor[0] + offset[0], anchor[1] + offset[1]]);
                        implot::LinePlot::new("##handle", &[a[0], b[0]], &[a[1], b[1]])
                            .with_line_color(color)
                            .with_line_weight(1.)
                            .plot(plot);
                    }
                }
            }
        }
    }
    Ok(())
}
