//! Atomic multi-clip editing. Preview and commit use the same proposals.
use crate::{ClipEdit, Sequence, Track, TrackKind};
use fold_foundation::{ObjectId, Time};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub enum EditMode {
    #[default]
    Move,
    Insert,
    Overwrite,
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub enum SelectionEdit {
    Move {
        delta: Time,
        anchor: ObjectId,
        track: ObjectId,
        new_track: Option<TrackKind>,
        mode: EditMode,
    },
    Lift,
    RippleDelete,
    Split {
        at: Time,
    },
    RippleTrim {
        id: ObjectId,
        start: Time,
        end: Time,
    },
    Roll {
        id: ObjectId,
        end: Time,
    },
}
impl Sequence {
    pub(crate) fn apply_selection(
        &mut self,
        selected: &[ObjectId],
        linked: bool,
        edit: &SelectionEdit,
    ) -> Result<(), String> {
        let order: std::collections::BTreeMap<_, _> = self
            .clips
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect();
        if let SelectionEdit::RippleTrim { id, .. } = edit
            && (selected.len() != 1 || selected[0] != *id)
        {
            return Err("Ripple trim requires one selected boundary".into());
        }
        if matches!(edit, SelectionEdit::Roll { .. }) && selected.len() != 1 {
            return Err("Roll requires one selected boundary".into());
        }
        let mut ids = BTreeSet::new();
        for id in selected {
            ids.extend(self.linked_ids(*id, linked)?);
        }
        if ids.is_empty() {
            return Err("Select clips first".into());
        }
        for clip in self.clips.iter().filter(|c| ids.contains(&c.id)) {
            for id in self.linked_ids(clip.id, true)? {
                let member = self.clips.iter().find(|c| c.id == id).unwrap();
                if self.track(member.track)?.locked {
                    return Err("Edit touches a locked track".into());
                }
            }
        }
        if !linked {
            let links: BTreeSet<_> = self
                .clips
                .iter()
                .filter(|c| ids.contains(&c.id))
                .filter_map(|c| c.link)
                .collect();
            for c in &mut self.clips {
                if c.link.is_some_and(|l| links.contains(&l)) {
                    c.link = None;
                }
            }
        }
        match edit {
            SelectionEdit::Lift => self.clips.retain(|c| !ids.contains(&c.id)),
            SelectionEdit::Split { at } => {
                let selected: Vec<_> = self
                    .clips
                    .iter()
                    .filter(|c| {
                        ids.contains(&c.id) && c.start < *at && c.end().is_ok_and(|e| e > *at)
                    })
                    .map(|c| c.id)
                    .collect();
                self.split_members(&selected, *at)?;
            }
            SelectionEdit::RippleDelete => {
                let mut ranges: Vec<_> = self
                    .clips
                    .iter()
                    .filter(|c| ids.contains(&c.id))
                    .map(|c| Ok((c.start, c.end()?)))
                    .collect::<Result<_, String>>()?;
                ranges.sort();
                let mut merged: Vec<(Time, Time)> = vec![];
                for (a, b) in ranges {
                    if let Some(last) = merged.last_mut()
                        && a <= last.1
                    {
                        last.1 = last.1.max(b);
                    } else {
                        merged.push((a, b));
                    }
                }
                self.clips.retain(|c| !ids.contains(&c.id));
                for (a, b) in merged.into_iter().rev() {
                    if self
                        .clips
                        .iter()
                        .any(|c| c.start < b && c.end().is_ok_and(|e| e > a))
                    {
                        return Err(
                            "Ripple delete would cut unselected material; select it or use Lift"
                                .into(),
                        );
                    }
                    self.shift_after(
                        b,
                        a.checked_sub(b).map_err(|e| e.to_string())?,
                        &BTreeSet::new(),
                    )?;
                }
            }
            SelectionEdit::Move {
                delta,
                anchor,
                track,
                new_track,
                mode,
            } => {
                let original = self
                    .clips
                    .iter()
                    .find(|c| c.id == *anchor && ids.contains(&c.id))
                    .ok_or("Missing selection anchor")?
                    .clone();
                let kind = self.track(original.track)?.kind;
                if let Some(new_kind) = new_track {
                    if *new_kind != kind {
                        return Err("Incompatible track type".into());
                    }
                    let mut new = Track::new(
                        kind,
                        format!(
                            "{}{}",
                            if kind == TrackKind::Video { "V" } else { "A" },
                            self.tracks.iter().filter(|t| t.kind == kind).count() + 1
                        ),
                    );
                    new.id = *track;
                    self.tracks.push(new);
                }
                if self.track(*track)?.kind != kind || self.track(*track)?.locked {
                    return Err("Destination track is locked or incompatible".into());
                }
                let tracks: Vec<_> = self
                    .tracks
                    .iter()
                    .filter(|t| t.kind == kind)
                    .map(|t| t.id)
                    .collect();
                let offset = tracks.iter().position(|t| t == track).unwrap() as isize
                    - tracks.iter().position(|t| *t == original.track).unwrap() as isize;
                let mut moved: Vec<_> = self
                    .clips
                    .iter()
                    .filter(|c| ids.contains(&c.id))
                    .cloned()
                    .collect();
                for c in &mut moved {
                    c.start = c.start.checked_add(*delta).map_err(|e| e.to_string())?;
                    if self.track(c.track)?.kind == kind {
                        let index =
                            tracks.iter().position(|t| *t == c.track).unwrap() as isize + offset;
                        c.track = *tracks
                            .get(
                                usize::try_from(index)
                                    .map_err(|_| "No destination for selected track")?,
                            )
                            .ok_or("No destination for selected track")?;
                        if self.track(c.track)?.locked {
                            return Err("Destination track is locked".into());
                        }
                    }
                }
                self.clips.retain(|c| !ids.contains(&c.id));
                if *mode == EditMode::Insert {
                    let start = moved.iter().map(|c| c.start).min().unwrap();
                    let end = moved
                        .iter()
                        .map(|c| c.end())
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .max()
                        .unwrap();
                    let all: Vec<_> = self.clips.iter().map(|c| c.id).collect();
                    self.split_members(&all, start)?;
                    self.shift_after(
                        start,
                        end.checked_sub(start).map_err(|e| e.to_string())?,
                        &BTreeSet::new(),
                    )?;
                } else if *mode == EditMode::Overwrite {
                    for c in &moved {
                        self.overwrite_range(c.track, c.start, c.end()?)?;
                    }
                }
                self.clips.extend(moved);
            }
            SelectionEdit::RippleTrim { id, start, end } => {
                let original = self
                    .clips
                    .iter()
                    .find(|c| c.id == *id)
                    .ok_or("Missing clip")?
                    .clone();
                self.apply_clip(
                    *id,
                    linked,
                    &ClipEdit::Trim {
                        start: *start,
                        end: *end,
                    },
                )?;
                let delta = end
                    .checked_sub(*start)
                    .and_then(|duration| duration.checked_sub(original.duration))
                    .map_err(|e| e.to_string())?;
                // Ripple in-trim changes source in while keeping the sequence cut anchored.
                for c in self.clips.iter_mut().filter(|c| ids.contains(&c.id)) {
                    c.start = original.start;
                }
                self.shift_after(original.end()?, delta, &ids)?;
            }
            SelectionEdit::Roll { id, end } => {
                let left = self
                    .clips
                    .iter()
                    .find(|c| c.id == *id)
                    .ok_or("Missing clip")?
                    .clone();
                let right = self
                    .clips
                    .iter()
                    .find(|c| c.track == left.track && c.start == left.end().unwrap())
                    .ok_or("Roll requires an adjacent clip")?
                    .clone();
                self.apply_clip(
                    *id,
                    linked,
                    &ClipEdit::Trim {
                        start: left.start,
                        end: *end,
                    },
                )?;
                self.apply_clip(
                    right.id,
                    linked,
                    &ClipEdit::Trim {
                        start: *end,
                        end: right.end()?,
                    },
                )?;
            }
        }
        self.clips
            .sort_by_key(|c| order.get(&c.id).copied().unwrap_or(usize::MAX));
        Ok(())
    }
    fn split_members(&mut self, selected: &[ObjectId], at: Time) -> Result<(), String> {
        let mut done = BTreeSet::new();
        for id in selected {
            if done.contains(id) {
                continue;
            }
            let c = self
                .clips
                .iter()
                .find(|c| c.id == *id)
                .ok_or("Missing clip")?;
            if c.start < at && c.end()? > at {
                done.extend(self.linked_ids(*id, true)?);
                self.apply_clip(
                    *id,
                    true,
                    &ClipEdit::Split {
                        at,
                        right_id: ObjectId::new(),
                    },
                )?;
            }
        }
        Ok(())
    }
    fn shift_after(
        &mut self,
        at: Time,
        delta: Time,
        excluded: &BTreeSet<ObjectId>,
    ) -> Result<(), String> {
        if delta == Time::ZERO {
            return Ok(());
        }
        for c in self
            .clips
            .iter()
            .filter(|c| !excluded.contains(&c.id) && c.start >= at)
        {
            if self.track(c.track)?.locked {
                return Err("Ripple edit touches a locked track".into());
            }
        }
        for c in self
            .clips
            .iter_mut()
            .filter(|c| !excluded.contains(&c.id) && c.start >= at)
        {
            c.start = c.start.checked_add(delta).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn overwrite_range(&mut self, track: ObjectId, start: Time, end: Time) -> Result<(), String> {
        for boundary in [start, end] {
            let ids: Vec<_> = self
                .clips
                .iter()
                .filter(|c| c.track == track)
                .map(|c| c.id)
                .collect();
            self.split_members(&ids, boundary)?;
        }
        let mut remove = BTreeSet::new();
        for c in self
            .clips
            .iter()
            .filter(|c| c.track == track && c.start >= start && c.start < end)
        {
            remove.extend(self.linked_ids(c.id, true)?);
        }
        for c in self.clips.iter().filter(|c| remove.contains(&c.id)) {
            if self.track(c.track)?.locked {
                return Err("Overwrite touches a locked track".into());
            }
        }
        self.clips.retain(|c| !remove.contains(&c.id));
        Ok(())
    }
}
