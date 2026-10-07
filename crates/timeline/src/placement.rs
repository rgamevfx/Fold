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
    if let PlacementSource::Output(source) = &request.source {
        let mut pending = vec![source.document];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(id) = pending.pop() {
            if id == request.target {
                return Err("Placement would create a document cycle".into());
            }
            if visited.insert(id) {
                let document = snapshot
                    .state()
                    .documents
                    .get(&id)
                    .ok_or("Missing source document")?;
                pending.extend(document.dependencies.iter().map(|r| r.document));
            }
        }
    }
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
                SourceProfile::Ppm { .. } | SourceProfile::Exr(_) => {
                    let defaults = fold_media::VideoInfo {
                        width: sequence.dimensions[0],
                        height: sequence.dimensions[1],
                        rate: sequence.rate,
                        frames: fold_foundation::Time::new(5, 1)
                            .unwrap()
                            .to_ticks(
                                sequence.rate[0],
                                sequence.rate[1],
                                fold_foundation::Rounding::Ceil,
                            )
                            .map_err(|e| e.to_string())? as u32,
                    };
                    SourceMedia::Image(fold_platform::image_source::asset_info(
                        snapshot, *id, &defaults,
                    )?)
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
    if request.new_track {
        let kind = if request.audio_only || matches!(info, SourceMedia::Audio(_)) {
            TrackKind::Audio
        } else {
            TrackKind::Video
        };
        let count = sequence.tracks.iter().filter(|t| t.kind == kind).count() + 1;
        sequence.tracks.push(Track::new(
            kind,
            format!(
                "{}{}",
                if kind == TrackKind::Video { "V" } else { "A" },
                count
            ),
        ));
    }
    let selected = if request.new_track {
        sequence.tracks.last().cloned()
    } else {
        request
            .track
            .map(|id| sequence.track(id).cloned())
            .transpose()?
    };
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
    let mut placed = Vec::new();
    let link = (!audio_only && has_audio).then(ObjectId::new);
    for kind in [TrackKind::Video, TrackKind::Audio] {
        if (kind == TrackKind::Video && audio_only) || (kind == TrackKind::Audio && !has_audio) {
            continue;
        }
        let chosen = selected
            .as_ref()
            .filter(|t| t.kind == kind)
            .cloned()
            .or_else(|| {
                sequence
                    .tracks
                    .iter()
                    .find(|t| {
                        t.kind == kind
                            && !t.locked
                            && !sequence.clips.iter().any(|c| {
                                c.track == t.id
                                    && c.start < request.at.checked_add(duration).unwrap()
                                    && c.end().is_ok_and(|e| e > request.at)
                            })
                    })
                    .cloned()
            });
        let track = if let Some(track) = chosen {
            track
        } else {
            let count = sequence.tracks.iter().filter(|t| t.kind == kind).count() + 1;
            let track = Track::new(
                kind,
                format!(
                    "{}{}",
                    if kind == TrackKind::Video { "V" } else { "A" },
                    count
                ),
            );
            sequence.tracks.push(track.clone());
            track
        };
        if track.locked {
            return Err("destination track is locked".into());
        }
        let id = ObjectId::new();
        placed.push(id);
        sequence.clips.push(Clip {
            id,
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
    if request.mode != fold_platform::browser::PlacementMode::Place {
        let anchor = sequence.clips.iter().find(|c| c.id == placed[0]).unwrap();
        let edit = crate::SelectionEdit::Move {
            delta: fold_foundation::Time::ZERO,
            anchor: anchor.id,
            track: anchor.track,
            new_track: None,
            mode: if request.mode == fold_platform::browser::PlacementMode::Insert {
                crate::EditMode::Insert
            } else {
                crate::EditMode::Overwrite
            },
        };
        sequence.apply_selection(&placed, true, &edit)?;
    }
    let mut document = sequence.document(request.target)?;
    document.extensions = old.extensions.clone();
    Ok(document)
}
