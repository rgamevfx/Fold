//! One guarded placement path for Gallery drops, buttons and MCP proposals.
use super::storage::{Component, Store};
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_platform::{
    browser::{BrowserCommand, PlacementSource},
    gallery::{Kind, PROVENANCE, Place},
};
use fold_project::{EditBatch, Item, ItemId, Mutation, Snapshot};
use serde_json::json;

pub fn stage(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    let request: Place = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    place(&Store::user(), snapshot, &request, cancel)
}
pub fn place(
    store: &Store,
    snapshot: &Snapshot,
    args: &Place,
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    cancel.check()?;
    let c = store.get(args.component)?;
    c.validate()?;
    if c.kind == Kind::Effect && args.objects.len() > 1 && args.update.is_none() {
        let mut working = snapshot.clone();
        let mut mutations = Vec::new();
        for object in &args.objects {
            let mut single = args.clone();
            single.objects = vec![*object];
            let batch = place(store, &working, &single, cancel)?;
            working = working.overlay(batch.clone()).map_err(|e| e.to_string())?;
            mutations.extend(batch.mutations);
        }
        return Ok(EditBatch {
            base: snapshot.revision(),
            mutations,
        });
    }

    let target = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing component destination")?;
    if args.at < fold_foundation::Time::ZERO {
        return Err("Component time must be nonnegative".into());
    }
    // Reuse owning-layer protection for every route, including direct MCP placement.
    crate::browser::check_placement_locks(snapshot, args.document)?;
    let mut mutations = Vec::new();
    for asset in store.materialize(&c, cancel)? {
        if let Some(old) = snapshot.state().assets.get(&asset.id) {
            if old.fingerprint != asset.fingerprint
                || super::storage::read_bytes(
                    std::path::Path::new(&old.location),
                    super::storage::MAX_BYTES,
                )? != super::storage::read_bytes(
                    std::path::Path::new(&asset.location),
                    super::storage::MAX_BYTES,
                )?
            {
                return Err("Component asset identity conflicts with this project".into());
            }
        } else {
            mutations.push(Mutation::PutAsset(asset));
        }
    }
    let provenance = json!({"component":c.reference,"name":c.name,"kind":c.kind});
    if target.type_id == fold_motion_canvas::TYPE {
        let document = match c.kind {
            Kind::Graphic => fold_motion_context::components::insert(
                target,
                &c.document,
                &c.name,
                args.position,
                args.update,
                provenance,
            )?,
            Kind::Motion => {
                if args.objects.is_empty() {
                    return Err("Select native artwork to apply this motion".into());
                }
                let source = fold_motion_canvas::model::Canvas::from_document(&c.document)?;
                let spec = &source
                    .objects
                    .values()
                    .find_map(|o| o.motion.as_ref())
                    .ok_or("Missing component motion")?
                    .spec;
                let mut working = snapshot.clone();
                for object in &args.objects {
                    let bytes = serde_json::to_vec(&fold_motion_context::package::AnimateArgs {
                        document: args.document,
                        object: *object,
                        spec: spec.clone(),
                    })
                    .map_err(|e| e.to_string())?;
                    let batch = crate::motion_conversion::animate(&working, &bytes, cancel)?;
                    working = working.overlay(batch).map_err(|e| e.to_string())?;
                }
                working.state().documents[&args.document].as_ref().clone()
            }
            _ => {
                return Err(
                    "Apply image effects and transitions in Composition or Timeline".into(),
                );
            }
        };
        mutations.push(Mutation::PutDocument(document));
    } else {
        if c.kind == Kind::Motion {
            return Err("Open the native Motion artwork to apply a motion behavior".into());
        }
        let mut definition = c.document.clone();
        definition.id = DocumentId::new();
        definition
            .extensions
            .insert(PROVENANCE.into(), provenance.clone());
        if let Some(instance) = args.update {
            let old = instance_source(snapshot, args.document, instance, c.kind)?;
            let original = snapshot
                .state()
                .documents
                .get(&old)
                .ok_or("Missing instance definition")?;
            if !original.extensions.contains_key(PROVENANCE) {
                return Err("Select a Gallery instance to update".into());
            }
            if original.type_id != definition.type_id {
                return Err("Component update changes its document type".into());
            }
            definition.id = old;
            definition.extensions = original.extensions.clone();
            definition.extensions.insert(PROVENANCE.into(), provenance);
            if !fold_project::components::preserves_extensions(
                &original.extensions,
                &definition.extensions,
            ) {
                return Err("Select an instance of this component to update".into());
            }
            let registry = crate::packages::builtins();
            mutations.push(Mutation::PutDocument(definition));
            let updated = snapshot
                .overlay(EditBatch {
                    base: snapshot.revision(),
                    mutations: mutations.clone(),
                })
                .map_err(|e| e.to_string())?;
            if c.kind == Kind::Graphic
                && registry.output(snapshot, old)? != registry.output(&updated, old)?
            {
                return Err("Updated graphics must retain their size, frame rate and duration; place a new instance instead".into());
            }
            if matches!(c.kind, Kind::Effect | Kind::Transition)
                && registry.effect_interface(snapshot, old)?
                    != registry.effect_interface(&updated, old)?
            {
                return Err("Updated effects must retain their exposed interface; place a new instance instead".into());
            }
        } else {
            let definition_id = definition.id;
            mutations.push(Mutation::PutDocument(definition));
            mutations.push(Mutation::PutItem(Item {
                id: ItemId::Document(definition_id),
                name: c.name.clone(),
                parent: Default::default(),
                order: snapshot.state().organization.items.len() as u64,
                extensions: Default::default(),
            }));
            let staged = snapshot
                .overlay(EditBatch {
                    base: snapshot.revision(),
                    mutations: mutations.clone(),
                })
                .map_err(|e| e.to_string())?;
            let extra = match c.kind {
                Kind::Graphic => {
                    let mut request = fold_platform::browser::Placement {
                        graph: None,
                        canvas_position: args.position,
                        source: PlacementSource::Output(
                            crate::packages::builtins().default_output(&staged, definition_id)?,
                        ),
                        target: args.document,
                        track: args.track,
                        at: args.at,
                        source_start: fold_foundation::Time::ZERO,
                        duration: None,
                        audio_only: false,
                        new_track: false,
                        mode: Default::default(),
                    };
                    if target.type_id == fold_timeline::SEQUENCE && args.track.is_none() {
                        request.new_track = true;
                    }
                    crate::browser::edit(&staged, BrowserCommand::Place(request))?
                }
                Kind::Effect => effect(&staged, args, definition_id, &c)?,
                Kind::Transition => {
                    if target.type_id != fold_timeline::SEQUENCE {
                        return Err("Drop a transition at a Timeline cut".into());
                    }
                    let sequence = fold_timeline::Sequence::from_document(target)?;
                    let outgoing = args
                        .objects
                        .first()
                        .copied()
                        .or_else(|| {
                            sequence
                                .clips
                                .iter()
                                .find(|clip| {
                                    args.track.is_none_or(|t| clip.track == t)
                                        && clip.start <= args.at
                                        && clip.end().is_ok_and(|end| args.at <= end)
                                })
                                .map(|c| c.id)
                        })
                        .ok_or("Select the outgoing clip or drop on a cut")?;
                    crate::timeline_transition::add(&staged,&serde_json::to_vec(&json!({"document":args.document,"outgoing":outgoing,"id":ObjectId::new(),"definition":definition_id})).map_err(|e|e.to_string())?,cancel)?
                }
                Kind::Motion => unreachable!(),
            };
            mutations.extend(extra.mutations);
        }
    }
    // Provider gates enforce protected fields, locks and schema on both UI and MCP.
    let registry = crate::packages::builtins();
    for mutation in &mut mutations {
        if let Mutation::PutDocument(document) = mutation {
            let prepared = registry
                .scoped_proposals(document)
                .map_err(|e| e.message)?
                .prepare_change(
                    snapshot,
                    fold_platform::proposals::Scope {
                        document: document.id,
                    },
                    document,
                    cancel,
                )
                .map_err(|e| e.message)?;
            let changes = prepared
                .publication(snapshot, cancel)
                .map_err(|e| e.message)?;
            if changes.documents.len() != 1 {
                return Err("Component document preparation changed its write scope".into());
            }
            *document = changes.documents.into_iter().next().unwrap();
        }
    }
    let batch = EditBatch {
        base: snapshot.revision(),
        mutations,
    };
    snapshot.overlay(batch.clone()).map_err(|e| e.to_string())?;
    Ok(batch)
}
fn effect(
    snapshot: &Snapshot,
    args: &Place,
    definition: DocumentId,
    c: &Component,
) -> Result<EditBatch, String> {
    let original = &snapshot.state().documents[&args.document];
    let interface = crate::packages::builtins().effect_interface(snapshot, definition)?;
    let make = || fold_platform::effects::Instance {
        id: ObjectId::new(),
        definition: fold_project::DocumentRef {
            document: definition,
            output: "video".into(),
            extensions: Default::default(),
        },
        version: interface.version,
        values: Default::default(),
        animation: Default::default(),
        enabled: true,
        extensions: [(
            PROVENANCE.into(),
            json!({"component":c.reference,"kind":c.kind}),
        )]
        .into(),
    };
    if original.type_id == fold_composition::COMPOSITION {
        let mut composition = fold_composition::Composition::from_document(original)?;
        let objects = args
            .track
            .map_or_else(|| args.objects.clone(), |id| vec![id]);
        if objects.is_empty() {
            return Err("Select a layer to apply the effect".into());
        }
        for id in &objects {
            if composition.locked(*id) {
                return Err("Destination layer is locked".into());
            }
            composition.layer_mut(*id)?.effects.push(make());
        }
        let mut doc = composition.document(args.document)?;
        doc.extensions = original.extensions.clone();
        Ok(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(doc)],
        })
    } else if original.type_id == fold_timeline::SEQUENCE {
        let sequence = fold_timeline::Sequence::from_document(original)?;
        let id = args
            .objects
            .first()
            .copied()
            .or_else(|| {
                sequence
                    .clips
                    .iter()
                    .find(|c| {
                        args.track.is_none_or(|t| c.track == t)
                            && c.start <= args.at
                            && c.end().is_ok_and(|end| args.at < end)
                    })
                    .map(|c| c.id)
            })
            .ok_or("Select a clip to apply the effect")?;
        let mut effects = sequence
            .clips
            .iter()
            .find(|c| c.id == id)
            .ok_or("Missing destination clip")?
            .effects
            .clone();
        effects.push(make());
        fold_timeline::edit_sequence(
            snapshot,
            args.document,
            &fold_timeline::SequenceEdit::Effects { id, effects },
        )
    } else {
        Err("Apply this effect to a Composition layer or Timeline clip".into())
    }
}
fn instance_source(
    snapshot: &Snapshot,
    document: DocumentId,
    id: ObjectId,
    kind: Kind,
) -> Result<DocumentId, String> {
    let d = &snapshot.state().documents[&document];
    if d.type_id == fold_composition::COMPOSITION {
        let c = fold_composition::Composition::from_document(d)?;
        if c.locked(id) {
            return Err("Destination layer is locked".into());
        }
        let layer = c.layer(id)?;
        if kind == Kind::Graphic {
            if let Some(fold_platform::image_source::Source::Output(r)) = &layer.source {
                // Existing composition placement wraps footage in a layer graph.
                let source = &snapshot.state().documents[&r.document];
                if source.extensions.contains_key(PROVENANCE) {
                    return Ok(r.document);
                }
                if source.dependencies.len() == 1 {
                    return Ok(source.dependencies[0].document);
                }
            }
        } else if let Some(e) = layer
            .effects
            .iter()
            .rev()
            .find(|e| e.extensions.contains_key(PROVENANCE))
        {
            return Ok(e.definition.document);
        }
    } else if d.type_id == fold_timeline::SEQUENCE {
        let s = fold_timeline::Sequence::from_document(d)?;
        if let Some(t) = s.transitions.iter().find(|t| t.id == id) {
            if s.track(
                s.clips
                    .iter()
                    .find(|c| c.id == t.outgoing)
                    .ok_or("Missing outgoing clip")?
                    .track,
            )?
            .locked
            {
                return Err("Track is locked".into());
            }
            return Ok(t.effect.definition.document);
        }
        let clip = s.clips.iter().find(|c| c.id == id).ok_or("Missing clip")?;
        if s.track(clip.track)?.locked {
            return Err("Track is locked".into());
        }
        if kind == Kind::Graphic {
            if let fold_timeline::SourceMedia::Document { source, .. } = &clip.info {
                return Ok(source.document);
            }
        } else if let Some(e) = clip
            .effects
            .iter()
            .rev()
            .find(|e| e.extensions.contains_key(PROVENANCE))
        {
            return Ok(e.definition.document);
        }
    }
    Err("Select a placed Gallery instance to update".into())
}
