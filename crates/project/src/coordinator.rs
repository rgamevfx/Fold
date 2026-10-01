use crate::{Asset, Document, Metadata, ProjectState, Revision, Snapshot};
use fold_foundation::{AssetId, DocumentId};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub enum Mutation {
    PutDocument(Document),
    RemoveDocument(DocumentId),
    PutAsset(Asset),
    RemoveAsset(AssetId),
    SetSettings(Metadata),
    SetEnvironment(Metadata),
}

#[derive(Clone, Debug)]
pub struct EditBatch {
    pub base: Revision,
    pub mutations: Vec<Mutation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectError {
    StaleRevision {
        expected: Revision,
        actual: Revision,
    },
    RevisionOverflow,
    MissingDocument(DocumentId),
    MissingAsset(AssetId),
    InvalidDocument(DocumentId),
    InvalidAsset(AssetId),
    Cycle,
    EmptyEdit,
    NoUndo,
    NoRedo,
}
impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProjectError {}

/// A transient overlay, deliberately not a committed Snapshot accepted by save.
/// Replace its batch on each update; consuming it commits once, dropping cancels.
pub struct EditSession {
    batch: EditBatch,
    state: Arc<ProjectState>,
    generation: u64,
}
impl EditSession {
    pub fn state(&self) -> &ProjectState {
        &self.state
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

struct HistoryEntry {
    before: Arc<ProjectState>,
    after: Arc<ProjectState>,
}

/// Single writer. History is bounded by entry count; each entry shares unchanged roots.
pub struct Project {
    current: Arc<ProjectState>,
    undo: VecDeque<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    history_limit: usize,
}

impl Project {
    pub fn new(history_limit: usize) -> Self {
        Self::from_state(ProjectState::default(), history_limit)
    }

    pub(crate) fn from_state(state: ProjectState, history_limit: usize) -> Self {
        Self {
            current: Arc::new(state),
            undo: VecDeque::new(),
            redo: Vec::new(),
            history_limit,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot(self.current.clone())
    }

    fn next_revision(&self) -> Result<Revision, ProjectError> {
        self.current
            .revision
            .0
            .checked_add(1)
            .map(Revision)
            .ok_or(ProjectError::RevisionOverflow)
    }

    fn stage(&self, batch: &EditBatch) -> Result<Arc<ProjectState>, ProjectError> {
        if batch.base != self.current.revision {
            return Err(ProjectError::StaleRevision {
                expected: batch.base,
                actual: self.current.revision,
            });
        }
        if batch.mutations.is_empty() {
            return Err(ProjectError::EmptyEdit);
        }
        let mut state = (*self.current).clone();
        state.revision = self.next_revision()?;
        for mutation in &batch.mutations {
            match mutation {
                Mutation::PutDocument(document) => {
                    let mut document = document.clone();
                    document.revision = state.revision;
                    state.documents.insert(document.id, Arc::new(document));
                }
                Mutation::RemoveDocument(id) => {
                    state
                        .documents
                        .remove(id)
                        .ok_or(ProjectError::MissingDocument(*id))?;
                }
                Mutation::PutAsset(asset) => {
                    state.assets.insert(asset.id, Arc::new(asset.clone()));
                }
                Mutation::RemoveAsset(id) => {
                    state
                        .assets
                        .remove(id)
                        .ok_or(ProjectError::MissingAsset(*id))?;
                }
                Mutation::SetSettings(settings) => state.settings = settings.clone(),
                Mutation::SetEnvironment(environment) => state.environment = environment.clone(),
            }
        }
        validate(&state)?;
        Ok(Arc::new(state))
    }

    /// Validate a transient proposal without publishing or changing history.
    /// Hosts must keep this render-only snapshot separate from save/export roots.
    pub fn preview(&self, batch: &EditBatch) -> Result<Snapshot, ProjectError> {
        self.stage(batch).map(Snapshot)
    }

    pub fn commit(&mut self, batch: EditBatch) -> Result<Snapshot, ProjectError> {
        let after = self.stage(&batch)?;
        let before = std::mem::replace(&mut self.current, after.clone());
        self.redo.clear();
        if self.history_limit > 0 {
            self.undo.push_back(HistoryEntry { before, after });
            if self.undo.len() > self.history_limit {
                self.undo.pop_front();
            }
        }
        Ok(self.snapshot())
    }

    // Undo restores content but always publishes a fresh project revision, so
    // asynchronous proposals cannot accidentally match an old revision after undo.
    pub fn undo(&mut self) -> Result<Snapshot, ProjectError> {
        let entry = self.undo.back().ok_or(ProjectError::NoUndo)?;
        let mut state = (*entry.before).clone();
        state.revision = self.next_revision()?;
        self.current = Arc::new(state);
        self.redo.push(self.undo.pop_back().unwrap());
        Ok(self.snapshot())
    }

    pub fn redo(&mut self) -> Result<Snapshot, ProjectError> {
        let entry = self.redo.last().ok_or(ProjectError::NoRedo)?;
        let mut state = (*entry.after).clone();
        state.revision = self.next_revision()?;
        self.current = Arc::new(state);
        self.undo.push_back(self.redo.pop().unwrap());
        Ok(self.snapshot())
    }

    pub fn begin_edit(&self) -> EditSession {
        EditSession {
            batch: EditBatch {
                base: self.current.revision,
                mutations: Vec::new(),
            },
            state: self.current.clone(),
            generation: 0,
        }
    }

    pub fn update_edit(
        &self,
        session: &mut EditSession,
        mutations: Vec<Mutation>,
    ) -> Result<(), ProjectError> {
        let batch = EditBatch {
            base: session.batch.base,
            mutations,
        };
        let state = self.stage(&batch)?;
        let generation = session
            .generation
            .checked_add(1)
            .ok_or(ProjectError::RevisionOverflow)?;
        session.batch = batch;
        session.state = state;
        session.generation = generation;
        Ok(())
    }

    pub fn commit_edit(&mut self, session: EditSession) -> Result<Snapshot, ProjectError> {
        self.commit(session.batch)
    }
}

// Flattened metadata must not emit duplicate authoritative fields on save.
fn has_reserved_key(metadata: &Metadata, reserved: &[&str]) -> bool {
    reserved.iter().any(|key| metadata.contains_key(*key))
}

pub(crate) fn validate(state: &ProjectState) -> Result<(), ProjectError> {
    for (id, asset) in &state.assets {
        if *id != asset.id
            || has_reserved_key(&asset.extensions, &["id", "location", "fingerprint"])
        {
            return Err(ProjectError::InvalidAsset(*id));
        }
    }
    let mut incoming = BTreeMap::new();
    for (id, document) in &state.documents {
        if *id != document.id
            || document.type_id.is_empty()
            || document.package_id.is_empty()
            || document.revision > state.revision
            || has_reserved_key(
                &document.extensions,
                &[
                    "id",
                    "type_id",
                    "package_id",
                    "schema_version",
                    "revision",
                    "dependencies",
                    "assets",
                    "payload",
                ],
            )
        {
            return Err(ProjectError::InvalidDocument(*id));
        }
        incoming.insert(*id, 0usize);
        for asset in &document.assets {
            if !state.assets.contains_key(asset) {
                return Err(ProjectError::MissingAsset(*asset));
            }
        }
    }
    for document in state.documents.values() {
        for reference in &document.dependencies {
            if reference.output.is_empty()
                || has_reserved_key(&reference.extensions, &["document", "output"])
            {
                return Err(ProjectError::InvalidDocument(document.id));
            }
            *incoming
                .get_mut(&reference.document)
                .ok_or(ProjectError::MissingDocument(reference.document))? += 1;
        }
    }
    // Iterative topological validation avoids stack overflow on deep nesting.
    let mut ready: Vec<_> = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        for reference in &state.documents[&id].dependencies {
            let count = incoming.get_mut(&reference.document).unwrap();
            *count -= 1;
            if *count == 0 {
                ready.push(reference.document);
            }
        }
    }
    if visited != state.documents.len() {
        return Err(ProjectError::Cycle);
    }
    Ok(())
}
