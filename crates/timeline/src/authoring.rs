//! Cancellable import proposals. Media inspection is worker-only; project state
//! is never mutated here. Existing tracks, clips, and extension fields survive.
use crate::{Clip, SEQUENCE, SEQUENCE_SCHEMA, Sequence, SourceMedia, Track, TrackKind};
use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_project::{Asset, DocumentRef, EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn has_sequence(snapshot: &Snapshot) -> bool {
    snapshot.state().documents.values().any(|d| {
        d.package_id == crate::PACKAGE
            && d.type_id == SEQUENCE
            && d.schema_version == SEQUENCE_SCHEMA
    })
}
pub fn active(snapshot: &Snapshot) -> Result<(DocumentRef, Sequence), String> {
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| {
            d.package_id == crate::PACKAGE
                && d.type_id == SEQUENCE
                && d.schema_version == SEQUENCE_SCHEMA
        })
        .ok_or("no editable multi-track sequence")?;
    Ok((
        DocumentRef {
            document: document.id,
            output: "video".into(),
            extensions: Default::default(),
        },
        Sequence::from_document(document)?,
    ))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportArgs {
    pub document: Option<DocumentId>,
    pub paths: Vec<PathBuf>,
    pub at: Option<Time>,
    pub video_track: Option<ObjectId>,
    pub audio_track: Option<ObjectId>,
    pub audio_only: bool,
}
pub fn import(
    snapshot: &Snapshot,
    paths: &[PathBuf],
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    import_media(
        snapshot,
        &ImportArgs {
            document: active(snapshot).ok().map(|(r, _)| r.document),
            paths: paths.to_vec(),
            at: None,
            video_track: None,
            audio_track: None,
            audio_only: false,
        },
        cancel,
    )
}
pub fn import_media(
    snapshot: &Snapshot,
    args: &ImportArgs,
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    if args.paths.is_empty() || args.paths.len() > 32 {
        return Err("import requires 1–32 MP4/WAVE files".into());
    }
    let previous = args
        .document
        .map(|id| {
            snapshot
                .state()
                .documents
                .get(&id)
                .ok_or("missing destination sequence")
        })
        .transpose()?;
    let mut sequence = previous
        .map(|d| Sequence::from_document(d))
        .transpose()?
        .unwrap_or_else(|| Sequence {
            dimensions: [640, 360],
            rate: [24, 1],
            tracks: vec![
                Track::new(TrackKind::Video, "V1"),
                Track::new(TrackKind::Video, "V2"),
                Track::new(TrackKind::Audio, "A1"),
                Track::new(TrackKind::Audio, "A2"),
            ],
            clips: vec![],
            extensions: Default::default(),
        });
    let mut start = args.at.unwrap_or(sequence.end()?);
    let mut mutations = Vec::new();
    for (index, path) in args.paths.iter().enumerate() {
        cancel.check()?;
        let (info, path, fingerprint) = if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        {
            let source = fold_media::inspect_wave(path, cancel)?;
            (
                SourceMedia::Audio(source.info),
                source.path,
                source.fingerprint,
            )
        } else {
            let source = fold_media::inspect(path, cancel)?;
            if previous.is_none() && index == 0 {
                sequence.dimensions = [source.info.width, source.info.height];
                sequence.rate = source.info.rate;
            }
            (
                SourceMedia::Video(source.info),
                source.path,
                source.fingerprint,
            )
        };
        let has_audio =
            matches!(info, SourceMedia::Audio(_)) || fold_media::audio::has_audio(&path, cancel)?;
        let audio_only = args.audio_only || matches!(info, SourceMedia::Audio(_));
        if audio_only && !has_audio {
            return Err("source has no audio stream".into());
        }
        let asset = AssetId::new();
        let duration = info.duration()?;
        let link = if !audio_only && has_audio {
            Some(ObjectId::new())
        } else {
            None
        };
        for kind in [TrackKind::Video, TrackKind::Audio] {
            if (kind == TrackKind::Video && audio_only) || (kind == TrackKind::Audio && !has_audio)
            {
                continue;
            }
            let target = if kind == TrackKind::Video {
                args.video_track
            } else {
                args.audio_track
            };
            let track = if let Some(id) = target {
                sequence.track(id)?
            } else {
                sequence
                    .tracks
                    .iter()
                    .find(|t| t.kind == kind)
                    .ok_or("add a destination track first")?
            };
            if track.kind != kind || track.locked {
                return Err("import destination is locked or incompatible".into());
            }
            sequence.clips.push(Clip {
                id: ObjectId::new(),
                track: track.id,
                link,
                asset,
                info: info.clone(),
                start,
                source_start: Time::ZERO,
                duration,
                level: 1.0,
                extensions: Default::default(),
            });
        }
        mutations.push(Mutation::PutAsset(Asset {
            id: asset,
            location: path.to_str().ok_or("media path must be UTF-8")?.into(),
            fingerprint,
            extensions: Default::default(),
        }));
        start = start.checked_add(duration).map_err(|e| e.to_string())?;
    }
    let mut document = sequence.document(args.document.unwrap_or_default())?;
    if let Some(previous) = previous {
        document.extensions = previous.extensions.clone();
    }
    mutations.push(Mutation::PutDocument(document));
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations,
    })
}
