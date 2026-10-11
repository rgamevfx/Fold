//! Aggregate accounting follows allocation ownership, including caller-held
//! decoded leases. Cache eviction alone does not release referenced storage.
use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicU64, Ordering},
};

#[derive(Debug)]
pub(crate) struct Budget {
    used: AtomicU64,
    peak: AtomicU64,
    limit: AtomicU64,
}
impl Budget {
    pub const fn new(limit: u64) -> Self {
        Self {
            used: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            limit: AtomicU64::new(limit),
        }
    }
    pub fn reserve(&'static self, bytes: u64) -> Result<Arc<Lease>, fold_foundation::Failure> {
        if bytes > self.limit.load(Ordering::Acquire) {
            return Err(fold_foundation::Failure::new(
                fold_foundation::FailureKind::RequestTooLarge,
                format!("Allocation needs {:.1} MiB; the configured resource allowance is {:.1} MiB. Increase the relevant budget or reduce the request.", bytes as f64 / 1048576., self.limit.load(Ordering::Acquire) as f64 / 1048576.),
            )
            .context(bytes));
        }
        let previous = self
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|n| *n <= self.limit.load(Ordering::Acquire))
            })
            .map_err(|_| {
                fold_foundation::Failure::new(
                    fold_foundation::FailureKind::ResourcePressure,
                    "aggregate media storage budget exhausted; release unused leases and retry",
                )
            })?;
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
            budget: self.limit.load(Ordering::Acquire),
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
static PREPARATION: LazyLock<Budget> = LazyLock::new(|| Budget::new(automatic_image_memory() / 6));

/// Admission allowance, not additional physical storage: prepared samples also
/// retain their normal decoded leases. Leave half the decode budget for demand.
pub fn reserve_preparation(bytes: u64) -> Result<Arc<Lease>, fold_foundation::Failure> {
    PREPARATION.reserve(bytes)
}
pub fn preparation_usage() -> Usage {
    PREPARATION.usage()
}

pub(crate) static DECODED: LazyLock<Budget> =
    LazyLock::new(|| Budget::new(automatic_image_memory() / 3));
// One 4K 12-bit 4:4:4+A frame exceeds 64 MiB. All decoder pipes share this cap.
pub(crate) static PIPE: Budget = Budget::new(128 * 1024 * 1024);
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
        assert_eq!(
            BUDGET.reserve(8).unwrap_err().kind,
            fold_foundation::FailureKind::ResourcePressure
        );
        drop(held);
        let next = BUDGET.reserve(16).unwrap();
        assert_eq!(BUDGET.usage().peak, 16);
        assert_eq!(
            BUDGET.reserve(u64::MAX).unwrap_err().kind,
            fold_foundation::FailureKind::RequestTooLarge
        );
        drop(next);
        assert_eq!(BUDGET.usage().bytes, 0);
    }
}

static WORKING: LazyLock<Budget> = LazyLock::new(|| {
    let total = automatic_image_memory();
    Budget::new(total - total / 3)
});
static OUTPUT: LazyLock<Budget> = LazyLock::new(|| Budget::new(automatic_image_memory() / 6));
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
pub fn reserve_working(bytes: u64) -> Result<Arc<Lease>, fold_foundation::Failure> {
    WORKING.reserve(bytes)
}
pub fn reserve_output(bytes: u64) -> Result<Arc<Lease>, fold_foundation::Failure> {
    OUTPUT.reserve(bytes)
}
pub fn reserve_delivery(bytes: u64) -> Result<Arc<Lease>, fold_foundation::Failure> {
    DELIVERY.reserve(bytes)
}
pub fn reserve_delivery_pcm(bytes: u64) -> Result<Arc<Lease>, fold_foundation::Failure> {
    DELIVERY_PCM.reserve(bytes)
}

/// Shared CPU image allowance: one third decoded planes, two thirds working images.
/// Pipe, output, project/history and allocator overhead remain separately bounded.
pub fn configure_image_memory(bytes: u64) {
    OUTPUT.limit.store(bytes / 6, Ordering::Release);
    DECODED.limit.store(bytes / 3, Ordering::Release);
    PREPARATION.limit.store(bytes / 6, Ordering::Release);
    WORKING.limit.store(bytes - bytes / 3, Ordering::Release);
}

/// Automatic CPU image allowance, sampled once at process startup/first use.
pub fn automatic_image_memory() -> u64 {
    crate::system_memory::automatic()
}
