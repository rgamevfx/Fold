//! Retained native GPU demands. Call on a render worker; application publication
//! must compare the returned intent against its current document/time demand.
#[allow(unsafe_code)]
mod native;
use super::{Body, Config, Runtime, io_error, serialize};
use fold_foundation::{DocumentId, Failure, FailureKind, ObjectId, Time};
use fold_gpu_sharing::{identity, socket};
use fold_media::process::{Cancel, Process};
use fold_motion_canvas::{
    compilation,
    model::Canvas,
    protocol::{self, Request, Response},
};
use fold_project::{Document, Revision};
use fold_render::{
    WorkingSpace,
    gpu::{GpuFrame, Host, SharedImage, SharedSource},
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intent {
    pub epoch: ObjectId,
    /// Changes for each new frame intent, including transient previews and
    /// font/resource changes as well as committed edits.
    pub generation: u64,
    pub document: DocumentId,
    pub revision: Revision,
    pub time: Time,
    pub dimensions: [u32; 2],
}
#[derive(Clone)]
pub struct Snapshot {
    intent: Intent,
    canvas: Arc<Canvas>,
    assets: Vec<Arc<fold_project::Asset>>,
}
impl Snapshot {
    pub fn from_document(
        epoch: ObjectId,
        generation: u64,
        document: &Document,
        time: Time,
        dimensions: [u32; 2],
    ) -> Result<Self, Failure> {
        Ok(Self {
            intent: Intent {
                epoch,
                generation,
                document: document.id,
                revision: document.revision,
                time,
                dimensions,
            },
            canvas: Arc::new(Canvas::from_document(document)?),
            assets: Vec::new(),
        })
    }
    /// Bind project-owned asset records without opening files on the caller thread.
    pub fn bind_assets(mut self, project: &fold_project::Snapshot) -> Result<Self, Failure> {
        self.assets = crate::resources::bind(&self.canvas, project)?;
        Ok(self)
    }
    pub fn intent(&self) -> Intent {
        self.intent
    }
}
pub struct Rendered {
    intent: Intent,
    frame: GpuFrame,
}
impl Rendered {
    /// Consumes stale output instead of exposing a frame from an old demand.
    pub fn into_current(self, current: Intent) -> Result<GpuFrame, Failure> {
        if self.intent != current {
            return Err(Failure::new(
                FailureKind::Cancelled,
                "Motion demand was superseded",
            ));
        }
        if !self.frame.is_ready()? {
            return Err("Motion frame is not complete".into());
        }
        Ok(self.frame)
    }
}
pub struct GpuRuntime {
    compiler: Runtime,
    host: Host,
    worker: Option<Worker>,
    next_id: u64,
    statistics: Statistics,
}
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct Statistics {
    pub workers_started: u64,
    pub frames_completed: u64,
    pub image_snapshots_sent: u64,
}
impl GpuRuntime {
    pub fn new(config: Config, host: Host, cancel: &Cancel) -> Result<Self, Failure> {
        identity(host.device())?;
        Ok(Self {
            compiler: Runtime::new(config, cancel)?,
            host,
            worker: None,
            next_id: 1,
            statistics: Statistics::default(),
        })
    }
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }
    /// Bounded worker diagnostics for explicit profiling and failure review.
    pub fn diagnostics(&self) -> String {
        self.worker
            .as_ref()
            .and_then(|worker| worker.process.as_ref())
            .map(Process::diagnostics)
            .unwrap_or_default()
    }
    pub fn render(
        &mut self,
        snapshot: &Snapshot,
        fonts: &[Vec<u8>],
        cancel: &Cancel,
    ) -> Result<Rendered, Failure> {
        cancel.check()?;
        self.host.check()?;
        // A previous unwound/abandoned demand can still own a producer image.
        // Reap it before accepting another allocation or frame demand.
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.inflight.is_some())
        {
            self.worker.take();
        }
        let intent = snapshot.intent;
        let duration = Time::new(i64::from(snapshot.canvas.rate[1]), snapshot.canvas.rate[0])
            .map_err(|e| e.to_string())?;
        let limit = self
            .host
            .device()
            .limits()
            .max_texture_dimension_2d
            .min(16_384);
        if intent.dimensions.contains(&0) || intent.dimensions.iter().any(|n| *n > limit) {
            return Err(Failure::new(
                FailureKind::InvalidInput,
                "Invalid Motion frame dimensions",
            ));
        }
        crate::resources::validate(&snapshot.canvas, &snapshot.assets)?;
        let image_key = crate::resources::key(&snapshot.assets, cancel)?;
        let program = compilation::program(
            &snapshot.canvas,
            &self.compiler.config.bridge,
            &self.compiler.identity,
        )?;
        let images_changed = self.worker.as_ref().is_none_or(|worker| {
            worker.identity != program.identity || worker.images_key.as_ref() != Some(&image_key)
        });
        let resources = if images_changed {
            Some(crate::resources::load(&snapshot.assets, cancel)?)
        } else {
            None
        };
        let permit = self.host.prepare_shared_image(intent.dimensions)?;
        if self
            .worker
            .as_ref()
            .is_none_or(|w| w.identity != program.identity)
        {
            self.worker.take();
            let binary = self
                .compiler
                .executable(&snapshot.canvas, &program, cancel)?;
            self.worker = Some(Worker::start(
                &binary,
                program.identity,
                self.host.clone(),
                cancel,
                self.compiler.config.frame_timeout,
            )?);
            self.statistics.workers_started += 1;
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or("Motion request IDs exhausted")?;
        let resource_storage = resources
            .as_ref()
            .map(|resources| fold_media::budget::reserve_working(resources.bytes))
            .transpose()?;
        let (images, _host_storage) = match resources {
            Some(resources) => (Some(resources.snapshot), resources._storage),
            None => (None, Vec::new()),
        };
        let font_identity = self.compiler.fonts.identity(fonts)?;
        let worker = self.worker.as_mut().expect("started worker");
        let body = serialize(&Request {
            operation: protocol::Operation::Render,
            id,
            canvas: &*snapshot.canvas,
            time: intent.time,
            dimensions: intent.dimensions,
            allocation_limit: permit.description().allocation_bytes,
            cache_limit: worker.cache.bytes(),
            fonts: (worker.fonts.as_ref() != Some(&font_identity)).then_some(fonts),
            images,
        })?;
        let (send, receive) = mpsc::sync_channel(1);
        worker.inflight = Some(permit.retain_source());
        let result = (|| {
            worker
                .input
                .as_ref()
                .ok_or("Motion worker is closed")?
                .send(Demand {
                    id,
                    intent,
                    body,
                    cancel: cancel.clone(),
                    result: send,
                    permit: Some(permit),
                    #[cfg(test)]
                    receiving_image: None,
                })
                .map_err(|_| Failure::from("Motion worker stopped"))?;
            let start = Instant::now();
            loop {
                cancel.check()?;
                if start.elapsed() >= self.compiler.config.frame_timeout {
                    return Err("Motion GPU frame deadline exceeded".into());
                }
                match receive.recv_timeout(Duration::from_millis(10)) {
                    Ok(result) => return result,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("Motion GPU worker stopped".into());
                    }
                }
            }
        })();
        match result {
            Ok(frame) => {
                worker.inflight.take(); // producer confirmed that its image was freed
                worker.fonts = Some(font_identity);
                if let Some(storage) = resource_storage {
                    worker.image_storage = Some(storage);
                    worker.images_key = Some(image_key);
                    self.statistics.image_snapshots_sent += 1;
                }
                self.statistics.frames_completed += 1;
                Ok(Rendered {
                    intent,
                    frame: frame.with_timing(intent.time, duration)?,
                })
            }
            Err(mut error) => {
                if let Some(worker) = self.worker.take() {
                    let diagnostics = worker.stop();
                    if !diagnostics.is_empty() {
                        error.message.push_str(&format!("\n{diagnostics}"));
                    }
                }
                Err(error)
            }
        }
    }
}
struct Demand {
    id: u64,
    intent: Intent,
    body: Body,
    cancel: Cancel,
    result: mpsc::SyncSender<Result<GpuFrame, Failure>>,
    permit: Option<SharedImage>,
    #[cfg(test)]
    receiving_image: Option<Arc<AtomicBool>>,
}
struct Worker {
    cache: fold_render::gpu::ResourceAllowance,
    image_storage: Option<Arc<fold_media::budget::Lease>>,
    images_key: Option<crate::resources::Key>,
    identity: String,
    fonts: Option<String>,
    input: Option<mpsc::SyncSender<Demand>>,
    process: Option<Process>,
    io: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    inflight: Option<SharedSource>,
}
impl Worker {
    fn start(
        binary: &Path,
        identity: String,
        host: Host,
        cancel: &Cancel,
        timeout: Duration,
    ) -> Result<Self, Failure> {
        let cache =
            host.reserve_external_resources((host.memory().budget / 8).min(64 * 1024 * 1024))?;
        let directory = tempfile::tempdir().map_err(io_error)?;
        let path = directory.path().join("gpu.sock");
        let listener = UnixListener::bind(&path).map_err(io_error)?;
        listener.set_nonblocking(true).map_err(io_error)?;
        let (uuid, _) = fold_gpu_sharing::identity(host.device())?;
        let mut command = Command::new(binary);
        command
            .env("FOLD_MOTION_GPU_SOCKET", &path)
            .env(
                "FOLD_MOTION_GPU_DEVICE",
                serde_json::to_string(&uuid).map_err(|e| e.to_string())?,
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut process = Process::retained(command)?;
        let start = Instant::now();
        let socket = loop {
            cancel.check()?;
            if start.elapsed() >= timeout {
                return Err("Motion GPU worker connection timed out".into());
            }
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if process.is_finished() {
                        let diagnostics = process.diagnostics();
                        return Err(format!(
                            "Motion GPU worker exited before connection\n{diagnostics}"
                        )
                        .into());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(io_error(error)),
            }
        };
        socket.set_read_timeout(Some(timeout)).map_err(io_error)?;
        socket.set_write_timeout(Some(timeout)).map_err(io_error)?;
        let mut input = process.stdin.take().ok_or("Missing Motion input")?;
        let mut output = BufReader::new(process.stdout.take().ok_or("Missing Motion output")?);
        let (tx, rx) = mpsc::sync_channel::<Demand>(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let io = thread::spawn(move || {
            let mut socket = socket;
            while let Ok(mut demand) = rx.recv() {
                let result = (|| {
                    demand.cancel.check()?;
                    input.write_all(&demand.body.bytes).map_err(io_error)?;
                    input.flush().map_err(io_error)?;
                    receive(
                        &mut output,
                        &mut socket,
                        &host,
                        &mut demand,
                        &stopped,
                        timeout,
                    )
                })();
                let failed = result.is_err();
                let _ = demand.result.send(result);
                if failed {
                    break;
                }
            }
        });
        Ok(Self {
            cache,
            identity,
            fonts: None,
            image_storage: None,
            images_key: None,
            input: Some(tx),
            process: Some(process),
            io: Some(io),
            stop,
            inflight: None,
        })
    }
    fn stop(mut self) -> String {
        self.stop.store(true, Ordering::Relaxed);
        self.input.take();
        let diagnostics = self
            .process
            .as_ref()
            .map(Process::diagnostics)
            .unwrap_or_default();
        let result = self.process.take().map(Process::abort);
        if let Some(io) = self.io.take() {
            let _ = io.join();
        }
        match result {
            Some(Err(error)) => format!("{diagnostics}\n{error}"),
            _ => diagnostics,
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.input.take();
        self.process.take(); // kill/reap before joining blocked pipe/socket readers
        if let Some(io) = self.io.take() {
            let _ = io.join();
        }
    }
}
fn receive(
    output: &mut impl BufRead,
    socket: &mut UnixStream,
    host: &Host,
    demand: &mut Demand,
    stop: &AtomicBool,
    timeout: Duration,
) -> Result<GpuFrame, Failure> {
    let mut header = Vec::new();
    output
        .take((protocol::MAX_HEADER_BYTES + 1) as u64)
        .read_until(b'\n', &mut header)
        .map_err(io_error)?;
    if header.len() > protocol::MAX_HEADER_BYTES || header.last() != Some(&b'\n') {
        return Err("Invalid Motion GPU header".into());
    }
    let response: Response = serde_json::from_slice(&header).map_err(|e| e.to_string())?;
    if response.id != demand.id {
        return Err("Motion GPU response does not match demand".into());
    }
    if let Some(error) = response.error {
        if response.bytes != 0 || response.dimensions != [0, 0] {
            return Err("Invalid Motion GPU error response".into());
        }
        return Err(error.into());
    }
    if response.bytes != 0 || response.dimensions != demand.intent.dimensions {
        return Err("Invalid Motion GPU output dimensions or payload".into());
    }
    #[cfg(test)]
    if let Some(phase) = &demand.receiving_image {
        phase.store(true, Ordering::Release);
    }
    let (description, fd) = socket::receive(socket)?;
    demand.cancel.check()?;
    let permit = demand
        .permit
        .take()
        .ok_or("Missing Motion image reservation")?;
    if &description != permit.description() {
        return Err("Motion GPU allocation does not match demand".into());
    }
    let imported = native::import(host, description, fd)?;
    let frame = permit.receive(imported, WorkingSpace::LegacyLinearSrgb, None)?;
    let start = Instant::now();
    loop {
        demand.cancel.check()?;
        if stop.load(Ordering::Relaxed) {
            return Err(Failure::new(
                FailureKind::Cancelled,
                "Motion worker retired",
            ));
        }
        host.poll()?;
        if frame.is_ready()? {
            break;
        }
        if start.elapsed() >= timeout {
            return Err("Motion GPU completion timed out".into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    socket
        .write_all(&demand.id.to_le_bytes())
        .map_err(io_error)?;
    let mut released = [0u8; 8];
    socket.read_exact(&mut released).map_err(io_error)?;
    if u64::from_le_bytes(released) != demand.id {
        return Err("Motion GPU release does not match demand".into());
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    #[ignore = "requires native Vulkan GPU and Unix process supervision"]
    fn rejects_stale_headers_and_retires_blocked_socket_with_descendant() {
        let (host, _) = pollster::block_on(Host::headless(16 * 1024 * 1024)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("worker.py");
        let marker = dir.path().join("descendant");
        let source = format!(
            r#"#!/usr/bin/python3
import os, sys, socket, json, time
s = socket.socket(socket.AF_UNIX)
s.connect(os.environ['FOLD_MOTION_GPU_SOCKET'])
r = json.loads(sys.stdin.readline())
if r['id'] == 2:
    child = os.fork()
    if child == 0:
        time.sleep(60)
        os._exit(0)
    with open({marker:?}, 'w') as f: f.write(str(child))
print(json.dumps(dict(id=(999 if r['id']==1 else r['id']), dimensions=[1,1], bytes=0,
    encoding='linear_srgb', alpha='premultiplied', error=None)), flush=True)
time.sleep(60)
"#,
            marker = marker.to_str().unwrap()
        );
        std::fs::write(&binary, source).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cancel = Cancel::default();
        for id in [1, 2] {
            let mut worker = Worker::start(
                &binary,
                "fixture".into(),
                host.clone(),
                &cancel,
                Duration::from_secs(30),
            )
            .unwrap();
            let intent = Intent {
                epoch: ObjectId::new(),
                generation: id,
                document: DocumentId::new(),
                revision: Revision(id),
                time: Time::ZERO,
                dimensions: [1, 1],
            };
            let canvas = Canvas::default();
            let permit = host.prepare_shared_image([1, 1]).unwrap();
            let allocation_limit = permit.description().allocation_bytes;
            worker.inflight = Some(permit.retain_source());
            let body = serialize(&Request {
                operation: protocol::Operation::Render,
                id,
                canvas: &canvas,
                time: Time::ZERO,
                dimensions: [1, 1],
                allocation_limit,
                cache_limit: worker.cache.bytes(),
                fonts: None::<&[Vec<u8>]>,
                images: None,
            })
            .unwrap();
            let (tx, rx) = mpsc::sync_channel(1);
            let receiving = Arc::new(AtomicBool::new(false));
            worker
                .input
                .as_ref()
                .unwrap()
                .send(Demand {
                    id,
                    intent,
                    body,
                    cancel: cancel.clone(),
                    result: tx,
                    permit: Some(permit),
                    receiving_image: Some(receiving.clone()),
                })
                .unwrap();
            if id == 1 {
                let error = rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .err()
                    .unwrap();
                assert!(error.message.contains("does not match demand"));
            } else {
                let start = Instant::now();
                while !marker.exists() || !receiving.load(Ordering::Acquire) {
                    assert!(start.elapsed() < Duration::from_secs(5));
                    thread::sleep(Duration::from_millis(10));
                }
            }
            assert!(
                host.memory().allocated >= allocation_limit,
                "producer allowance was released before retirement"
            );
            let start = Instant::now();
            worker.stop();
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "retirement waited for the 30s socket timeout"
            );
        }
        assert_eq!(host.memory().allocated, 0);
    }
}
