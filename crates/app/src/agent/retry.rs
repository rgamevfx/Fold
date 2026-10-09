//! Bounded, session-local request admission. Tombstones retain request identity
//! after response eviction so an uncertain mutation can never execute twice.
use fold_foundation::ObjectId;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REQUEST_IDS: usize = 4096;
pub const MAX_PENDING: usize = 64;
pub const MAX_CACHED_RESPONSES: usize = 64;
pub const MAX_CACHED_BYTES: usize = 32 * 1024 * 1024;
pub const RESPONSE_LIFETIME: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Epoch,
    Budget,
    Capacity,
    IdempotencyConflict,
    Expired,
    InvalidTicket,
}
#[derive(Debug)]
pub enum Admission {
    Execute(Ticket),
    Pending,
    Replay(Arc<[u8]>),
}
/// Only the first admission receives a ticket. Completion must not overwrite a
/// previous outcome. The caller retains this ticket until the job is finalized.
#[derive(Debug)]
#[must_use]
pub struct Ticket {
    epoch: ObjectId,
    request: ObjectId,
    nonce: ObjectId,
}
struct Record {
    fingerprint: [u8; 32],
    length: usize,
    state: State,
}
enum State {
    Pending(ObjectId),
    Ready(Arc<[u8]>),
    RejectedCapacity,
    Expired,
}

pub struct Tracker {
    epoch: ObjectId,
    records: BTreeMap<ObjectId, Record>,
    completed: VecDeque<(ObjectId, Instant)>,
    cached_bytes: usize,
    pending: usize,
}
impl Tracker {
    pub fn new(epoch: ObjectId) -> Self {
        Self {
            epoch,
            records: BTreeMap::new(),
            completed: VecDeque::new(),
            cached_bytes: 0,
            pending: 0,
        }
    }
    /// Call after validating the wire version/envelope, before any side effect.
    /// Hash the original request bytes, not reserialized/normalized JSON.
    pub fn begin(
        &mut self,
        epoch: ObjectId,
        request: ObjectId,
        bytes: &[u8],
        now: Instant,
    ) -> Result<Admission, Error> {
        if epoch != self.epoch {
            return Err(Error::Epoch);
        }
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(Error::Budget);
        }
        self.expire(now);
        let fingerprint: [u8; 32] = Sha256::digest(bytes).into();
        if let Some(record) = self.records.get(&request) {
            if record.length != bytes.len() || record.fingerprint != fingerprint {
                return Err(Error::IdempotencyConflict);
            }
            return match &record.state {
                State::Pending(_) => Ok(Admission::Pending),
                State::Ready(bytes) => Ok(Admission::Replay(bytes.clone())),
                State::RejectedCapacity => Err(Error::Capacity),
                State::Expired => Err(Error::Expired),
            };
        }
        if self.records.len() == MAX_REQUEST_IDS {
            return Err(Error::Capacity);
        }
        if self.pending == MAX_PENDING {
            self.records.insert(
                request,
                Record {
                    fingerprint,
                    length: bytes.len(),
                    state: State::RejectedCapacity,
                },
            );
            return Err(Error::Capacity);
        }
        let nonce = ObjectId::new();
        self.records.insert(
            request,
            Record {
                fingerprint,
                length: bytes.len(),
                state: State::Pending(nonce),
            },
        );
        self.pending += 1;
        Ok(Admission::Execute(Ticket {
            epoch,
            request,
            nonce,
        }))
    }
    /// Cache success and error responses alike. An oversized result leaves the
    /// reservation pending: the owner must finish with a bounded error response,
    /// or abandon the uncertain outcome. It cannot admit the write again.
    pub fn complete(
        &mut self,
        ticket: &Ticket,
        response: Vec<u8>,
        now: Instant,
    ) -> Result<Arc<[u8]>, Error> {
        self.validate(ticket)?;
        if response.len() > MAX_RESPONSE_BYTES {
            return Err(Error::Budget);
        }
        self.expire(now);
        while self.completed.len() >= MAX_CACHED_RESPONSES
            || self.cached_bytes + response.len() > MAX_CACHED_BYTES
        {
            self.evict_oldest();
        }
        let response: Arc<[u8]> = response.into();
        self.cached_bytes += response.len();
        self.records
            .get_mut(&ticket.request)
            .expect("validated ticket")
            .state = State::Ready(response.clone());
        self.pending -= 1;
        self.completed.push_back((ticket.request, now));
        Ok(response)
    }
    /// Retire an interrupted/uncertain job without allowing re-execution. Known
    /// cancellations should instead complete with their normal response bytes.
    pub fn abandon(&mut self, ticket: &Ticket) -> Result<(), Error> {
        self.validate(ticket)?;
        self.records
            .get_mut(&ticket.request)
            .expect("validated ticket")
            .state = State::Expired;
        self.pending -= 1;
        Ok(())
    }
    fn validate(&self, ticket: &Ticket) -> Result<(), Error> {
        if ticket.epoch != self.epoch {
            return Err(Error::Epoch);
        }
        if self.records.get(&ticket.request).is_some_and(
            |record| matches!(record.state, State::Pending(nonce) if nonce == ticket.nonce),
        ) {
            Ok(())
        } else {
            Err(Error::InvalidTicket)
        }
    }
    fn expire(&mut self, now: Instant) {
        while self
            .completed
            .front()
            .is_some_and(|(_, time)| now.saturating_duration_since(*time) >= RESPONSE_LIFETIME)
        {
            self.evict_oldest();
        }
    }
    fn evict_oldest(&mut self) {
        let (id, _) = self
            .completed
            .pop_front()
            .expect("cached response to evict");
        let record = self.records.get_mut(&id).expect("cached request record");
        if let State::Ready(bytes) = &record.state {
            self.cached_bytes -= bytes.len();
        }
        record.state = State::Expired;
    }
}
