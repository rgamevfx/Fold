//! Isolated, incrementally editable proposal. Only Project::commit publishes it.
use crate::{CommittedSnapshot, EditBatch, Project, Revision};

#[derive(Clone)]
pub struct Candidate {
    base: Revision,
    snapshot: CommittedSnapshot,
    mutations: Vec<crate::Mutation>,
    generation: u64,
}
impl Candidate {
    pub fn new(snapshot: CommittedSnapshot) -> Self {
        Self {
            base: snapshot.revision(),
            snapshot,
            mutations: Vec::new(),
            generation: 0,
        }
    }
    pub fn base(&self) -> Revision {
        self.base
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// A private fork suitable for existing immutable evaluators, never the live root.
    pub fn snapshot(&self) -> &CommittedSnapshot {
        &self.snapshot
    }
    pub fn apply(&self, batch: EditBatch) -> Result<Self, String> {
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("candidate generation overflow")?;
        let mut workspace = Project::fork(&self.snapshot);
        workspace.commit(batch.clone()).map_err(|e| e.to_string())?;
        let mut mutations = self.mutations.clone();
        mutations.extend(batch.mutations);
        if mutations.len() > 4096 {
            return Err("candidate exceeds 4096 mutations; discard or commit".into());
        }
        let retained_bytes: usize = mutations
            .iter()
            .filter_map(|m| match m {
                crate::Mutation::PutDocument(d) => Some(d.payload.len()),
                _ => None,
            })
            .sum();
        if retained_bytes > 64 * 1024 * 1024 {
            return Err("candidate mutation payload exceeds 64 MiB; discard or commit".into());
        }
        let snapshot = workspace.snapshot();
        let bytes: usize = snapshot
            .state()
            .documents
            .values()
            .map(|d| d.payload.len())
            .sum();
        if bytes > 64 * 1024 * 1024 {
            return Err("candidate document payload exceeds 64 MiB".into());
        }
        Ok(Self {
            base: self.base,
            snapshot,
            mutations,
            generation,
        })
    }
    pub fn proposal(&self) -> EditBatch {
        EditBatch {
            base: self.base,
            mutations: self.mutations.clone(),
        }
    }
}
