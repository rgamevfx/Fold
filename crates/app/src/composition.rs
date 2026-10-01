//! Cross-feature assembly: one proposal creates a composite and replaces video
//! clips. Neither creative package imports the other's implementation models.
use fold_compositor::{Composite, Node, Parameters as P, Source};
use fold_foundation::{DocumentId, ObjectId, Rounding, Time};
use fold_media::Cancel;
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
    let background = Node::new(P::Solid { rgba: [0.0; 4] }, vec![]);
    let mut output = background.id;
    let mut nodes = vec![background];
    for clip in &selected {
        let source = match &clip.info {
            SourceMedia::Video(info) => Source::Asset {
                asset: clip.asset,
                info: info.clone(),
            },
            SourceMedia::Document { source, info } => Source::Document {
                source: source.clone(),
                info: info.clone(),
            },
            _ => return Err("cannot nest audio into an image-only composite".into()),
        };
        let read = Node::new(
            P::Read {
                source,
                start: clip.start.checked_sub(start).map_err(|e| e.to_string())?,
                source_start: clip.source_start,
                duration: clip.duration,
            },
            vec![],
        );
        let transform = Node::new(
            P::Transform {
                translate: [0.0; 2],
                scale: [1.0; 2],
                opacity: clip.level,
            },
            vec![read.id],
        );
        let merge = Node::new(P::Merge, vec![transform.id, output]);
        output = merge.id;
        nodes.extend([read, transform, merge]);
    }
    let output_node = Node::new(P::Output, vec![output]);
    let output = output_node.id;
    nodes.push(output_node);
    let composite = Composite {
        info: info.clone(),
        nodes,
        output,
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
