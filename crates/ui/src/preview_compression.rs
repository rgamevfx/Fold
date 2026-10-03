//! One background compression request, never an unbounded cache-fill queue.
use fold_platform::{
    desktop::PreviewKey,
    gpu::{Display, Host, PresentationCompressor},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(super) struct Compression {
    requests: Option<mpsc::SyncSender<(PreviewKey, Display)>>,
    worker: Option<std::thread::JoinHandle<()>>,
    results: mpsc::Receiver<Result<(PreviewKey, Display), String>>,
    stopped: Arc<AtomicBool>,
    busy: bool,
    failed: bool,
}
impl Compression {
    pub fn new(host: Host) -> Self {
        let (requests, receive) = mpsc::sync_channel::<(PreviewKey, Display)>(1);
        let (send, results) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let worker = std::thread::spawn(move || {
            let mut compressor = PresentationCompressor::new(host.clone());
            while let Ok((key, frame)) = receive.recv() {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let result = compressor
                    .as_mut()
                    .map_err(|e| e.clone())
                    .and_then(|compressor| compressor.compress(&frame))
                    .and_then(|result| {
                        let start = std::time::Instant::now();
                        while !result.is_ready()? {
                            if stop.load(Ordering::Acquire) || start.elapsed().as_secs() >= 30 {
                                return Err("BC7 cache task stopped or timed out".into());
                            }
                            host.poll()?;
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        Ok(result)
                    });
                if send.send(result.map(|frame| (key, frame))).is_err() {
                    break;
                }
            }
        });
        Self {
            requests: Some(requests),
            worker: Some(worker),
            results,
            stopped,
            busy: false,
            failed: false,
        }
    }
    pub fn can_accept(&self) -> bool {
        !self.busy && !self.failed
    }
    pub fn request(&mut self, key: PreviewKey, frame: Display) {
        if self.can_accept()
            && self
                .requests
                .as_ref()
                .is_some_and(|sender| sender.try_send((key, frame)).is_ok())
        {
            self.busy = true;
        }
    }
    pub fn take(&mut self) -> Option<Result<(PreviewKey, Display), String>> {
        if self.failed {
            return None;
        }
        match self.results.try_recv() {
            Ok(result) => {
                self.busy = false;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.failed = true;
                self.busy = false;
                Some(Err("BC7 worker stopped; retain RGBA8".into()))
            }
        }
    }
}
impl Drop for Compression {
    fn drop(&mut self) {
        // Shutdown only: stop accepting work and join before device/driver
        // teardown. A detached GPU worker can outlive the native runtime.
        // Ordinary request cancellation/polling never waits on this thread.
        self.stopped.store(true, Ordering::Release);
        self.requests.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disconnected_worker_reports_once_and_stops_accepting_work() {
        let (requests, _) = mpsc::sync_channel(1);
        let (send, results) = mpsc::sync_channel(1);
        drop(send);
        let mut compression = Compression {
            requests: Some(requests),
            results,
            worker: None,
            stopped: Arc::new(AtomicBool::new(false)),
            busy: true,
            failed: false,
        };
        assert!(
            compression
                .take()
                .unwrap()
                .unwrap_err()
                .contains("worker stopped")
        );
        assert!(!compression.can_accept());
        assert!(compression.take().is_none());
    }
    #[test]
    #[ignore = "requires native Vulkan BC7 support"]
    fn shutdown_joins_pipeline_worker_before_device_teardown() {
        let (host, _) = pollster::block_on(Host::headless(1024 * 1024)).unwrap();
        let compression = Compression::new(host.clone());
        let stopped = compression.stopped.clone();
        drop(compression);
        assert!(stopped.load(Ordering::Acquire));
        assert_eq!(
            Arc::strong_count(&stopped),
            1,
            "GPU worker must not survive shutdown"
        );
        host.check().unwrap();
    }
}
