//! Supervised codec processes. Cancellation kills/reaps the child, including
//! while an encoder caller is blocked writing to its pipe. No shell expansion.
use std::{
    io::Read,
    process::{ChildStdin, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::Acquire) {
            Err("job cancelled".into())
        } else {
            Ok(())
        }
    }
}

pub(crate) struct Process {
    pub stdin: Option<ChildStdin>,
    worker: Option<JoinHandle<Result<(), String>>>,
    stop: Cancel,
}
impl Process {
    pub fn spawn(
        mut command: Command,
        cancel: Cancel,
        limit: Option<(std::path::PathBuf, u64)>,
    ) -> Result<Self, String> {
        cancel.check()?;
        let log = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        command.stderr(Stdio::from(log.reopen().map_err(|e| e.to_string())?));
        let mut child = command
            .spawn()
            .map_err(|e| format!("codec unavailable: {e}"))?;
        let stdin = child.stdin.take();
        let stop = Cancel::default();
        let worker_stop = stop.clone();
        let worker = std::thread::spawn(move || {
            let start = Instant::now();
            let result = loop {
                let failure = cancel
                    .check()
                    .and_then(|_| worker_stop.check())
                    .and_then(|_| {
                        if start.elapsed() > Duration::from_secs(600) {
                            Err("codec timeout (600s)".into())
                        } else if log.as_file().metadata().map(|m| m.len()).unwrap_or(0)
                            > 1024 * 1024
                        {
                            Err("codec diagnostic budget exceeded".into())
                        } else if limit.as_ref().is_some_and(|(p, n)| {
                            std::fs::metadata(p).map(|m| m.len() > *n).unwrap_or(false)
                        }) {
                            Err("codec output budget exceeded".into())
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
                            Err(format!("codec exited with {status}"))
                        };
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                    Err(error) => break Err(error.to_string()),
                }
            };
            // Also reap on cancellation, timeout, and monitoring failures.
            let _ = child.kill();
            let _ = child.wait();
            result.map_err(|error| {
                let mut details = String::new();
                if let Ok(file) = log.reopen() {
                    let _ = file.take(8192).read_to_string(&mut details);
                }
                format!("{error}: {details}")
            })
        });
        Ok(Self {
            stdin,
            worker: Some(worker),
            stop,
        })
    }
    pub fn finish(mut self) -> Result<(), String> {
        self.stdin.take();
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| "codec monitor panicked".to_owned())?
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.stop.cancel();
        self.stdin.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
