//! Fixed track and clip controls; provider keys are independent of artist names.
use crate::{Clip, Sequence, Track};
use fold_foundation::Time;
use fold_media::Cancel;
use fold_platform::parameters::{self, Descriptor, Literal, Space, Target, Unit};
use fold_project::Document;

fn track_values(track: &Track) -> Vec<(&'static str, Literal)> {
    vec![
        (
            "name",
            Literal::Text {
                value: track.name.clone(),
            },
        ),
        (
            "enabled",
            Literal::Bool {
                value: track.enabled,
            },
        ),
        ("solo", Literal::Bool { value: track.solo }),
        (
            "locked",
            Literal::Bool {
                value: track.locked,
            },
        ),
    ]
}
fn clip_values(clip: &Clip, video: bool) -> Result<Vec<(&'static str, Literal)>, String> {
    let mut values = vec![
        ("start", Literal::Duration { value: clip.start }),
        (
            "duration",
            Literal::Duration {
                value: clip.duration,
            },
        ),
        (
            "source_start",
            Literal::Duration {
                value: clip.source_start,
            },
        ),
        (
            "level",
            Literal::Scalar {
                value: f64::from(clip.level),
            },
        ),
    ];
    if !video {
        return Ok(values);
    }
    let transform = clip.transform()?;
    values.extend([
        (
            "transform.position",
            Literal::Vector {
                value: transform.position.to_vec(),
                unit: Unit::Canvas,
                space: Space::Screen,
            },
        ),
        (
            "transform.scale",
            Literal::Vector {
                value: transform.scale.to_vec(),
                unit: Unit::Unitless,
                space: Space::Screen,
            },
        ),
        // Degrees, matching the native clip control. Angle units are not yet a
        // distinct type in the common graph language.
        (
            "transform.rotation",
            Literal::Scalar {
                value: transform.rotation,
            },
        ),
    ]);
    Ok(values)
}

pub(crate) fn catalog(document: &Document) -> Result<Vec<Descriptor>, String> {
    let sequence = Sequence::from_document(document)?;
    let mut result = vec![];
    for track in &sequence.tracks {
        for (key, value) in track_values(track) {
            result.extend(parameters::property(
                document.id,
                Some(track.id),
                key,
                vec![track.name.clone(), key.into()],
                &value,
                !track.locked,
            ));
        }
        for clip in sequence.clips.iter().filter(|clip| clip.track == track.id) {
            for (key, value) in clip_values(clip, track.kind == crate::TrackKind::Video)? {
                // Clips do not have authored names. Retain an unambiguous native
                // owner segment instead of an unstable row number or asset label.
                let mut path = vec![track.name.clone(), format!("Clip {}", clip.id)];
                path.extend(key.split('.').map(String::from));
                result.extend(parameters::property(
                    document.id,
                    Some(clip.id),
                    key,
                    path,
                    &value,
                    !track.locked,
                ));
            }
        }
    }
    Ok(result)
}

pub(crate) fn sample(
    document: &Document,
    target: &Target,
    _time: Time,
    cancel: &Cancel,
) -> Result<Literal, String> {
    cancel.check()?;
    if !target.occurrence.is_empty() {
        return Err("Timeline controls have no graph occurrence".into());
    }
    let sequence = Sequence::from_document(document)?;
    let id = target.object.ok_or("Missing Timeline owner")?;
    let key = target
        .parameter
        .property()
        .ok_or("Expected a Timeline property key")?;
    let values = if let Some(track) = sequence.tracks.iter().find(|track| track.id == id) {
        track_values(track)
    } else {
        let clip = sequence
            .clips
            .iter()
            .find(|clip| clip.id == id)
            .ok_or("Missing Timeline owner")?;
        clip_values(
            clip,
            sequence.track(clip.track)?.kind == crate::TrackKind::Video,
        )?
    };
    let value = values
        .into_iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
        .ok_or("Unknown Timeline property")?;
    parameters::select_component(value, target)
}

pub(crate) fn sources(document: &Document) -> Result<Vec<parameters::Source>, String> {
    let sequence = Sequence::from_document(document)?;
    sequence
        .clips
        .iter()
        .filter_map(|clip| match &clip.info {
            crate::SourceMedia::Document { source, .. } => Some((clip, source)),
            _ => None,
        })
        .map(|(clip, reference)| {
            Ok(parameters::Source {
                owner: clip.id,
                path: vec![
                    sequence.track(clip.track)?.name.clone(),
                    format!("Clip {}", clip.id),
                ],
                reference: reference.clone(),
                origin: clip.start,
                source_start: clip.source_start,
                editable: !sequence.track(clip.track)?.locked,
            })
        })
        .collect()
}
