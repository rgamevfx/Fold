//! Bounded protocol job ownership. Workers receive cancellation tokens; only their
//! completion tickets can publish outcomes. No project or artist state lives here.
use super::retry::MAX_RESPONSE_BYTES;
use fold_foundation::ObjectId;
use fold_media::Cancel;
use fold_project::Revision;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

pub const MAX_JOBS: usize = 64;
pub const RETENTION: Duration = Duration::from_secs(600);
pub const MAX_RESULT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_LIFETIME: Duration = Duration::from_secs(15 * 60);
pub const MAX_LONG_POLL: Duration = Duration::from_secs(25);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub epoch: ObjectId,
    pub job: ObjectId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Completion {
    Response {
        bytes: Arc<[u8]>,
        committed: Option<Revision>,
    },
    Cancelled,
}
impl Completion {
    fn bytes(&self) -> usize {
        match self {
            Self::Response { bytes, .. } => bytes.len(),
            Self::Cancelled => 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Running { cancellation_requested: bool },
    Finished(Completion),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Epoch,
    Expired,
    Capacity,
    Budget,
    InvalidTicket,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancellation {
    Requested,
    Cancelled,
    Finished,
    AlreadyCommitted(Revision),
}
#[must_use]
pub struct Ticket {
    handle: Handle,
    nonce: ObjectId,
    cancel: Cancel,
}
impl Ticket {
    pub fn cancellation(&self) -> Cancel {
        self.cancel.clone()
    }
}
struct Active {
    nonce: ObjectId,
    cancel: Cancel,
    deadline: Instant,
}
enum Entry {
    Active(Active),
    Finished(Completion),
}
pub struct Jobs {
    epoch: ObjectId,
    entries: BTreeMap<ObjectId, Entry>,
    completed: VecDeque<(ObjectId, Instant)>,
    result_bytes: usize,
}
impl Jobs {
    pub fn new(epoch: ObjectId) -> Self {
        Self {
            epoch,
            entries: BTreeMap::new(),
            completed: VecDeque::new(),
            result_bytes: 0,
        }
    }
    pub fn start(&mut self, lifetime: Duration, now: Instant) -> Result<(Handle, Ticket), Error> {
        if lifetime.is_zero() || lifetime > MAX_LIFETIME {
            return Err(Error::Budget);
        }
        self.maintain(now);
        while self.entries.len() >= MAX_JOBS {
            if self.completed.is_empty() {
                return Err(Error::Capacity);
            }
            self.evict();
        }
        let handle = Handle {
            epoch: self.epoch,
            job: ObjectId::new(),
        };
        let nonce = ObjectId::new();
        let cancel = Cancel::default();
        self.entries.insert(
            handle.job,
            Entry::Active(Active {
                nonce,
                cancel: cancel.clone(),
                deadline: now + lifetime,
            }),
        );
        Ok((
            handle,
            Ticket {
                handle,
                nonce,
                cancel,
            },
        ))
    }
    pub fn status(&mut self, handle: Handle, now: Instant) -> Result<Status, Error> {
        self.check_epoch(handle)?;
        self.maintain(now);
        match self.entries.get(&handle.job).ok_or(Error::Expired)? {
            Entry::Active(active) => Ok(Status::Running {
                cancellation_requested: active.cancel.check().is_err(),
            }),
            Entry::Finished(result) => Ok(Status::Finished(result.clone())),
        }
    }
    /// Requests cooperative cancellation. This does not claim the worker has
    /// stopped, free its capacity slot, or discard a concurrently committed edit.
    pub fn cancel(&mut self, handle: Handle, now: Instant) -> Result<Cancellation, Error> {
        match self.status(handle, now)? {
            Status::Running { .. } => {
                if let Entry::Active(active) = &self.entries[&handle.job] {
                    active.cancel.cancel();
                }
                Ok(Cancellation::Requested)
            }
            Status::Finished(Completion::Cancelled) => Ok(Cancellation::Cancelled),
            Status::Finished(Completion::Response {
                committed: Some(revision),
                ..
            }) => Ok(Cancellation::AlreadyCommitted(revision)),
            Status::Finished(_) => Ok(Cancellation::Finished),
        }
    }
    /// Errors are response bytes too. An oversized response leaves ownership with
    /// the worker so its caller can publish a bounded error instead.
    pub fn complete(
        &mut self,
        ticket: &Ticket,
        result: Completion,
        now: Instant,
    ) -> Result<(), Error> {
        self.check_epoch(ticket.handle)?;
        self.maintain(now);
        if !matches!(self.entries.get(&ticket.handle.job), Some(Entry::Active(active)) if active.nonce == ticket.nonce)
        {
            return Err(Error::InvalidTicket);
        }
        if result.bytes() > MAX_RESPONSE_BYTES {
            return Err(Error::Budget);
        }
        while self.result_bytes + result.bytes() > MAX_RESULT_BYTES {
            self.evict();
        }
        self.result_bytes += result.bytes();
        self.entries
            .insert(ticket.handle.job, Entry::Finished(result));
        self.completed.push_back((ticket.handle.job, now));
        Ok(())
    }
    /// The host must tick this while jobs run, including when no client polls.
    /// Deadlines request cancellation; active work stays owned until acknowledged.
    pub fn maintain(&mut self, now: Instant) {
        for entry in self.entries.values() {
            if let Entry::Active(active) = entry
                && now >= active.deadline
            {
                active.cancel.cancel();
            }
        }
        while self
            .completed
            .front()
            .is_some_and(|(_, time)| now.saturating_duration_since(*time) >= RETENTION)
        {
            self.evict();
        }
    }
    fn check_epoch(&self, handle: Handle) -> Result<(), Error> {
        if handle.epoch == self.epoch {
            Ok(())
        } else {
            Err(Error::Epoch)
        }
    }
    fn evict(&mut self) {
        let (id, _) = self
            .completed
            .pop_front()
            .expect("completed result to evict");
        if let Some(Entry::Finished(result)) = self.entries.remove(&id) {
            self.result_bytes -= result.bytes();
        }
    }
}
impl Drop for Jobs {
    fn drop(&mut self) {
        for entry in self.entries.values() {
            if let Entry::Active(active) = entry {
                active.cancel.cancel();
            }
        }
    }
}
