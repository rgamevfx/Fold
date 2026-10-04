//! Asset-only linking service. One bounded worker per service; no UI, placement,
//! transport or delivery mutation. Proposals carry revision and cancellation.
use fold_foundation::AssetId;
use fold_media::{
    Cancel,
    ingest::{
        INTERPRETATION_KEY, InputInterpretation, METADATA_KEY, SourceMetadata, inspect_linked,
    },
};
use fold_project::{
    Asset, CommittedSnapshot, EditBatch, Item, ItemId, Metadata, Mutation, Parent, Project,
    Revision, Snapshot,
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
};

fn document_name(snapshot: &Snapshot, id: fold_foundation::DocumentId) -> &str {
    snapshot
        .state()
        .organization
        .items
        .iter()
        .find(|item| item.id == ItemId::Document(id))
        .map(|item| item.name.as_str())
        .unwrap_or("Document")
}

#[derive(Clone, Debug)]
pub struct ImportRequest {
    pub base: Revision,
    pub destination: Parent,
    pub paths: Vec<PathBuf>,
}
#[derive(Clone, Debug)]
pub struct RelinkRequest {
    pub base: Revision,
    pub asset: AssetId,
    pub path: PathBuf,
}

pub struct IngestProposal {
    batch: EditBatch,
    cancel: Cancel,
    pub items: Vec<ItemId>,
}
impl IngestProposal {
    pub(crate) fn into_batch(self) -> Result<EditBatch, String> {
        self.cancel.check()?;
        Ok(self.batch)
    }

    /// Check cancellation again at publication, including completed-but-unclaimed jobs.
    pub fn commit(self, project: &mut Project) -> Result<CommittedSnapshot, String> {
        self.cancel.check()?;
        if project.snapshot().revision() != self.batch.base {
            return Err("stale ingest proposal".into());
        }
        if self.batch.mutations.is_empty() {
            return Ok(project.snapshot());
        }
        project.commit(self.batch).map_err(|e| e.to_string())
    }
}

// Legacy archives may contain relative or symlink locations. All new links are
// canonical; resolving an existing location is worker-only and never rewrites it.
fn same_location(asset: &Asset, canonical: &std::path::Path) -> bool {
    std::path::Path::new(&asset.location) == canonical
        || std::fs::canonicalize(&asset.location).is_ok_and(|path| path == canonical)
}

fn check_base(snapshot: &Snapshot, base: Revision, cancel: &Cancel) -> Result<(), String> {
    cancel.check()?;
    if snapshot.revision() != base {
        return Err("stale ingest request".into());
    }
    Ok(())
}

/// Worker-side implementation, also usable by headless callers on their I/O thread.
pub fn import(
    snapshot: &Snapshot,
    request: &ImportRequest,
    cancel: &Cancel,
) -> Result<IngestProposal, String> {
    let _permit = fold_render::scheduling::Scheduler::shared()
        .enter(fold_render::scheduling::Class::Background, cancel)?;
    check_base(snapshot, request.base, cancel)?;
    if request.paths.is_empty() || request.paths.len() > 32 {
        return Err("import requires 1–32 files".into());
    }
    if let Parent::Bin(id) = request.destination
        && !snapshot.state().organization.bins.contains_key(&id)
    {
        return Err("missing import bin".into());
    }
    let mut assets = snapshot.state().assets.clone();
    let mut mutations = Vec::new();
    let mut items = Vec::new();
    for path in &request.paths {
        let source =
            inspect_linked(path, cancel).map_err(|e| format!("{}: {e}", path.display()))?;
        let location = source
            .path
            .to_str()
            .ok_or("media path must be UTF-8")?
            .to_owned();
        let matches: Vec<_> = assets
            .values()
            .filter(|a| same_location(a, &source.path))
            .collect();
        if matches.iter().any(|a| a.fingerprint != source.fingerprint) {
            return Err(format!(
                "{} changed: explicit relink required",
                source.path.display()
            ));
        }
        if let Some(existing) = matches.first() {
            items.push(ItemId::Asset(existing.id));
            continue;
        }
        let interpretation = InputInterpretation {
            color_space: None,
            alpha: None,
            provenance: "unassigned; consult probed facts (no OCIO transform selected)".into(),
        };
        let asset = Asset {
            id: AssetId::new(),
            location,
            fingerprint: source.fingerprint,
            extensions: Metadata::from([
                (
                    METADATA_KEY.into(),
                    serde_json::to_value(source.metadata).map_err(|e| e.to_string())?,
                ),
                (
                    INTERPRETATION_KEY.into(),
                    serde_json::to_value(interpretation).map_err(|e| e.to_string())?,
                ),
            ]),
        };
        let id = ItemId::Asset(asset.id);
        let item = Item {
            id,
            name: source
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Media")
                .into(),
            parent: request.destination,
            order: (snapshot.state().organization.items.len() + items.len()) as u64,
            extensions: Metadata::new(),
        };
        assets.insert(asset.id, std::sync::Arc::new(asset.clone()));
        mutations.extend([Mutation::PutAsset(asset), Mutation::PutItem(item)]);
        items.push(id);
    }
    cancel.check()?;
    Ok(IngestProposal {
        batch: EditBatch {
            base: request.base,
            mutations,
        },
        cancel: cancel.clone(),
        items,
    })
}

pub fn relink(
    snapshot: &Snapshot,
    request: &RelinkRequest,
    cancel: &Cancel,
) -> Result<IngestProposal, String> {
    let _permit = fold_render::scheduling::Scheduler::shared()
        .enter(fold_render::scheduling::Class::Background, cancel)?;
    check_base(snapshot, request.base, cancel)?;
    let previous = snapshot
        .state()
        .assets
        .get(&request.asset)
        .ok_or("missing relink asset")?;
    let source = inspect_linked(&request.path, cancel)?;
    let location = source
        .path
        .to_str()
        .ok_or("media path must be UTF-8")?
        .to_owned();
    if snapshot
        .state()
        .assets
        .values()
        .any(|a| a.id != request.asset && same_location(a, &source.path))
    {
        return Err("relink destination already belongs to another asset".into());
    }
    let old_metadata = previous
        .extensions
        .get(METADATA_KEY)
        .map(|value| {
            serde_json::from_value::<SourceMetadata>(value.clone()).map_err(|e| e.to_string())
        })
        .transpose()?;
    let registry = crate::packages::builtins();
    for document in snapshot.state().documents.values() {
        cancel.check()?;
        // Unknown payloads may contain undeclared uses: never assume safety.
        if !registry.supports(document) {
            return Err(format!(
                "cannot prove relink safety: unavailable provider for {}",
                document_name(snapshot, document.id)
            ));
        }
        registry.validate_document(document).map_err(|e| {
            format!(
                "cannot prove relink safety for {}: {e}",
                document_name(snapshot, document.id)
            )
        })?;
        if document.assets.contains(&request.asset) {
            let old = old_metadata.as_ref().ok_or(
                "referenced legacy asset lacks verified stream metadata; relink cannot prove compatibility",
            )?;
            if !old.compatible_with(&source.metadata) {
                return Err(format!(
                    "incompatible relink for {}",
                    document_name(snapshot, document.id)
                ));
            }
            registry.validate_relink(document, request.asset, &source.metadata)?;
        }
    }
    let mut asset = (**previous).clone();
    asset.location = location;
    asset.fingerprint = source.fingerprint;
    asset.extensions.insert(
        METADATA_KEY.into(),
        serde_json::to_value(source.metadata).map_err(|e| e.to_string())?,
    );
    cancel.check()?;
    Ok(IngestProposal {
        batch: EditBatch {
            base: request.base,
            mutations: vec![Mutation::PutAsset(asset)],
        },
        cancel: cancel.clone(),
        items: vec![ItemId::Asset(request.asset)],
    })
}

/// Usage diagnostics are conservative and include delivery and unavailable providers.
/// Deletion never touches a source file and never cascades through authored uses.
pub fn delete_item(snapshot: &Snapshot, id: ItemId) -> Result<EditBatch, Vec<String>> {
    let mut uses = Vec::new();
    if !snapshot
        .state()
        .organization
        .items
        .iter()
        .any(|i| i.id == id)
    {
        return Err(vec!["missing project item".into()]);
    }
    let registry = crate::packages::builtins();
    for document in snapshot.state().documents.values() {
        if let Err(error) = registry.validate_document(document) {
            uses.push(format!(
                "unavailable provider or invalid document {}: {error}",
                document_name(snapshot, document.id)
            ));
        }
        let referenced = match id {
            ItemId::Asset(asset) => document.assets.contains(&asset),
            ItemId::Document(target) => document.dependencies.iter().any(|r| r.document == target),
        };
        if referenced {
            uses.push(format!(
                "authored use: {}",
                document_name(snapshot, document.id)
            ));
        }
    }
    if let ItemId::Document(target) = id
        && let Some(value) = snapshot.state().settings.get("fold.output")
    {
        match serde_json::from_value::<fold_project::DocumentRef>(value.clone()) {
            Ok(reference) if reference.document != target => {}
            _ => uses.push("delivery output (or invalid delivery reference)".into()),
        }
    }
    if !uses.is_empty() {
        return Err(uses);
    }
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![match id {
            ItemId::Asset(id) => Mutation::RemoveAsset(id),
            ItemId::Document(id) => Mutation::RemoveDocument(id),
        }],
    })
}

#[derive(Default)]
pub struct IngestWorker {
    pending: Option<Receiver<Result<IngestProposal, String>>>,
    cancel: Cancel,
}
impl IngestWorker {
    fn start(
        &mut self,
        work: impl FnOnce(Cancel) -> Result<IngestProposal, String> + Send + 'static,
    ) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("ingest worker busy; poll completion before submitting again".into());
        }
        self.cancel = Cancel::default();
        let cancel = self.cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("fold-ingest".into())
            .spawn(move || {
                let _ = sender.send(work(cancel));
            })
            .map_err(|e| e.to_string())?;
        self.pending = Some(receiver);
        Ok(())
    }
    pub fn import(&mut self, snapshot: Snapshot, request: ImportRequest) -> Result<(), String> {
        self.start(move |cancel| import(&snapshot, &request, &cancel))
    }
    pub fn relink(&mut self, snapshot: Snapshot, request: RelinkRequest) -> Result<(), String> {
        self.start(move |cancel| relink(&snapshot, &request, &cancel))
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub fn poll(&mut self) -> Option<Result<IngestProposal, String>> {
        let result = match self.pending.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("ingest worker terminated".into()),
        };
        self.pending = None;
        Some(result)
    }
}
impl Drop for IngestWorker {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
