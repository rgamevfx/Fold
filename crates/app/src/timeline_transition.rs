//! Atomic default-transition creation across independent timeline/compositor packages.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{EditBatch, Item, ItemId, Mutation, Snapshot};
use serde::Deserialize;
pub const ADD: &str = "fold.app.add-transition";
#[derive(Deserialize)]
struct Args {
    document: DocumentId,
    outgoing: ObjectId,
    id: ObjectId,
}
pub fn add(
    snapshot: &Snapshot,
    bytes: &[u8],
    cancel: &fold_media::Cancel,
) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: Args = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing timeline")?;
    let sequence = fold_timeline::Sequence::from_document(original)?;
    let a = sequence
        .clips
        .iter()
        .find(|c| c.id == args.outgoing)
        .ok_or("Select a cut")?;
    let cut = a.end()?;
    let b = sequence
        .clips
        .iter()
        .find(|c| c.track == a.track && c.start == cut)
        .ok_or("Select adjacent video clips")?;
    if sequence
        .transitions
        .iter()
        .any(|t| t.outgoing == a.id && t.incoming == b.id)
    {
        return Err("This cut already has a transition".into());
    }
    let quarter = Time::new(1, 4).unwrap();
    let lead = quarter.min(b.source_start).min(a.duration);
    let tail = a
        .info
        .duration()?
        .checked_sub(a.source_start)
        .and_then(|v| v.checked_sub(a.duration))
        .map_err(|e| e.to_string())?
        .min(quarter)
        .min(b.duration);
    let duration = lead.checked_add(tail).map_err(|e| e.to_string())?;
    if duration <= Time::ZERO {
        return Err(
            "No source handles available; trim the clips before adding a transition".into(),
        );
    }
    let registry = crate::packages::builtins();
    let existing = snapshot.state().documents.values().find(|d| {
        registry
            .effect_interface(snapshot, d.id)
            .is_ok_and(|i| i.images == ["outgoing", "incoming"])
            && fold_compositor::Composite::from_document(d).is_ok_and(|g| {
                g.nodes.iter().any(|n| {
                    matches!(
                        n.configuration(),
                        fold_compositor::OperationConfig::Dissolve { .. }
                    )
                })
            })
    });
    let definition = existing.map(|d| d.id).unwrap_or_default();
    let mut mutations = Vec::new();
    if existing.is_none() {
        mutations.push(Mutation::PutDocument(fold_compositor::presets::create(
            fold_compositor::presets::Preset::Dissolve,
            definition,
        )?));
        mutations.push(Mutation::PutItem(Item {
            id: ItemId::Document(definition),
            name: "Dissolve".into(),
            parent: Default::default(),
            order: snapshot.state().organization.items.len() as u64,
            extensions: Default::default(),
        }));
    }
    let reference = fold_project::DocumentRef {
        document: definition,
        output: "video".into(),
        extensions: Default::default(),
    };
    let version = existing
        .map(|_| {
            registry
                .effect_interface(snapshot, definition)
                .map(|i| i.version)
        })
        .transpose()?
        .unwrap_or(1);
    let updated = sequence.edited(&fold_timeline::SequenceEdit::Transition {
        transition: fold_timeline::Transition {
            id: args.id,
            outgoing: a.id,
            incoming: b.id,
            start: cut.checked_sub(lead).map_err(|e| e.to_string())?,
            duration,
            effect: fold_platform::effects::Instance {
                id: ObjectId::new(),
                definition: reference,
                version,
                enabled: true,
                values: Default::default(),
                animation: Default::default(),
                extensions: Default::default(),
            },
        },
    })?;
    let mut document = updated.document(args.document)?;
    document.extensions = original.extensions.clone();
    mutations.push(Mutation::PutDocument(document));
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_transition_and_definition_are_one_undoable_edit() {
        use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
        let mut project = fold_project::Project::new(10);
        let document = DocumentId::new();
        let asset = fold_foundation::AssetId::new();
        let track = Track::new(TrackKind::Video, "Video");
        let clips = [0, 2].map(|start| Clip {
            id: ObjectId::new(),
            track: track.id,
            link: None,
            asset,
            info: SourceMedia::Video(fold_media::VideoInfo {
                width: 16,
                height: 16,
                rate: [24, 1],
                frames: 240,
            }),
            start: Time::new(start, 1).unwrap(),
            source_start: Time::new(1, 1).unwrap(),
            duration: Time::new(2, 1).unwrap(),
            level: 1.,
            effects: vec![],
            extensions: Default::default(),
        });
        let outgoing = clips[0].id;
        let sequence = Sequence {
            tracks: vec![track],
            clips: clips.into(),
            transitions: vec![],
            dimensions: [16, 16],
            rate: [24, 1],
            extensions: Default::default(),
        };
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![
                    Mutation::PutAsset(fold_project::Asset {
                        id: asset,
                        location: "fixture.mov".into(),
                        fingerprint: "fixture".into(),
                        extensions: Default::default(),
                    }),
                    Mutation::PutDocument(sequence.document(document).unwrap()),
                ],
            })
            .unwrap();
        let snapshot = project.snapshot();
        let id = ObjectId::new();
        let batch = add(
            &snapshot,
            &serde_json::to_vec(
                &serde_json::json!({"document":document,"outgoing":outgoing,"id":id}),
            )
            .unwrap(),
            &fold_media::Cancel::default(),
        )
        .unwrap();
        project.commit(batch).unwrap();
        let updated = project.snapshot();
        let result = Sequence::from_document(&updated.state().documents[&document]).unwrap();
        assert_eq!(
            serde_json::to_value(&result.clips).unwrap(),
            serde_json::to_value(&sequence.clips).unwrap()
        );
        assert_eq!(result.transitions[0].id, id);
        assert_eq!(result.transitions[0].duration, Time::new(1, 2).unwrap());
        assert_eq!(updated.state().documents.len(), 2);
        project.undo().unwrap();
        let restored = project.snapshot();
        assert_eq!(
            restored.state().documents.len(),
            snapshot.state().documents.len()
        );
        assert_eq!(
            restored.state().documents[&document].payload,
            snapshot.state().documents[&document].payload
        );
        assert_eq!(restored.state().organization, snapshot.state().organization);
    }
}
