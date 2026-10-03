//! Process-wide immutable video-source copies. Workers share verified bytes, not
//! mutable linked files. The registry holds weak references, never a disk cache.
use crate::{Cancel, VideoSource, video};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(test)]
#[path = "source_pin_tests.rs"]
mod tests;

const BUDGET: u64 = 4 * 1024 * 1024 * 1024;
static BYTES: AtomicU64 = AtomicU64::new(0);
static SOURCES: Mutex<Vec<Weak<Pinned>>> = Mutex::new(Vec::new());
struct Reservation(u64);
impl Drop for Reservation {
    fn drop(&mut self) {
        BYTES.fetch_sub(self.0, Ordering::AcqRel);
    }
}
pub(crate) struct Pinned {
    pub source: VideoSource,
    pub file: tempfile::NamedTempFile,
    _reservation: Reservation,
}
pub fn pinned_source_bytes() -> u64 {
    BYTES.load(Ordering::Acquire)
}

pub(crate) fn acquire(source: &VideoSource, cancel: &Cancel) -> Result<Arc<Pinned>, String> {
    let existing = {
        let mut registry = SOURCES.lock().unwrap();
        registry.retain(|entry| entry.strong_count() > 0);
        registry.iter().filter_map(Weak::upgrade).find(|entry| {
            entry.source.fingerprint == source.fingerprint && entry.source.info == source.info
        })
    };
    if let Some(existing) = existing {
        // A new worker must still reject a changed/missing linked source. Only
        // consumers already holding an immutable pin may continue after unlink.
        if video::fingerprint(&source.path, cancel)? != source.fingerprint {
            return Err("source fingerprint changed; reimport required".into());
        }
        return Ok(existing);
    }
    let mut input = std::fs::File::open(&source.path)
        .map_err(|e| format!("missing media {}: {e}", source.path.display()))?;
    let metadata = input.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 * 1024 {
        return Err("media must be a regular file within the 2 GiB source budget".into());
    }
    let bytes = metadata.len();
    BYTES
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
            used.checked_add(bytes).filter(|total| *total <= BUDGET)
        })
        .map_err(|_| "aggregate immutable-source budget exhausted (4 GiB)")?;
    let reservation = Reservation(bytes);
    let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut size = 0;
    loop {
        cancel.check()?;
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > bytes {
            return Err("source changed during pinning".into());
        }
        hash.update(&buffer[..n]);
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
    }
    if format!("sha256:{:x}", hash.finalize()) != source.fingerprint {
        return Err("source fingerprint changed; reimport required".into());
    }
    let actual = match video::verified_info(&source.fingerprint) {
        Some(info) => info,
        None => video::probe(file.path(), cancel)?,
    };
    if actual != source.info {
        return Err("source metadata mismatch".into());
    }
    video::remember_verified(&source.fingerprint, &actual);
    let pinned = Arc::new(Pinned {
        source: source.clone(),
        file,
        _reservation: reservation,
    });
    let mut registry = SOURCES.lock().unwrap();
    // Concurrent first-use copies may overlap under the aggregate reservation.
    // Coalesce before publication rather than retaining duplicate whole files.
    if let Some(existing) = registry.iter().filter_map(Weak::upgrade).find(|entry| {
        entry.source.fingerprint == source.fingerprint && entry.source.info == source.info
    }) {
        return Ok(existing);
    }
    registry.push(Arc::downgrade(&pinned));
    Ok(pinned)
}
