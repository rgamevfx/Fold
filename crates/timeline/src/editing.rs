//! Pure edit proposals shared by the canvas, commands, and headless tests.
//! A rejected edit leaves both the input sequence and the project untouched.
use crate::{Sequence, Track, TrackKind};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ClipEdit {
    Move { start: Time },
    MoveTo { start: Time, track: ObjectId },
    Trim { start: Time, end: Time },
    Split { at: Time, right_id: ObjectId },
    Lift,
    Unlink,
    Level(f32),
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub enum SequenceEdit {
    Transform {
        id: ObjectId,
        transform: crate::Transform,
    },
    AddAdjustment {
        id: ObjectId,
        track: ObjectId,
        start: Time,
        duration: Time,
    },
    Transition {
        transition: crate::Transition,
    },
    RemoveTransition {
        id: ObjectId,
    },
    Effects {
        id: ObjectId,
        effects: Vec<fold_platform::effects::Instance>,
    },
    Selection {
        ids: Vec<ObjectId>,
        linked: bool,
        edit: crate::SelectionEdit,
    },
    RenameTrack {
        id: ObjectId,
        name: String,
    },
    Clip {
        id: ObjectId,
        linked: bool,
        edit: ClipEdit,
    },
    AddTrack {
        kind: TrackKind,
    },
    Track {
        id: ObjectId,
        enabled: bool,
        solo: bool,
        locked: bool,
    },
    RemoveTrack {
        id: ObjectId,
    },
}
impl Sequence {
    pub fn edited(&self, edit: &SequenceEdit) -> Result<Self, String> {
        self.validate()?;
        let mut result = self.clone();
        match edit {
            SequenceEdit::Transform { id, transform } => {
                transform.validate()?;
                let clip = result
                    .clips
                    .iter()
                    .find(|c| c.id == *id)
                    .ok_or("Missing clip")?;
                if result.track(clip.track)?.locked
                    || result.track(clip.track)?.kind != TrackKind::Video
                {
                    return Err("Choose an unlocked video clip".into());
                }
                result
                    .clips
                    .iter_mut()
                    .find(|c| c.id == *id)
                    .unwrap()
                    .extensions
                    .insert(
                        crate::transform::KEY.into(),
                        serde_json::to_value(transform).map_err(|e| e.to_string())?,
                    );
            }
            SequenceEdit::AddAdjustment {
                id,
                track,
                start,
                duration,
            } => {
                if result.track(*track)?.locked || result.track(*track)?.kind != TrackKind::Video {
                    return Err("Choose an unlocked video track for an adjustment".into());
                }
                result.clips.push(crate::Clip {
                    id: *id,
                    track: *track,
                    link: None,
                    asset: fold_foundation::AssetId::new(),
                    info: crate::SourceMedia::Adjustment,
                    start: *start,
                    source_start: Time::ZERO,
                    duration: *duration,
                    level: 1.,
                    effects: vec![],
                    extensions: Default::default(),
                });
            }
            SequenceEdit::Transition { transition } => {
                for id in [transition.outgoing, transition.incoming] {
                    let clip = result
                        .clips
                        .iter()
                        .find(|c| c.id == id)
                        .ok_or("Missing transition clip")?;
                    if result.track(clip.track)?.locked {
                        return Err("Unlock transition clips before editing".into());
                    }
                }
                result.transitions.retain(|t| t.id != transition.id);
                result.transitions.push(transition.clone());
            }
            SequenceEdit::RemoveTransition { id } => {
                let t = result
                    .transitions
                    .iter()
                    .find(|t| t.id == *id)
                    .ok_or("Missing transition")?;
                let clip = result
                    .clips
                    .iter()
                    .find(|c| c.id == t.outgoing)
                    .ok_or("Missing transition clip")?;
                if result.track(clip.track)?.locked {
                    return Err("Unlock transition track before editing".into());
                }
                result.transitions.retain(|t| t.id != *id);
            }

            SequenceEdit::Effects { id, effects } => {
                let clip = result
                    .clips
                    .iter()
                    .find(|c| c.id == *id)
                    .ok_or("Missing clip")?;
                if result.track(clip.track)?.locked {
                    return Err("Unlock the track before editing effects".into());
                }
                result
                    .clips
                    .iter_mut()
                    .find(|c| c.id == *id)
                    .unwrap()
                    .effects = effects.clone();
            }

            SequenceEdit::Selection { ids, linked, edit } => {
                result.apply_selection(ids, *linked, edit)?
            }
            SequenceEdit::RenameTrack { id, name } => {
                result
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == *id)
                    .ok_or("Missing track")?
                    .name = name.trim().into();
            }
            SequenceEdit::AddTrack { kind } => {
                let count = self.tracks.iter().filter(|t| t.kind == *kind).count() + 1;
                result.tracks.push(Track::new(
                    *kind,
                    format!(
                        "{}{}",
                        if *kind == TrackKind::Video { "V" } else { "A" },
                        count
                    ),
                ));
            }
            SequenceEdit::Track {
                id,
                enabled,
                solo,
                locked,
            } => {
                let track = result
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == *id)
                    .ok_or("missing track")?;
                track.enabled = *enabled;
                track.solo = *solo;
                track.locked = *locked;
            }
            SequenceEdit::RemoveTrack { id } => {
                if self.track(*id)?.locked || self.clips.iter().any(|c| c.track == *id) {
                    return Err("only empty unlocked tracks can be removed".into());
                }
                result.tracks.retain(|t| t.id != *id);
            }
            SequenceEdit::Clip { id, linked, edit } => result.apply_clip(*id, *linked, edit)?,
        }
        for clip in &mut result.clips {
            if let Some(before) = self.clips.iter().find(|old| old.id == clip.id)
                && clip.start != before.start
                && clip.source_start == before.source_start
                && clip.duration == before.duration
            {
                let delta = clip
                    .start
                    .checked_sub(before.start)
                    .map_err(|e| e.to_string())?;
                for effect in &mut clip.effects {
                    effect.shift_time(delta)?;
                }
            }
        }
        // Preserve transition alignment when its cut moves. A split of the
        // outgoing clip transfers ownership to the new segment touching the cut.
        for transition in &mut result.transitions {
            let Some(old) = self.transitions.iter().find(|t| t.id == transition.id) else {
                continue;
            };
            let Some(before) = self.clips.iter().find(|c| c.id == old.outgoing) else {
                continue;
            };
            let old_cut = before.end()?;
            if let Some(right) = result.clips.iter().find(|c| {
                c.track == before.track
                    && !self.clips.iter().any(|old_clip| old_clip.id == c.id)
                    && c.end().ok() == Some(old_cut)
                    && c.start > before.start
            }) {
                transition.outgoing = right.id;
            }
            if let (Some(a), Some(b)) = (
                result.clips.iter().find(|c| c.id == transition.outgoing),
                result.clips.iter().find(|c| c.id == transition.incoming),
            ) {
                if a.end()? == b.start {
                    let delta = b.start.checked_sub(old_cut).map_err(|e| e.to_string())?;
                    transition.effect.shift_time(delta)?;
                    transition.start = transition
                        .start
                        .checked_add(delta)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        result.transitions.retain(|t| {
            result.clips.iter().any(|c| c.id == t.outgoing)
                && result.clips.iter().any(|c| c.id == t.incoming)
        });
        result.validate()?;
        Ok(result)
    }
    pub(crate) fn apply_clip(
        &mut self,
        id: ObjectId,
        linked: bool,
        edit: &ClipEdit,
    ) -> Result<(), String> {
        let original = self
            .clips
            .iter()
            .find(|c| c.id == id)
            .ok_or("missing clip")?
            .clone();
        let ids = self.linked_ids(id, linked)?;
        let lock_ids = if !matches!(edit, ClipEdit::Level(_)) && original.link.is_some() {
            self.linked_ids(id, true)?
        } else {
            vec![id]
        };
        for clip in self.clips.iter().filter(|c| lock_ids.contains(&c.id)) {
            if self.track(clip.track)?.locked {
                return Err("linked edit touches a locked track".into());
            }
        }
        // Independent temporal editing explicitly dissolves the link on both
        // sides. Level edits do not alter timing and always affect only one clip.
        if (matches!(edit, ClipEdit::Unlink) || (!linked && !matches!(edit, ClipEdit::Level(_))))
            && let Some(link) = original.link
        {
            for clip in &mut self.clips {
                if clip.link == Some(link) {
                    clip.link = None;
                }
            }
        }
        if let ClipEdit::Level(level) = edit {
            self.clips.iter_mut().find(|c| c.id == id).unwrap().level = *level;
            return Ok(());
        }
        if let ClipEdit::MoveTo { track, .. } = edit
            && (self.track(*track)?.kind != self.track(original.track)?.kind
                || self.track(*track)?.locked)
        {
            return Err("destination track is locked or incompatible".into());
        }
        let new_link = if matches!(edit, ClipEdit::Split { .. }) && ids.len() == 2 {
            Some(ObjectId::new())
        } else {
            None
        };
        let mut right_clips = Vec::new();
        for clip in self.clips.iter_mut().filter(|c| ids.contains(&c.id)) {
            match edit {
                ClipEdit::Move { start } | ClipEdit::MoveTo { start, .. } => {
                    clip.start = clip
                        .start
                        .checked_add(
                            start
                                .checked_sub(original.start)
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                    if clip.id == id
                        && let ClipEdit::MoveTo { track, .. } = edit
                    {
                        clip.track = *track;
                    }
                }
                ClipEdit::Trim { start, end } => {
                    clip.source_start = start
                        .checked_sub(original.start)
                        .and_then(|delta| clip.source_start.checked_add(delta))
                        .map_err(|e| e.to_string())?;
                    if matches!(clip.info, crate::SourceMedia::Adjustment) {
                        clip.source_start = Time::ZERO;
                    }
                    clip.start = *start;
                    clip.duration = end.checked_sub(*start).map_err(|e| e.to_string())?;
                }
                ClipEdit::Split { at, right_id } => {
                    if *at <= clip.start || *at >= clip.end()? {
                        return Err("split must be strictly inside clip".into());
                    }
                    let mut right = clip.clone();
                    for effect in &mut right.effects {
                        effect.id = ObjectId::new();
                    }
                    right.id = if clip.id == id {
                        *right_id
                    } else {
                        ObjectId::new()
                    };
                    right.link = new_link;
                    right.start = *at;
                    right.source_start = clip.source_time(*at)?.ok_or("split outside clip")?;
                    if matches!(right.info, crate::SourceMedia::Adjustment) {
                        right.source_start = Time::ZERO;
                    }
                    right.duration = clip.end()?.checked_sub(*at).map_err(|e| e.to_string())?;
                    clip.duration = at.checked_sub(clip.start).map_err(|e| e.to_string())?;
                    right_clips.push(right);
                }
                ClipEdit::Lift | ClipEdit::Unlink | ClipEdit::Level(_) => {}
            }
        }
        if matches!(edit, ClipEdit::Lift) {
            self.clips.retain(|c| !ids.contains(&c.id));
        }
        self.clips.extend(right_clips);
        Ok(())
    }
}
pub fn edit_sequence(
    snapshot: &Snapshot,
    document: DocumentId,
    edit: &SequenceEdit,
) -> Result<EditBatch, String> {
    let original = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or("missing sequence document")?;
    let sequence = Sequence::from_document(original)?.edited(edit)?;
    let mut replacement = sequence.document(document)?;
    replacement.extensions = original.extensions.clone();
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(replacement)],
    })
}
pub fn edit_clip(
    snapshot: &Snapshot,
    document: DocumentId,
    clip: ObjectId,
    edit: ClipEdit,
) -> Result<EditBatch, String> {
    edit_sequence(
        snapshot,
        document,
        &SequenceEdit::Clip {
            id: clip,
            linked: true,
            edit,
        },
    )
}
