//! Bounded newline JSON IPC in a private per-user runtime directory.
use super::*;
#[cfg(unix)]
use std::os::unix::{
    fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    net::{UnixListener, UnixStream},
};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
pub struct Incoming {
    pub request: Request,
    pub reply: mpsc::SyncSender<Value>,
}
pub struct Server {
    pub session: String,
    pub receiver: mpsc::Receiver<Incoming>,
    stop: Arc<AtomicBool>,
    directory: PathBuf,
}
pub fn line(reader: &mut impl BufRead, max: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    loop {
        let chunk = reader.fill_buf().map_err(|e| e.to_string())?;
        if chunk.is_empty() {
            return Err("connection ended before newline".into());
        }
        let end = chunk.iter().position(|b| *b == b'\n').map(|i| i + 1);
        let n = end.unwrap_or(chunk.len());
        if bytes.len() + n > max {
            return Err("message exceeds byte limit".into());
        }
        bytes.extend_from_slice(&chunk[..n]);
        reader.consume(n);
        if end.is_some() {
            return Ok(bytes);
        }
    }
}
#[cfg(unix)]
pub fn runtime() -> Result<PathBuf, String> {
    let uid = std::fs::metadata("/proc/self")
        .map_err(|e| e.to_string())?
        .uid();
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/fold-runtime-{uid}")));
    private_directory(&base, uid)?;
    let dir = base.join("fold-agents");
    private_directory(&dir, uid)?;
    Ok(dir)
}
#[cfg(unix)]
fn private_directory(path: &Path, uid: u32) -> Result<(), String> {
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        return Err("agent runtime directory must be an owned directory with mode 0700".into());
    }
    Ok(())
}
#[cfg(unix)]
impl Server {
    pub fn start() -> Result<Self, String> {
        let session = serde_json::to_value(fold_foundation::ObjectId::new())
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let directory = runtime()?.join(&session);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|e| e.to_string())?;
        let socket = directory.join("ipc");
        let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        std::fs::write(
            directory.join("session.json"),
            json!({"session":session,"pid":std::process::id(),"version":VERSION}).to_string(),
        )
        .map_err(|e| e.to_string())?;
        let (sender, receiver) = mpsc::sync_channel(16);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let clients = Arc::new(AtomicUsize::new(0));
        std::thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if clients.load(Ordering::Acquire) >= 16 {
                            drop(stream);
                            continue;
                        }
                        clients.fetch_add(1, Ordering::AcqRel);
                        let clients = clients.clone();
                        let sender = sender.clone();
                        let stopping = stopping.clone();
                        std::thread::spawn(move || {
                            let _ = serve(stream, sender, &stopping);
                            clients.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            session,
            receiver,
            stop,
            directory,
        })
    }
}
#[cfg(unix)]
fn serve(
    mut stream: UnixStream,
    sender: mpsc::SyncSender<Incoming>,
    stop: &AtomicBool,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let parsed = line(&mut BufReader::new(&stream), MAX_REQUEST)
        .and_then(|b| serde_json::from_slice::<Request>(&b).map_err(|e| e.to_string()));
    let reply = match parsed {
        Ok(request) if request.version == VERSION => {
            let (tx, rx) = mpsc::sync_channel(1);
            if stop.load(Ordering::Acquire) {
                error("session disconnected")
            } else if sender.try_send(Incoming { request, reply: tx }).is_err() {
                error("session busy; retry with the same request identity")
            } else {
                rx.recv_timeout(Duration::from_secs(30))
                    .unwrap_or_else(|_| {
                        error("session timeout; retry mutations with the same request identity")
                    })
            }
        }
        Ok(_) => error("unsupported Fold automation version"),
        Err(e) => error(e),
    };
    let mut bytes = serde_json::to_vec(&reply).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_RESPONSE {
        bytes = serde_json::to_vec(&error("response exceeds limit")).unwrap();
    }
    bytes.push(b'\n');
    stream.write_all(&bytes).map_err(|e| e.to_string())
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = std::fs::remove_file(self.directory.join("ipc"));
        let _ = std::fs::remove_file(self.directory.join("session.json"));
        let _ = std::fs::remove_dir(&self.directory);
    }
}
#[cfg(not(unix))]
pub fn runtime() -> Result<PathBuf, String> {
    Err("live agent IPC currently requires Unix".into())
}
#[cfg(not(unix))]
impl Server {
    pub fn start() -> Result<Self, String> {
        Err("live agent IPC currently requires Unix".into())
    }
}
