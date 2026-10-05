use super::{Channel, Context, Editor, Response, model};
use crate::sdk::{
    EditorColors,
    imgui::{Key, MouseButton, Ui},
    time_view::{frame, time},
};
use fold_animation::{Handle, Interpolation};
use fold_foundation::{ObjectId, Time};
use std::collections::BTreeSet;

pub(super) enum Gesture {
    Divider,
    Track {
        id: ObjectId,
        start: [f32; 2],
    },
    Range {
        id: ObjectId,
        original: (Time, Time),
        start: f32,
        edge: i8,
        channels: Vec<Channel>,
        keys: BTreeSet<ObjectId>,
    },
    Keys {
        original: Vec<Channel>,
        start: [f32; 2],
    },
    Handle {
        original: Vec<Channel>,
        channel: usize,
        key: usize,
        incoming: bool,
    },
    Box {
        start: [f32; 2],
        prior: BTreeSet<ObjectId>,
    },
    Pan {
        start: [f32; 2],
        first: f64,
        scroll: f32,
        values: [f64; 2],
    },
    Seek,
}
enum Row {
    Node(ObjectId, String, usize, Vec<usize>),
    Property(ObjectId, String, Vec<usize>),
    Channel(usize),
}
impl Row {
    fn channels(&self) -> Vec<usize> {
        match self {
            Self::Node(_, _, _, v) | Self::Property(_, _, v) => v.clone(),
            Self::Channel(i) => vec![*i],
        }
    }
}
fn inside(p: [f32; 2], min: [f32; 2], max: [f32; 2]) -> bool {
    p[0] >= min[0] && p[0] < max[0] && p[1] >= min[1] && p[1] < max[1]
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum DropTarget {
    Into(Option<ObjectId>),
    Beside(ObjectId, bool),
}

fn track_drop(
    tracks: &[super::Track],
    source: ObjectId,
    target: Option<(ObjectId, f32)>,
) -> Option<DropTarget> {
    let source = tracks.iter().find(|t| t.id == source && !t.locked)?;
    let Some((id, fraction)) = target else {
        return (source.layer.is_some() && source.parent.is_some())
            .then_some(DropTarget::Into(None));
    };
    if source.id == id {
        return None;
    }
    let target = tracks.iter().find(|t| t.id == id)?;
    let into = (0.25..=0.75).contains(&fraction)
        && target.layer.as_ref().is_some_and(|l| l.accepts_children);
    if source.layer.is_none() {
        return (!into
            && target.layer.is_none()
            && source.parent == target.parent
            && !target.locked)
            .then_some(DropTarget::Beside(id, fraction > 0.5));
    }
    target.layer.as_ref()?;
    let parent = if into { Some(id) } else { target.parent };
    let mut ancestor = parent;
    for _ in 0..=tracks.len() {
        let Some(id) = ancestor else {
            return Some(if into {
                DropTarget::Into(parent)
            } else {
                DropTarget::Beside(target.id, fraction > 0.5)
            });
        };
        if id == source.id {
            return None;
        }
        let track = tracks.iter().find(|t| t.id == id)?;
        if track.locked {
            return None;
        }
        ancestor = track.parent;
    }
    None
}

impl Editor {
    fn rows(&self, channels: &[Channel], context: &Context<'_>) -> Vec<Row> {
        let mut rows = vec![];
        if !context.tracks.is_empty() {
            for track in context.tracks {
                let mut parent = track.parent;
                let mut depth = 0;
                let mut hidden = false;
                while let Some(id) = parent {
                    depth += 1;
                    if depth > context.tracks.len() {
                        hidden = true;
                        break;
                    }
                    hidden |= self.collapsed_nodes.contains(&id);
                    parent = context
                        .tracks
                        .iter()
                        .find(|t| t.id == id)
                        .and_then(|t| t.parent);
                }
                if hidden {
                    continue;
                }
                let descendants = |object| {
                    let mut id = Some(object);
                    for _ in 0..=context.tracks.len() {
                        if id == Some(track.id) {
                            return true;
                        }
                        id = id
                            .and_then(|id| context.tracks.iter().find(|t| t.id == id))
                            .and_then(|t| t.parent);
                        if id.is_none() {
                            break;
                        }
                    }
                    false
                };
                let indices: Vec<_> = channels
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| descendants(c.object))
                    .map(|(i, _)| i)
                    .collect();
                rows.push(Row::Node(track.id, track.label.clone(), depth, indices));
                if !self.collapsed_nodes.contains(&track.id) {
                    let own: Vec<_> = channels
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.object == track.id && self.matches(c, context))
                        .map(|(i, _)| i)
                        .collect();
                    self.property_rows(&mut rows, channels, track.id, &own);
                }
            }
            return rows;
        }
        let mut nodes = BTreeSet::new();
        for c in channels.iter().filter(|c| self.matches(c, context)) {
            if !nodes.insert(c.object) {
                continue;
            }
            let node_channels: Vec<_> = channels
                .iter()
                .enumerate()
                .filter(|(_, v)| v.object == c.object && self.matches(v, context))
                .map(|(i, _)| i)
                .collect();
            rows.push(Row::Node(
                c.object,
                c.node_label.clone(),
                0,
                node_channels.clone(),
            ));
            if self.collapsed_nodes.contains(&c.object) {
                continue;
            }
            self.property_rows(&mut rows, channels, c.object, &node_channels);
        }
        rows
    }
    fn property_rows(
        &self,
        rows: &mut Vec<Row>,
        channels: &[Channel],
        object: ObjectId,
        node_channels: &[usize],
    ) {
        let mut properties = BTreeSet::new();
        for &i in node_channels {
            let c = &channels[i];
            if !properties.insert(c.property.clone()) {
                continue;
            }
            let items: Vec<_> = node_channels
                .iter()
                .copied()
                .filter(|&j| channels[j].property == c.property)
                .collect();
            if items.len() == 1 && channels[items[0]].component == "Value" {
                rows.push(Row::Channel(items[0]));
                continue;
            }
            rows.push(Row::Property(object, c.property.clone(), items.clone()));
            if !self
                .collapsed_properties
                .contains(&(c.object, c.property.clone()))
            {
                rows.extend(items.into_iter().map(Row::Channel));
            }
        }
    }
    pub(super) fn canvas(
        &mut self,
        ui: &Ui,
        channels: &mut [Channel],
        context: &Context<'_>,
        response: &mut Response,
    ) {
        let origin = ui.cursor_screen_pos();
        let size = [
            ui.content_region_avail()[0].max(1.),
            (ui.content_region_avail()[1]
                - if self.error.is_empty() {
                    0.
                } else {
                    ui.text_line_height() * 2.
                })
            .max(40.),
        ];
        let row_height = ui.text_line_height() + 6.;
        let header = if self.hide_timing {
            size[0]
        } else {
            self.header_pixels
                .unwrap_or_else(|| header_width(size[0]))
                .clamp(40., (size[0] - 40.).max(40.))
        };
        let body = [origin[0] + header, origin[1] + row_height];
        let end = [origin[0] + size[0], origin[1] + size[1]];
        let value_height = (end[1] - body[1] - 16.).max(1.);
        let value_range = self.value_range;
        let y_value = |value: f64| {
            end[1]
                - 8.
                - ((value - value_range[0]) / (value_range[1] - value_range[0])) as f32
                    * value_height
        };
        ui.invisible_button_flags(
            "animation-canvas",
            size,
            crate::sdk::imgui::ButtonFlags::ENABLE_NAV,
        );
        let hovered = ui.is_item_hovered();
        let focused = ui.is_window_focused();
        let mouse = ui.io().mouse_pos();
        let colors = EditorColors::from_ui(ui);
        let draw = ui.get_window_draw_list();
        let rows = self.rows(channels, context);
        let mut keys: Vec<(ObjectId, usize, usize, [f32; 2])> = vec![];
        let mut handles: Vec<(usize, usize, bool, [f32; 2])> = vec![];
        let mut row_hit = None;
        let controls_width = if context.tracks.iter().any(|t| t.layer.is_some()) {
            54.
        } else {
            0.
        };
        let mut layer_hit = None;
        let mut range_hit = None;
        let mut summary_hit = None;
        draw.with_clip_rect(origin, end, || {
            draw.add_rect(origin, end, colors.background)
                .filled(true)
                .build();
            draw.add_rect(origin, [body[0], end[1]], colors.header)
                .filled(true)
                .build();
            draw.add_line([body[0], origin[1]], [body[0], end[1]], colors.grid)
                .build();
            for (r, row) in rows.iter().enumerate() {
                let top = body[1] + r as f32 * row_height - self.view.scroll_y;
                if top + row_height < body[1] || top > end[1] {
                    continue;
                }
                let list = row.channels();
                let owner = match row {
                    Row::Node(id, ..) | Row::Property(id, ..) => *id,
                    Row::Channel(i) => channels[*i].object,
                };
                let mut depth = 0;
                let mut parent = context
                    .tracks
                    .iter()
                    .find(|t| t.id == owner)
                    .and_then(|t| t.parent);
                while let Some(id) = parent {
                    depth += 1;
                    if depth > context.tracks.len() {
                        break;
                    }
                    parent = context
                        .tracks
                        .iter()
                        .find(|t| t.id == id)
                        .and_then(|t| t.parent);
                }
                let (indent, label, collapsed) = match row {
                    Row::Node(id, label, depth, _) => (
                        *depth as f32 * 12.,
                        label.as_str(),
                        Some(self.collapsed_nodes.contains(id)),
                    ),
                    Row::Property(id, p, _) => (
                        12. + depth as f32 * 12.,
                        channels[list[0]].property_label.as_str(),
                        Some(self.collapsed_properties.contains(&(*id, p.clone()))),
                    ),
                    Row::Channel(i) => {
                        let c = &channels[*i];
                        (
                            28. + depth as f32 * 12.,
                            if c.component == "Value" {
                                c.property_label.as_str()
                            } else {
                                c.component.as_str()
                            },
                            None,
                        )
                    }
                };
                let selected = list.iter().any(|&i| {
                    self.visible_channels
                        .contains(&(channels[i].object, channels[i].path.clone()))
                });
                draw.with_clip_rect([origin[0], body[1]], [body[0] - 2., end[1]], || {
                    if (selected && matches!(row, Row::Channel(_)))
                        || (matches!(row, Row::Node(..)) && context.nodes.contains(&owner))
                    {
                        draw.add_rect([origin[0], top], [body[0], top + row_height], colors.lane)
                            .filled(true)
                            .build();
                    }
                    if let Row::Node(id, ..) = row
                        && let Some(track) = context.tracks.iter().find(|t| t.id == *id)
                        && let Some(layer) = &track.layer
                    {
                        let size = (row_height - 2.).min(17.);
                        for (index, icon, tip) in [
                            (
                                0,
                                crate::sdk::toolbar::ToolbarIcon::View,
                                if layer.visible {
                                    "Hide layer"
                                } else {
                                    "Show layer"
                                },
                            ),
                            (
                                1,
                                if track.locked {
                                    crate::sdk::toolbar::ToolbarIcon::Lock
                                } else {
                                    crate::sdk::toolbar::ToolbarIcon::Unlock
                                },
                                if track.locked {
                                    "Unlock layer"
                                } else {
                                    "Lock layer"
                                },
                            ),
                        ] {
                            let p = [origin[0] + index as f32 * 18., top + 1.];
                            crate::sdk::toolbar::draw_icon_on(
                                ui,
                                &draw,
                                icon,
                                p,
                                size,
                                colors.text,
                            );
                            if index == 0 && !layer.visible {
                                draw.add_line(
                                    [p[0] + 3., p[1] + size - 2.],
                                    [p[0] + size, p[1] + 2.],
                                    colors.muted,
                                )
                                .build();
                            }
                            if inside(mouse, p, [p[0] + 18., top + row_height])
                                && inside(mouse, [origin[0], body[1]], [body[0], end[1]])
                            {
                                ui.tooltip_text(tip);
                                layer_hit = Some((
                                    *id,
                                    index,
                                    if index == 0 {
                                        !layer.visible
                                    } else {
                                        !track.locked
                                    },
                                ));
                            }
                        }
                        draw.add_text([origin[0] + 39., top + 2.], colors.muted, layer.icon);
                    }
                    if let Some(collapsed) = collapsed {
                        draw.add_text(
                            [origin[0] + controls_width + 4. + indent, top + 2.],
                            colors.muted,
                            if collapsed { ">" } else { "v" },
                        );
                    }
                    draw.add_text(
                        [origin[0] + controls_width + 16. + indent, top + 2.],
                        colors.text,
                        label,
                    );
                });
                if inside(
                    mouse,
                    [origin[0], top.max(body[1])],
                    [body[0], top + row_height],
                ) {
                    row_hit = Some(r);
                }
                if !self.curves {
                    draw.with_clip_rect(body, end, || {
                        draw.add_line(
                            [body[0], top + row_height],
                            [end[0], top + row_height],
                            colors.grid,
                        )
                        .build();
                        if let Row::Node(id, _, _, _) = row
                            && let Some(track) = context.tracks.iter().find(|t| t.id == *id)
                            && let Some((start, finish)) = track.range
                        {
                            let left = self.view.x(frame(start, context.rate), body[0]);
                            let right = self.view.x(frame(finish, context.rate), body[0]);
                            let min = [left, top + 2.];
                            let max = [right.max(left + 3.), top + row_height - 2.];
                            let hovered = inside(mouse, min, max) && inside(mouse, body, end);
                            let selected = context.nodes.contains(id);
                            let edge_width = (row_height * 0.3).min((right - left).max(0.) * 0.25);
                            draw.add_rect(
                                min,
                                max,
                                if track.locked {
                                    colors.muted
                                } else if selected {
                                    colors.selected
                                } else {
                                    colors.video
                                },
                            )
                            .rounding(2.)
                            .filled(true)
                            .build();
                            if selected || hovered {
                                draw.add_rect(min, max, colors.text).rounding(2.).build();
                                for x in [left + 3., right - 3.] {
                                    if right - left > 12. {
                                        draw.add_line(
                                            [x, min[1] + 3.],
                                            [x, max[1] - 3.],
                                            colors.text,
                                        )
                                        .build();
                                    }
                                }
                            }
                            if !track.locked && hovered {
                                range_hit = Some((
                                    *id,
                                    (start, finish),
                                    if (mouse[0] - left).abs() < edge_width {
                                        -1
                                    } else if (mouse[0] - right).abs() < edge_width {
                                        1
                                    } else {
                                        0
                                    },
                                ));
                            }
                        }
                        let has_range = matches!(row, Row::Node(id, ..)
                            if context.tracks.iter().any(|t| t.id == *id && t.range.is_some()));
                        if !matches!(row, Row::Channel(_)) && !has_range {
                            let times: Vec<_> = list
                                .iter()
                                .flat_map(|&i| channels[i].curve.keys.iter().map(|k| k.time))
                                .collect();
                            if let (Some(first), Some(last)) =
                                (times.iter().min(), times.iter().max())
                            {
                                let left = self.view.x(frame(*first, context.rate), body[0]);
                                let right = self.view.x(frame(*last, context.rate), body[0]);
                                let y = top + row_height * 0.5;
                                let half = row_height * 0.28;
                                let min = [left - 4., y - half];
                                let max = [right + 4., y + half];
                                let selected = list.iter().any(|&i| {
                                    channels[i]
                                        .curve
                                        .keys
                                        .iter()
                                        .any(|k| self.selected.contains(&k.id))
                                });
                                let outline = if selected {
                                    colors.selected
                                } else {
                                    colors.muted
                                };
                                let mut fill = outline;
                                fill[3] = if selected { 0.55 } else { 0.22 };
                                draw.add_rect(min, max, fill)
                                    .rounding(2.)
                                    .filled(true)
                                    .build();
                                draw.add_rect(min, max, outline).rounding(2.).build();
                                if inside(mouse, min, max) && inside(mouse, body, end) {
                                    summary_hit = Some(r);
                                }
                            }
                        }
                        for i in list {
                            for (j, key) in channels[i].curve.keys.iter().enumerate() {
                                let p = [
                                    self.view.x(frame(key.time, context.rate), body[0]),
                                    top + row_height * 0.5,
                                ];
                                if inside(p, body, end) {
                                    diamond(
                                        &draw,
                                        p,
                                        if matches!(row, Row::Channel(_)) {
                                            4.
                                        } else {
                                            3.
                                        },
                                        if self.selected.contains(&key.id) {
                                            colors.playhead
                                        } else {
                                            colors.text
                                        },
                                    );
                                    keys.push((key.id, i, j, p));
                                }
                            }
                        }
                    });
                }
            }
            draw.with_clip_rect(body, end, || {
                if self.curves {
                    for step in 0..=5 {
                        let value = value_range[0]
                            + (value_range[1] - value_range[0]) * f64::from(step) / 5.;
                        let y = y_value(value);
                        draw.add_line([body[0], y], [end[0], y], colors.grid)
                            .build();
                        draw.add_text([body[0] + 4., y + 2.], colors.muted, format!("{value:.2}"));
                    }
                    for (i, c) in channels
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| self.matches(c, context) && c.curve.validate().is_ok())
                    {
                        let selected = self.visible_channels.contains(&(c.object, c.path.clone()));
                        if !selected && !self.visible_channels.is_empty() {
                            continue;
                        }
                        let palette = crate::sdk::GRAPH_COLORS;
                        let color =
                            [palette.image, palette.mask, palette.source, palette.merge][i % 4];
                        let mut previous = None;
                        let samples = ((end[0] - body[0]) / 3.).ceil().clamp(2., 1024.) as usize;
                        for s in 0..=samples {
                            let x = body[0] + (end[0] - body[0]) * s as f32 / samples as f32;
                            let frame = self.view.frame_at(x, body[0]);
                            if let Ok(time) = subframe_time(frame, context.rate)
                                && let Some(value) = c.curve.sample(time)
                            {
                                let p = [x, y_value(value)];
                                if let Some(prev) = previous {
                                    draw.add_line(prev, p, color).thickness(1.6).build();
                                }
                                previous = Some(p);
                            }
                        }
                        for (j, key) in c.curve.keys.iter().enumerate() {
                            let p = [
                                self.view.x(frame(key.time, context.rate), body[0]),
                                y_value(key.value),
                            ];
                            if inside(p, body, end) {
                                diamond(
                                    &draw,
                                    p,
                                    4.,
                                    if self.selected.contains(&key.id) {
                                        colors.playhead
                                    } else {
                                        colors.text
                                    },
                                );
                                keys.push((key.id, i, j, p));
                            }
                            if self.selected.contains(&key.id) {
                                for incoming in [true, false] {
                                    let adjacent = if incoming {
                                        j.checked_sub(1)
                                    } else {
                                        (j + 1 < c.curve.keys.len()).then_some(j + 1)
                                    };
                                    let Some(adjacent) = adjacent else {
                                        continue;
                                    };
                                    let segment = if incoming { adjacent } else { j };
                                    if c.curve.keys[segment].interpolation != Interpolation::Bezier
                                    {
                                        continue;
                                    }
                                    let handle = c.curve.handle(j, incoming);
                                    let f = frame(key.time, context.rate);
                                    let hf = f
                                        + (frame(c.curve.keys[adjacent].time, context.rate) - f)
                                            * handle.fraction;
                                    let hp = [
                                        self.view.x(hf, body[0]),
                                        y_value(key.value + handle.value),
                                    ];
                                    draw.add_line(p, hp, colors.muted).build();
                                    draw.add_circle(hp, 3., colors.playhead).build();
                                    if inside(hp, body, end) {
                                        handles.push((i, j, incoming, hp));
                                    }
                                }
                            }
                        }
                    }
                }
            });
            // Ruler stays fixed while rows scroll.
            draw.add_rect(origin, [end[0], body[1]], colors.header)
                .filled(true)
                .build();
            let spacing = (64. / self.view.pixels_per_frame).max(1.);
            let power = 10f64.powf(spacing.log10().floor());
            let step = if spacing / power <= 2. {
                2. * power
            } else if spacing / power <= 5. {
                5. * power
            } else {
                10. * power
            };
            let first = (self.view.first / step).floor() * step;
            let count =
                ((end[0] - body[0]) as f64 / self.view.pixels_per_frame / step).ceil() as usize + 2;
            for i in 0..count.min(200) {
                let f = first + i as f64 * step;
                let x = self.view.x(f, body[0]);
                if x < body[0] || x > end[0] {
                    continue;
                }
                draw.add_line([x, body[1] - 5.], [x, body[1]], colors.muted)
                    .build();
                draw.add_text([x + 3., origin[1] + 2.], colors.muted, format!("{f:.0}"));
            }
            let x = self.view.x(frame(context.time, context.rate), body[0]);
            if x >= body[0] && x <= end[0] {
                draw.add_line([x, origin[1]], [x, end[1]], colors.playhead)
                    .thickness(1.5)
                    .build();
            }
            if let Some(Gesture::Box { start, .. }) = &self.gesture {
                draw.add_rect(*start, mouse, colors.selected).build();
            }
        });
        if focused && !ui.io().want_text_input() && self.gesture.is_none() {
            let visible: Vec<_> = channels
                .iter()
                .filter(|c| self.matches(c, context))
                .collect();
            if !visible.is_empty()
                && (ui.is_key_pressed(Key::DownArrow) || ui.is_key_pressed(Key::UpArrow))
            {
                let current = visible
                    .iter()
                    .position(|c| self.visible_channels.contains(&(c.object, c.path.clone())));
                let index = if ui.is_key_pressed(Key::DownArrow) {
                    current.map(|i| (i + 1) % visible.len()).unwrap_or(0)
                } else {
                    current
                        .map(|i| (i + visible.len() - 1) % visible.len())
                        .unwrap_or(visible.len() - 1)
                };
                let c = visible[index];
                self.visible_channels = BTreeSet::from([(c.object, c.path.clone())]);
                response.select_node = Some(c.object);
            }
            if ui.is_key_pressed(Key::Enter) {
                self.selected = visible
                    .iter()
                    .filter(|c| self.visible_channels.contains(&(c.object, c.path.clone())))
                    .flat_map(|c| c.curve.keys.iter().map(|k| k.id))
                    .collect();
            }
        }
        if hovered && self.gesture.is_none() {
            let wheel = ui.io().mouse_wheel();
            if wheel != 0. {
                if self.curves && ui.io().key_alt() {
                    let anchor = value_range[0]
                        + f64::from(end[1] - 8. - mouse[1]) / f64::from(value_height)
                            * (value_range[1] - value_range[0]);
                    let factor = 1.2f64.powf(-f64::from(wheel));
                    let low = anchor + (value_range[0] - anchor) * factor;
                    let high = anchor + (value_range[1] - anchor) * factor;
                    if low.is_finite() && high.is_finite() && high - low > 1e-9 {
                        self.value_range = [low, high];
                    }
                } else if ui.io().key_ctrl() {
                    let anchor = self.view.frame_at(mouse[0], body[0]);
                    self.view
                        .zoom(1.2f64.powf(f64::from(wheel)), mouse[0], body[0]);
                    self.view.first =
                        anchor - f64::from(mouse[0] - body[0]) / self.view.pixels_per_frame;
                } else if ui.io().key_shift() {
                    self.view.first -= f64::from(wheel) * 40. / self.view.pixels_per_frame;
                } else {
                    self.view.scroll_y = (self.view.scroll_y - wheel * row_height * 3.).clamp(
                        0.,
                        (rows.len() as f32 * row_height - (end[1] - body[1])).max(0.),
                    );
                }
            }
            if ui.is_mouse_clicked(MouseButton::Middle) {
                self.gesture = Some(Gesture::Pan {
                    start: mouse,
                    first: self.view.first,
                    scroll: self.view.scroll_y,
                    values: self.value_range,
                });
            }
            let hit = keys
                .iter()
                .rev()
                .find(|(_, _, _, p)| (mouse[0] - p[0]).hypot(mouse[1] - p[1]) < 8.);
            if ui.is_mouse_clicked(MouseButton::Right) {
                if let Some((id, _, _, _)) = hit
                    && !self.selected.contains(id)
                {
                    self.selected.clear();
                    self.selected.insert(*id);
                }
                ui.open_popup("animation-key-menu");
            }
            if ui.is_mouse_clicked(MouseButton::Left) {
                if let Some((id, index, value)) = layer_hit {
                    if index == 0 {
                        response.visibility = Some((id, value));
                    } else {
                        response.lock = Some((id, value));
                    }
                } else if !self.hide_timing
                    && (mouse[0] - body[0]).abs() <= 4.
                    && range_hit.is_none()
                {
                    self.gesture = Some(Gesture::Divider);
                } else if let Some(r) = row_hit {
                    let row = &rows[r];
                    let indices = row.channels();
                    response.select_node = match row {
                        Row::Node(id, ..) => Some(*id),
                        _ => indices.first().map(|&i| channels[i].object),
                    };
                    match row {
                        Row::Node(id, _, _, _) => {
                            if mouse[0]
                                < origin[0]
                                    + controls_width
                                    + 16.
                                    + match row {
                                        Row::Node(_, _, depth, _) => *depth as f32 * 12.,
                                        _ => 0.,
                                    }
                            {
                                if !self.collapsed_nodes.remove(id) {
                                    self.collapsed_nodes.insert(*id);
                                }
                            } else if context.tracks.iter().any(|t| t.id == *id && !t.locked) {
                                self.gesture = Some(Gesture::Track {
                                    id: *id,
                                    start: mouse,
                                });
                            }
                        }
                        Row::Property(id, p, _) => {
                            let key = (*id, p.clone());
                            if !self.collapsed_properties.remove(&key) {
                                self.collapsed_properties.insert(key);
                            }
                        }
                        Row::Channel(i) => {
                            if !ui.io().key_ctrl() {
                                self.visible_channels.clear();
                            }
                            let c = &channels[*i];
                            let id = (c.object, c.path.clone());
                            if !self.visible_channels.remove(&id) {
                                self.visible_channels.insert(id);
                            }
                        }
                    }
                } else if let Some((id, original, edge)) = range_hit {
                    let keys = rows
                        .iter()
                        .find_map(|row| match row {
                            Row::Node(owner, ..) if *owner == id => Some(row.channels()),
                            _ => None,
                        })
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|&i| {
                            !context
                                .tracks
                                .iter()
                                .any(|t| t.id == channels[i].object && t.locked)
                        })
                        .flat_map(|i| channels[i].curve.keys.iter().map(|k| k.id))
                        .collect();
                    response.select_node = Some(id);
                    self.gesture = Some(Gesture::Range {
                        id,
                        original,
                        edge,
                        start: mouse[0],
                        channels: channels.to_vec(),
                        keys,
                    });
                } else if let Some(r) = summary_hit {
                    self.selected = rows[r]
                        .channels()
                        .iter()
                        .filter(|&&i| {
                            !context
                                .tracks
                                .iter()
                                .any(|t| t.id == channels[i].object && t.locked)
                        })
                        .flat_map(|&i| channels[i].curve.keys.iter().map(|k| k.id))
                        .collect();
                    if !self.selected.is_empty() {
                        self.gesture = Some(Gesture::Keys {
                            original: channels.to_vec(),
                            start: mouse,
                        });
                    }
                } else if mouse[1] < body[1] && mouse[0] >= body[0] {
                    self.gesture = Some(Gesture::Seek);
                } else if let Some(&(channel, key, incoming, _)) = handles
                    .iter()
                    .find(|(_, _, _, p)| (mouse[0] - p[0]).hypot(mouse[1] - p[1]) < 8.)
                {
                    self.gesture = Some(Gesture::Handle {
                        original: channels.to_vec(),
                        channel,
                        key,
                        incoming,
                    });
                } else if let Some((id, _, _, point)) = hit {
                    // Parent summaries move the complete animation, including collapsed channels.
                    let row_index =
                        ((mouse[1] - body[1] + self.view.scroll_y) / row_height).floor() as usize;
                    let summary = (!self.curves)
                        .then(|| rows.get(row_index))
                        .flatten()
                        .filter(|r| !matches!(r, Row::Channel(_)));
                    let hits: BTreeSet<_> = if let Some(row) = summary {
                        row.channels()
                            .iter()
                            .flat_map(|&i| channels[i].curve.keys.iter().map(|k| k.id))
                            .collect()
                    } else {
                        keys.iter()
                            .filter(|(_, _, _, p)| p == point)
                            .map(|(id, _, _, _)| *id)
                            .collect()
                    };
                    if ui.io().key_ctrl() {
                        if hits.iter().all(|id| self.selected.contains(id)) {
                            for id in hits {
                                self.selected.remove(&id);
                            }
                        } else {
                            self.selected.extend(hits);
                        }
                    } else if !self.selected.contains(id) {
                        self.selected = hits;
                    }
                    self.gesture = Some(Gesture::Keys {
                        original: channels.to_vec(),
                        start: mouse,
                    });
                } else if inside(mouse, body, end) {
                    let prior = if ui.io().key_ctrl() {
                        self.selected.clone()
                    } else {
                        BTreeSet::new()
                    };
                    self.gesture = Some(Gesture::Box {
                        start: mouse,
                        prior,
                    });
                }
            }
        }
        if focused && ui.is_key_pressed(Key::Escape) {
            if let Some(Gesture::Range {
                id,
                original,
                channels: before,
                ..
            }) = &self.gesture
            {
                channels.clone_from_slice(before);
                response.range = Some((*id, original.0, original.1));
                response.edit.cancelled = true;
            }
            if let Some(Gesture::Keys { original, .. } | Gesture::Handle { original, .. }) =
                &self.gesture
            {
                channels.clone_from_slice(original);
                response.edit.cancelled = true;
            }
            self.gesture = None;
            self.drop_hover = None;
        }
        if let Some(gesture) = &self.gesture {
            match gesture {
                Gesture::Track { id, start } => {
                    let dragging = (mouse[0] - start[0]).hypot(mouse[1] - start[1]) > 4.;
                    let in_header = inside(mouse, origin, [body[0], end[1]]);
                    let target = if dragging && in_header {
                        if let Some(r) = row_hit {
                            if let Row::Node(target, ..) = &rows[r] {
                                let top = body[1] + r as f32 * row_height - self.view.scroll_y;
                                track_drop(
                                    context.tracks,
                                    *id,
                                    Some((*target, (mouse[1] - top) / row_height)),
                                )
                            } else {
                                None
                            }
                        } else if mouse[1] < body[1]
                            || mouse[1]
                                >= body[1] + rows.len() as f32 * row_height - self.view.scroll_y
                        {
                            track_drop(context.tracks, *id, None)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let hover = match target {
                        Some(DropTarget::Into(Some(id))) => Some(id),
                        _ => None,
                    };
                    if let Some(id) = hover {
                        let elapsed = match self.drop_hover {
                            Some((prior, elapsed)) if prior == id => elapsed + ui.io().delta_time(),
                            _ => 0.,
                        };
                        self.drop_hover = Some((id, elapsed));
                        if elapsed >= 0.6 {
                            self.collapsed_nodes.remove(&id);
                        }
                    } else {
                        self.drop_hover = None;
                    }
                    if dragging {
                        draw.with_clip_rect(origin, end, || {
                            draw.add_text(
                                [origin[0] + 5., origin[1] + 2.],
                                colors.muted,
                                "Scene root",
                            );
                            match target {
                                Some(DropTarget::Into(parent)) => {
                                    let top = if parent.is_some() {
                                        body[1] + row_hit.unwrap() as f32 * row_height
                                            - self.view.scroll_y
                                    } else {
                                        origin[1]
                                    };
                                    draw.add_rect(
                                        [origin[0] + 1., top],
                                        [body[0] - 1., top + row_height],
                                        colors.selected,
                                    )
                                    .thickness(2.)
                                    .build();
                                }
                                Some(DropTarget::Beside(_, after)) => {
                                    let y = body[1]
                                        + (row_hit.unwrap() as f32 + if after { 1. } else { 0. })
                                            * row_height
                                        - self.view.scroll_y;
                                    draw.add_line(
                                        [origin[0] + 2., y],
                                        [body[0] - 2., y],
                                        colors.selected,
                                    )
                                    .thickness(2.)
                                    .build();
                                }
                                None => {}
                            }
                        });
                    }
                    if ui.is_mouse_released(MouseButton::Left)
                        && let Some(target) = target
                    {
                        match target {
                            DropTarget::Into(parent) => response.reparent = Some((*id, parent)),
                            DropTarget::Beside(target, after) => {
                                response.reorder = Some((*id, target, after))
                            }
                        }
                        response.edit.changed = true;
                        response.edit.finished = true;
                    }
                }
                Gesture::Divider => {
                    self.header_pixels =
                        Some((mouse[0] - origin[0]).clamp(40., (size[0] - 40.).max(40.)));
                }
                Gesture::Range {
                    id,
                    original,
                    start,
                    edge,
                    channels: before,
                    keys,
                } => {
                    let delta =
                        (f64::from(mouse[0] - start) / self.view.pixels_per_frame).round() as i64;
                    if let Ok(delta) = time(delta, context.rate) {
                        let from = if *edge <= 0 {
                            original.0.checked_add(delta).ok()
                        } else {
                            Some(original.0)
                        };
                        let to = if *edge >= 0 {
                            original.1.checked_add(delta).ok()
                        } else {
                            Some(original.1)
                        };
                        if let (Some(from), Some(to)) = (from, to)
                            && from < to
                        {
                            let proposal = if *edge == 0 {
                                model::move_keys(before, keys, delta, 0.)
                            } else {
                                Ok(before.clone())
                            };
                            match proposal {
                                Ok(proposal) => {
                                    response.edit.changed = context
                                        .tracks
                                        .iter()
                                        .find(|t| t.id == *id)
                                        .and_then(|t| t.range)
                                        != Some((from, to))
                                        || channels
                                            .iter()
                                            .zip(&proposal)
                                            .any(|(a, b)| a.curve != b.curve);
                                    channels.clone_from_slice(&proposal);
                                    response.range = Some((*id, from, to));
                                    self.error.clear();
                                }
                                Err(error) => self.error = error,
                            }
                        }
                    }
                }
                Gesture::Seek => {
                    let f = self
                        .view
                        .frame_at(mouse[0], body[0])
                        .round()
                        .clamp(0., f64::from(context.frames.saturating_sub(1)));
                    response.seek = time(f as i64, context.rate).ok();
                }
                Gesture::Pan {
                    start,
                    first,
                    scroll,
                    values,
                } => {
                    self.view.first =
                        first - f64::from(mouse[0] - start[0]) / self.view.pixels_per_frame;
                    if self.curves {
                        let delta = f64::from(mouse[1] - start[1]) / f64::from(value_height)
                            * (values[1] - values[0]);
                        self.value_range = [values[0] + delta, values[1] + delta];
                    } else {
                        self.view.scroll_y = (scroll - (mouse[1] - start[1])).max(0.);
                    }
                }
                Gesture::Box { start, prior } => {
                    self.selected = prior.clone();
                    let min = [start[0].min(mouse[0]), start[1].min(mouse[1])];
                    let max = [start[0].max(mouse[0]), start[1].max(mouse[1])];
                    self.selected.extend(
                        keys.iter()
                            .filter(|(_, _, _, p)| inside(*p, min, max))
                            .map(|(id, _, _, _)| *id),
                    );
                }
                Gesture::Keys { original, start } => {
                    let delta = f64::from(mouse[0] - start[0]) / self.view.pixels_per_frame;
                    let delta = if self.snapping {
                        time(delta.round() as i64, context.rate)
                    } else {
                        subframe_time(delta, context.rate)
                    };
                    let value_delta = if self.curves {
                        f64::from(start[1] - mouse[1]) / f64::from(value_height)
                            * (value_range[1] - value_range[0])
                    } else {
                        0.
                    };
                    match delta
                        .and_then(|d| model::move_keys(original, &self.selected, d, value_delta))
                    {
                        Ok(proposal) => {
                            response.edit.changed = channels
                                .iter()
                                .zip(&proposal)
                                .any(|(a, b)| a.curve != b.curve);
                            channels.clone_from_slice(&proposal);
                            self.error.clear();
                        }
                        Err(e) => {
                            response.edit.changed = channels
                                .iter()
                                .zip(original)
                                .any(|(a, b)| a.curve != b.curve);
                            channels.clone_from_slice(original);
                            self.error = e;
                        }
                    }
                }
                Gesture::Handle {
                    original,
                    channel,
                    key,
                    incoming,
                } => {
                    let curve = &original[*channel].curve;
                    let k = &curve.keys[*key];
                    let adjacent = if *incoming { key - 1 } else { key + 1 };
                    let f = frame(k.time, context.rate);
                    let span = frame(curve.keys[adjacent].time, context.rate) - f;
                    let fraction =
                        ((self.view.frame_at(mouse[0], body[0]) - f) / span).clamp(0., 0.5);
                    let value = value_range[0]
                        + f64::from(end[1] - 8. - mouse[1]) / f64::from(value_height)
                            * (value_range[1] - value_range[0])
                        - k.value;
                    let mut proposal = curve.clone();
                    proposal.set_handle(*key, *incoming, Handle { fraction, value });
                    if proposal.validate().is_ok() {
                        response.edit.changed = channels[*channel].curve != proposal;
                        channels[*channel].curve = proposal;
                    }
                }
            }
            if ui.is_mouse_released(MouseButton::Left) || ui.is_mouse_released(MouseButton::Middle)
            {
                response.edit.finished |= matches!(
                    gesture,
                    Gesture::Keys { .. } | Gesture::Handle { .. } | Gesture::Range { .. }
                );
                self.gesture = None;
                self.drop_hover = None;
            }
        }
        if channels.is_empty() && context.tracks.is_empty() {
            draw.add_text(
                [body[0] + 8., body[1] + 8.],
                colors.muted,
                "Key a parameter in the Inspector.",
            );
        }
    }
}
fn subframe_time(frame: f64, rate: [u32; 2]) -> Result<Time, String> {
    // Quantize new UI placement only. Bound the denominator for fractional
    // frame rates as well as integer rates; never round existing authored keys.
    let mut precision = 1_000_000u32;
    while u64::from(rate[0]) * u64::from(precision) > u64::from(u32::MAX) {
        precision /= 10;
    }
    Time::new((frame * f64::from(precision)).round() as i64, rate[0])
        .and_then(|t| t.checked_scale(i64::from(rate[1]), precision))
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drop_zones_reject_cycles_and_locks_and_distinguish_objects_from_modifiers() {
        let group = ObjectId::new();
        let child = ObjectId::new();
        let path = ObjectId::new();
        let modifier = ObjectId::new();
        let mut tracks: Vec<_> = [
            (group, None, true),
            (child, Some(group), true),
            (path, None, false),
            (modifier, Some(path), false),
        ]
        .into_iter()
        .map(|(id, parent, accepts_children)| super::super::Track {
            id,
            parent,
            label: String::new(),
            locked: false,
            range: None,
            layer: (id != modifier).then_some(super::super::LayerControls {
                accepts_children,
                visible: true,
                icon: "G",
            }),
        })
        .collect();
        assert_eq!(
            track_drop(&tracks, path, Some((group, 0.5))),
            Some(DropTarget::Into(Some(group)))
        );
        assert_eq!(
            track_drop(&tracks, path, Some((child, 0.1))),
            Some(DropTarget::Beside(child, false))
        );
        assert_eq!(
            track_drop(&tracks, path, Some((child, 0.9))),
            Some(DropTarget::Beside(child, true))
        );
        assert_eq!(
            track_drop(&tracks, child, None),
            Some(DropTarget::Into(None))
        );
        assert_eq!(track_drop(&tracks, group, Some((child, 0.5))), None);
        assert_eq!(track_drop(&tracks, group, Some((child, 0.1))), None);
        assert_eq!(track_drop(&tracks, path, Some((modifier, 0.5))), None);
        assert_eq!(track_drop(&tracks, modifier, Some((group, 0.5))), None);
        tracks[0].locked = true;
        assert_eq!(track_drop(&tracks, path, Some((group, 0.5))), None);
    }
    #[test]
    fn fractional_frame_rates_allow_subframe_placement() {
        assert_eq!(
            subframe_time(0.25, [30000, 1001]).unwrap(),
            Time::new(1001, 120000).unwrap()
        );
        assert!(subframe_time(3.123456, [60000, 1001]).is_ok());
        assert_eq!(
            subframe_time(-0.25, [24000, 1001]).unwrap(),
            Time::new(-1001, 96000).unwrap()
        );
    }
}

fn diamond(draw: &crate::sdk::imgui::DrawListMut<'_>, p: [f32; 2], r: f32, color: [f32; 4]) {
    draw.add_polyline(
        vec![
            [p[0], p[1] - r],
            [p[0] + r, p[1]],
            [p[0], p[1] + r],
            [p[0] - r, p[1]],
        ],
        color,
    )
    .closed(true)
    .filled(true)
    .build();
}

pub(super) fn header_width(width: f32) -> f32 {
    (width * 0.35).clamp(110., 210.).min((width - 40.).max(0.))
}
