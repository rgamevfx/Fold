//! UI-independent canvas geometry and edit gestures. Tests exercise the same
//! quantization, snapping, and proposals used by the actual ImGui canvas.
use crate::{ClipEdit, Sequence, SequenceEdit};
use fold_foundation::{ObjectId, Time};
use fold_project::Revision;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}
impl Rect {
    pub fn contains(self, p: [f32; 2]) -> bool {
        p[0] >= self.min[0] && p[0] < self.max[0] && p[1] >= self.min[1] && p[1] < self.max[1]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Move,
    Left,
    Right,
}
pub use fold_ui::sdk::time_view::{View, frame, ruler_step, time, timecode};
pub fn handle(rect: Rect, point: [f32; 2], scale: f32) -> Handle {
    let edge = (6.0 * scale).min((rect.max[0] - rect.min[0]) / 3.0);
    if point[0] - rect.min[0] < edge {
        Handle::Left
    } else if rect.max[0] - point[0] < edge {
        Handle::Right
    } else {
        Handle::Move
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrimMode {
    #[default]
    Normal,
    Ripple,
    Roll,
}
pub struct Drag {
    pub base: Revision,
    pub original: Sequence,
    pub clip: ObjectId,
    pub linked: bool,
    pub handle: Handle,
    pub grab_frame: f64,
    pub selected: Vec<ObjectId>,
    pub new_track: ObjectId,
    pub mode: crate::EditMode,
    pub trim: TrimMode,
}
impl Drag {
    pub fn proposal(
        &self,
        pointer_frame: f64,
        track: ObjectId,
        snap: Option<(Time, f64)>,
        new_track: Option<crate::TrackKind>,
    ) -> Result<SequenceEdit, String> {
        let clip = self
            .original
            .clips
            .iter()
            .find(|c| c.id == self.clip)
            .ok_or("missing dragged clip")?;
        let delta = time(
            (pointer_frame - self.grab_frame).round() as i64,
            self.original.rate,
        )?;
        let mut start = clip.start;
        let mut end = clip.end()?;
        if self.handle != Handle::Right {
            start = start.checked_add(delta).map_err(|e| e.to_string())?;
        }
        if self.handle != Handle::Left {
            end = end.checked_add(delta).map_err(|e| e.to_string())?;
        }
        if let Some((playhead, tolerance)) = snap {
            let mut excluded = Vec::new();
            for id in &self.selected {
                excluded.extend(self.original.linked_ids(*id, self.linked)?);
            }
            let edges = match self.handle {
                Handle::Left => vec![start],
                Handle::Right => vec![end],
                Handle::Move => vec![start, end],
            };
            if let Some(offset) =
                snap_offset(&self.original, &excluded, &edges, playhead, tolerance)?
            {
                if self.handle != Handle::Right {
                    start = start.checked_add(offset).map_err(|e| e.to_string())?;
                }
                if self.handle != Handle::Left {
                    end = end.checked_add(offset).map_err(|e| e.to_string())?;
                }
            }
        }
        let edit = if self.handle == Handle::Move {
            ClipEdit::MoveTo { start, track }
        } else {
            ClipEdit::Trim { start, end }
        };
        if let ClipEdit::MoveTo { start, track } = edit {
            return Ok(SequenceEdit::Selection {
                ids: self.selected.clone(),
                linked: self.linked,
                edit: crate::SelectionEdit::Move {
                    delta: start.checked_sub(clip.start).map_err(|e| e.to_string())?,
                    anchor: clip.id,
                    track,
                    new_track,
                    mode: self.mode,
                },
            });
        }
        if self.handle != Handle::Move && self.trim != TrimMode::Normal {
            return Ok(SequenceEdit::Selection {
                ids: vec![clip.id],
                linked: self.linked,
                edit: if self.trim == TrimMode::Ripple {
                    crate::SelectionEdit::RippleTrim {
                        id: clip.id,
                        start,
                        end,
                    }
                } else {
                    if self.handle == Handle::Left {
                        let left = self
                            .original
                            .clips
                            .iter()
                            .find(|c| c.track == clip.track && c.end().ok() == Some(clip.start))
                            .ok_or("Roll requires an adjacent clip")?;
                        crate::SelectionEdit::Roll {
                            id: left.id,
                            end: start,
                        }
                    } else {
                        crate::SelectionEdit::Roll { id: clip.id, end }
                    }
                },
            });
        }
        Ok(SequenceEdit::Clip {
            id: self.clip,
            linked: self.linked,
            edit,
        })
    }
}

/// Shared pixel-tolerance snapping for existing clips and Project placement.
pub fn snap_offset(
    sequence: &Sequence,
    excluded: &[ObjectId],
    edges: &[Time],
    playhead: Time,
    tolerance: f64,
) -> Result<Option<Time>, String> {
    let mut anchors = vec![Time::ZERO, playhead];
    for c in sequence.clips.iter().filter(|c| !excluded.contains(&c.id)) {
        anchors.push(c.start);
        anchors.push(c.end()?);
    }
    let mut best: Option<(f64, Time)> = None;
    for edge in edges {
        for anchor in &anchors {
            let offset = anchor.checked_sub(*edge).map_err(|e| e.to_string())?;
            let distance = frame(offset, sequence.rate).abs();
            if distance <= tolerance && best.as_ref().is_none_or(|(d, _)| distance < *d) {
                best = Some((distance, offset));
            }
        }
    }
    Ok(best.map(|(_, offset)| offset))
}

/// A transition gesture only changes its interval; clip clocks stay untouched.
pub struct TransitionDrag {
    pub base: Revision,
    pub original: Sequence,
    pub id: ObjectId,
    pub handle: Handle,
    pub grab_frame: f64,
}
impl TransitionDrag {
    pub fn proposal(&self, pointer: f64) -> Result<SequenceEdit, String> {
        let original = self
            .original
            .transitions
            .iter()
            .find(|t| t.id == self.id)
            .ok_or("Missing transition")?;
        let delta = (pointer - self.grab_frame).round() as i64;
        let proposal = |frames: i64| -> Result<SequenceEdit, String> {
            let delta = time(frames, self.original.rate)?;
            let mut transition = original.clone();
            let mut end = original.end()?;
            if self.handle != Handle::Right {
                transition.start = transition
                    .start
                    .checked_add(delta)
                    .map_err(|e| e.to_string())?;
            }
            if self.handle != Handle::Left {
                end = end.checked_add(delta).map_err(|e| e.to_string())?;
            }
            transition.duration = end
                .checked_sub(transition.start)
                .map_err(|e| e.to_string())?;
            let edit = SequenceEdit::Transition { transition };
            self.original.edited(&edit)?;
            Ok(edit)
        };
        // Valid intervals form a contiguous range around the original. Clamp to
        // source handles, adjacent cuts and neighbouring transitions in O(log n).
        let mut low = 0i64;
        let mut high = delta.saturating_abs();
        while low < high {
            let mid = low + (high - low + 1) / 2;
            if proposal(mid * delta.signum()).is_ok() {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        proposal(low * delta.signum())
    }
}
