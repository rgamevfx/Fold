//! Transition intervals own timing; source samples continue in their clip clocks.
use crate::{Sequence, SourceMedia, TrackKind};
use fold_foundation::{ObjectId, Time};
use fold_platform::effects::Instance;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Transition {
    pub id: ObjectId,
    pub outgoing: ObjectId,
    pub incoming: ObjectId,
    pub start: Time,
    pub duration: Time,
    pub effect: Instance,
}
impl Transition {
    pub fn end(&self) -> Result<Time, String> {
        self.start
            .checked_add(self.duration)
            .map_err(|e| e.to_string())
    }
    pub fn active(&self, time: Time) -> Result<bool, String> {
        Ok(time >= self.start && time < self.end()?)
    }
}
impl Sequence {
    pub(crate) fn validate_transitions(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        if self.transitions.len() > 4096 {
            return Err("Transition budget exceeded".into());
        }
        for transition in &self.transitions {
            transition.effect.validate()?;
            let a = self
                .clips
                .iter()
                .find(|c| c.id == transition.outgoing)
                .ok_or("Missing outgoing clip")?;
            let b = self
                .clips
                .iter()
                .find(|c| c.id == transition.incoming)
                .ok_or("Missing incoming clip")?;
            let cut = a.end()?;
            let end = transition.end()?;
            if !ids.insert(transition.id)
                || self.clips.iter().any(|c| c.id == transition.id)
                || matches!(a.info, SourceMedia::Adjustment)
                || matches!(b.info, SourceMedia::Adjustment)
                || a.id == b.id
                || a.track != b.track
                || self.track(a.track)?.kind != TrackKind::Video
                || cut != b.start
                || transition.duration <= Time::ZERO
                || transition.start < a.start
                || transition.start > cut
                || end < cut
                || end > b.end()?
            {
                return Err("Transition requires adjacent video clips and a positive interval containing their cut".into());
            }
            for clip in [a, b] {
                let first = clip
                    .source_start
                    .checked_add(
                        transition
                            .start
                            .checked_sub(clip.start)
                            .map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                let last = clip
                    .source_start
                    .checked_add(end.checked_sub(clip.start).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                if first < Time::ZERO
                    || last > clip.info.duration()?
                    || matches!(clip.info, SourceMedia::Audio(_))
                {
                    return Err(
                        "Insufficient source handles for transition; shorten or realign it".into(),
                    );
                }
            }
            for other in &self.transitions {
                if other.id != transition.id
                    && self
                        .clips
                        .iter()
                        .find(|c| c.id == other.outgoing)
                        .is_some_and(|c| c.track == a.track)
                    && transition.start < other.end()?
                    && other.start < end
                {
                    return Err("Transitions on the same track cannot overlap".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clip, Track};
    use fold_foundation::{AssetId, DocumentId};
    fn t(n: i64, d: u32) -> Time {
        Time::new(n, d).unwrap()
    }
    fn fixture() -> Sequence {
        let track = Track::new(TrackKind::Video, "V1");
        let info = fold_media::VideoInfo {
            width: 2,
            height: 2,
            rate: [24, 1],
            frames: 96,
        };
        let clips: Vec<_> = (0..2)
            .map(|i| Clip {
                id: ObjectId::new(),
                track: track.id,
                link: None,
                asset: AssetId::new(),
                info: SourceMedia::Video(info.clone()),
                start: t(i, 1),
                source_start: t(1, 1),
                duration: t(1, 1),
                level: 1.,
                effects: vec![],
                extensions: Default::default(),
            })
            .collect();
        let transition = Transition {
            id: ObjectId::new(),
            outgoing: clips[0].id,
            incoming: clips[1].id,
            start: t(1, 2),
            duration: t(1, 1),
            effect: Instance {
                id: ObjectId::new(),
                definition: fold_project::DocumentRef {
                    document: DocumentId::new(),
                    output: "video".into(),
                    extensions: Default::default(),
                },
                version: 1,
                values: Default::default(),
                animation: Default::default(),
                enabled: true,
                extensions: Default::default(),
            },
        };
        Sequence {
            dimensions: [2, 2],
            rate: [24, 1],
            tracks: vec![track],
            clips,
            transitions: vec![transition],
            extensions: Default::default(),
        }
    }
    #[test]
    fn handles_and_half_open_interval_are_explicit() {
        let mut sequence = fixture();
        sequence.validate().unwrap();
        assert!(sequence.transitions[0].active(t(1, 2)).unwrap());
        assert!(!sequence.transitions[0].active(t(3, 2)).unwrap());
        sequence.clips[1].source_start = Time::ZERO;
        assert!(sequence.validate().unwrap_err().contains("handles"));
    }
    #[test]
    fn locked_transition_edits_reject_without_changing_source_timing() {
        let mut sequence = fixture();
        let transition = sequence.transitions[0].clone();
        sequence.tracks[0].locked = true;
        assert!(
            sequence
                .edited(&crate::SequenceEdit::Transition { transition })
                .is_err()
        );
        assert_eq!(sequence.clips[1].source_start, t(1, 1));
    }
    #[test]
    fn transitions_follow_moved_cuts_and_split_outgoing_segments() {
        let sequence = fixture();
        let moved = sequence
            .edited(&crate::SequenceEdit::Selection {
                ids: sequence.clips.iter().map(|c| c.id).collect(),
                linked: false,
                edit: crate::SelectionEdit::Move {
                    delta: t(2, 1),
                    anchor: sequence.clips[0].id,
                    track: sequence.tracks[0].id,
                    new_track: None,
                    mode: crate::EditMode::Move,
                },
            })
            .unwrap();
        assert_eq!(moved.transitions[0].start, t(5, 2));
        let right = ObjectId::new();
        let split = sequence
            .edited(&crate::SequenceEdit::Clip {
                id: sequence.clips[0].id,
                linked: false,
                edit: crate::ClipEdit::Split {
                    at: t(1, 4),
                    right_id: right,
                },
            })
            .unwrap();
        assert_eq!(split.transitions[0].outgoing, right);
        assert_eq!(split.transitions[0].start, t(1, 2));
    }
}
