//! Cross-feature placement lives in app assembly, never in either peer package.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_project::{DocumentRef, EditBatch, Mutation, Snapshot};
use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
use serde::{Deserialize, Serialize};
pub const INSERT: &str = "fold.app.insert-document";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InsertArgs {
    pub source: DocumentId,
    pub sequence: Option<DocumentId>,
    pub at: Time,
}
pub fn insert(snapshot: &Snapshot, args: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: InsertArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    if args.at < Time::ZERO {
        return Err("placement time must be nonnegative".into());
    }
    let info = crate::packages::builtins().output(snapshot, args.source)?;
    let target = args.sequence.or_else(|| {
        crate::timeline_workflow::active(snapshot)
            .ok()
            .map(|(r, _)| r.document)
    });
    if target == Some(args.source) {
        return Err("cannot place a document inside itself".into());
    }
    let mut sequence = if let Some(id) = target {
        Sequence::from_document(
            snapshot
                .state()
                .documents
                .get(&id)
                .ok_or("missing target sequence")?,
        )?
    } else {
        Sequence {
            dimensions: [info.width, info.height],
            rate: info.rate,
            tracks: vec![],
            clips: vec![],
            extensions: Default::default(),
        }
    };
    let track = Track::new(TrackKind::Video, "Motion / document");
    sequence.clips.push(Clip {
        id: ObjectId::new(),
        track: track.id,
        link: None,
        asset: Default::default(),
        info: SourceMedia::Document {
            source: DocumentRef {
                document: args.source,
                output: "video".into(),
                extensions: Default::default(),
            },
            info: info.clone(),
        },
        start: args.at,
        source_start: Time::ZERO,
        duration: info.time(info.frames)?,
        level: 1.,
        extensions: Default::default(),
    });
    sequence.tracks.push(track);
    let id = target.unwrap_or_default();
    let mut document = sequence.document(id)?;
    if let Some(old) = snapshot.state().documents.get(&id) {
        document.extensions = old.extensions.clone();
    }
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
