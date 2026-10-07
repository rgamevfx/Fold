//! Custom immediate-mode NLE canvas: ruler, fixed headers, virtualized lanes,
//! clip bodies/handles, playhead, and direct manipulation. No media work here.
#[path = "paint.rs"]
mod paint;
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
    pub placement: Option<
        &'a dyn Fn(
            fold_platform::browser::Entry,
            Option<ObjectId>,
            Time,
            bool,
            bool,
        ) -> Result<Sequence, String>,
    >,
    pub labels: &'a BTreeMap<AssetId, String>,
}
pub enum Action {
    Place {
        entry: fold_platform::browser::Entry,
        track: Option<ObjectId>,
        new_track: bool,
        audio_only: bool,
        at: Time,
    },
    Open(ObjectId),
    AddTransition(ObjectId),
    Select(Vec<ObjectId>),
    Seek(u32),
    Pause,
    Preview {
        base: Revision,
        edit: SequenceEdit,
    },
    CancelPreview,
    Commit {
        base: Revision,
        edit: SequenceEdit,
    },
}
pub struct ClipVisual {
    pub name: String,
    pub icon: fold_ui::sdk::toolbar::ToolbarIcon,
    pub kind: &'static str,
    pub error: Option<String>,
    pub texture: Option<fold_ui::sdk::imgui::texture::ManagedTextureId>,
}
pub struct Canvas {
    pub external_pointer: Option<[f32; 2]>,
    drop_layout: Option<Layout>,
    pub visuals: BTreeMap<ObjectId, ClipVisual>,
    pub cache: BTreeMap<u32, fold_ui::sdk::cache_indicator::Coverage>,
    pub blade: bool,
    pub mode: crate::EditMode,
    pub trim: interaction::TrimMode,
    pub row_height: f32,
    pub track_heights: BTreeMap<ObjectId, f32>,
    pub header_width: f32,
    pub media_previews: bool,
    marquee: Option<([f32; 2], Vec<ObjectId>)>,
    resize_header: bool,
    resize_row: bool,
    context_clip: Option<ObjectId>,
    context_track: Option<ObjectId>,
    rename: String,

    pub view: View,
    pub linked: bool,
    pub snapping: bool,
    pub fit: bool,
    pub message: String,
    drag: Option<Drag>,
    pub(super) transition_drag: Option<interaction::TransitionDrag>,
    cut: Option<ObjectId>,
    scrub: bool,
    pan: Option<[f32; 2]>,
    scroll_bar: bool,
}
impl Default for Canvas {
    fn default() -> Self {
        Self {
            external_pointer: None,
            drop_layout: None,
            visuals: BTreeMap::new(),
            cache: BTreeMap::new(),
            blade: false,
            mode: Default::default(),
            trim: Default::default(),
            row_height: 58.,
            track_heights: BTreeMap::new(),
            header_width: 156.,
            media_previews: true,
            marquee: None,
            resize_header: false,
            resize_row: false,
            context_clip: None,
            context_track: None,
            rename: String::new(),
            view: View::default(),
            linked: true,
            snapping: true,
            fit: true,
            message: String::new(),
            drag: None,
            transition_drag: None,
            cut: None,
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
    heights: [f32; 32],
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
            row: 58.0 * scale,
            heights: [58.0 * scale; 32],
            header,
            scale,
        }
    }
    fn row_rect(self, index: usize, scroll: f32) -> Rect {
        let y =
            self.body.min[1] + 18. * self.scale + self.heights[..index.min(32)].iter().sum::<f32>()
                - scroll;
        Rect {
            min: [self.body.min[0], y],
            max: [
                self.body.max[0],
                y + self.heights.get(index).copied().unwrap_or(self.row),
            ],
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
    if let SequenceEdit::Selection {
        edit:
            crate::SelectionEdit::Move {
                delta,
                anchor,
                track,
                new_track,
                ..
            },
        ..
    } = edit
    {
        return *delta == Time::ZERO
            && new_track.is_none()
            && sequence
                .clips
                .iter()
                .any(|c| c.id == *anchor && c.track == *track);
    }
    if let SequenceEdit::Selection {
        edit: crate::SelectionEdit::RippleTrim { id, start, end },
        ..
    } = edit
    {
        return sequence
            .clips
            .iter()
            .any(|c| c.id == *id && c.start == *start && c.end().ok() == Some(*end));
    }
    if let SequenceEdit::Selection {
        edit: crate::SelectionEdit::Roll { id, end },
        ..
    } = edit
    {
        return sequence
            .clips
            .iter()
            .any(|c| c.id == *id && c.end().ok() == Some(*end));
    }
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
        self.drop_layout = None;
        let mut actions = Vec::new();
        let sequence = model.sequence;
        let size = ui.content_region_avail();
        if size[0] < 100.0 || size[1] < 80.0 {
            ui.text("Enlarge the timeline panel to edit.");
            return actions;
        }
        let scale = (ui.text_line_height() / 13.0).max(0.75);
        let mut layout = Layout::new(ui.cursor_screen_pos(), size, scale);
        layout.row = self.row_height * scale;
        let header = (self.header_width * scale).min(size[0] * 0.45);
        layout.header = header;
        layout.body.min[0] = layout.area.min[0] + header;
        layout.ruler.min[0] = layout.body.min[0];
        layout.bar.min[0] = layout.body.min[0];
        let track_rows = rows(sequence);
        for (i, track) in track_rows.iter().enumerate() {
            layout.heights[i] = self
                .track_heights
                .get(&track.id)
                .copied()
                .unwrap_or(self.row_height)
                * scale;
        }
        let width = f64::from(layout.body.max[0] - layout.body.min[0]);
        let end = interaction::frame(sequence.end().unwrap_or(Time::ZERO), sequence.rate).max(1.0);
        if self.fit {
            self.view.first = 0.0;
            self.view.pixels_per_frame = (width / (end * 1.12).max(24.0)).clamp(0.05, 80.0);
            self.fit = false;
        }
        let max_y = (36. * scale + layout.heights[..track_rows.len()].iter().sum::<f32>()
            - (layout.body.max[1] - layout.body.min[1]))
            .max(0.0);
        self.drop_layout = ui
            .is_window_hovered_with_flags(fold_ui::sdk::imgui::WindowHoveredFlags::CHILD_WINDOWS)
            .then_some(layout);
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
            if point[1] < layout.body.min[1] + edge {
                self.view.scroll_y -= 5. * scale;
            }
            if point[1] > layout.body.max[1] - edge {
                self.view.scroll_y += 5. * scale;
            }
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
        if self
            .transition_drag
            .as_ref()
            .is_some_and(|d| d.base != model.revision)
        {
            self.transition_drag = None;
            actions.push(Action::CancelPreview);
            self.message = "Project changed; drag cancelled.".into();
        }
        if self.drag.as_ref().is_some_and(|d| d.base != model.revision) {
            self.drag = None;
            self.message = "Project changed; drag cancelled.".into();
        }
        if self.drag.is_some() && ui.is_key_pressed(Key::Escape) {
            self.drag = None;
            self.message.clear();
        }
        let hovered_row = track_rows.iter().enumerate().find(|(index, _)| {
            let row = layout.row_rect(*index, self.view.scroll_y);
            point[1] >= row.min[1]
                && point[1] < row.max[1]
                && point[1] >= layout.body.min[1]
                && point[1] < layout.body.max[1]
        });
        let mut placement_preview = None;
        let project_drop = fold_ui::sdk::project_drop::preview_target(ui);
        if let Some((entry, delivery)) = project_drop
            && layout.body.contains(point)
        {
            let new_track = hovered_row.is_none();
            let audio_only = hovered_row.is_some_and(|(_, t)| t.kind == TrackKind::Audio)
                || point[1] >= layout.row_rect(track_rows.len(), self.view.scroll_y).min[1];
            self.message = if new_track {
                if audio_only {
                    "New audio track"
                } else {
                    "New video track"
                }
            } else {
                "Place on track"
            }
            .into();
            if let Ok(mut at) = interaction::time(
                self.view
                    .frame_at(point[0], layout.body.min[0])
                    .round()
                    .max(0.) as i64,
                sequence.rate,
            ) {
                if self.snapping
                    && !ui.io().key_shift()
                    && let Some(offset) = interaction::snap_offset(
                        sequence,
                        &[],
                        &[at],
                        interaction::time(i64::from(model.playhead), sequence.rate).unwrap(),
                        7. * f64::from(scale) / self.view.pixels_per_frame,
                    )
                    .ok()
                    .flatten()
                {
                    at = at.checked_add(offset).unwrap();
                }
                let track = hovered_row.map(|(_, t)| t.id);
                if let Some(validate) = model.placement {
                    match validate(entry, track, at, new_track, audio_only) {
                        Ok(preview) => {
                            placement_preview = Some(preview);
                            if delivery {
                                actions.push(Action::Place {
                                    entry,
                                    track,
                                    new_track,
                                    audio_only,
                                    at,
                                });
                                self.message.clear();
                            }
                        }
                        Err(error) => self.message = format!("Cannot place: {error}"),
                    }
                }
            }
        }
        let hovered_cut = hovered_row
            .and_then(|(_, track)| {
                sequence.clips.iter().find(|a| {
                    a.track == track.id
                        && a.end().is_ok_and(|cut| {
                            (self
                                .view
                                .x(interaction::frame(cut, sequence.rate), layout.body.min[0])
                                - point[0])
                                .abs()
                                <= 5. * scale
                                && sequence
                                    .clips
                                    .iter()
                                    .any(|b| b.track == a.track && b.start == cut)
                        })
                })
            })
            .map(|c| c.id);
        if hovered && ui.is_mouse_clicked(MouseButton::Left) {
            self.cut = hovered_cut;
        }
        if hovered && ui.is_mouse_clicked(MouseButton::Right) {
            self.cut = hovered_cut;
            self.context_track = hovered_row.map(|(_, t)| t.id);
            self.context_clip = hovered_row
                .and_then(|(i, t)| {
                    sequence.clips.iter().find(|c| {
                        c.track == t.id
                            && clip_rect(
                                c,
                                layout.row_rect(i, self.view.scroll_y),
                                self.view,
                                sequence.rate,
                                scale,
                            )
                            .contains(point)
                    })
                })
                .map(|c| c.id);
            if let Some(id) = self.context_clip
                && !model.selected.contains(&id)
            {
                actions.push(Action::Select(
                    sequence.linked_ids(id, self.linked).unwrap_or(vec![id]),
                ));
            }
            self.rename = self
                .context_track
                .and_then(|id| sequence.track(id).ok())
                .map(|t| t.name.clone())
                .unwrap_or_default();
            ui.open_popup("timeline-context");
        }
        if let Some(_popup) = ui.begin_popup("timeline-context") {
            if let Some(outgoing) = self.cut {
                if ui.menu_item("Add Transition") {
                    actions.push(Action::AddTransition(outgoing));
                }
                ui.separator();
            }
            if let Some(id) = self.context_clip {
                if sequence
                    .clips
                    .iter()
                    .any(|c| c.id == id && matches!(c.info, crate::SourceMedia::Document { .. }))
                    && ui.menu_item("Open source")
                {
                    actions.push(Action::Open(id));
                }
                let ids = if model.selected.contains(&id) {
                    model.selected.to_vec()
                } else {
                    vec![id]
                };
                if let Some(edit) = super::controls::edit_items(
                    ui,
                    self,
                    sequence,
                    &ids,
                    interaction::time(i64::from(model.playhead), sequence.rate).unwrap(),
                ) {
                    actions.push(Action::Commit {
                        base: model.revision,
                        edit,
                    });
                }
            } else if let Some(id) = self.context_track {
                if ui
                    .input_text("Name", &mut self.rename)
                    .enter_returns_true(true)
                    .build()
                {
                    actions.push(Action::Commit {
                        base: model.revision,
                        edit: SequenceEdit::RenameTrack {
                            id,
                            name: self.rename.clone(),
                        },
                    });
                    ui.close_current_popup();
                }
                if ui.menu_item("Remove empty track") {
                    actions.push(Action::Commit {
                        base: model.revision,
                        edit: SequenceEdit::RemoveTrack { id },
                    });
                }
            }
            ui.separator();
            for kind in [TrackKind::Video, TrackKind::Audio] {
                if ui.menu_item(if kind == TrackKind::Video {
                    "Add video track"
                } else {
                    "Add audio track"
                }) {
                    actions.push(Action::Commit {
                        base: model.revision,
                        edit: SequenceEdit::AddTrack { kind },
                    });
                }
            }
        }
        if hovered
            && point[0] < layout.body.min[0]
            && (point[0] - layout.body.min[0]).abs() < 4. * scale
        {
            ui.set_mouse_cursor(Some(MouseCursor::ResizeEW));
        }
        if hovered
            && point[0] < layout.body.min[0]
            && hovered_row.is_some_and(|(i, _)| {
                point[1] > layout.row_rect(i, self.view.scroll_y).max[1] - 5. * scale
            })
        {
            ui.set_mouse_cursor(Some(MouseCursor::ResizeNS));
        }
        if self.resize_header {
            if ui.is_mouse_down(MouseButton::Left) {
                self.header_width = ((point[0] - layout.area.min[0]) / scale).clamp(110., 300.);
            } else {
                self.resize_header = false;
            }
        }
        if self.resize_row {
            if ui.is_mouse_down(MouseButton::Left) {
                if let Some(id) = self.context_track
                    && let Some(i) = track_rows.iter().position(|t| t.id == id)
                {
                    self.track_heights.insert(
                        id,
                        ((point[1] - layout.row_rect(i, self.view.scroll_y).min[1]) / scale)
                            .clamp(38., 140.),
                    );
                }
            } else {
                self.resize_row = false;
            }
        }
        if hovered && ui.is_mouse_clicked(MouseButton::Left) {
            self.message.clear();
            if point[0] < layout.body.min[0] && (point[0] - layout.body.min[0]).abs() < 4. * scale {
                self.resize_header = true;
            } else if layout.ruler.contains(point) {
                self.scrub = true;
            } else if layout.bar.contains(point) {
                self.scroll_bar = true;
            } else if let Some((index, track)) = hovered_row {
                let row = layout.row_rect(index, self.view.scroll_y);
                if point[0] < layout.body.min[0] {
                    if point[1] > row.max[1] - 5. * scale {
                        self.resize_row = true;
                        self.context_track = Some(track.id);
                        return actions;
                    }
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
                } else if let Some((transition, rect)) = sequence
                    .transitions
                    .iter()
                    .filter_map(|t| {
                        let outgoing = sequence
                            .clips
                            .iter()
                            .find(|c| c.id == t.outgoing && c.track == track.id)?;
                        let _ = outgoing;
                        let rect = Rect {
                            min: [
                                self.view.x(
                                    interaction::frame(t.start, sequence.rate),
                                    layout.body.min[0],
                                ),
                                row.max[1] - 18. * scale,
                            ],
                            max: [
                                self.view.x(
                                    interaction::frame(t.end().ok()?, sequence.rate),
                                    layout.body.min[0],
                                ),
                                row.max[1] - 3. * scale,
                            ],
                        };
                        rect.contains(point).then_some((t, rect))
                    })
                    .next()
                {
                    actions.push(Action::Select(vec![transition.id]));
                    if !track.locked {
                        actions.push(Action::Pause);
                        self.transition_drag = Some(interaction::TransitionDrag {
                            base: model.revision,
                            original: sequence.clone(),
                            id: transition.id,
                            handle: interaction::handle(rect, point, scale),
                            grab_frame: self.view.frame_at(point[0], layout.body.min[0]),
                        });
                    }
                } else if let Some(clip) = sequence.clips.iter().find(|c| {
                    c.track == track.id
                        && clip_rect(c, row, self.view, sequence.rate, scale).contains(point)
                }) {
                    let linked = self.linked && !ui.io().key_alt();
                    let mut selected = sequence
                        .linked_ids(clip.id, linked)
                        .unwrap_or_else(|_| vec![clip.id]);
                    if ui.io().key_ctrl() && selected.iter().all(|id| model.selected.contains(id)) {
                        actions.push(Action::Select(
                            model
                                .selected
                                .iter()
                                .copied()
                                .filter(|id| !selected.contains(id))
                                .collect(),
                        ));
                        return actions;
                    }
                    if ui.io().key_shift() || ui.io().key_ctrl() {
                        for id in model.selected {
                            if !selected.contains(id) {
                                selected.push(*id);
                            }
                        }
                    } else if model.selected.contains(&clip.id) {
                        selected = model.selected.to_vec();
                    }
                    actions.push(Action::Select(selected.clone()));
                    if ui.is_mouse_double_clicked(MouseButton::Left)
                        && matches!(clip.info, crate::SourceMedia::Document { .. })
                    {
                        self.drag = None;
                        actions.push(Action::Open(clip.id));
                        return actions;
                    }
                    if self.blade {
                        let at = interaction::time(
                            self.view.frame_at(point[0], layout.body.min[0]).round() as i64,
                            sequence.rate,
                        )
                        .unwrap();
                        actions.push(Action::Commit {
                            base: model.revision,
                            edit: super::controls::selection(
                                &selected,
                                linked,
                                crate::SelectionEdit::Split { at },
                            ),
                        });
                        return actions;
                    }
                    if track.locked {
                        self.message = "Track is locked.".into();
                    } else {
                        actions.push(Action::Pause);
                        self.drag = Some(Drag {
                            selected,
                            new_track: ObjectId::new(),
                            mode: self.mode,
                            trim: self.trim,
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
                    self.marquee = Some((
                        point,
                        if ui.io().key_shift() {
                            model.selected.to_vec()
                        } else {
                            vec![]
                        },
                    ));
                }
            } else if layout.body.contains(point) {
                self.marquee = Some((point, vec![]));
            }
        }
        if let Some((start, initial)) = &self.marquee {
            let rect = Rect {
                min: [start[0].min(point[0]), start[1].min(point[1])],
                max: [start[0].max(point[0]), start[1].max(point[1])],
            };
            let mut selected = initial.clone();
            for (index, track) in track_rows.iter().enumerate() {
                for c in sequence.clips.iter().filter(|c| c.track == track.id) {
                    let r = clip_rect(
                        c,
                        layout.row_rect(index, self.view.scroll_y),
                        self.view,
                        sequence.rate,
                        scale,
                    );
                    if r.min[0] < rect.max[0]
                        && r.max[0] > rect.min[0]
                        && r.min[1] < rect.max[1]
                        && r.max[1] > rect.min[1]
                    {
                        for id in sequence.linked_ids(c.id, self.linked).unwrap_or_default() {
                            if !selected.contains(&id) {
                                selected.push(id);
                            }
                        }
                    }
                }
            }
            actions.push(Action::Select(if ui.is_key_pressed(Key::Escape) {
                initial.clone()
            } else {
                selected
            }));
            if ui.is_mouse_released(MouseButton::Left) || ui.is_key_pressed(Key::Escape) {
                self.marquee = None;
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
        let mut preview = placement_preview;
        if let Some(drag) = &self.transition_drag {
            ui.set_mouse_cursor(Some(if drag.handle == Handle::Move {
                MouseCursor::ResizeAll
            } else {
                MouseCursor::ResizeEW
            }));
            if ui.is_key_pressed(Key::Escape) {
                self.transition_drag = None;
                actions.push(Action::CancelPreview);
            } else if let Ok(edit) = drag.proposal(self.view.frame_at(point[0], layout.body.min[0]))
            {
                preview = drag.original.edited(&edit).ok();
                actions.push(Action::Preview {
                    base: drag.base,
                    edit: edit.clone(),
                });
                if ui.is_mouse_released(MouseButton::Left) {
                    if let SequenceEdit::Transition { transition } = &edit {
                        let old = drag
                            .original
                            .transitions
                            .iter()
                            .find(|t| t.id == drag.id)
                            .unwrap();
                        if transition.start != old.start || transition.duration != old.duration {
                            actions.push(Action::Commit {
                                base: drag.base,
                                edit,
                            });
                        }
                    }
                    self.transition_drag = None;
                    actions.push(Action::CancelPreview);
                }
            }
        }

        let mut proposed = None;
        if let Some(drag) = &self.drag {
            ui.set_mouse_cursor(Some(if drag.handle == Handle::Move {
                MouseCursor::ResizeAll
            } else {
                MouseCursor::ResizeEW
            }));
            let source = drag
                .original
                .clips
                .iter()
                .find(|c| c.id == drag.clip)
                .unwrap();
            let kind = drag.original.track(source.track).unwrap().kind;
            let new_lane =
                drag.handle == Handle::Move && hovered_row.is_none() && layout.body.contains(point);
            let new_kind = if point[1] < layout.row_rect(0, self.view.scroll_y).min[1] {
                TrackKind::Video
            } else {
                TrackKind::Audio
            };
            let track = if new_lane {
                drag.new_track
            } else {
                hovered_row.map(|(_, t)| t.id).unwrap_or(source.track)
            };
            let playhead = interaction::time(i64::from(model.playhead), sequence.rate).unwrap();
            let snap = (self.snapping && !ui.io().key_shift()).then_some((
                playhead,
                7.0 * f64::from(scale) / self.view.pixels_per_frame,
            ));
            match drag.proposal(
                self.view.frame_at(point[0], layout.body.min[0]),
                track,
                snap,
                new_lane.then_some(new_kind),
            ) {
                Ok(edit) => {
                    match drag.original.edited(&edit) {
                        Ok(sequence) => {
                            let clip = sequence.clips.iter().find(|c| c.id == drag.clip).unwrap();
                            self.message = format!(
                                "{}{}  ·  {} → {}  ·  Duration {}",
                                if new_lane {
                                    if kind == TrackKind::Video {
                                        "New video track · "
                                    } else {
                                        "New audio track · "
                                    }
                                } else {
                                    ""
                                },
                                if drag.handle == Handle::Move {
                                    format!("{:?}", drag.mode)
                                } else {
                                    format!("{:?} trim", drag.trim)
                                },
                                interaction::timecode(
                                    interaction::frame(clip.start, sequence.rate).round() as i64,
                                    sequence.rate
                                ),
                                interaction::timecode(
                                    interaction::frame(clip.end().unwrap(), sequence.rate).round()
                                        as i64,
                                    sequence.rate
                                ),
                                interaction::timecode(
                                    interaction::frame(clip.duration, sequence.rate).round() as i64,
                                    sequence.rate
                                )
                            );
                            preview = Some(sequence);
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
        if hovered
            && self.drag.is_none()
            && self.transition_drag.is_none()
            && let Some((index, track)) = hovered_row
        {
            let row = layout.row_rect(index, self.view.scroll_y);
            for transition in &sequence.transitions {
                if !sequence
                    .clips
                    .iter()
                    .any(|c| c.id == transition.outgoing && c.track == track.id)
                {
                    continue;
                }
                let rect = Rect {
                    min: [
                        self.view.x(
                            interaction::frame(transition.start, sequence.rate),
                            layout.body.min[0],
                        ),
                        row.max[1] - 18. * scale,
                    ],
                    max: [
                        self.view.x(
                            interaction::frame(transition.end().unwrap(), sequence.rate),
                            layout.body.min[0],
                        ),
                        row.max[1] - 3. * scale,
                    ],
                };
                if rect.contains(point) {
                    ui.set_mouse_cursor(Some(
                        if interaction::handle(rect, point, scale) == Handle::Move {
                            MouseCursor::ResizeAll
                        } else {
                            MouseCursor::ResizeEW
                        },
                    ));
                    break;
                }
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
        if layout.ruler.contains(point) && point[1] > layout.ruler.max[1] - 6. {
            ui.tooltip_text("Preview cache · green: ready at linked viewer settings · amber: other resolution/crop");
        }
        if !self.message.is_empty() {
            let draw = ui.get_window_draw_list();
            let text = &self.message;
            let extent = ui.calc_text_size(text);
            let p = [
                layout.body.min[0] + 8.,
                layout.body.max[1] - extent[1] - 10.,
            ];
            draw.add_rect(
                [p[0] - 4., p[1] - 3.],
                [p[0] + extent[0] + 4., p[1] + extent[1] + 3.],
                EditorColors::from_ui(ui).header,
            )
            .filled(true)
            .build();
            draw.add_text(p, EditorColors::from_ui(ui).text, text);
        }
        if let Some(pointer) = self.external_pointer.filter(|p| layout.body.contains(*p)) {
            let draw = ui.get_window_draw_list();
            draw.add_line(
                [pointer[0], layout.body.min[1]],
                [pointer[0], layout.body.max[1]],
                ui.style_color(fold_ui::sdk::imgui::StyleColor::DragDropTarget),
            )
            .thickness(2.)
            .build();
        }

        actions
    }
}

impl Canvas {
    pub fn native_placement(
        &self,
        document: fold_foundation::DocumentId,
        sequence: &Sequence,
        point: [f32; 2],
        playhead: u32,
    ) -> Option<fold_platform::browser::Placement> {
        let layout = self.drop_layout?;
        if !layout.body.contains(point) {
            return None;
        }
        let tracks = rows(sequence);
        let hovered = tracks
            .iter()
            .enumerate()
            .find(|(i, _)| layout.row_rect(*i, self.view.scroll_y).contains(point));
        let mut at = interaction::time(
            self.view
                .frame_at(point[0], layout.body.min[0])
                .round()
                .max(0.) as i64,
            sequence.rate,
        )
        .ok()?;
        if self.snapping
            && let Some(offset) = interaction::snap_offset(
                sequence,
                &[],
                &[at],
                interaction::time(i64::from(playhead), sequence.rate).ok()?,
                7. * f64::from(layout.scale) / self.view.pixels_per_frame,
            )
            .ok()
            .flatten()
        {
            at = at.checked_add(offset).ok()?;
        }
        let mut request = fold_ui::sdk::project_drop::placement(document, at);
        request.track = hovered.map(|(_, t)| t.id);
        request.new_track = hovered.is_none();
        request.audio_only = hovered.is_some_and(|(_, t)| t.kind == TrackKind::Audio)
            || point[1] >= layout.row_rect(tracks.len(), self.view.scroll_y).min[1];
        request.mode = match self.mode {
            crate::EditMode::Move => fold_platform::browser::PlacementMode::Place,
            crate::EditMode::Insert => fold_platform::browser::PlacementMode::Insert,
            crate::EditMode::Overwrite => fold_platform::browser::PlacementMode::Overwrite,
        };
        Some(request)
    }
}
