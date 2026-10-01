//! Pure edit proposals shared by the canvas, commands, and headless tests.
//! A rejected edit leaves both the input sequence and the project untouched.
use crate::{Sequence, Track, TrackKind};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClipEdit {
    Move { start: Time },
    MoveTo { start: Time, track: ObjectId },
    Trim { start: Time, end: Time },
    Split { at: Time, right_id: ObjectId },
    Lift,
    Unlink,
    Level(f32),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SequenceEdit {
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
        result.validate()?;
        Ok(result)
    }
    fn apply_clip(&mut self, id: ObjectId, linked: bool, edit: &ClipEdit) -> Result<(), String> {
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
                    clip.start = *start;
                    clip.duration = end.checked_sub(*start).map_err(|e| e.to_string())?;
                }
                ClipEdit::Split { at, right_id } => {
                    if *at <= clip.start || *at >= clip.end()? {
                        return Err("split must be strictly inside clip".into());
                    }
                    let mut right = clip.clone();
                    right.id = if clip.id == id {
                        *right_id
                    } else {
                        ObjectId::new()
                    };
                    right.link = new_link;
                    right.start = *at;
                    right.source_start = clip.source_time(*at)?.ok_or("split outside clip")?;
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
