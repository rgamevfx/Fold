//! Placement reuses verified project sources; never probes or registers files.
use crate::{Clip, Sequence, SourceMedia, Track, TrackKind};
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::browser::{Placement, PlacementSource, asset_metadata};
use fold_project::{Document, Snapshot};

pub fn create(id: DocumentId) -> Result<Document, String> {
    Sequence {
        dimensions: [1280, 720],
        rate: [24, 1],
        tracks: vec![
            Track::new(TrackKind::Video, "V1"),
            Track::new(TrackKind::Audio, "A1"),
        ],
        clips: vec![],
        extensions: Default::default(),
    }
    .document(id)
}
pub fn place(
    snapshot: &Snapshot,
    request: &Placement,
    output: Option<fold_media::VideoInfo>,
) -> Result<Document, String> {
    use fold_media::ingest::SourceProfile;
    let old = snapshot
        .state()
        .documents
        .get(&request.target)
        .ok_or("missing sequence")?;
    let mut sequence = Sequence::from_document(old)?;
    let (asset, info, has_audio) = match &request.source {
        PlacementSource::Asset(id) => {
            let metadata = asset_metadata(snapshot, *id)?;
            let audio = metadata.streams.iter().any(|s| s.kind == "audio");
            let info = match metadata.profile {
                SourceProfile::Video(info) => SourceMedia::Video(info),
                SourceProfile::Wave(info) => SourceMedia::Audio(info),
                SourceProfile::Ppm { .. } => {
                    return Err(
                        "still-image clips are not supported by this timeline provider".into(),
                    );
                }
            };
            (*id, info, audio)
        }
        PlacementSource::Output(source) => (
            Default::default(),
            SourceMedia::Document {
                source: source.clone(),
                info: output.ok_or("missing output capability")?,
            },
            false,
        ),
    };
    let duration = request.range(info.duration()?)?;
    let selected = request
        .track
        .map(|id| sequence.track(id).cloned())
        .transpose()?;
    if selected
        .as_ref()
        .is_some_and(|t| t.kind == TrackKind::Video)
        && (request.audio_only || matches!(info, SourceMedia::Audio(_)))
    {
        return Err("audio-only media requires an audio track".into());
    }
    let audio_only = request.audio_only
        || matches!(info, SourceMedia::Audio(_))
        || selected
            .as_ref()
            .is_some_and(|t| t.kind == TrackKind::Audio);
    if audio_only && !has_audio {
        return Err("source has no supported audio stream".into());
    }
    let link = (!audio_only && has_audio).then(ObjectId::new);
    for kind in [TrackKind::Video, TrackKind::Audio] {
        if (kind == TrackKind::Video && audio_only) || (kind == TrackKind::Audio && !has_audio) {
            continue;
        }
        let track = if let Some(track) = selected.as_ref().filter(|t| t.kind == kind) {
            track
        } else {
            sequence
                .tracks
                .iter()
                .find(|t| t.kind == kind && !t.locked)
                .ok_or("add an unlocked destination track first")?
        };
        if track.locked {
            return Err("destination track is locked".into());
        }
        sequence.clips.push(Clip {
            id: ObjectId::new(),
            track: track.id,
            link,
            asset,
            info: info.clone(),
            start: request.at,
            source_start: request.source_start,
            duration,
            level: 1.,
            extensions: Default::default(),
        });
    }
    let mut document = sequence.document(request.target)?;
    document.extensions = old.extensions.clone();
    Ok(document)
}
