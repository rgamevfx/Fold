//! Linux same-user transport. One length-prefixed request/response per connection.
//! I/O threads never own a project; the native owner drains bounded requests.
use super::retry::{MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES};
use super::{dispatch::Dispatcher, jobs, wire};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
const CONNECTIONS: usize = 8;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const OWNER_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) struct Request {
    pub bytes: Vec<u8>,
    pub response: SyncSender<Vec<u8>>,
}
struct Waiting {
    handle: jobs::Handle,
    deadline: Instant,
    response: SyncSender<Vec<u8>>,
}
pub struct Endpoint {
    directory: tempfile::TempDir,
    path: PathBuf,
    requests: Receiver<Request>,
    stop: Arc<AtomicBool>,
    waiting: Vec<Waiting>,
}
impl Endpoint {
    /// `parent` is chosen explicitly by the host. Never reuse another endpoint.
    pub fn bind(parent: &Path) -> io::Result<Self> {
        let parent = fs::canonicalize(parent)?;
        let directory = tempfile::Builder::new()
            .prefix("fold-session-")
            .tempdir_in(parent)?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        let path = directory.path().join("protocol.sock");
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let (sender, requests) = mpsc::sync_channel(CONNECTIONS);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            let active = Arc::new(AtomicUsize::new(0));
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if active.load(Ordering::Acquire) >= CONNECTIONS || peer(&stream).is_err() {
                            continue;
                        }
                        active.fetch_add(1, Ordering::AcqRel);
                        let active = active.clone();
                        let sender = sender.clone();
                        std::thread::spawn(move || {
                            let _ = serve(stream, sender);
                            active.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            directory,
            path,
            requests,
            stop,
            waiting: Vec::new(),
        })
    }
    /// Queue only valid job reads; all other input goes through ordinary dispatch.
    pub(crate) fn defer(
        &mut self,
        request: Request,
        dispatcher: &mut Dispatcher,
        now: Instant,
    ) -> Option<Request> {
        if let Ok(wire::Request {
            version: 2,
            command: wire::Command::JobStatus { handle, wait_ms },
        }) = serde_json::from_slice(&request.bytes)
            && wait_ms > 0
            && u128::from(wait_ms) <= jobs::MAX_LONG_POLL.as_millis()
            && self.waiting.len() < CONNECTIONS
        {
            if let Some(bytes) = dispatcher.job_response(handle, now, true) {
                let _ = request.response.try_send(bytes);
            } else {
                self.waiting.push(Waiting {
                    handle,
                    deadline: now + Duration::from_millis(u64::from(wait_ms)),
                    response: request.response,
                });
            }
            None
        } else {
            Some(request)
        }
    }
    pub(crate) fn poll_waiters(&mut self, dispatcher: &mut Dispatcher, now: Instant) {
        self.waiting.retain(|waiter| {
            if let Some(bytes) = dispatcher.job_response(waiter.handle, now, now < waiter.deadline)
            {
                let _ = waiter.response.try_send(bytes);
                false
            } else {
                true
            }
        });
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn next(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Private directory removal disconnects discovery immediately; detached
        // I/O threads have bounded deadlines and cannot dispatch without this owner.
        let _ = fs::remove_file(self.directory.path().join("protocol.sock"));
    }
}
fn peer(stream: &UnixStream) -> io::Result<()> {
    let credentials = rustix::net::sockopt::socket_peercred(stream)?;
    if credentials.uid != rustix::process::geteuid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Socket peer has a different user",
        ));
    }
    Ok(())
}
fn serve(mut stream: UnixStream, sender: SyncSender<Request>) -> io::Result<()> {
    let bytes = read_frame(
        &mut Deadline::new(&mut stream, IO_TIMEOUT),
        MAX_REQUEST_BYTES,
    )?;
    let (response, receive) = mpsc::sync_channel(1);
    sender
        .try_send(Request { bytes, response })
        .map_err(|_| io::Error::other("Session request queue is full or closed"))?;
    let bytes = receive.recv_timeout(OWNER_TIMEOUT).map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "Session response timed out; retry with the same request identity",
        )
    })?;
    write_frame(
        &mut Deadline::new(&mut stream, IO_TIMEOUT),
        &bytes,
        MAX_RESPONSE_BYTES,
    )
}
/// Validate ownership and modes before connecting, then authenticate the peer too.
pub fn request(path: &Path, bytes: &[u8]) -> io::Result<Vec<u8>> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Request exceeds 1 MiB",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing socket directory"))?;
    for (entry, mode, directory) in [(parent, 0o700, true), (path, 0o600, false)] {
        let metadata = fs::symlink_metadata(entry)?;
        if metadata.file_type().is_symlink()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o7777 != mode
            || if directory {
                !metadata.is_dir()
            } else {
                !metadata.file_type().is_socket()
            }
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Endpoint ownership or permissions differ",
            ));
        }
    }
    let mut stream = UnixStream::connect(path)?;
    peer(&stream)?;
    write_frame(
        &mut Deadline::new(&mut stream, IO_TIMEOUT),
        bytes,
        MAX_REQUEST_BYTES,
    )?;
    read_frame(
        &mut Deadline::new(&mut stream, OWNER_TIMEOUT + IO_TIMEOUT),
        MAX_RESPONSE_BYTES,
    )
}
struct Deadline<'a> {
    stream: &'a mut UnixStream,
    end: Instant,
}
impl<'a> Deadline<'a> {
    fn new(stream: &'a mut UnixStream, duration: Duration) -> Self {
        Self {
            stream,
            end: Instant::now() + duration,
        }
    }
    fn remaining(&self) -> io::Result<Duration> {
        self.end
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Frame deadline exceeded"))
    }
}
impl Read for Deadline<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(bytes)
    }
}
impl Write for Deadline<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
fn read_frame(input: &mut impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Frame exceeds bounds",
        ));
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn write_frame(output: &mut impl Write, bytes: &[u8], limit: usize) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Frame exceeds bounds",
        ));
    }
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(bytes)?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_rejects_unbounded_truncated_and_empty_input() {
        assert!(read_frame(&mut &u32::MAX.to_be_bytes()[..], MAX_REQUEST_BYTES).is_err());
        assert!(read_frame(&mut &[0, 0, 0, 0][..], MAX_REQUEST_BYTES).is_err());
        assert!(read_frame(&mut &[0, 0, 0, 2, 1][..], MAX_REQUEST_BYTES).is_err());
        let mut bytes = vec![];
        write_frame(&mut bytes, b"original request bytes", MAX_REQUEST_BYTES).unwrap();
        assert_eq!(
            read_frame(&mut bytes.as_slice(), MAX_REQUEST_BYTES).unwrap(),
            b"original request bytes"
        );
    }
}
