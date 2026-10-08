//! One opportunistic retention task. UI sees metadata and ready GPU leases only.
use fold_platform::{
    application::Application,
    desktop::PreviewKey,
    gpu::{Display, Host, PresentationCompressor, PresentationTransfer},
    playback_cache::{Disk, Memory, TRANSFER_LIMIT},
};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Tier {
    Ram,
    Timeline,
    Composition,
}
enum Request {
    Retain(PreviewKey, bool, Display),
    Restore(PreviewKey, bool),
    Clear(Tier),
}
#[derive(Default)]
struct Response {
    added: Vec<(Tier, PreviewKey)>,
    removed: Vec<(Tier, PreviewKey)>,
    restored: Option<(PreviewKey, Result<Display, String>)>,
    usage: [(u64, u64); 3],
    error: Option<String>,
    readback: u64,
    upload: u64,
    disabled: bool,
    retry: Option<PreviewKey>,
    retention_failed: Option<PreviewKey>,
}
struct Storage {
    memory: Memory,
    disks: [Option<Disk>; 2],
    transfer: PresentationTransfer,
    compressor: Option<PresentationCompressor>,
    idle: Arc<AtomicBool>,
}
impl Storage {
    fn usage(&self) -> [(u64, u64); 3] {
        let (used, budget) = self.memory.usage();
        [
            (used as u64, budget as u64),
            self.disks[0].as_ref().map_or((0, 0), Disk::usage),
            self.disks[1].as_ref().map_or((0, 0), Disk::usage),
        ]
    }
    fn remember(
        &mut self,
        key: PreviewKey,
        pixels: Arc<fold_platform::presentation::Pixels>,
        response: &mut Response,
    ) {
        response.removed.extend(
            self.memory
                .insert(key.clone(), pixels)
                .into_iter()
                .map(|key| (Tier::Ram, key)),
        );
        if self.memory.get(&key).is_some() {
            response.added.push((Tier::Ram, key));
        }
    }
    fn execute(
        &mut self,
        request: Request,
        host: &Host,
        cancel: &fold_platform::scheduling::Cancel,
    ) -> Response {
        let mut response = Response::default();
        let readback_before = self.transfer.readback_bytes();
        match request {
            Request::Retain(key, timeline, frame) => {
                // Opportunistic work never waits in the render scheduler queue.
                let scheduler = fold_platform::scheduling::Scheduler::shared();
                let result = if self.idle.load(Ordering::Acquire)
                    && let Some(permit) = scheduler.try_enter_idle()
                {
                    (|| {
                        let compressed = if !frame.is_compressed() {
                            Some(
                                self.compressor
                                    .as_mut()
                                    .ok_or("BC7 playback retention unavailable on this GPU")?
                                    .compress(&frame)?,
                            )
                        } else {
                            None
                        };
                        // Submission is serialized; completion/readback never holds
                        // graphics admission while a new foreground request arrives.
                        drop(permit);
                        let mut source = &frame;
                        if let Some(compressed) = &compressed {
                            let start = Instant::now();
                            loop {
                                cancel.check()?;
                                match compressed.is_ready() {
                                    Ok(true) => {
                                        source = compressed;
                                        break;
                                    }
                                    Err(error) => return Err(error),
                                    Ok(false) => {}
                                }
                                if start.elapsed().as_secs() >= 10 {
                                    return Err("BC7 retention timed out".into());
                                }
                                host.poll()?;
                                std::thread::sleep(std::time::Duration::from_millis(1));
                            }
                        }
                        response.restored = Some((key.clone(), Ok(source.clone())));
                        let disk = if timeline { 0 } else { 1 };
                        if self.memory.usage().1 == 0
                            && self.disks[disk]
                                .as_ref()
                                .is_none_or(|disk| disk.usage().1 == 0)
                        {
                            return Ok(None); // GPU-only caching transfers no pixels.
                        }
                        if !self.idle.load(Ordering::Acquire) {
                            response.retry = Some(key.clone());
                            return Ok(None);
                        }
                        self.transfer
                            .retain(source, TRANSFER_LIMIT, cancel)
                            .map(Some)
                    })()
                } else {
                    response.retry = Some(key.clone());
                    Ok(None)
                };
                match result {
                    Ok(Some(pixels)) => {
                        let pixels = Arc::new(pixels);
                        self.remember(key.clone(), pixels.clone(), &mut response);
                        let index = if timeline { 0 } else { 1 };
                        let tier = if timeline {
                            Tier::Timeline
                        } else {
                            Tier::Composition
                        };
                        if let Some(disk) = &mut self.disks[index] {
                            match disk.insert(key.clone(), &pixels) {
                                Ok(evicted) => {
                                    response
                                        .removed
                                        .extend(evicted.into_iter().map(|key| (tier, key)));
                                    if disk.contains(&key) {
                                        response.added.push((tier, key));
                                    }
                                }
                                Err(error) => response.error = Some(error),
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        response.error = Some(error);
                        response.retention_failed = Some(key);
                    }
                }
            }
            Request::Restore(key, timeline) => {
                let result = (|| {
                    let pixels = if let Some(pixels) = self.memory.get(&key) {
                        pixels
                    } else {
                        let index = if timeline { 0 } else { 1 };
                        let tier = if timeline {
                            Tier::Timeline
                        } else {
                            Tier::Composition
                        };
                        let result = self.disks[index]
                            .as_mut()
                            .map(|disk| disk.get(&key))
                            .transpose();
                        let pixels = match result {
                            Ok(Some(Some(pixels))) => Arc::new(pixels),
                            other => {
                                response.removed.push((tier, key.clone()));
                                return Err(other
                                    .err()
                                    .unwrap_or_else(|| "Playback cache entry unavailable".into()));
                            }
                        };
                        self.remember(key.clone(), pixels.clone(), &mut response);
                        pixels
                    };
                    cancel.check()?;
                    let frame = host.restore_presentation(&pixels)?;
                    response.upload = pixels.data.len() as u64;
                    Ok(frame)
                })();
                response.restored = Some((key, result));
            }
            Request::Clear(tier) => match tier {
                Tier::Ram => {
                    response
                        .removed
                        .extend(self.memory.keys().cloned().map(|key| (Tier::Ram, key)));
                    self.memory.clear();
                }
                Tier::Timeline | Tier::Composition => {
                    let index = if matches!(tier, Tier::Timeline) { 0 } else { 1 };
                    if let Some(disk) = &mut self.disks[index] {
                        let keys: Vec<_> = disk.keys().cloned().collect();
                        if let Err(error) = disk.clear() {
                            response.error = Some(error);
                        }
                        response.removed.extend(
                            keys.into_iter()
                                .filter(|key| !disk.contains(key))
                                .map(|key| (tier, key)),
                        );
                    }
                }
            },
        }
        response.usage = self.usage();
        response.readback = self.transfer.readback_bytes() - readback_before;
        response
    }
}
pub(super) struct Retention {
    requests: Option<mpsc::SyncSender<Request>>,
    results: mpsc::Receiver<Response>,
    worker: Option<std::thread::JoinHandle<()>>,
    cancel: fold_platform::scheduling::Cancel,
    idle: Arc<AtomicBool>,
    keys: [HashSet<PreviewKey>; 3],
    busy: bool,
    failed: bool,
    clear: Vec<Tier>,
    pub usage: [(u64, u64); 3],
    pub changes: Vec<PreviewKey>,
    pub retry: Option<PreviewKey>,
    pub retention_failed: Option<PreviewKey>,
    pub error: Option<String>,
    pub readback: u64,
    pub upload: u64,
}
impl Retention {
    pub fn new(host: Host, settings: Application) -> Self {
        let (requests, receive) = mpsc::sync_channel(1);
        // Single outstanding request plus startup means this never grows unbounded.
        let (send, results) = mpsc::channel();
        let cancel = fold_platform::scheduling::Cancel::default();
        let token = cancel.clone();
        let idle = Arc::new(AtomicBool::new(true));
        let worker_idle = idle.clone();
        let worker = std::thread::spawn(move || {
            let mut response = Response::default();
            let disks = [true, false].map(|timeline| {
                let budget = if timeline {
                    settings.timeline_disk_gib
                } else {
                    settings.composition_disk_gib
                };
                match Disk::open(
                    settings.playback_path(timeline),
                    u64::from(budget) * 1024 * 1024 * 1024,
                    &token,
                ) {
                    Ok(disk) => {
                        let tier = if timeline {
                            Tier::Timeline
                        } else {
                            Tier::Composition
                        };
                        response
                            .added
                            .extend(disk.keys().cloned().map(|key| (tier, key)));
                        Some(disk)
                    }
                    Err(error) => {
                        response.error = Some(error);
                        None
                    }
                }
            });
            let compressor = match PresentationCompressor::new(host.clone()) {
                Ok(compressor) => Some(compressor),
                Err(error) => {
                    response.error = Some(error);
                    response.disabled = true;
                    None
                }
            };
            let mut storage = Storage {
                memory: Memory::new(settings.viewer_ram_mib as usize * 1024 * 1024),
                disks,
                transfer: Default::default(),
                compressor,
                idle: worker_idle,
            };
            response.usage = storage.usage();
            if send.send(response).is_err() {
                return;
            }
            while let Ok(request) = receive.recv() {
                if token.check().is_err() {
                    break;
                }
                if send.send(storage.execute(request, &host, &token)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests: Some(requests),
            results,
            worker: Some(worker),
            cancel,
            idle,
            keys: Default::default(),
            busy: true,
            failed: false,
            clear: vec![],
            usage: [(0, 0); 3],
            changes: vec![],
            retry: None,
            retention_failed: None,
            error: None,
            readback: 0,
            upload: 0,
        }
    }
    pub fn keys(&self) -> impl Iterator<Item = &PreviewKey> {
        self.keys.iter().flat_map(|keys| keys.iter())
    }
    pub fn enabled(&self, timeline: bool) -> bool {
        !self.failed && (self.usage[0].1 > 0 || self.usage[if timeline { 1 } else { 2 }].1 > 0)
    }
    pub fn contains(&self, key: &PreviewKey) -> bool {
        !self.failed && self.keys.iter().any(|keys| keys.contains(key))
    }
    fn send(&mut self, request: Request) -> bool {
        if self.busy || self.failed {
            return false;
        }
        if self
            .requests
            .as_ref()
            .is_some_and(|send| send.try_send(request).is_ok())
        {
            self.busy = true;
            true
        } else {
            false
        }
    }
    pub fn restore(&mut self, key: &PreviewKey, timeline: bool) {
        if self.contains(key) {
            self.send(Request::Restore(key.clone(), timeline));
        }
    }
    pub fn set_idle(&self, idle: bool) {
        self.idle.store(idle, Ordering::Release);
    }
    pub fn can_retain(&self) -> bool {
        !self.busy && !self.failed && self.clear.is_empty()
    }
    pub fn retain(&mut self, key: PreviewKey, timeline: bool, frame: Display) {
        self.send(Request::Retain(key, timeline, frame));
    }
    pub fn clear(&mut self, tier: Tier) {
        if self.clear.len() < 3 {
            self.clear.push(tier);
        }
    }
    pub fn poll(&mut self) -> Option<(PreviewKey, Result<Display, String>)> {
        let mut restored = None;
        match self.results.try_recv() {
            Ok(response) => {
                self.busy = false;
                self.failed |= response.disabled;
                for (tier, key) in response.removed {
                    self.keys[tier as usize].remove(&key);
                    self.changes.push(key);
                }
                for (tier, key) in response.added {
                    self.keys[tier as usize].insert(key.clone());
                    self.changes.push(key);
                }
                self.usage = response.usage;
                self.retry = response.retry;
                self.retention_failed = response.retention_failed;
                if response.error.is_some() {
                    self.error = response.error;
                }
                self.readback += response.readback;
                self.upload += response.upload;
                if let Some((key, Err(_))) = &response.restored {
                    for keys in &mut self.keys {
                        keys.remove(key);
                    }
                    self.changes.push(key.clone());
                }
                restored = response.restored;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.failed = true;
                self.busy = false;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if !self.busy
            && let Some(tier) = self.clear.pop()
        {
            self.send(Request::Clear(tier));
        }
        restored
    }
}
impl Drop for Retention {
    fn drop(&mut self) {
        self.cancel.cancel();
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
    fn busy_cached_reads_never_request_graph_work_and_channel_switch_keeps_old_entries() {
        use fold_platform::desktop::{
            DesktopClient, DesktopCommand, DesktopState, PreviewDemand, PreviewResult,
        };
        #[derive(Default)]
        struct Client {
            state: DesktopState,
            batches: Vec<Vec<PreviewDemand>>,
        }
        impl DesktopClient for Client {
            fn state(&self) -> &DesktopState {
                &self.state
            }
            fn poll(&mut self) {}
            fn command(&mut self, _: DesktopCommand) {}
            fn request_preview(&mut self, _: PreviewKey) {
                panic!("unexpected render");
            }
            fn preview_demands(&mut self, demands: Vec<PreviewDemand>) {
                self.batches.push(demands);
            }
            fn cancel_preview(&mut self) {}
            fn take_preview(&mut self) -> Option<PreviewResult> {
                None
            }
        }
        let key = PreviewKey {
            region: None,
            target: None,
            output: "video".into(),
            content: "test".into(),
            frame: 0,
            dimensions: [64, 64],
            view: 1,
            channels: Default::default(),
        };
        let mut host = super::super::PreviewHost::new();
        let mut client = Client::default();
        let id = fold_platform::workspace::PanelInstanceId(1);
        host.select_viewers(&[(id, Some(key.clone()))], &mut client);
        assert_eq!(client.batches.last().unwrap().len(), 1);
        let (_send, results) = mpsc::channel();
        let mut keys: [HashSet<PreviewKey>; 3] = Default::default();
        keys[0].insert(key.clone());
        host.retention = Some(Retention {
            requests: None,
            results,
            worker: None,
            cancel: Default::default(),
            idle: Arc::new(AtomicBool::new(true)),
            keys,
            busy: true,
            failed: false,
            clear: vec![],
            usage: [(0, 1024); 3],
            changes: vec![],
            retry: None,
            retention_failed: None,
            error: None,
            readback: 0,
            upload: 0,
        });
        for _ in 0..100 {
            host.select_viewers(&[(id, Some(key.clone()))], &mut client);
        }
        assert!(
            client.batches.last().unwrap().is_empty(),
            "known cache hit cancels old graph demand even while storage is busy"
        );
        assert_eq!(client.batches.len(), 2);
        let depth = PreviewKey {
            channels: fold_platform::desktop::ChannelView::Channel {
                name: "depth.Z".to_string().try_into().unwrap(),
                range: Default::default(),
            },
            ..key.clone()
        };
        host.select_viewers(&[(id, Some(depth.clone()))], &mut client);
        assert_eq!(client.batches.last().unwrap()[0].key, depth);
        assert!(host.retention.as_ref().unwrap().contains(&key));
        host.select_viewers(&[(id, Some(key))], &mut client);
        assert!(client.batches.last().unwrap().is_empty());
    }
    fn wait(cache: &mut Retention) -> Option<(PreviewKey, Result<Display, String>)> {
        let start = Instant::now();
        loop {
            let result = cache.poll();
            if !cache.busy && cache.clear.is_empty() {
                return result;
            }
            assert!(start.elapsed().as_secs() < 30);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    #[ignore = "requires native Vulkan BC7 support"]
    fn native_bc7_ram_disk_restart_clear_and_gpu_only_transfers() {
        let (host, _) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
        let pixels = fold_platform::presentation::Pixels {
            description: fold_platform::presentation::Description {
                dimensions: [64, 64],
                format: fold_platform::presentation::Format::Rgba8,
            },
            data: [60, 120, 180, 255].repeat(64 * 64),
        };
        let frame = host.restore_presentation(&pixels).unwrap();
        while !frame.is_ready().unwrap() {
            host.poll().unwrap();
        }
        let key = PreviewKey {
            region: None,
            target: None,
            output: "video".into(),
            content: "native-test".into(),
            frame: 0,
            dimensions: [64, 64],
            view: 1,
            channels: Default::default(),
        };
        let dir = tempfile::tempdir().unwrap();
        let settings = Application {
            viewer_ram_mib: 1,
            timeline_disk_gib: 1,
            composition_disk_gib: 1,
            cache_folder: dir.path().to_string_lossy().into_owned(),
            ..Default::default()
        };
        let mut cache = Retention::new(host.clone(), settings.clone());
        wait(&mut cache);
        cache.retain(key.clone(), true, frame.clone());
        wait(&mut cache);
        assert!(cache.contains(&key));
        assert_eq!(cache.usage[0].0, 4096);
        assert!(cache.usage[1].0 > 4096);
        assert_eq!(cache.usage[2].0, 0);
        assert_eq!(cache.readback, 4096);
        cache.clear(Tier::Ram);
        wait(&mut cache);
        assert_eq!(cache.usage[0].0, 0);
        cache.restore(&key, true);
        let restored = wait(&mut cache).unwrap().1.unwrap();
        assert!(restored.is_compressed());
        assert_eq!(restored.statistics.compute_passes, 0);
        assert_eq!(cache.upload, 4096);
        drop(cache);
        let mut cache = Retention::new(host.clone(), settings);
        wait(&mut cache);
        assert!(cache.contains(&key));
        let changed = PreviewKey {
            view: 2,
            ..key.clone()
        };
        assert!(!cache.contains(&changed));
        cache.clear(Tier::Timeline);
        wait(&mut cache);
        assert!(!cache.contains(&key));
        drop(cache);
        let mut gpu_only = Retention::new(
            host,
            Application {
                viewer_ram_mib: 0,
                ..Default::default()
            },
        );
        wait(&mut gpu_only);
        gpu_only.retain(key, false, frame);
        let compressed = wait(&mut gpu_only).unwrap().1.unwrap();
        assert!(compressed.is_compressed());
        assert_eq!(gpu_only.readback, 0);
        assert_eq!(compressed.statistics.status_readback_bytes, 0);
        assert_eq!(compressed.statistics.compute_passes, 2);
    }
}
