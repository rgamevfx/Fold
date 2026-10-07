//! Small primary surface; editing depth lives in menus and shortcuts.
use super::{canvas::Canvas, interaction::TrimMode};
use crate::{EditMode, SelectionEdit, Sequence, SequenceEdit, TrackKind};
use fold_foundation::{ObjectId, Time};
use fold_ui::sdk::{
    imgui::{Key, Ui},
    toolbar::{self, ToolbarIcon},
};

pub fn toolbar(
    ui: &Ui,
    canvas: &mut Canvas,
    sequence: &Sequence,
    selected: &[ObjectId],
    frame: u32,
) -> Option<SequenceEdit> {
    let selected_transition = sequence
        .transitions
        .iter()
        .find(|t| selected.contains(&t.id))
        .map(|t| t.id);
    let selected_clips: Vec<_> = selected
        .iter()
        .copied()
        .filter(|id| sequence.clips.iter().any(|c| c.id == *id))
        .collect();
    let selected = selected_clips.as_slice();
    let toolbar_width = ui.content_region_avail()[0];
    let keyboard = ui.is_window_focused() && !ui.io().want_text_input();
    if keyboard {
        if ui.is_key_pressed(Key::V) {
            canvas.blade = false;
        }
        if ui.is_key_pressed(Key::C) {
            canvas.blade = true;
        }
        if ui.is_key_pressed(Key::N) {
            canvas.snapping = !canvas.snapping;
        }
        if ui.is_key_pressed(Key::F) {
            canvas.fit = true;
        }
    }
    if toolbar::toggle_icon(
        ui,
        "select",
        ToolbarIcon::Select,
        "Select / trim (V)",
        !canvas.blade,
    ) {
        canvas.blade = false;
    }
    ui.same_line();
    if toolbar::toggle_icon(
        ui,
        "blade",
        ToolbarIcon::Blade,
        "Blade (C) — click a clip to split",
        canvas.blade,
    ) {
        canvas.blade = true;
    }
    ui.same_line();
    if toolbar::toggle_icon(
        ui,
        "snap",
        ToolbarIcon::Snap,
        "Snapping (N); hold Shift to bypass",
        canvas.snapping,
    ) {
        canvas.snapping = !canvas.snapping;
    }
    ui.same_line();
    if toolbar::toggle_icon(
        ui,
        "linked",
        ToolbarIcon::Link,
        "Edit linked audio/video together",
        canvas.linked,
    ) {
        canvas.linked = !canvas.linked;
    }
    ui.same_line();
    if toolbar::icon_button(ui, "fit", ToolbarIcon::FrameAll, "Fit sequence (F)") {
        canvas.fit = true;
    }
    ui.same_line();
    let at = super::interaction::time(i64::from(frame), sequence.rate).unwrap_or(Time::ZERO);
    let mut edit = None;
    if let Some(_menu) = toolbar::icon_menu(
        ui,
        "timeline-options",
        "Timeline options and editing commands",
    ) {
        edit = edit_items(ui, canvas, sequence, selected, at);
        ui.separator();
        for kind in [TrackKind::Video, TrackKind::Audio] {
            if ui.menu_item(if kind == TrackKind::Video {
                "Add video track"
            } else {
                "Add audio track"
            }) {
                edit = Some(SequenceEdit::AddTrack { kind });
            }
        }
        ui.separator();
        if let Some(_menu) = ui.begin_menu("Placement behavior") {
            for (label, mode) in [
                ("Move — reject overlaps", EditMode::Move),
                ("Insert — shift following material", EditMode::Insert),
                (
                    "Overwrite — replace destination material",
                    EditMode::Overwrite,
                ),
            ] {
                if ui.menu_item_enabled_selected_no_shortcut(label, canvas.mode == mode, true) {
                    canvas.mode = mode;
                }
            }
        }
        if let Some(_menu) = ui.begin_menu("Trim behavior") {
            for (label, mode) in [
                ("Trim", TrimMode::Normal),
                ("Ripple trim", TrimMode::Ripple),
                ("Roll adjacent edit", TrimMode::Roll),
            ] {
                if ui.menu_item_enabled_selected_no_shortcut(label, canvas.trim == mode, true) {
                    canvas.trim = mode;
                }
            }
        }
        ui.separator();
        for (label, height) in [
            ("Compact tracks", 38.),
            ("Standard tracks", 58.),
            ("Expanded tracks", 88.),
        ] {
            if ui.menu_item(label) {
                canvas.row_height = height;
                canvas.track_heights.clear();
            }
        }
        ui.menu_item_toggle_no_shortcut("Media previews", &mut canvas.media_previews, true);
        if ui.menu_item("Zoom in") {
            canvas.view.zoom(1.3, 0., 0.);
        }
        if ui.menu_item("Zoom out") {
            canvas.view.zoom(1. / 1.3, 0., 0.);
        }
        ui.separator();
        ui.text_disabled("Ctrl+wheel zoom · Shift+wheel pan");
        ui.text_disabled("Shift-click add · drag empty space to select");
        ui.text_disabled("Alt+arrows nudge · Alt+Up/Down change track");
        ui.text_disabled("Double-click nested clips to open source");
    }
    if keyboard
        && ui.is_key_pressed(Key::Delete)
        && let Some(id) = selected_transition
    {
        return Some(SequenceEdit::RemoveTransition { id });
    }
    if keyboard && !selected.is_empty() {
        if ui.is_key_pressed(Key::Delete) {
            edit = Some(selection(
                selected,
                canvas.linked,
                if ui.io().key_shift() {
                    SelectionEdit::RippleDelete
                } else {
                    SelectionEdit::Lift
                },
            ));
        }
        if ui.is_key_pressed(Key::S) || (ui.io().key_ctrl() && ui.is_key_pressed(Key::K)) {
            edit = Some(selection(
                selected,
                canvas.linked,
                SelectionEdit::Split { at },
            ));
        }
        if ui.io().key_alt()
            && let Some(anchor) = selected
                .iter()
                .find_map(|id| sequence.clips.iter().find(|c| c.id == *id))
        {
            let delta = if ui.is_key_pressed(Key::LeftArrow) {
                -1
            } else if ui.is_key_pressed(Key::RightArrow) {
                1
            } else {
                0
            };
            let vertical = if ui.is_key_pressed(Key::UpArrow) {
                1
            } else if ui.is_key_pressed(Key::DownArrow) {
                -1
            } else {
                0
            };
            let kind = sequence.track(anchor.track).unwrap().kind;
            let tracks: Vec<_> = sequence.tracks.iter().filter(|t| t.kind == kind).collect();
            let index = tracks.iter().position(|t| t.id == anchor.track).unwrap() as i32
                + if kind == TrackKind::Video {
                    vertical
                } else {
                    -vertical
                };
            if (delta != 0 || vertical != 0)
                && let Some(track) = usize::try_from(index).ok().and_then(|i| tracks.get(i))
            {
                edit = Some(selection(
                    selected,
                    canvas.linked,
                    SelectionEdit::Move {
                        delta: super::interaction::time(delta, sequence.rate).unwrap(),
                        anchor: anchor.id,
                        track: track.id,
                        new_track: None,
                        mode: EditMode::Move,
                    },
                ));
            }
        }
    }
    if canvas.mode != EditMode::Move || canvas.trim != TrimMode::Normal {
        // Special modes stay visible, while the default needs no label.
        if toolbar_width > 500. {
            ui.same_line();
            ui.text_disabled(format!("{:?} / {:?}", canvas.mode, canvas.trim));
        }
    }
    edit
}
pub fn selection(ids: &[ObjectId], linked: bool, edit: SelectionEdit) -> SequenceEdit {
    SequenceEdit::Selection {
        ids: ids.to_vec(),
        linked,
        edit,
    }
}
pub fn edit_items(
    ui: &Ui,
    canvas: &Canvas,
    sequence: &Sequence,
    ids: &[ObjectId],
    at: Time,
) -> Option<SequenceEdit> {
    let ids: Vec<_> = ids
        .iter()
        .copied()
        .filter(|id| sequence.clips.iter().any(|c| c.id == *id))
        .collect();
    let _disabled = ui.begin_disabled_with_cond(ids.is_empty());
    let mut result = None;
    for (label, shortcut, edit) in [
        ("Split at playhead", "Ctrl+K", SelectionEdit::Split { at }),
        ("Lift", "Delete", SelectionEdit::Lift),
        ("Ripple delete", "Shift+Delete", SelectionEdit::RippleDelete),
    ] {
        if ui.menu_item_with_shortcut(label, shortcut) {
            result = Some(selection(&ids, canvas.linked, edit));
        }
    }
    if ids.len() == 1 && ui.menu_item("Unlink audio and video") {
        result = Some(SequenceEdit::Clip {
            id: ids[0],
            linked: true,
            edit: crate::ClipEdit::Unlink,
        });
    }
    result
}
