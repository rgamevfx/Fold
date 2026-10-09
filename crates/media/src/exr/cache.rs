//! Shared, verified source samples. A ticket owns consumer interest, separately
//! from the worker and cache. Cancelling one viewer cannot cancel another viewer
//! or delivery using identical bytes. No mutex is held across I/O or decoding.
use super::{Cancel, Pixels, Source, budget};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    #[cfg(unix)]
    identity: [i128; 7],
    #[cfg(not(unix))]
    fingerprint: String,
}
impl Stamp {
    pub fn read(path: &Path, cancel: &Cancel) -> Result<Self, fold_foundation::Failure> {
        cancel.check()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let m = std::fs::metadata(path).map_err(|e| e.to_string())?;
            if !m.is_file() {
                return Err("EXR source must be a regular file".into());
            }
            Ok(Self {
                identity: [
                    m.dev() as i128,
                    m.ino() as i128,
                    m.len() as i128,
                    m.mtime() as i128,
                    m.mtime_nsec() as i128,
                    m.ctime() as i128,
                    m.ctime_nsec() as i128,
                ],
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Self {
                fingerprint: crate::fingerprint(path, cancel)?,
            })
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub source: Source,
    pub channels: [Option<String>; 4],
    pub dimensions: [u32; 2],
}
impl Request {
    fn bytes(&self) -> u64 {
        u64::from(self.dimensions[0]) * u64::from(self.dimensions[1]) * 16
    }
}
struct Work {
    request: Request,
    stamp: Stamp,
    result: Mutex<Option<Result<Arc<Pixels>, fold_foundation::Failure>>>,
    ready: Condvar,
    cancel: Cancel,
    interests: AtomicUsize,
    foreground: AtomicBool,
}
pub struct Ticket {
    work: Arc<Work>,
}
impl Ticket {
    pub fn wait(&self, cancel: &Cancel) -> Result<Arc<Pixels>, fold_foundation::Failure> {
        let mut state = self.work.result.lock().unwrap();
        loop {
            cancel.check()?;
            if let Some(result) = &*state {
                return result.clone();
            }
            state = self
                .work
                .ready
                .wait_timeout(state, std::time::Duration::from_millis(2))
                .unwrap()
                .0;
        }
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        let result = self.work.result.lock().unwrap();
        if self.work.interests.fetch_sub(1, Ordering::AcqRel) == 1 && result.is_none() {
            self.work.cancel.cancel();
        }
    }
}
#[derive(Default)]
struct State {
    entries: VecDeque<Arc<Work>>,
    queue: VecDeque<Arc<Work>>,
}
impl State {
    fn trim(&mut self, limit: u64) {
        self.entries.retain(|e| {
            e.cancel.check().is_ok() && !matches!(&*e.result.lock().unwrap(), Some(Err(_)))
        });
        let mut bytes: u64 = self
            .entries
            .iter()
            .filter(|e| matches!(&*e.result.lock().unwrap(), Some(Ok(_))))
            .map(|e| e.request.bytes())
            .sum();
        while bytes > limit || self.entries.len() > 64 {
            let Some(index) = self.entries.iter().position(|e| {
                e.interests.load(Ordering::Acquire) == 0 && e.result.lock().unwrap().is_some()
            }) else {
                break;
            };
            bytes = bytes.saturating_sub(self.entries.remove(index).unwrap().request.bytes());
        }
        self.queue.retain(|e| e.cancel.check().is_ok());
    }
}
struct Service {
    state: Mutex<State>,
    wake: Condvar,
}
fn service() -> Result<&'static Arc<Service>, fold_foundation::Failure> {
    static SERVICE: OnceLock<Result<Arc<Service>, fold_foundation::Failure>> = OnceLock::new();
    SERVICE
        .get_or_init(|| {
            let service = Arc::new(Service {
                state: Mutex::new(State::default()),
                wake: Condvar::new(),
            });
            let worker = service.clone();
            std::thread::Builder::new()
                .name("fold-exr-sources".into())
                .spawn(move || {
                    loop {
                        let work = {
                            let mut state = worker.state.lock().unwrap();
                            state.trim(budget::decoded_usage().budget / 2);
                            while state.queue.is_empty() {
                                state = worker.wake.wait(state).unwrap();
                            }
                            let index = state
                                .queue
                                .iter()
                                .position(|e| e.foreground.load(Ordering::Acquire))
                                .unwrap_or(0);
                            state.queue.remove(index).unwrap()
                        };
                        let decode = || -> Result<Arc<Pixels>, fold_foundation::Failure> {
                            work.cancel.check()?;
                            let r = &work.request;
                            let pixels = super::decode(
                                &r.source,
                                r.channels.each_ref().map(|c| c.as_deref()),
                                r.dimensions,
                                &work.cancel,
                            )?;
                            if Stamp::read(&r.source.path, &work.cancel)? != work.stamp {
                                return Err("EXR source changed during preparation".into());
                            }
                            Ok(Arc::new(pixels))
                        };
                        let mut result = decode();
                        if result.as_ref().err().is_some_and(|e| e.retryable()) {
                            worker.state.lock().unwrap().trim(0);
                            result = decode();
                        }
                        *work.result.lock().unwrap() = Some(result);
                        work.ready.notify_all();
                        worker
                            .state
                            .lock()
                            .unwrap()
                            .trim(budget::decoded_usage().budget / 2);
                    }
                })
                .map_err(|e| format!("EXR source worker creation failed: {e}"))?;
            Ok(service)
        })
        .as_ref()
        .map_err(Clone::clone)
}
/// `foreground` promotes an existing speculative request without duplicating it.
/// Ingress is bounded independently of the allocation budgets.
pub fn request(
    request: Request,
    foreground: bool,
    cancel: &Cancel,
) -> Result<Ticket, fold_foundation::Failure> {
    request.source.info.validate()?;
    let count = u64::from(request.dimensions[0]) * u64::from(request.dimensions[1]);
    if count == 0 || count > crate::MAX_PIXELS as u64 {
        return Err(fold_foundation::Failure::new(
            fold_foundation::FailureKind::InvalidInput,
            "Invalid EXR output dimensions",
        ));
    }
    let stamp = Stamp::read(&request.source.path, cancel)?;
    let service = service()?;
    let mut state = service.state.lock().unwrap();
    loop {
        cancel.check()?;
        state.trim(budget::decoded_usage().budget / 2);
        while let Some(index) = state
            .entries
            .iter()
            .position(|e| e.request == request && e.stamp == stamp)
        {
            let work = state.entries.remove(index).unwrap();
            let result = work.result.lock().unwrap();
            if work.cancel.check().is_err() {
                continue;
            }
            work.interests.fetch_add(1, Ordering::AcqRel);
            drop(result);
            if foreground {
                work.foreground.store(true, Ordering::Release);
            }
            state.entries.push_back(work.clone());
            return Ok(Ticket { work });
        }
        if state.queue.len() < 8 {
            break;
        }
        if !foreground {
            return Err(fold_foundation::Failure::new(
                fold_foundation::FailureKind::ResourcePressure,
                "EXR preparation capacity exhausted",
            ));
        }
        // Demand waits cancellably for bounded ingress; speculative callers
        // can skip without causing a CPU or GPU render to fail under contention.
        state = service
            .wake
            .wait_timeout(state, std::time::Duration::from_millis(2))
            .unwrap()
            .0;
    }
    let work = Arc::new(Work {
        request,
        stamp,
        result: Mutex::new(None),
        ready: Condvar::new(),
        cancel: Cancel::default(),
        interests: AtomicUsize::new(1),
        foreground: AtomicBool::new(foreground),
    });
    state.entries.push_back(work.clone());
    state.queue.push_back(work.clone());
    service.wake.notify_all();
    Ok(Ticket { work })
}
/// Release expendable cache references. Active tickets retain identities;
/// held results and worker allocations retain their leases until completion.
pub fn clear() {
    if let Ok(service) = service() {
        service.state.lock().unwrap().trim(0);
    }
}
