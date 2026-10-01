//! Timeline-owned multi-track authoring. Track order is bottom-to-top for
//! video; audio tracks mix. Clips on one track may touch but never overlap.
use fold_foundation::{AssetId, DocumentId, ObjectId, Rounding, Time};
use fold_media::{VideoInfo, WaveInfo};
use fold_project::{Document, Metadata, Revision};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SEQUENCE: &str = "fold.timeline.sequence";
pub const SEQUENCE_SCHEMA: u32 = 2;
const MAX_PAYLOAD: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub id: ObjectId,
    pub kind: TrackKind,
    pub name: String,
    pub enabled: bool,
    pub solo: bool,
    pub locked: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Track {
    pub fn new(kind: TrackKind, name: impl Into<String>) -> Self {
        Self {
            id: ObjectId::new(),
            kind,
            name: name.into(),
            enabled: true,
            solo: false,
            locked: false,
            extensions: Metadata::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceMedia {
    Video(VideoInfo),
    Audio(WaveInfo),
}
impl SourceMedia {
    pub fn duration(&self) -> Result<Time, String> {
        match self {
            Self::Video(info) => {
                info.validate()?;
                info.time(info.frames)
            }
            Self::Audio(info) => info.duration(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: ObjectId,
    pub track: ObjectId,
    /// A link contains exactly one video and one audio clip with identical timing.
    pub link: Option<ObjectId>,
    pub asset: AssetId,
    pub info: SourceMedia,
    pub start: Time,
    pub source_start: Time,
    pub duration: Time,
    /// Video opacity or audio gain, depending on the destination track.
    pub level: f32,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Clip {
    pub fn end(&self) -> Result<Time, String> {
        self.start
            .checked_add(self.duration)
            .map_err(|e| e.to_string())
    }
    pub fn source_time(&self, time: Time) -> Result<Option<Time>, String> {
        if time < self.start || time >= self.end()? {
            return Ok(None);
        }
        time.checked_sub(self.start)
            .and_then(|offset| self.source_start.checked_add(offset))
            .map(Some)
            .map_err(|e| e.to_string())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sequence {
    pub dimensions: [u32; 2],
    pub rate: [u32; 2],
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
fn unshadowed(extensions: &Metadata, fields: &[&str]) -> Result<(), String> {
    if fields.iter().any(|field| extensions.contains_key(*field)) {
        return Err("extensions cannot shadow sequence fields".into());
    }
    Ok(())
}
impl Sequence {
    pub fn end(&self) -> Result<Time, String> {
        self.clips
            .iter()
            .try_fold(Time::ZERO, |end, clip| Ok(end.max(clip.end()?)))
    }
    pub fn info(&self) -> Result<VideoInfo, String> {
        let frames = self
            .end()?
            .to_ticks(self.rate[0], self.rate[1], Rounding::Ceil)
            .map_err(|e| e.to_string())?;
        let info = VideoInfo {
            width: self.dimensions[0],
            height: self.dimensions[1],
            rate: self.rate,
            frames: u32::try_from(frames.max(1)).map_err(|e| e.to_string())?,
        };
        info.validate()?;
        if info.time(info.frames)? > Time::new(600, 1).unwrap() {
            return Err("editable sequences are limited to ten minutes".into());
        }
        Ok(info)
    }
    pub fn track(&self, id: ObjectId) -> Result<&Track, String> {
        self.tracks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| "missing track".into())
    }
    pub fn audible(&self, track: &Track) -> bool {
        track.kind == TrackKind::Audio
            && track.enabled
            && (track.solo
                || !self
                    .tracks
                    .iter()
                    .any(|t| t.kind == TrackKind::Audio && t.solo))
    }
    pub fn linked_ids(&self, id: ObjectId, linked: bool) -> Result<Vec<ObjectId>, String> {
        let clip = self
            .clips
            .iter()
            .find(|c| c.id == id)
            .ok_or("missing clip")?;
        let mut ids = vec![id];
        ids.extend(
            self.clips
                .iter()
                .filter(|c| c.id != id && linked && clip.link.is_some() && c.link == clip.link)
                .map(|c| c.id),
        );
        Ok(ids)
    }
    pub fn validate(&self) -> Result<(), String> {
        unshadowed(&self.extensions, &["clips", "tracks", "dimensions", "rate"])?;
        if self.clips.len() > 4096 || self.tracks.len() > 32 {
            return Err("sequence exceeds 4096 clips / 32 tracks".into());
        }
        self.info()?;
        let mut ids = BTreeSet::new();
        for track in &self.tracks {
            unshadowed(
                &track.extensions,
                &["id", "kind", "name", "enabled", "solo", "locked"],
            )?;
            if !ids.insert(track.id)
                || track.name.is_empty()
                || track.name.len() > 128
                || (track.kind == TrackKind::Video && track.solo)
            {
                return Err("invalid track identity/name/solo".into());
            }
        }
        let mut links: BTreeMap<ObjectId, Vec<&Clip>> = BTreeMap::new();
        for clip in &self.clips {
            unshadowed(
                &clip.extensions,
                &[
                    "id",
                    "track",
                    "link",
                    "asset",
                    "info",
                    "start",
                    "source_start",
                    "duration",
                    "level",
                ],
            )?;
            let track = self.track(clip.track)?;
            let source_end = clip
                .source_start
                .checked_add(clip.duration)
                .map_err(|e| e.to_string())?;
            let source_duration = clip.info.duration()?;
            if !ids.insert(clip.id)
                || clip.start < Time::ZERO
                || clip.source_start < Time::ZERO
                || clip.duration <= Time::ZERO
                || source_end > source_duration
                || source_duration > Time::new(600, 1).unwrap()
                || !clip.level.is_finite()
                || !(0.0..=1.0).contains(&clip.level)
                || (track.kind == TrackKind::Video && matches!(clip.info, SourceMedia::Audio(_)))
            {
                return Err("invalid clip identity, placement, source range, or level".into());
            }
            if let Some(link) = clip.link {
                links.entry(link).or_default().push(clip);
            }
        }
        for track in &self.tracks {
            let mut clips: Vec<_> = self.clips.iter().filter(|c| c.track == track.id).collect();
            clips.sort_by_key(|c| c.start);
            for pair in clips.windows(2) {
                if pair[0].end()? > pair[1].start {
                    return Err(format!("overlap on {}", track.name));
                }
            }
        }
        for (id, clips) in links {
            if ids.contains(&id) || clips.len() != 2 {
                return Err("invalid A/V link identity or membership".into());
            }
            let (a, b) = (clips[0], clips[1]);
            if self.track(a.track)?.kind == self.track(b.track)?.kind
                || a.asset != b.asset
                || a.info != b.info
                || a.start != b.start
                || a.source_start != b.source_start
                || a.duration != b.duration
            {
                return Err("linked A/V timing differs; unlink before independent editing".into());
            }
        }
        Ok(())
    }
    fn assets(&self) -> Vec<AssetId> {
        self.clips
            .iter()
            .map(|c| c.asset)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        let payload = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if payload.len() > MAX_PAYLOAD {
            return Err("sequence exceeds 1 MiB payload budget".into());
        }
        Ok(Document {
            id,
            type_id: SEQUENCE.into(),
            package_id: super::PACKAGE.into(),
            schema_version: SEQUENCE_SCHEMA,
            revision: Revision::default(),
            dependencies: vec![],
            assets: self.assets(),
            payload,
            extensions: Metadata::default(),
        })
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        if document.package_id != super::PACKAGE
            || document.type_id != SEQUENCE
            || document.schema_version != SEQUENCE_SCHEMA
            || !document.dependencies.is_empty()
            || document.payload.len() > MAX_PAYLOAD
        {
            return Err("unsupported sequence document (multi-track schema 2 required)".into());
        }
        let sequence: Self =
            serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        sequence.validate()?;
        if sequence.assets() != document.assets {
            return Err("sequence asset declarations disagree with payload".into());
        }
        Ok(sequence)
    }
}
