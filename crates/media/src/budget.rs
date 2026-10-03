//! Aggregate accounting follows allocation ownership, including caller-held
//! decoded leases. Cache eviction alone does not release referenced storage.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Debug)]
pub(crate) struct Budget {
    used: AtomicU64,
    peak: AtomicU64,
    limit: u64,
}
impl Budget {
    pub const fn new(limit: u64) -> Self {
        Self {
            used: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            limit,
        }
    }
    pub fn reserve(&'static self, bytes: u64) -> Result<Arc<Lease>, &'static str> {
        let previous = self
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|n| *n <= self.limit)
            })
            .map_err(
                |_| "aggregate media storage budget exhausted; release unused leases and retry",
            )?;
        self.peak.fetch_max(previous + bytes, Ordering::AcqRel);
        Ok(Arc::new(Lease {
            budget: self,
            bytes,
        }))
    }
    pub fn usage(&self) -> Usage {
        Usage {
            bytes: self.used.load(Ordering::Acquire),
            peak: self.peak.load(Ordering::Acquire),
            budget: self.limit,
        }
    }
}
#[derive(Debug)]
pub struct Lease {
    budget: &'static Budget,
    bytes: u64,
}
impl Lease {
    pub(crate) fn shrink(mut self: Arc<Self>, bytes: u64) -> Arc<Self> {
        let lease = Arc::get_mut(&mut self).expect("only unpublished reservations may shrink");
        assert!(bytes <= lease.bytes);
        lease
            .budget
            .used
            .fetch_sub(lease.bytes - bytes, Ordering::AcqRel);
        lease.bytes = bytes;
        self
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Usage {
    pub bytes: u64,
    pub peak: u64,
    pub budget: u64,
}
pub(crate) static DECODED: Budget = Budget::new(256 * 1024 * 1024);
pub(crate) static PIPE: Budget = Budget::new(64 * 1024 * 1024);
pub fn pipe_usage() -> Usage {
    PIPE.usage()
}
pub(crate) static PCM: Budget = Budget::new(512 * 1024 * 1024);
pub(crate) static WAVE_COPY: Budget = Budget::new(512 * 1024 * 1024);
pub fn decoded_usage() -> Usage {
    DECODED.usage()
}
pub fn pcm_disk_usage() -> Usage {
    PCM.usage()
}
pub fn wave_copy_usage() -> Usage {
    WAVE_COPY.usage()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn referenced_leases_remain_accounted_and_pressure_recovers() {
        static BUDGET: Budget = Budget::new(16);
        let first = BUDGET.reserve(12).unwrap();
        let held = first.clone();
        drop(first);
        assert_eq!(BUDGET.usage().bytes, 12);
        assert!(BUDGET.reserve(8).is_err());
        drop(held);
        let next = BUDGET.reserve(16).unwrap();
        assert_eq!(BUDGET.usage().peak, 16);
        assert!(BUDGET.reserve(u64::MAX).is_err());
        drop(next);
        assert_eq!(BUDGET.usage().bytes, 0);
    }
}

static WORKING: Budget = Budget::new(512 * 1024 * 1024);
static OUTPUT: Budget = Budget::new(64 * 1024 * 1024);
static DELIVERY: Budget = Budget::new(4 * 1024 * 1024 * 1024);
static DELIVERY_PCM: Budget = Budget::new(256 * 1024 * 1024);
pub fn working_usage() -> Usage {
    WORKING.usage()
}
pub fn output_usage() -> Usage {
    OUTPUT.usage()
}
pub fn delivery_disk_usage() -> Usage {
    DELIVERY.usage()
}
pub fn delivery_pcm_usage() -> Usage {
    DELIVERY_PCM.usage()
}
pub fn reserve_working(bytes: u64) -> Result<Arc<Lease>, &'static str> {
    WORKING.reserve(bytes)
}
pub fn reserve_output(bytes: u64) -> Result<Arc<Lease>, &'static str> {
    OUTPUT.reserve(bytes)
}
pub fn reserve_delivery(bytes: u64) -> Result<Arc<Lease>, &'static str> {
    DELIVERY.reserve(bytes)
}
pub fn reserve_delivery_pcm(bytes: u64) -> Result<Arc<Lease>, &'static str> {
    DELIVERY_PCM.reserve(bytes)
}
