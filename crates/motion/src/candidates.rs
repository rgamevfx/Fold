//! Session-owned immutable candidate lifecycle. Transport envelopes and retry
//! records live above this service; no artist state or unlock route is exposed.
use crate::{diff, editing, model::Construction, source};
use fold_foundation::{DocumentId, ObjectId};
use fold_project::{CommittedSnapshot, Project, Revision};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

pub const MAX_LIVE: usize = 4;
pub const IDLE_LIFETIME: Duration = Duration::from_secs(15 * 60);
const MAX_TERMINAL: usize = 64;
const TERMINAL_LIFETIME: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub epoch: ObjectId,
    pub generation: ObjectId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Receipt(ObjectId);
#[derive(Debug, Serialize)]
pub struct Inspection {
    pub handle: Handle,
    pub receipt: Receipt,
    pub base: Revision,
    pub source: String,
    pub changes: Vec<diff::Change>,
}
/// Owned immutable projection input; contains no project mutation capability.
pub struct InspectionTask {
    handle: Handle,
    receipt: Receipt,
    candidate: Arc<editing::Candidate>,
}
/// Non-forgeable evidence that source and semantic diff were successfully built.
pub struct InspectionProof {
    handle: Handle,
    receipt: Receipt,
}
impl InspectionTask {
    pub fn run(self) -> Result<(Inspection, InspectionProof), Error> {
        let value = Construction::from_document(self.candidate.document())
            .map_err(editing::Error::Invalid)?;
        let source = source::canonical(&value).map_err(editing::Error::Invalid)?;
        let changes = self.candidate.diff()?;
        Ok((
            Inspection {
                handle: self.handle,
                receipt: self.receipt,
                base: self.candidate.base_revision(),
                source,
                changes,
            },
            InspectionProof {
                handle: self.handle,
                receipt: self.receipt,
            },
        ))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Epoch,
    Expired,
    Capacity,
    Uninspected,
    Conflict,
    Cancelled,
    AlreadyCommitted(Revision),
    Edit(editing::Error),
}
impl From<editing::Error> for Error {
    fn from(value: editing::Error) -> Self {
        Self::Edit(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancellation {
    Cancelled,
    AlreadyCommitted(Revision),
}
struct Live {
    candidate: Arc<editing::Candidate>,
    touched: Instant,
    receipt: Receipt,
    inspected: bool,
}
#[derive(Clone, Copy)]
enum Terminal {
    Cancelled,
    Committed(Revision),
}
struct Retired {
    id: ObjectId,
    touched: Instant,
    state: Terminal,
    document: DocumentId,
    scope: editing::Scope,
}
/// Create a fresh store whenever a project session is opened/replaced. Handles
/// cannot cross epochs, and generations never change their content or base.
pub struct Store {
    epoch: ObjectId,
    live: BTreeMap<ObjectId, Live>,
    terminal: VecDeque<Retired>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            epoch: ObjectId::new(),
            live: BTreeMap::new(),
            terminal: VecDeque::new(),
        }
    }
}
impl Store {
    pub fn epoch(&self) -> ObjectId {
        self.epoch
    }
    pub fn insert(&mut self, candidate: editing::Candidate, now: Instant) -> Result<Handle, Error> {
        self.expire(now);
        if self.live.len() >= MAX_LIVE {
            return Err(Error::Capacity);
        }
        let generation = ObjectId::new();
        self.live.insert(
            generation,
            Live {
                candidate: Arc::new(candidate),
                touched: now,
                receipt: Receipt(ObjectId::new()),
                inspected: false,
            },
        );
        Ok(Handle {
            epoch: self.epoch,
            generation,
        })
    }
    /// The receipt is returned only with successful inspection, never proposal.
    /// It stays valid for this generation; a rebased generation needs its own.
    pub fn inspect(&mut self, handle: Handle, now: Instant) -> Result<Inspection, Error> {
        let (inspection, proof) = self.prepare_inspection(handle, now)?.run()?;
        self.publish_inspection(proof, now)?;
        Ok(inspection)
    }
    /// Retain immutable input without granting Apply authority. Projection can run
    /// on a worker; only successful publication authorizes the returned receipt.
    pub fn prepare_inspection(
        &mut self,
        handle: Handle,
        now: Instant,
    ) -> Result<InspectionTask, Error> {
        self.check(handle, now)?;
        let live = self
            .live
            .get_mut(&handle.generation)
            .expect("checked handle");
        live.touched = now;
        Ok(InspectionTask {
            handle,
            receipt: live.receipt,
            candidate: live.candidate.clone(),
        })
    }
    pub fn publish_inspection(
        &mut self,
        proof: InspectionProof,
        now: Instant,
    ) -> Result<(), Error> {
        self.check(proof.handle, now)?;
        let live = self
            .live
            .get_mut(&proof.handle.generation)
            .expect("checked handle");
        if live.receipt != proof.receipt {
            return Err(Error::Uninspected);
        }
        live.inspected = true;
        live.touched = now;
        Ok(())
    }
    /// CPU observation may borrow the immutable candidate without marking it
    /// inspected. Observation adapters must not leak an Apply receipt.
    pub fn document(
        &mut self,
        handle: Handle,
        now: Instant,
    ) -> Result<&fold_project::Document, Error> {
        self.check(handle, now)?;
        let live = self
            .live
            .get_mut(&handle.generation)
            .expect("checked live handle");
        live.touched = now;
        Ok(live.candidate.document())
    }
    pub fn shared(
        &mut self,
        handle: Handle,
        now: Instant,
    ) -> Result<Arc<editing::Candidate>, Error> {
        self.check(handle, now)?;
        let live = self
            .live
            .get_mut(&handle.generation)
            .expect("live candidate");
        live.touched = now;
        Ok(live.candidate.clone())
    }
    pub fn base_revision(&mut self, handle: Handle, now: Instant) -> Result<Revision, Error> {
        self.check(handle, now)?;
        Ok(self.live[&handle.generation].candidate.base_revision())
    }
    pub fn target(
        &mut self,
        handle: Handle,
        now: Instant,
    ) -> Result<(DocumentId, editing::Scope), Error> {
        match self.check(handle, now) {
            Ok(()) => {
                let candidate = &self.live[&handle.generation].candidate;
                Ok((candidate.document().id, candidate.scope()))
            }
            Err(Error::Cancelled | Error::AlreadyCommitted(_)) => {
                let record = self
                    .terminal
                    .iter()
                    .find(|r| r.id == handle.generation)
                    .expect("retired handle");
                Ok((record.document, record.scope))
            }
            Err(error) => Err(error),
        }
    }
    pub fn rebase(
        &mut self,
        handle: Handle,
        current: &CommittedSnapshot,
        now: Instant,
    ) -> Result<Handle, Error> {
        self.check(handle, now)?;
        if self.live.len() >= MAX_LIVE {
            return Err(Error::Capacity);
        }
        let candidate = self.live[&handle.generation].candidate.rebase(current)?;
        let next = self.insert(candidate, now)?;
        self.live
            .get_mut(&handle.generation)
            .expect("retained generation")
            .touched = now;
        Ok(next)
    }
    pub fn apply(
        &mut self,
        handle: Handle,
        receipt: Receipt,
        expected_base: Revision,
        project: &mut Project,
        now: Instant,
    ) -> Result<Revision, Error> {
        self.check(handle, now)?;
        let live = &self.live[&handle.generation];
        if !live.inspected || live.receipt != receipt {
            return Err(Error::Uninspected);
        }
        if live.candidate.base_revision() != expected_base
            || project.snapshot().revision() != expected_base
        {
            return Err(Error::Conflict);
        }
        // A failed commit leaves the immutable generation available for inspection
        // or explicit rebase. Retire only after the project confirms the commit.
        let revision = live.candidate.as_ref().clone().apply(project)?.revision();
        self.retire(handle.generation, now, Terminal::Committed(revision));
        Ok(revision)
    }
    pub fn cancel(&mut self, handle: Handle, now: Instant) -> Result<Cancellation, Error> {
        match self.check(handle, now) {
            Err(Error::AlreadyCommitted(revision)) => {
                return Ok(Cancellation::AlreadyCommitted(revision));
            }
            Err(Error::Cancelled) => return Ok(Cancellation::Cancelled),
            result => result?,
        }
        self.retire(handle.generation, now, Terminal::Cancelled);
        Ok(Cancellation::Cancelled)
    }
    fn check(&mut self, handle: Handle, now: Instant) -> Result<(), Error> {
        if handle.epoch != self.epoch {
            return Err(Error::Epoch);
        }
        self.expire(now);
        if self.live.contains_key(&handle.generation) {
            return Ok(());
        }
        match self
            .terminal
            .iter()
            .find(|r| r.id == handle.generation)
            .map(|r| &r.state)
        {
            Some(Terminal::Cancelled) => Err(Error::Cancelled),
            Some(Terminal::Committed(revision)) => Err(Error::AlreadyCommitted(*revision)),
            None => Err(Error::Expired),
        }
    }
    fn expire(&mut self, now: Instant) {
        self.live
            .retain(|_, entry| now.saturating_duration_since(entry.touched) < IDLE_LIFETIME);
        self.terminal
            .retain(|r| now.saturating_duration_since(r.touched) < TERMINAL_LIFETIME);
    }
    fn retire(&mut self, id: ObjectId, now: Instant, state: Terminal) {
        if self.terminal.len() == MAX_TERMINAL {
            self.terminal.pop_front();
        }
        let live = self.live.remove(&id).expect("retiring live generation");
        self.terminal.push_back(Retired {
            id,
            touched: now,
            state,
            document: live.candidate.document().id,
            scope: live.candidate.scope(),
        });
    }
}
