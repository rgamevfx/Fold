//! Cross-feature assembly: one proposal creates a composite and replaces video
//! clips. Neither creative package imports the other's implementation models.
use fold_composition::{Composition, Layer};
use fold_foundation::{DocumentId, ObjectId, Rounding, Time};
use fold_media::Cancel;
use fold_platform::image_source::Source;
use fold_project::{DocumentRef, EditBatch, Mutation, Snapshot};
use fold_timeline::{Clip, Sequence, SourceMedia, TrackKind};
use serde::{Deserialize, Serialize};

pub const CREATE: &str = "fold.app.create-composition";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateArgs {
    pub document: DocumentId,
    pub clips: Vec<ObjectId>,
}

pub fn create(snapshot: &Snapshot, args: &[u8], _: &Cancel) -> Result<EditBatch, String> {
    let args: CreateArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("missing sequence")?;
    let mut sequence = Sequence::from_document(original)?;
    if args.clips.is_empty() {
        return Err("select video clips first".into());
    }
    let mut selected: Vec<_> = sequence
        .clips
        .iter()
        .filter(|c| args.clips.contains(&c.id))
        .cloned()
        .collect();
    if selected.len() != args.clips.len() {
        return Err("missing or duplicate clip selection".into());
    }
    // The timeline selects linked A/V together. Only the selected video moves;
    // its selected or unselected linked audio remains on the original track.
    for clip in &selected {
        if sequence.track(clip.track)?.kind == TrackKind::Audio
            && !clip.link.is_some_and(|link| {
                selected.iter().any(|c| {
                    c.link == Some(link)
                        && sequence
                            .track(c.track)
                            .is_ok_and(|t| t.kind == TrackKind::Video)
                })
            })
        {
            return Err(
                "select video clips; independent audio cannot enter an image composite".into(),
            );
        }
    }
    selected.retain(|c| {
        sequence
            .track(c.track)
            .is_ok_and(|t| t.kind == TrackKind::Video)
    });
    if selected.is_empty() {
        return Err("select video clips first".into());
    }
    if selected.iter().any(|c| !c.extensions.is_empty()) {
        return Err(
            "cannot convert clips with unknown extension data; no changes were made".into(),
        );
    }
    let track = sequence.track(selected[0].track)?;
    if track.kind != TrackKind::Video
        || track.locked
        || selected.iter().any(|c| c.track != track.id)
    {
        return Err(
            "Create Composition currently requires video clips on one unlocked track".into(),
        );
    }
    for clip in &sequence.clips {
        if clip
            .link
            .is_some_and(|link| selected.iter().any(|c| c.link == Some(link)))
            && sequence.track(clip.track)?.locked
        {
            return Err("cannot unlink audio on a locked track".into());
        }
    }
    let start = selected.iter().map(|c| c.start).min().unwrap();
    let end = selected
        .iter()
        .try_fold(Time::ZERO, |end, c| Ok::<_, String>(end.max(c.end()?)))?;
    if sequence.clips.iter().any(|c| {
        c.track == track.id
            && !args.clips.contains(&c.id)
            && c.start < end
            && c.end().is_ok_and(|t| t > start)
    }) {
        return Err("selection encloses an unselected clip; include it before nesting".into());
    }
    let duration = end.checked_sub(start).map_err(|e| e.to_string())?;
    let mut info = sequence.info()?;
    info.frames = u32::try_from(
        duration
            .to_ticks(info.rate[0], info.rate[1], Rounding::Ceil)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut layers = vec![];
    for clip in selected.iter().rev() {
        let source = match &clip.info {
            SourceMedia::Video(_) | SourceMedia::Image(_) => Source::Asset(clip.asset),
            SourceMedia::Document { source, .. } => Source::Output(source.clone()),
            SourceMedia::Adjustment => return Err("An adjustment reads lower tracks; nest its complete lower-track scope instead of this clip alone".into()),
            _ => return Err("Cannot nest audio into a composition layer".into()),
        };
        let source_info = match &clip.info {
            SourceMedia::Video(info)
            | SourceMedia::Image(info)
            | SourceMedia::Document { info, .. } => info.clone(),
            _ => unreachable!(),
        };
        let mut layer = Layer::new("Clip", Some(source), source_info)?;
        layer.state.id = clip.id;
        layer.state.start = clip.start.checked_sub(start).map_err(|e| e.to_string())?;
        layer.state.end = layer
            .start
            .checked_add(clip.duration)
            .map_err(|e| e.to_string())?;
        layer.state.opacity = clip.level as f64;
        layer.effects = clip.effects.clone();
        for effect in &mut layer.effects {
            for curve in effect.animation.values_mut().flatten() {
                for key in &mut curve.keys {
                    key.time = key.time.checked_sub(start).map_err(|e| e.to_string())?;
                }
            }
        }
        layer.origin = layer.start;
        layer.source_start = clip.source_start;
        layers.push(layer);
    }
    let composite = Composition {
        info: info.clone(),
        layers,
        extensions: Default::default(),
    };
    let id = DocumentId::new();
    let links: Vec<_> = selected.iter().filter_map(|c| c.link).collect();
    sequence
        .clips
        .retain(|c| !selected.iter().any(|selected| selected.id == c.id));
    for clip in &mut sequence.clips {
        if clip.link.is_some_and(|l| links.contains(&l)) {
            clip.link = None;
        }
    }
    sequence.clips.push(Clip {
        effects: vec![],
        id: selected[0].id,
        track: selected[0].track,
        link: None,
        asset: selected[0].asset,
        info: SourceMedia::Document {
            source: DocumentRef {
                document: id,
                output: "video".into(),
                extensions: Default::default(),
            },
            info,
        },
        start,
        source_start: Time::ZERO,
        duration,
        level: 1.0,
        extensions: selected[0].extensions.clone(),
    });
    let mut document = sequence.document(args.document)?;
    document.extensions = original.extensions.clone();
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![
            Mutation::PutDocument(composite.document(id)?),
            Mutation::PutDocument(document),
        ],
    })
}
