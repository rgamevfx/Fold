//! Gallery storage and cross-workspace assembly; native models remain provider-owned.
mod builtins;
mod placement;
mod preview;
pub mod storage;
#[cfg(feature = "desktop")]
pub mod ui;
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_platform::gallery::{Kind, Reference, Request};
use fold_project::{Document, Snapshot};
pub use placement::{place, stage};
#[cfg(feature = "gpu")]
pub(crate) use preview::set_host;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};
use storage::{Component, Resource, Store};

pub fn validate_kind(c: &Component) -> Result<(), String> {
    match c.kind {
        Kind::Graphic | Kind::Motion => {
            let canvas = fold_motion_canvas::model::Canvas::from_document(&c.document)?;
            if c.kind == Kind::Motion
                && canvas
                    .objects
                    .values()
                    .filter(|o| o.motion.is_some())
                    .count()
                    != 1
            {
                return Err("A motion preset requires exactly one prepared behavior".into());
            }
        }
        Kind::Effect | Kind::Transition => {
            let graph = fold_compositor::Composite::from_document(&c.document)?;
            let definition = graph.reusable()?;
            let required = if c.kind == Kind::Transition {
                vec!["outgoing", "incoming"]
            } else {
                vec!["source"]
            };
            if definition
                .interface
                .images
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != required
            {
                return Err("Component image inputs do not match its effect type".into());
            }
        }
    }
    Ok(())
}
fn source(
    snapshot: &Snapshot,
    document: DocumentId,
    objects: &[ObjectId],
    kind: Kind,
) -> Result<Document, String> {
    let original = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or("Missing source document")?;
    if original.type_id == fold_motion_canvas::TYPE {
        return fold_motion_context::components::capture(original, objects);
    }
    if original.type_id == fold_compositor::COMPOSITE {
        return Ok(original.as_ref().clone());
    }
    let source = if original.type_id == fold_composition::COMPOSITION {
        let c = fold_composition::Composition::from_document(original)?;
        let [id] = objects else {
            return Err("Select one source layer to save".into());
        };
        let layer = c.layer(*id)?;
        if matches!(kind, Kind::Effect | Kind::Transition) {
            layer.effects.last().map(|e| e.definition.document)
        } else {
            match &layer.source {
                Some(fold_platform::image_source::Source::Output(r)) => Some(r.document),
                _ => None,
            }
        }
    } else if original.type_id == fold_timeline::SEQUENCE {
        let s = fold_timeline::Sequence::from_document(original)?;
        let [id] = objects else {
            return Err("Select one clip or transition to save".into());
        };
        if kind == Kind::Transition {
            s.transitions
                .iter()
                .find(|t| t.id == *id)
                .map(|t| t.effect.definition.document)
        } else {
            let clip = s
                .clips
                .iter()
                .find(|c| c.id == *id)
                .ok_or("Missing selected clip")?;
            if kind == Kind::Effect {
                clip.effects.last().map(|e| e.definition.document)
            } else {
                match &clip.info {
                    fold_timeline::SourceMedia::Document { source, .. } => Some(source.document),
                    _ => None,
                }
            }
        }
    } else {
        None
    }
    .ok_or("Select a native graphic, effect or transition to save")?;
    // Composition may own an ordinary source wrapper. Resolve a single native source.
    let mut document = source;
    for _ in 0..32 {
        let d = snapshot
            .state()
            .documents
            .get(&document)
            .ok_or("Missing component source")?;
        if d.type_id == fold_motion_canvas::TYPE {
            return fold_motion_context::components::capture(d, &[]);
        }
        if matches!(kind, Kind::Effect | Kind::Transition) {
            return Ok(d.as_ref().clone());
        }
        if d.dependencies.len() != 1 {
            return Err("Open the native source to save this component".into());
        }
        document = d.dependencies[0].document;
    }
    Err("Component source nesting exceeds 32 levels".into())
}
pub fn summary(c: &storage::Entry, favorite: bool) -> Value {
    json!({"reference":c.reference,"name":c.name,"category":c.category,"kind":c.kind,"built_in":builtins::contains(c.reference.id),"favorite":favorite,"has_preview":c.has_preview})
}
pub fn operate(
    store: &Store,
    snapshot: &Snapshot,
    request: Request,
    cancel: &Cancel,
) -> Result<Value, String> {
    cancel.check()?;
    if let Request::Preview { component } = request {
        return serde_json::to_value(preview::get(store, &store.get(component)?, cancel)?)
            .map_err(|e| e.to_string());
    }
    static WRITES: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = WRITES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Library operation failed")?;
    match request {
        Request::List => {
            let favorites = store.favorites()?;
            Ok(
                json!({"entries":store.list()?.iter().map(|c|summary(c,favorites.contains(&c.reference.id))).collect::<Vec<_>>()}),
            )
        }
        Request::Inspect { component } => {
            let c = store.get(component)?;
            Ok(
                json!({"component":summary(&c.entry(),store.favorites()?.contains(&component.id)),"native":crate::packages::builtins().inspect(&c.document)?,"resources":c.resources.iter().map(|r|json!({"id":r.asset.id,"bytes":r.bytes.len()})).collect::<Vec<_>>(),"update":"Placement pins a copied revision. Update replaces component contents and retains placement; native edits inside the component may be replaced."}),
            )
        }
        Request::Save {
            document,
            objects,
            name,
            category,
            kind,
            update,
        } => {
            let document = source(snapshot, document, &objects, kind)?;
            let mut remaining = storage::MAX_BYTES;
            let resources = document
                .assets
                .iter()
                .map(|id| {
                    cancel.check()?;
                    let asset = snapshot
                        .state()
                        .assets
                        .get(id)
                        .ok_or("Missing component asset")?;
                    if std::fs::metadata(&asset.location)
                        .map_err(|e| e.to_string())?
                        .len()
                        > remaining
                    {
                        return Err("Component asset exceeds 64 MiB".into());
                    }
                    let bytes = storage::read_bytes(Path::new(&asset.location), remaining)?;
                    remaining -= bytes.len() as u64;
                    Ok(Resource {
                        asset: asset.as_ref().clone(),
                        bytes,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let c = Component {
                schema: 1,
                reference: Reference {
                    id: update.map_or_else(ObjectId::new, |r| r.id),
                    version: update.map_or(1, |r| r.version.saturating_add(1)),
                },
                name: name.trim().into(),
                category: category.trim().into(),
                kind,
                document,
                resources,
                preview: None,
                extensions: Default::default(),
            };
            store.put(&c, update, cancel)?;
            Ok(summary(&c.entry(), false))
        }
        Request::Import { path } => {
            let c = storage::read(absolute(&path)?)?;
            if builtins::contains(c.reference.id) {
                return Err("Built-in identities cannot be imported".into());
            }
            let dest = store.path(c.reference);
            if dest.exists() {
                let mut existing = storage::read(&dest)?;
                let mut incoming = c.clone();
                // Rendered thumbnails are derived data, not a new creative revision.
                existing.preview = None;
                incoming.preview = None;
                if serde_json::to_value(&existing).map_err(|e| e.to_string())?
                    != serde_json::to_value(&incoming).map_err(|e| e.to_string())?
                {
                    return Err(
                        "This component version already exists with different content".into(),
                    );
                }
            } else {
                storage::write_new(&dest, &c, cancel)?;
            }
            Ok(summary(&c.entry(), false))
        }
        Request::Export { component, path } => {
            let mut c = store.get(component)?;
            if c.preview.is_none() {
                c.preview = preview::cached(store, component)?;
            }
            storage::write_new(absolute(&path)?, &c, cancel)?;
            Ok(json!({"path":path,"saved":true}))
        }
        Request::Favorite { component, value } => {
            store.favorite(component, value)?;
            Ok(json!({"favorite":value}))
        }
        Request::Preview { .. } => unreachable!(),
    }
}
fn absolute(path: &str) -> Result<&Path, String> {
    let p = Path::new(path);
    if !p.is_absolute() || p.extension().is_none_or(|s| s != "foldcomponent") {
        return Err("Use an absolute .foldcomponent path".into());
    }
    Ok(p)
}
