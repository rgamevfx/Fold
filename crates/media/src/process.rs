//! Supervised child processes. Cancellation kills/reaps the child, including
//! while an encoder caller is blocked writing to its pipe. No shell expansion.
use std::{
    io::{Read, Seek, SeekFrom},
    process::{ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

// All media entry points share this hard ceiling, including browser and probe
// jobs. No caller can accidentally create an unbounded fleet of codec children.
struct Capacity {
    active: AtomicUsize,
    regular: AtomicUsize,
}
static CAPACITY: Capacity = Capacity {
    active: AtomicUsize::new(0),
    regular: AtomicUsize::new(0),
};
struct Slot {
    capacity: &'static Capacity,
    regular: bool,
}
impl Capacity {
    fn reserve(&'static self, audio: bool) -> Result<Slot, fold_foundation::Failure> {
        if !audio {
            self.regular
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    (n < 6).then_some(n + 1)
                })
                .map_err(|_| {
                    fold_foundation::Failure::new(
                        fold_foundation::FailureKind::ResourcePressure,
                        "media process capacity exhausted (6 regular; 2 reserved for audio)",
                    )
                })?;
        }
        if self
            .active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 8).then_some(n + 1)
            })
            .is_err()
        {
            if !audio {
                self.regular.fetch_sub(1, Ordering::AcqRel);
            }
            return Err(fold_foundation::Failure::new(
                fold_foundation::FailureKind::ResourcePressure,
                "media process capacity exhausted (8 active jobs)",
            ));
        }
        Ok(Slot {
            capacity: self,
            regular: !audio,
        })
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        self.capacity.active.fetch_sub(1, Ordering::AcqRel);
        if self.regular {
            self.capacity.regular.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<(), fold_foundation::Failure> {
        if self.0.load(Ordering::Acquire) {
            Err(fold_foundation::Failure::new(
                fold_foundation::FailureKind::Cancelled,
                "job cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

pub struct Process {
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    worker: Option<JoinHandle<Result<(), fold_foundation::Failure>>>,
    stop: Cancel,
    log: Arc<tempfile::NamedTempFile>,
}
fn diagnostic_tail(log: &tempfile::NamedTempFile) -> String {
    let mut details = Vec::new();
    if let Ok(mut file) = log.reopen() {
        let end = file.metadata().map(|m| m.len()).unwrap_or(0);
        let _ = file.seek(SeekFrom::Start(end.saturating_sub(8192)));
        let _ = file.take(8192).read_to_end(&mut details);
    }
    String::from_utf8_lossy(&details).into_owned()
}
impl Process {
    /// Bounded diagnostics for retained workers, including successful jobs.
    pub fn diagnostics(&self) -> String {
        diagnostic_tail(&self.log)
    }
    /// Bounded one-shot work, including compiler children. Callers retain the
    /// same cancellation, diagnostics and capacity policy as media workers.
    pub fn timed(
        command: Command,
        cancel: Cancel,
        timeout: Duration,
    ) -> Result<Self, fold_foundation::Failure> {
        Self::supervise(command, cancel, None, Some(timeout), false)
    }
    pub fn spawn(
        command: Command,
        cancel: Cancel,
        limit: Option<(std::path::PathBuf, u64)>,
    ) -> Result<Self, fold_foundation::Failure> {
        Self::supervise(
            command,
            cancel,
            limit,
            Some(Duration::from_secs(600)),
            false,
        )
    }
    pub fn audio(
        command: Command,
        cancel: Cancel,
        limit: Option<(std::path::PathBuf, u64)>,
    ) -> Result<Self, fold_foundation::Failure> {
        Self::supervise(command, cancel, limit, Some(Duration::from_secs(600)), true)
    }
    pub fn retained(command: Command) -> Result<Self, fold_foundation::Failure> {
        // Idle retained decoders are pipe-backpressured. Each demanded read has
        // its own cancellation/30s deadline, rather than a wall-lifetime limit.
        Self::supervise(command, Cancel::default(), None, None, false)
    }
    pub fn retained_audio(command: Command) -> Result<Self, fold_foundation::Failure> {
        Self::supervise(command, Cancel::default(), None, None, true)
    }
    fn supervise(
        mut command: Command,
        cancel: Cancel,
        limit: Option<(std::path::PathBuf, u64)>,
        timeout: Option<Duration>,
        audio: bool,
    ) -> Result<Self, fold_foundation::Failure> {
        cancel.check()?;
        let slot = CAPACITY.reserve(audio)?;
        let log = Arc::new(crate::scratch::file().map_err(|e| e.to_string())?);
        command.stderr(Stdio::from(log.reopen().map_err(|e| e.to_string())?));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("worker unavailable: {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stop = Cancel::default();
        let worker_stop = stop.clone();
        let worker_log = log.clone();
        let worker = std::thread::spawn(move || {
            let log = worker_log;
            let _slot = slot;
            let start = Instant::now();
            let result = loop {
                let failure = cancel
                    .check()
                    .and_then(|_| worker_stop.check())
                    .and_then(|_| {
                        if timeout.is_some_and(|timeout| start.elapsed() > timeout) {
                            Err(format!("worker timeout ({}s)", timeout.unwrap().as_secs()).into())
                        } else if log.as_file().metadata().map(|m| m.len()).unwrap_or(0)
                            > 1024 * 1024
                        {
                            Err("worker diagnostic budget exceeded".into())
                        } else if limit.as_ref().is_some_and(|(p, n)| {
                            std::fs::metadata(p).map(|m| m.len() > *n).unwrap_or(false)
                        }) {
                            Err("worker output budget exceeded".into())
                        } else {
                            Ok(())
                        }
                    });
                if let Err(error) = failure {
                    break Err(error);
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        break if status.success() {
                            Ok(())
                        } else {
                            Err(format!("worker exited with {status}").into())
                        };
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                    Err(error) => break Err(error.to_string().into()),
                }
            };
            // Also reap on cancellation, timeout, and monitoring failures.
            #[cfg(unix)]
            if let Ok(id) = i32::try_from(child.id())
                && let Some(group) = rustix::process::Pid::from_raw(id)
            {
                let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            result.map_err(|mut error| {
                error.message = format!("{}: {}", error.message, diagnostic_tail(&log));
                error
            })
        });
        Ok(Self {
            stdin,
            stdout,
            worker: Some(worker),
            stop,
            log,
        })
    }
    pub fn is_finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
    }
    pub fn abort(self) -> Result<(), fold_foundation::Failure> {
        self.stop.cancel();
        self.finish()
    }
    pub fn finish(mut self) -> Result<(), fold_foundation::Failure> {
        self.stdin.take();
        self.stdout.take();
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| "worker monitor panicked".to_owned())?
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.stop.cancel();
        self.stdin.take();
        self.stdout.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitored_audio_keeps_reserved_slots_and_capacity_recovers() {
        static CAPACITY: Capacity = Capacity {
            active: AtomicUsize::new(0),
            regular: AtomicUsize::new(0),
        };
        let mut regular: Vec<_> = (0..6).map(|_| CAPACITY.reserve(false).unwrap()).collect();
        assert!(CAPACITY.reserve(false).is_err());
        let audio: Vec<_> = (0..2).map(|_| CAPACITY.reserve(true).unwrap()).collect();
        assert!(CAPACITY.reserve(true).is_err());
        drop(regular.pop());
        let replacement = CAPACITY.reserve(false).unwrap();
        drop((regular, audio, replacement));
        assert_eq!(CAPACITY.active.load(Ordering::Acquire), 0);
        assert_eq!(CAPACITY.regular.load(Ordering::Acquire), 0);
    }
}
