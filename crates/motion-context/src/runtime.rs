//! Compiled fframes programs and retained workers. Call from a render worker,
//! never the UI thread. Reference readback and native GPU demands share the
//! compiler/cache; the GPU runtime never falls back to reference transport.
use fold_foundation::{Failure, FailureKind, ObjectId, Time};
use fold_media::{
    budget::{self, Lease},
    process::{Cancel, Process},
};
use fold_motion_canvas::{
    compilation,
    model::{Canvas, Captured, Content},
    protocol::{self, CaptureResponse, Operation, Request, Response},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[cfg(all(feature = "gpu", target_os = "linux"))]
pub mod gpu;
mod sdk;

/// Immutable resources for a native demand. Asset records must match the canvas.
#[derive(Clone, Copy)]
pub struct Inputs<'a> {
    pub fonts: &'a [Vec<u8>],
    pub assets: &'a [Arc<fold_project::Asset>],
}

#[derive(Clone)]
pub struct Config {
    pub bridge: PathBuf,
    pub cache: PathBuf,
    pub rustc: PathBuf,
    pub profile: Profile,
    pub compile_timeout: Duration,
    pub frame_timeout: Duration,
}
#[derive(Clone, Copy)]
pub enum Profile {
    Debug,
    Release,
}
impl Profile {
    fn directory(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}
/// Linear sRGB, premultiplied RGBA. Allocation remains accounted while held.
pub struct Readback {
    pub dimensions: [u32; 2],
    pixels: Vec<[f32; 4]>,
    _storage: Arc<Lease>,
}
impl Readback {
    pub fn pixels(&self) -> &[[f32; 4]] {
        &self.pixels
    }
}
/// Captured native artwork with its transport allocation retained until staged.
pub struct Capture {
    pub artwork: Captured,
    _storage: Arc<Lease>,
}
enum Reply {
    Frame(Readback),
    Capture(Capture),
}
pub struct Runtime {
    config: Config,
    identity: String,
    bridge_identity: String,
    libraries: sdk::Libraries,
    worker: Option<Worker>,
    next_id: u64,
    fonts: FontCache,
}
#[derive(Default)]
struct FontCache {
    data: Vec<Vec<u8>>,
    identity: Option<String>,
    _storage: Option<Arc<Lease>>,
}
impl FontCache {
    fn identity(&mut self, fonts: &[Vec<u8>]) -> Result<String, Failure> {
        if self.data == fonts {
            if let Some(identity) = &self.identity {
                return Ok(identity.clone());
            }
        }
        let bytes = fonts
            .iter()
            .try_fold(0usize, |size, font| {
                size.checked_add(font.len())?
                    .checked_add(std::mem::size_of::<Vec<u8>>())
            })
            .filter(|size| *size <= protocol::MAX_REQUEST_BYTES)
            .ok_or_else(|| {
                Failure::new(
                    FailureKind::RequestTooLarge,
                    "Motion font data exceeds request allowance",
                )
            })?;
        let storage = budget::reserve_working(bytes as u64)?;
        let mut hash = Sha256::new();
        for font in fonts {
            hash_part(&mut hash, font);
        }
        let identity = format!("{:x}", hash.finalize());
        self.data = fonts.to_vec();
        self.identity = Some(identity.clone());
        self._storage = Some(storage);
        Ok(identity)
    }
}
static BUILD: Mutex<()> = Mutex::new(());

impl Runtime {
    /// Prepare the same cached program for a native GPU transport worker. Call
    /// when script source changes, not on each parameter/frame demand.
    pub fn prepare(&self, canvas: &Canvas, cancel: &Cancel) -> Result<PathBuf, Failure> {
        let program = compilation::program(canvas, &self.config.bridge, &self.identity)?;
        self.executable(canvas, &program, cancel)
    }
    pub fn new(mut config: Config, cancel: &Cancel) -> Result<Self, Failure> {
        if config.compile_timeout.is_zero() || config.frame_timeout.is_zero() {
            return Err("Motion deadlines must be positive".into());
        }
        config.bridge = config.bridge.canonicalize().map_err(io_error)?;
        let mut hash = Sha256::new();
        hash_part(&mut hash, config.profile.directory().as_bytes());
        let bridge_identity = bridge_identity(&config.bridge)?;
        hash_part(&mut hash, bridge_identity.as_bytes());
        let libraries;
        {
            let mut command = Command::new(&config.rustc);
            command
                .arg("-vV")
                .stdout(Stdio::piped())
                .stdin(Stdio::null());
            let mut process = Process::timed(command, cancel.clone(), config.compile_timeout)?;
            let mut version = Vec::new();
            process
                .stdout
                .take()
                .ok_or("Missing toolchain output")?
                .take(16 * 1024)
                .read_to_end(&mut version)
                .map_err(io_error)?;
            process.finish()?;
            hash_part(&mut hash, &version);
            libraries =
                sdk::Libraries::load(&config.bridge, &version, cancel, config.compile_timeout)?;
            hash_part(&mut hash, libraries.identity.as_bytes());
        }
        let identity = format!("{:x}", hash.finalize());
        Ok(Self {
            config,
            identity,
            bridge_identity,
            libraries,
            worker: None,
            next_id: 1,
            fonts: FontCache::default(),
        })
    }

    /// One demand in flight. Controls and native artwork do not trigger builds.
    /// Cancellation retires the worker; no obsolete response can be reused.
    pub fn render(
        &mut self,
        canvas: &Canvas,
        time: Time,
        dimensions: [u32; 2],
        inputs: Inputs<'_>,
        cancel: &Cancel,
    ) -> Result<Readback, Failure> {
        match self.request(canvas, time, dimensions, inputs, Operation::Render, cancel)? {
            Reply::Frame(frame) => Ok(frame),
            _ => Err("Motion worker returned the wrong result kind".into()),
        }
    }

    /// Evaluate local script output with the same source compiler/cache and
    /// retained process supervision. This performs no pixel readback or GPU work.
    pub fn capture(
        &mut self,
        canvas: &Canvas,
        object: ObjectId,
        time: Time,
        inputs: Inputs<'_>,
        cancel: &Cancel,
    ) -> Result<Capture, Failure> {
        canvas.validate()?;
        if time < Time::ZERO || time >= canvas.duration {
            return Err("Capture time is outside the document".into());
        }
        if !canvas
            .objects
            .get(&object)
            .is_some_and(|value| matches!(value.content, Content::Script { .. }))
        {
            return Err("Only script objects can be captured".into());
        }
        match self.request(
            canvas,
            time,
            [canvas.width, canvas.height],
            inputs,
            Operation::Capture { object },
            cancel,
        )? {
            Reply::Capture(capture) => Ok(capture),
            _ => Err("Motion worker returned the wrong result kind".into()),
        }
    }

    fn request(
        &mut self,
        canvas: &Canvas,
        time: Time,
        dimensions: [u32; 2],
        inputs: Inputs<'_>,
        operation: Operation,
        cancel: &Cancel,
    ) -> Result<Reply, Failure> {
        cancel.check()?;
        let storage = match operation {
            Operation::Render => Some(budget::reserve_working(
                (pixel_count(dimensions)? as u64) * 16,
            )?),
            Operation::Capture { .. } => None,
        };
        crate::resources::validate(canvas, inputs.assets)?;
        let image_key = crate::resources::key(inputs.assets, cancel)?;
        let program = compilation::program(canvas, &self.config.bridge, &self.identity)?;
        let changed = self.worker.as_ref().is_none_or(|worker| {
            worker.identity != program.identity || worker.images_key.as_ref() != Some(&image_key)
        });
        let resources = if changed {
            Some(crate::resources::load(inputs.assets, cancel)?)
        } else {
            None
        };
        let resource_storage = resources
            .as_ref()
            .map(|resources| budget::reserve_working(resources.bytes))
            .transpose()?;
        let (images, _host_storage) = match resources {
            Some(resources) => (Some(resources.snapshot), resources._storage),
            None => (None, Vec::new()),
        };
        if self
            .worker
            .as_ref()
            .is_none_or(|worker| worker.identity != program.identity)
        {
            self.worker.take();
            let binary = self.executable(canvas, &program, cancel)?;
            self.worker = Some(Worker::start(&binary, program.identity)?);
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or("Motion request IDs exhausted")?;
        let font_identity = self.fonts.identity(inputs.fonts)?;
        let worker = self.worker.as_mut().expect("started worker");
        let request = Request {
            id,
            operation,
            canvas,
            time,
            dimensions,
            allocation_limit: 1024 * 1024 * 1024,
            cache_limit: 64 * 1024 * 1024,
            fonts: (worker.fonts.as_ref() != Some(&font_identity)).then_some(inputs.fonts),
            images,
        };
        let body = serialize(&request)?;
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let job = Demand {
            id,
            operation,
            dimensions,
            body,
            storage,
            result: result_tx,
        };
        let result = (|| {
            worker
                .input
                .as_ref()
                .ok_or("Motion worker is closed")?
                .send(job)
                .map_err(|_| Failure::from("Motion worker stopped"))?;
            let start = Instant::now();
            loop {
                cancel.check()?;
                if start.elapsed() >= self.config.frame_timeout {
                    return Err("Motion demand deadline exceeded".into());
                }
                match result_rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(result) => return result,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("Motion worker stopped".into());
                    }
                }
            }
        })();
        match result {
            Ok(frame) => {
                worker.fonts = Some(font_identity);
                if let Some(storage) = resource_storage {
                    worker.image_storage = Some(storage);
                    worker.images_key = Some(image_key);
                }
                Ok(frame)
            }
            Err(mut error) => {
                if let Some(worker) = self.worker.take()
                    && let Err(details) = worker.stop()
                {
                    error.message.push_str(&format!("\n{}", details.message));
                }
                Err(error)
            }
        }
    }

    fn executable(
        &self,
        canvas: &Canvas,
        program: &compilation::Program,
        cancel: &Cancel,
    ) -> Result<PathBuf, Failure> {
        cancel.check()?;
        if canvas
            .objects
            .values()
            .any(|object| matches!(object.content, Content::Script { .. }))
        {
            return self.compile(program, cancel);
        }
        if bridge_identity(&self.config.bridge)? != self.bridge_identity {
            return Err("Motion sources changed; create a new runtime".into());
        }
        if file_digest_guarded(&self.libraries.native_worker, || cancel.check())?
            != self.libraries.native_digest
        {
            return Err("Native Motion worker changed; rebuild the SDK".into());
        }
        Ok(self.libraries.native_worker.clone())
    }

    fn compile(&self, program: &compilation::Program, cancel: &Cancel) -> Result<PathBuf, Failure> {
        // Bound concurrent compiler work. Publish each program binary atomically.
        let start = Instant::now();
        let _build = loop {
            cancel.check()?;
            match BUILD.try_lock() {
                Ok(lock) => break lock,
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err("Motion build lock poisoned".into());
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    if start.elapsed() >= self.config.compile_timeout {
                        return Err("Motion build deadline exceeded".into());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }
        };
        let directory = self.config.cache.join(&program.identity);
        let before = bridge_identity(&self.config.bridge)?;
        if before != self.bridge_identity {
            return Err("Motion sources changed; create a new runtime".into());
        }
        self.libraries.verify_scripts(
            cancel,
            self.config
                .compile_timeout
                .checked_sub(start.elapsed())
                .ok_or("Motion build deadline exceeded")?,
        )?;
        let binary = directory
            .join(&program.executable)
            .with_extension(std::env::consts::EXE_EXTENSION);
        let digest_file = directory.join("binary.sha256");
        if binary.is_file()
            && fs::read_to_string(&digest_file).ok().as_deref()
                == Some(file_digest(&binary)?.as_str())
        {
            return Ok(binary);
        }
        fs::create_dir_all(directory.join("src")).map_err(io_error)?;
        fs::write(directory.join("Cargo.toml"), &program.manifest).map_err(io_error)?;
        fs::write(directory.join("src/main.rs"), &program.source).map_err(io_error)?;
        let temporary = tempfile::NamedTempFile::new_in(&directory).map_err(io_error)?;
        let mut command = Command::new(&self.config.rustc);
        command
            .arg(directory.join("src/main.rs"))
            .args(["--edition=2024", "--crate-name", "fold_motion_program"])
            .arg("-L")
            .arg(format!("dependency={}", self.libraries.directory.display()))
            .arg("-o")
            .arg(temporary.path())
            .env("CARGO_MANIFEST_DIR", &directory)
            .env("CARGO_PKG_NAME", "fold-motion-program")
            .env("CARGO_PKG_VERSION", "0.1.0")
            .env("CARGO_CRATE_NAME", "fold_motion_program")
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        for (name, path) in &self.libraries.externs {
            command
                .arg("--extern")
                .arg(format!("{name}={}", path.display()));
        }
        if matches!(self.config.profile, Profile::Release) {
            command.args(["-C", "opt-level=3"]);
        }
        let remaining = self
            .config
            .compile_timeout
            .checked_sub(start.elapsed())
            .ok_or("Motion build deadline exceeded")?;
        Process::timed(command, cancel.clone(), remaining)?.finish()?;
        cancel.check()?;
        if bridge_identity(&self.config.bridge)? != before {
            return Err(
                "Motion sources changed during compilation; retry with a new runtime".into(),
            );
        }
        let digest = file_digest(temporary.path())?;
        temporary.persist(&binary).map_err(|e| io_error(e.error))?;
        fs::write(digest_file, digest).map_err(io_error)?;
        Ok(binary)
    }
}

struct Body {
    bytes: Vec<u8>,
    _storage: Arc<Lease>,
}
struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n < protocol::MAX_REQUEST_BYTES)
            .ok_or_else(|| {
                std::io::Error::other("Motion request exceeds its transport allowance")
            })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn serialize(request: &impl serde::Serialize) -> Result<Body, Failure> {
    let mut count = Counter(0);
    serde_json::to_writer(&mut count, request).map_err(|e| e.to_string())?;
    let storage = budget::reserve_working((count.0 + 1) as u64)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(count.0 + 1)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut bytes, request).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(Body {
        bytes,
        _storage: storage,
    })
}
struct Demand {
    id: u64,
    operation: Operation,
    dimensions: [u32; 2],
    body: Body,
    storage: Option<Arc<Lease>>,
    result: mpsc::SyncSender<Result<Reply, Failure>>,
}
struct Worker {
    identity: String,
    fonts: Option<String>,
    images_key: Option<crate::resources::Key>,
    image_storage: Option<Arc<Lease>>,
    input: Option<mpsc::SyncSender<Demand>>,
    process: Option<Process>,
    io: Option<JoinHandle<()>>,
}
impl Worker {
    fn start(binary: &Path, identity: String) -> Result<Self, Failure> {
        let mut command = Command::new(binary);
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut process = Process::retained(command)?;
        let mut input = process.stdin.take().ok_or("Missing Motion input")?;
        let mut output = BufReader::new(process.stdout.take().ok_or("Missing Motion output")?);
        let (tx, rx) = mpsc::sync_channel::<Demand>(1);
        let io = thread::spawn(move || {
            while let Ok(demand) = rx.recv() {
                let result = (|| {
                    input.write_all(&demand.body.bytes).map_err(io_error)?;
                    input.flush().map_err(io_error)?;
                    match demand.operation {
                        Operation::Render => receive(
                            &mut output,
                            demand.id,
                            demand.dimensions,
                            demand.storage.ok_or("Missing frame allocation")?,
                        )
                        .map(Reply::Frame),
                        Operation::Capture { .. } => {
                            receive_capture(&mut output, demand.id).map(Reply::Capture)
                        }
                    }
                })();
                let failed = result.is_err();
                let _ = demand.result.send(result);
                if failed {
                    break;
                }
            }
        });
        Ok(Self {
            identity,
            fonts: None,
            images_key: None,
            image_storage: None,
            input: Some(tx),
            process: Some(process),
            io: Some(io),
        })
    }
    fn stop(mut self) -> Result<(), Failure> {
        self.input.take();
        let result = self.process.take().map(Process::abort).unwrap_or(Ok(()));
        if let Some(io) = self.io.take() {
            io.join().map_err(|_| "Motion pipe reader panicked")?;
        }
        result
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.input.take();
        self.process.take(); // kill before joining a possibly blocked pipe
        if let Some(io) = self.io.take() {
            let _ = io.join();
        }
    }
}
fn receive(
    output: &mut impl BufRead,
    id: u64,
    dimensions: [u32; 2],
    storage: Arc<Lease>,
) -> Result<Readback, Failure> {
    let header = receive_header(output)?;
    let response: Response = serde_json::from_slice(&header).map_err(|e| e.to_string())?;
    if response.id != id {
        return Err("Motion response does not match its demand".into());
    }
    if let Some(error) = response.error {
        if response.bytes != 0 || response.dimensions != [0, 0] {
            return Err("Invalid Motion error response".into());
        }
        return Err(error.into());
    }
    let count = pixel_count(dimensions)?;
    if response.dimensions != dimensions || response.bytes != count * 16 {
        return Err("Invalid Motion response dimensions or storage size".into());
    }
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(count).map_err(|e| e.to_string())?;
    pixels.resize(count, [0f32; 4]);
    output
        .read_exact(bytemuck::cast_slice_mut(&mut pixels))
        .map_err(io_error)?;
    #[cfg(target_endian = "big")]
    for value in pixels.iter_mut().flatten() {
        *value = f32::from_bits(value.to_bits().swap_bytes());
    }
    if pixels
        .iter()
        .any(|rgba| rgba.iter().any(|v| !v.is_finite()) || !(0.0..=1.0).contains(&rgba[3]))
    {
        return Err("Invalid Motion pixel values".into());
    }
    Ok(Readback {
        dimensions,
        pixels,
        _storage: storage,
    })
}
fn receive_header(output: &mut impl BufRead) -> Result<Vec<u8>, Failure> {
    let mut header = Vec::new();
    output
        .take((protocol::MAX_HEADER_BYTES + 1) as u64)
        .read_until(b'\n', &mut header)
        .map_err(io_error)?;
    if header.len() > protocol::MAX_HEADER_BYTES || header.last() != Some(&b'\n') {
        return Err("Invalid Motion response header".into());
    }
    Ok(header)
}
fn receive_capture(output: &mut impl BufRead, id: u64) -> Result<Capture, Failure> {
    let response: CaptureResponse =
        serde_json::from_slice(&receive_header(output)?).map_err(|error| error.to_string())?;
    if response.id != id {
        return Err("Motion response does not match its demand".into());
    }
    if let Some(error) = response.error {
        if response.bytes != 0 {
            return Err("Invalid Motion error response".into());
        }
        return Err(error.into());
    }
    if response.bytes == 0 || response.bytes > protocol::MAX_REQUEST_BYTES {
        return Err("Invalid Motion capture storage size".into());
    }
    let storage = budget::reserve_working(response.bytes as u64)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(response.bytes)
        .map_err(|error| error.to_string())?;
    bytes.resize(response.bytes, 0);
    output.read_exact(&mut bytes).map_err(io_error)?;
    let artwork = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    Ok(Capture {
        artwork,
        _storage: storage,
    })
}
fn pixel_count(dimensions: [u32; 2]) -> Result<usize, Failure> {
    if dimensions.contains(&0) {
        return Err("Motion dimensions must be positive".into());
    }
    (dimensions[0] as usize)
        .checked_mul(dimensions[1] as usize)
        .filter(|n| n.checked_mul(16).is_some())
        .ok_or_else(|| Failure::new(FailureKind::RequestTooLarge, "Motion frame is too large"))
}
fn io_error(error: std::io::Error) -> Failure {
    error.to_string().into()
}
fn hash_part(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
fn file_digest(path: &Path) -> Result<String, Failure> {
    file_digest_guarded(path, || Ok(()))
}
fn file_digest_guarded(
    path: &Path,
    mut check: impl FnMut() -> Result<(), Failure>,
) -> Result<String, Failure> {
    let mut file = fs::File::open(path).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    loop {
        check()?;
        let n = file.read(&mut bytes).map_err(io_error)?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn bridge_identity(bridge: &Path) -> Result<String, Failure> {
    let crates = bridge.parent().ok_or("Missing Motion bridge parent")?;
    let root = crates.parent().ok_or("Missing Fold source root")?;
    let mut paths = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        bridge.join("Cargo.lock"),
    ];
    let source_config = root.join(".cargo/config.toml");
    if source_config.is_file() {
        paths.push(source_config);
    }
    let library_manifest = root.join("libraries/manifest.json");
    if library_manifest.is_file() {
        paths.push(library_manifest);
    }
    let sdk_manifest = root.join("sources.json");
    if sdk_manifest.is_file() {
        paths.push(sdk_manifest);
    }
    fn sources(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), Failure> {
        for entry in fs::read_dir(directory).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.is_dir() {
                sources(&path, paths)?;
            } else {
                paths.push(path);
            }
        }
        Ok(())
    }
    for name in [
        "motion-fframes",
        "gpu-sharing",
        "motion-canvas",
        "graph",
        "foundation",
        "animation",
        "project",
    ] {
        let directory = crates.join(name);
        paths.push(directory.join("Cargo.toml"));
        sources(&directory.join("src"), &mut paths)?;
    }
    paths.sort();
    let mut hash = Sha256::new();
    for path in paths {
        hash_part(
            &mut hash,
            path.strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .as_bytes(),
        );
        hash_part(&mut hash, file_digest(&path)?.as_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn retained_font_identity_reuses_equal_data_and_detects_in_place_and_boundary_changes() {
        let mut cache = FontCache::default();
        let mut fonts = vec![b"ab".to_vec(), b"c".to_vec()];
        let original = cache.identity(&fonts).unwrap();
        let storage = cache.data[0].as_ptr();
        assert_eq!(cache.identity(&fonts.clone()).unwrap(), original);
        assert_eq!(cache.data[0].as_ptr(), storage);
        fonts[0][0] = b'x';
        assert_ne!(cache.identity(&fonts).unwrap(), original);
        // Font boundaries and order matter even when concatenated bytes match.
        assert_ne!(
            cache.identity(&[b"a".to_vec(), b"bc".to_vec()]).unwrap(),
            original
        );
        assert_ne!(
            cache.identity(&[b"c".to_vec(), b"ab".to_vec()]).unwrap(),
            original
        );
        assert_eq!(
            cache.identity(&[b"ab".to_vec(), b"c".to_vec()]).unwrap(),
            original
        );
        let empty = cache.identity(&[]).unwrap();
        assert_ne!(cache.identity(&[vec![]]).unwrap(), empty);
    }

    // Transport fixture only: this does not qualify fframes rendering.
    #[test]
    fn retained_runtime_reuses_controls_rejects_stale_frames_and_cancels_blocked_pipes() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir(root.join("crates")).unwrap();
        fs::write(root.join("Cargo.toml"), "fixture").unwrap();
        fs::write(root.join("Cargo.lock"), "fixture").unwrap();
        for name in [
            "motion-fframes",
            "gpu-sharing",
            "motion-canvas",
            "graph",
            "foundation",
            "animation",
            "project",
        ] {
            let package = root.join("crates").join(name);
            fs::create_dir_all(package.join("src")).unwrap();
            fs::write(package.join("Cargo.toml"), "fixture").unwrap();
            fs::write(package.join("src/lib.rs"), "fixture").unwrap();
        }
        let bridge = root.join("crates/motion-fframes");
        fs::write(bridge.join("Cargo.lock"), "fixture").unwrap();
        let identity = bridge_identity(&bridge).unwrap();
        let graph_source = root.join("crates/graph/src/lib.rs");
        fs::write(&graph_source, "changed track evaluation").unwrap();
        assert_ne!(bridge_identity(&bridge).unwrap(), identity);
        fs::write(&graph_source, "fixture").unwrap();
        assert_eq!(bridge_identity(&bridge).unwrap(), identity);
        let compiler = root.join("compiler");
        executable(
            &compiler,
            "#!/usr/bin/env python3\nimport sys\nassert sys.argv[1:] == ['-vV'], 'Cached programs must not compile'\nprint('transport fixture version')\n",
        );
        let libraries = root.join("libraries");
        fs::create_dir(&libraries).unwrap();
        let mut files = serde_json::Map::new();
        let mut externs = serde_json::Map::new();
        for name in ["fframes", "fold_motion_fframes"] {
            let file = format!("lib{name}-fixture.rlib");
            fs::write(libraries.join(&file), "transport fixture").unwrap();
            files.insert(
                file.clone(),
                file_digest(&libraries.join(&file)).unwrap().into(),
            );
            externs.insert(name.into(), file.into());
        }
        let child_marker = root.join("child-started");
        executable(
            &libraries.join("fold-motion-native-worker"),
            &r#"#!/usr/bin/env python3
import sys,json,struct,time,subprocess
first=True
for line in sys.stdin:
 r=json.loads(line)
 assert (r['fonts'] is not None) == first
 first=False
 width=r['canvas']['width']
 if width==99:
  subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])
  open(__CHILD_MARKER__, 'w').write('started')
  time.sleep(30)
 id=r['id']+(1 if width==20 else 0)
 header=dict(id=id,dimensions=r['dimensions'],bytes=16,encoding='linear_srgb',alpha='premultiplied',error=None)
 sys.stdout.buffer.write((json.dumps(header)+'\n').encode())
 sys.stdout.buffer.write(struct.pack('<ffff',float('nan') if width==21 else width/10000,0,0,1))
 sys.stdout.buffer.flush()
"#.replace("__CHILD_MARKER__", &serde_json::to_string(&child_marker).unwrap()),
        );
        files.insert(
            "fold-motion-native-worker".into(),
            file_digest(&libraries.join("fold-motion-native-worker"))
                .unwrap()
                .into(),
        );
        fs::write(
            libraries.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "version": 2, "profile": "debug", "rustc": "transport fixture version\n",
                "native_worker": "fold-motion-native-worker",
                "inputs": {"Cargo.toml": file_digest(&root.join("Cargo.toml")).unwrap()},
                "files": files, "externs": externs
            }))
            .unwrap(),
        )
        .unwrap();
        let version = b"transport fixture version\n";
        let verify = |version: &[u8], timeout| {
            sdk::Libraries::load(&bridge, version, &Cancel::default(), timeout)
        };
        assert!(verify(version, Duration::from_secs(5)).is_ok());
        assert!(verify(b"different compiler", Duration::from_secs(5)).is_err());
        let native_only = verify(version, Duration::from_secs(5)).unwrap();
        fs::write(libraries.join("libfframes-fixture.rlib"), "changed library").unwrap();
        assert!(verify(version, Duration::from_secs(5)).is_ok());
        assert!(
            native_only
                .verify_scripts(&Cancel::default(), Duration::from_secs(5))
                .is_err()
        );
        fs::write(
            libraries.join("libfframes-fixture.rlib"),
            "transport fixture",
        )
        .unwrap();
        assert!(
            native_only
                .verify_scripts(&Cancel::default(), Duration::from_secs(5))
                .is_ok()
        );

        assert!(verify(version, Duration::ZERO).is_err());
        fs::write(libraries.join("libunlisted.rlib"), "unlisted").unwrap();
        assert!(verify(version, Duration::from_secs(5)).is_err());
        fs::remove_file(libraries.join("libunlisted.rlib")).unwrap();
        fs::write(root.join("Cargo.toml"), "changed source").unwrap();
        assert!(verify(version, Duration::from_secs(5)).is_err());
        fs::write(root.join("Cargo.toml"), "fixture").unwrap();
        let cancel = Cancel::default();
        let mut runtime = Runtime::new(
            Config {
                bridge,
                cache: root.join("cache"),
                rustc: compiler,
                profile: Profile::Debug,
                compile_timeout: Duration::from_secs(5),
                frame_timeout: Duration::from_secs(5),
            },
            &cancel,
        )
        .unwrap();
        let mut canvas = Canvas::default();
        let program =
            compilation::program(&canvas, &runtime.config.bridge, &runtime.identity).unwrap();
        let fonts = vec![vec![1, 2, 3]];
        let first = runtime
            .render(
                &canvas,
                Time::ZERO,
                [1, 1],
                Inputs {
                    fonts: &fonts,
                    assets: &[],
                },
                &cancel,
            )
            .unwrap();
        canvas.width = 1000;
        let second = runtime
            .render(
                &canvas,
                Time::ZERO,
                [1, 1],
                Inputs {
                    fonts: &fonts,
                    assets: &[],
                },
                &cancel,
            )
            .unwrap();
        assert_ne!(first.pixels, second.pixels);
        assert_eq!(runtime.worker.as_ref().unwrap().identity, program.identity);
        canvas.width = 20;
        let stale = runtime
            .render(
                &canvas,
                Time::ZERO,
                [1, 1],
                Inputs {
                    fonts: &fonts,
                    assets: &[],
                },
                &cancel,
            )
            .err()
            .unwrap();
        assert!(stale.message.contains("does not match"));
        assert!(runtime.worker.is_none());
        canvas.width = 21;
        let invalid = runtime
            .render(
                &canvas,
                Time::ZERO,
                [1, 1],
                Inputs {
                    fonts: &fonts,
                    assets: &[],
                },
                &cancel,
            )
            .err()
            .unwrap();
        assert!(invalid.message.contains("Invalid Motion pixel values"));
        assert!(runtime.worker.is_none());
        canvas.width = 99;
        let obsolete = Cancel::default();
        let stop = obsolete.clone();
        let thread = thread::spawn(move || {
            let start = Instant::now();
            while !child_marker.is_file() && start.elapsed() < Duration::from_secs(2) {
                thread::sleep(Duration::from_millis(10));
            }
            stop.cancel();
            assert!(
                child_marker.is_file(),
                "worker did not start its descendant"
            );
        });
        let start = Instant::now();
        let error = runtime
            .render(
                &canvas,
                Time::ZERO,
                [1, 1],
                Inputs {
                    fonts: &fonts,
                    assets: &[],
                },
                &obsolete,
            )
            .err()
            .unwrap();
        thread.join().unwrap();
        assert_eq!(error.kind, FailureKind::Cancelled);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "descendant held a pipe open"
        );
        assert!(runtime.worker.is_none());
        canvas.width = 1000;
        let restarted = runtime
            .render(
                &canvas,
                Time::ZERO,
                [1, 1],
                Inputs {
                    fonts: &fonts,
                    assets: &[],
                },
                &cancel,
            )
            .unwrap();
        assert_eq!(second.pixels, restarted.pixels);
        fs::write(root.join("crates/motion-canvas/src/lib.rs"), "changed").unwrap();
        runtime.worker.take();
        let changed = runtime
            .render(
                &canvas,
                Time::ZERO,
                [1, 1],
                Inputs {
                    fonts: &fonts,
                    assets: &[],
                },
                &cancel,
            )
            .err()
            .unwrap();
        assert!(changed.message.contains("sources changed"));
        executable(
            &runtime.config.rustc,
            "#!/usr/bin/env python3\nimport sys\nsys.stderr.write('Ω'*5000 + '\\nRoot build error\\n')\nsys.exit(1)\n",
        );
        let mut command = Command::new(&runtime.config.rustc);
        command.stdin(Stdio::null()).stdout(Stdio::null());
        let details = Process::timed(command, Cancel::default(), Duration::from_secs(5))
            .unwrap()
            .finish()
            .unwrap_err();
        assert!(details.message.contains("Root build error"));
        assert!(details.message.len() < 8300);
    }
    fn executable(path: &Path, source: &str) {
        fs::write(path, source).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
}
