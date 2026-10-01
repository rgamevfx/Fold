//! Custom immediate-mode NLE canvas: ruler, fixed headers, virtualized lanes,
//! clip bodies/handles, playhead, and direct manipulation. No media work here.
#[cfg(test)]
#[path = "canvas_tests.rs"]
mod tests;
use super::interaction::{self, Drag, Handle, Rect, View};
use crate::{Clip, ClipEdit, Sequence, SequenceEdit, Track, TrackKind};
use fold_foundation::{AssetId, ObjectId, Time};
use fold_project::Revision;
use fold_ui::sdk::{
    EditorColors,
    imgui::{Key, MouseButton, MouseCursor, Ui},
};
use std::collections::BTreeMap;

pub struct CanvasModel<'a> {
    pub sequence: &'a Sequence,
    pub revision: Revision,
    pub playhead: u32,
    pub selected: &'a [ObjectId],
    pub labels: &'a BTreeMap<AssetId, String>,
}
pub enum Action {
    Select(Vec<ObjectId>),
    Seek(u32),
    Pause,
    Commit { base: Revision, edit: SequenceEdit },
}
pub struct Canvas {
    pub view: View,
    pub linked: bool,
    pub snapping: bool,
    pub fit: bool,
    pub message: String,
    drag: Option<Drag>,
    scrub: bool,
    pan: Option<[f32; 2]>,
    scroll_bar: bool,
}
impl Default for Canvas {
    fn default() -> Self {
        Self {
            view: View::default(),
            linked: true,
            snapping: true,
            fit: true,
            message: String::new(),
            drag: None,
            scrub: false,
            pan: None,
            scroll_bar: false,
        }
    }
}
#[derive(Clone, Copy)]
struct Layout {
    area: Rect,
    body: Rect,
    ruler: Rect,
    bar: Rect,
    row: f32,
    header: f32,
    scale: f32,
}
impl Layout {
    fn new(origin: [f32; 2], size: [f32; 2], scale: f32) -> Self {
        let header = (156.0 * scale).min(size[0] * 0.4);
        let top = origin[1] + 28.0 * scale;
        let bottom = origin[1] + size[1] - 12.0 * scale;
        Self {
            area: Rect {
                min: origin,
                max: [origin[0] + size[0], origin[1] + size[1]],
            },
            body: Rect {
                min: [origin[0] + header, top],
                max: [origin[0] + size[0], bottom],
            },
            ruler: Rect {
                min: [origin[0] + header, origin[1]],
                max: [origin[0] + size[0], top],
            },
            bar: Rect {
                min: [origin[0] + header, bottom],
                max: [origin[0] + size[0], origin[1] + size[1]],
            },
            row: 54.0 * scale,
            header,
            scale,
        }
    }
    fn row_rect(self, index: usize, scroll: f32) -> Rect {
        let y = self.body.min[1] + index as f32 * self.row - scroll;
        Rect {
            min: [self.body.min[0], y],
            max: [self.body.max[0], y + self.row],
        }
    }
    fn control(self, row: Rect, index: usize) -> Rect {
        let x = self.area.min[0] + self.header - (3 - index) as f32 * 25.0 * self.scale;
        Rect {
            min: [x, row.min[1] + 7.0 * self.scale],
            max: [x + 21.0 * self.scale, row.min[1] + 29.0 * self.scale],
        }
    }
}
fn rows(sequence: &Sequence) -> Vec<&Track> {
    sequence
        .tracks
        .iter()
        .rev()
        .filter(|t| t.kind == TrackKind::Video)
        .chain(
            sequence
                .tracks
                .iter()
                .filter(|t| t.kind == TrackKind::Audio),
        )
        .collect()
}
fn clip_rect(clip: &Clip, row: Rect, view: View, rate: [u32; 2], scale: f32) -> Rect {
    Rect {
        min: [
            view.x(interaction::frame(clip.start, rate), row.min[0]),
            row.min[1] + 4.0 * scale,
        ],
        max: [
            view.x(interaction::frame(clip.end().unwrap(), rate), row.min[0]),
            row.max[1] - 4.0 * scale,
        ],
    }
}
fn unchanged(sequence: &Sequence, edit: &SequenceEdit) -> bool {
    let SequenceEdit::Clip { id, edit, .. } = edit else {
        return false;
    };
    let Some(clip) = sequence.clips.iter().find(|c| c.id == *id) else {
        return false;
    };
    match edit {
        ClipEdit::MoveTo { start, track } => *start == clip.start && *track == clip.track,
        ClipEdit::Trim { start, end } => *start == clip.start && Ok(*end) == clip.end(),
        _ => false,
    }
}
impl Canvas {
    pub fn draw(&mut self, ui: &Ui, model: CanvasModel<'_>) -> Vec<Action> {
        let mut actions = Vec::new();
        let sequence = model.sequence;
        let size = ui.content_region_avail();
        if size[0] < 100.0 || size[1] < 80.0 {
            ui.text("Enlarge the timeline panel to edit.");
            return actions;
        }
        let scale = (ui.text_line_height() / 13.0).max(0.75);
        let layout = Layout::new(ui.cursor_screen_pos(), size, scale);
        let track_rows = rows(sequence);
        let width = f64::from(layout.body.max[0] - layout.body.min[0]);
        let end = interaction::frame(sequence.end().unwrap_or(Time::ZERO), sequence.rate).max(1.0);
        if self.fit {
            self.view.first = 0.0;
            self.view.pixels_per_frame = (width / (end * 1.12).max(24.0)).clamp(0.05, 80.0);
            self.fit = false;
        }
        let max_y = (track_rows.len() as f32 * layout.row
            - (layout.body.max[1] - layout.body.min[1]))
            .max(0.0);
        ui.invisible_button("timeline-canvas", size);
        let hovered = ui.is_item_hovered();
        let point = ui.mouse_pos();
        if hovered {
            if ui.io().key_ctrl() {
                self.view.zoom(
                    1.2f64.powf(f64::from(ui.io().mouse_wheel())),
                    point[0],
                    layout.body.min[0],
                );
            } else if ui.io().key_shift() {
                self.view.first -=
                    f64::from(ui.io().mouse_wheel()) * 64.0 / self.view.pixels_per_frame;
            } else {
                self.view.scroll_y -= ui.io().mouse_wheel() * layout.row;
            }
            self.view.first -=
                f64::from(ui.io().mouse_wheel_h()) * 64.0 / self.view.pixels_per_frame;
            if ui.is_mouse_clicked(MouseButton::Middle) {
                self.pan = Some(point);
            }
        }
        if let Some(last) = self.pan {
            if ui.is_mouse_down(MouseButton::Middle) {
                self.view.first -= f64::from(point[0] - last[0]) / self.view.pixels_per_frame;
                self.view.scroll_y -= point[1] - last[1];
                self.pan = Some(point);
            } else {
                self.pan = None;
            }
        }
        if self.drag.is_some() && ui.is_mouse_down(MouseButton::Left) {
            let edge = 16.0 * scale;
            if point[0] >= layout.body.min[0] && point[0] < layout.body.min[0] + edge {
                self.view.first -= 4.0 / self.view.pixels_per_frame;
            }
            if point[0] < layout.body.max[0] && point[0] > layout.body.max[0] - edge {
                self.view.first += 4.0 / self.view.pixels_per_frame;
            }
        }
        let visible = width / self.view.pixels_per_frame;
        let total =
            (end + f64::from(sequence.rate[0]) / f64::from(sequence.rate[1]) * 10.0).max(visible);
        self.view.scroll_y = self.view.scroll_y.clamp(0.0, max_y);
        self.view.first = self.view.first.clamp(0.0, (total - visible).max(0.0));
        if self.drag.as_ref().is_some_and(|d| d.base != model.revision) {
            self.drag = None;
            self.message = "Project changed; drag cancelled.".into();
        }
        if self.drag.is_some() && ui.is_key_pressed(Key::Escape) {
            self.drag = None;
            self.message = "Drag cancelled.".into();
        }
        let hovered_row = track_rows.iter().enumerate().find(|(index, _)| {
            let row = layout.row_rect(*index, self.view.scroll_y);
            point[1] >= row.min[1]
                && point[1] < row.max[1]
                && point[1] >= layout.body.min[1]
                && point[1] < layout.body.max[1]
        });
        if hovered && ui.is_mouse_clicked(MouseButton::Left) {
            self.message.clear();
            if layout.ruler.contains(point) {
                self.scrub = true;
            } else if layout.bar.contains(point) {
                self.scroll_bar = true;
            } else if let Some((index, track)) = hovered_row {
                let row = layout.row_rect(index, self.view.scroll_y);
                if point[0] < layout.body.min[0] {
                    let mut edit = None;
                    for control in 0..3 {
                        if layout.control(row, control).contains(point) {
                            let (mut enabled, mut solo, mut locked) =
                                (track.enabled, track.solo, track.locked);
                            match control {
                                0 => enabled = !enabled,
                                1 if track.kind == TrackKind::Audio => solo = !solo,
                                2 => locked = !locked,
                                _ => {}
                            }
                            edit = Some(SequenceEdit::Track {
                                id: track.id,
                                enabled,
                                solo,
                                locked,
                            });
                        }
                    }
                    if let Some(edit) = edit {
                        actions.push(Action::Commit {
                            base: model.revision,
                            edit,
                        });
                    } else {
                        actions.push(Action::Select(vec![track.id]));
                    }
                } else if let Some(clip) = sequence.clips.iter().find(|c| {
                    c.track == track.id
                        && clip_rect(c, row, self.view, sequence.rate, scale).contains(point)
                }) {
                    let linked = self.linked && !ui.io().key_alt();
                    actions.push(Action::Select(
                        sequence
                            .linked_ids(clip.id, linked)
                            .unwrap_or_else(|_| vec![clip.id]),
                    ));
                    if track.locked {
                        self.message = "Track is locked.".into();
                    } else {
                        actions.push(Action::Pause);
                        self.drag = Some(Drag {
                            base: model.revision,
                            original: sequence.clone(),
                            clip: clip.id,
                            linked,
                            handle: interaction::handle(
                                clip_rect(clip, row, self.view, sequence.rate, scale),
                                point,
                                scale,
                            ),
                            grab_frame: self.view.frame_at(point[0], layout.body.min[0]),
                        });
                    }
                } else {
                    actions.push(Action::Select(vec![track.id]));
                }
            }
        }
        if self.scrub {
            if ui.is_mouse_down(MouseButton::Left) {
                actions.push(Action::Seek(
                    self.view
                        .frame_at(point[0], layout.body.min[0])
                        .round()
                        .clamp(0.0, 18000.0) as u32,
                ));
            } else {
                self.scrub = false;
            }
        }
        if self.scroll_bar {
            if ui.is_mouse_down(MouseButton::Left) {
                self.view.first = ((f64::from(point[0] - layout.bar.min[0]) / width) * total
                    - visible / 2.0)
                    .clamp(0.0, (total - visible).max(0.0));
            } else {
                self.scroll_bar = false;
            }
        }
        let mut preview = None;
        let mut proposed = None;
        if let Some(drag) = &self.drag {
            ui.set_mouse_cursor(Some(if drag.handle == Handle::Move {
                MouseCursor::ResizeAll
            } else {
                MouseCursor::ResizeEW
            }));
            let track = hovered_row.map(|(_, t)| t.id).unwrap_or_else(|| {
                drag.original
                    .clips
                    .iter()
                    .find(|c| c.id == drag.clip)
                    .unwrap()
                    .track
            });
            let playhead = interaction::time(i64::from(model.playhead), sequence.rate).unwrap();
            let snap = (self.snapping && !ui.io().key_shift()).then_some((
                playhead,
                7.0 * f64::from(scale) / self.view.pixels_per_frame,
            ));
            match drag.proposal(
                self.view.frame_at(point[0], layout.body.min[0]),
                track,
                snap,
            ) {
                Ok(edit) => {
                    match drag.original.edited(&edit) {
                        Ok(sequence) => {
                            preview = Some(sequence);
                            self.message =
                                "Release to commit • Escape to cancel • Shift bypasses snapping"
                                    .into();
                        }
                        Err(error) => {
                            self.message = format!("Cannot drop: {error}");
                        }
                    }
                    proposed = Some(edit);
                }
                Err(error) => self.message = error,
            }
            if ui.is_mouse_released(MouseButton::Left) {
                if preview.is_some() && layout.body.contains(point) {
                    if let Some(edit) = proposed.take()
                        && !unchanged(&drag.original, &edit)
                    {
                        actions.push(Action::Commit {
                            base: drag.base,
                            edit,
                        });
                    }
                    self.message.clear();
                }
                self.drag = None;
            }
        } else if hovered
            && layout.body.contains(point)
            && let Some((index, track)) = hovered_row
        {
            let row = layout.row_rect(index, self.view.scroll_y);
            if let Some(clip) = sequence.clips.iter().find(|c| {
                c.track == track.id
                    && clip_rect(c, row, self.view, sequence.rate, scale).contains(point)
            }) && interaction::handle(
                clip_rect(clip, row, self.view, sequence.rate, scale),
                point,
                scale,
            ) != Handle::Move
            {
                ui.set_mouse_cursor(Some(MouseCursor::ResizeEW));
            }
        }
        self.paint(
            ui,
            &model,
            preview.as_ref().unwrap_or(sequence),
            layout,
            total,
            visible,
        );
        actions
    }
    fn paint(
        &self,
        ui: &Ui,
        model: &CanvasModel<'_>,
        shown: &Sequence,
        layout: Layout,
        total: f64,
        visible: f64,
    ) {
        let colors = EditorColors::from_ui(ui);
        let draw = ui.get_window_draw_list();
        let _clip = draw.push_clip_rect(layout.area.min, layout.area.max, true);
        draw.add_rect(layout.area.min, layout.area.max, colors.background)
            .filled(true)
            .build();
        draw.add_rect(
            layout.area.min,
            [layout.body.min[0], layout.body.max[1]],
            colors.header,
        )
        .filled(true)
        .build();
        draw.add_text(
            [layout.area.min[0] + 8.0, layout.area.min[1] + 6.0],
            colors.text,
            "TRACKS",
        );
        let mut step = 1.0;
        while step * self.view.pixels_per_frame < 85.0 * f64::from(layout.scale) {
            step *= 2.0;
        }
        let first_tick = (self.view.first / step).ceil() * step;
        let last_frame = self.view.first + visible;
        {
            let _body_clip = draw.push_clip_rect(layout.ruler.min, layout.body.max, true);
            let mut tick = first_tick;
            while tick <= last_frame {
                let x = self.view.x(tick, layout.body.min[0]);
                draw.add_line(
                    [x, layout.ruler.max[1] - 6.0],
                    [x, layout.body.max[1]],
                    colors.grid,
                )
                .build();
                let seconds = tick * f64::from(shown.rate[1]) / f64::from(shown.rate[0]);
                draw.add_text(
                    [x + 4.0, layout.ruler.min[1] + 6.0],
                    colors.muted,
                    format!("{seconds:.2}s"),
                );
                tick += step;
            }
        }
        for (index, track) in rows(shown).iter().enumerate() {
            let row = layout.row_rect(index, self.view.scroll_y);
            if row.max[1] <= layout.body.min[1] || row.min[1] >= layout.body.max[1] {
                continue;
            }
            let _row_clip = draw.push_clip_rect(
                [layout.area.min[0], row.min[1].max(layout.body.min[1])],
                [row.max[0], row.max[1].min(layout.body.max[1])],
                true,
            );
            let mut lane = colors.lane;
            lane[3] = if index % 2 == 0 { 0.34 } else { 0.20 };
            draw.add_rect(row.min, row.max, lane).filled(true).build();
            draw.add_line([layout.area.min[0], row.max[1]], row.max, colors.grid)
                .build();
            draw.add_text(
                [
                    layout.area.min[0] + 9.0 * layout.scale,
                    row.min[1] + 10.0 * layout.scale,
                ],
                if model.selected.contains(&track.id) {
                    colors.selected
                } else {
                    colors.text
                },
                &track.name,
            );
            draw.add_text(
                [
                    layout.area.min[0] + 9.0 * layout.scale,
                    row.min[1] + 30.0 * layout.scale,
                ],
                colors.muted,
                if track.kind == TrackKind::Video {
                    "Video"
                } else {
                    "Audio"
                },
            );
            for (control, text, on) in [
                (
                    0,
                    if track.kind == TrackKind::Video {
                        "E"
                    } else {
                        "M"
                    },
                    if track.kind == TrackKind::Video {
                        track.enabled
                    } else {
                        !track.enabled
                    },
                ),
                (1, "S", track.solo),
                (2, "L", track.locked),
            ] {
                if control == 1 && track.kind == TrackKind::Video {
                    continue;
                }
                let rect = layout.control(row, control);
                draw.add_rect(
                    rect.min,
                    rect.max,
                    if on { colors.selected } else { colors.lane },
                )
                .filled(true)
                .rounding(2.0)
                .build();
                draw.add_text(
                    [
                        rect.min[0] + 5.0 * layout.scale,
                        rect.min[1] + 4.0 * layout.scale,
                    ],
                    colors.text,
                    text,
                );
            }
            let _lane_clip = draw.push_clip_rect(row.min, row.max, true);
            for clip in shown.clips.iter().filter(|c| c.track == track.id) {
                let rect = clip_rect(clip, row, self.view, shown.rate, layout.scale);
                if rect.max[0] <= row.min[0] || rect.min[0] >= row.max[0] {
                    continue;
                }
                let selected = model.selected.contains(&clip.id)
                    || self.drag.as_ref().is_some_and(|d| {
                        d.original
                            .linked_ids(d.clip, d.linked)
                            .is_ok_and(|ids| ids.contains(&clip.id))
                    });
                let mut color = if track.kind == TrackKind::Video {
                    colors.video
                } else {
                    colors.audio
                };
                if !track.enabled || track.locked {
                    color[3] = 0.45;
                }
                draw.add_rect(rect.min, rect.max, color)
                    .filled(true)
                    .rounding(3.0 * layout.scale)
                    .build();
                if selected {
                    draw.add_rect(rect.min, rect.max, colors.selected)
                        .thickness(2.0 * layout.scale)
                        .rounding(3.0 * layout.scale)
                        .build();
                }
                if rect.max[0].min(row.max[0]) - rect.min[0].max(row.min[0]) < 12.0 * layout.scale {
                    continue;
                }
                let _label_clip = draw.push_clip_rect(
                    [rect.min[0].max(row.min[0]) + 3.0, rect.min[1]],
                    [rect.max[0].min(row.max[0]) - 2.0, rect.max[1]],
                    true,
                );
                let x = rect.min[0].max(row.min[0]) + 8.0 * layout.scale;
                let name = if let crate::SourceMedia::Document { .. } = &clip.info {
                    "Nested source"
                } else {
                    model
                        .labels
                        .get(&clip.asset)
                        .map(String::as_str)
                        .unwrap_or("Unavailable media")
                };
                draw.add_text([x, rect.min[1] + 6.0 * layout.scale], colors.text, name);
                draw.add_text(
                    [x, rect.min[1] + 25.0 * layout.scale],
                    colors.text,
                    if clip.link.is_some() {
                        "Linked A/V"
                    } else if track.kind == TrackKind::Video {
                        "Video"
                    } else {
                        "Audio"
                    },
                );
            }
        }
        {
            let _body = draw.push_clip_rect(layout.ruler.min, layout.body.max, true);
            let x = self.view.x(f64::from(model.playhead), layout.body.min[0]);
            draw.add_line(
                [x, layout.ruler.min[1]],
                [x, layout.body.max[1]],
                colors.playhead,
            )
            .thickness(2.0 * layout.scale)
            .build();
            draw.add_rect(
                [x - 4.0 * layout.scale, layout.ruler.min[1]],
                [
                    x + 4.0 * layout.scale,
                    layout.ruler.min[1] + 8.0 * layout.scale,
                ],
                colors.playhead,
            )
            .filled(true)
            .build();
        }
        draw.add_line(
            [layout.body.min[0], layout.area.min[1]],
            [layout.body.min[0], layout.body.max[1]],
            colors.grid,
        )
        .build();
        draw.add_rect(layout.bar.min, layout.bar.max, colors.header)
            .filled(true)
            .build();
        let width = layout.bar.max[0] - layout.bar.min[0];
        let left = layout.bar.min[0] + (self.view.first / total) as f32 * width;
        let right = left + (visible / total) as f32 * width;
        draw.add_rect(
            [left, layout.bar.min[1] + 2.0],
            [right, layout.bar.max[1] - 2.0],
            colors.muted,
        )
        .filled(true)
        .rounding(3.0)
        .build();
    }
}
