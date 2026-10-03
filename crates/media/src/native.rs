//! Explicit GPU-resident NVDEC backend. Separate from CUDA-download compatibility.
//! Two retained supervised helpers share aggregate process permits and immutable
//! source pins with software decoding. No media dependency on renderer/wgpu.
#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
use crate::{Cancel, VideoSource, process::Process, source_pin};
use fold_foundation::Time;
use std::{
    collections::VecDeque,
    os::unix::net::UnixDatagram,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nv12Layout {
    pub dimensions: [u32; 2],
    pub stride: u32,
    pub uv_offset: u64,
    pub bytes: u64,
}
impl Nv12Layout {
    pub fn for_source(source: &VideoSource) -> Result<Self, String> {
        source.info.validate()?;
        let [w, h] = [source.info.width, source.info.height];
        if !(48..=4096).contains(&w)
            || !(16..=4096).contains(&h)
            || !w.is_multiple_of(2)
            || !h.is_multiple_of(2)
            || source.info.frames > 18000
        {
            return Err(
                "native NVDEC requires even geometry 48..4096 by 16..4096 and <=18000 frames"
                    .into(),
            );
        }
        let stride = w.div_ceil(4) * 4;
        let uv_offset = u64::from(stride) * u64::from(h);
        Ok(Self {
            dimensions: [w, h],
            stride,
            uv_offset,
            bytes: uv_offset * 3 / 2,
        })
    }
}
struct Session {
    process: Option<Process>,
    socket: UnixDatagram,
    _directory: tempfile::TempDir,
    pin: Arc<source_pin::Pinned>,
    uuid: [u8; 16],
    next: Option<u32>,
}
impl Session {
    fn receive(&mut self, cancel: &Cancel, deadline: Instant) -> Result<Vec<u8>, String> {
        loop {
            self.check_wait(cancel, deadline)?;
            let mut bytes = [0; 64];
            match self.socket.recv(&mut bytes) {
                Ok(n) => return Ok(bytes[..n].to_vec()),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(format!("native helper socket: {e}")),
            }
        }
    }
    fn check_wait(&mut self, cancel: &Cancel, deadline: Instant) -> Result<(), String> {
        cancel.check()?;
        if Instant::now() >= deadline {
            return Err("native decoder deadline exceeded".into());
        }
        if self.process.as_ref().is_some_and(Process::is_finished) {
            let result = self.process.take().unwrap().finish();
            return Err(format!(
                "native helper stopped: {}",
                result.err().unwrap_or_else(|| "unexpected exit".into())
            ));
        }
        Ok(())
    }
    fn open(
        pin: Arc<source_pin::Pinned>,
        uuid: [u8; 16],
        helper: &std::path::Path,
        cancel: &Cancel,
        deadline: Instant,
    ) -> Result<Self, String> {
        let source = &pin.source;
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let parent = directory.path().join("parent.sock");
        let child = directory.path().join("child.sock");
        let socket = UnixDatagram::bind(&parent).map_err(|e| e.to_string())?;
        socket
            .set_read_timeout(Some(Duration::from_millis(10)))
            .map_err(|e| e.to_string())?;
        socket
            .set_write_timeout(Some(Duration::from_millis(10)))
            .map_err(|e| e.to_string())?;
        let info = &source.info;
        let mut command = Command::new(helper);
        command
            .arg(&parent)
            .arg(&child)
            .arg(pin.file.path())
            .args([
                info.width.to_string(),
                info.height.to_string(),
                info.rate[0].to_string(),
                info.rate[1].to_string(),
                info.frames.to_string(),
                uuid.iter().map(|b| format!("{b:02x}")).collect(),
                fold_native_video::PROTOCOL_NAME.into(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        let mut session = Self {
            process: Some(Process::retained(command)?),
            socket,
            _directory: directory,
            pin,
            uuid,
            next: None,
        };
        if session.receive(cancel, deadline)? != fold_native_video::READY_MESSAGE {
            return Err("invalid native helper handshake".into());
        }
        session.socket.connect(&child).map_err(|e| e.to_string())?;
        Ok(session)
    }
}
/// Parent-side range scheduling witnesses. Seeks/reuses count attempted requests;
/// requests counts only completed copies, launches only successful handshakes.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeStatistics {
    pub launches: u64,
    /// Includes the initial seek on each new range.
    pub seeks: u64,
    pub forward_reuses: u64,
    pub requests: u64,
    /// Completed requests' helper decode() wall time, including demux/seek and
    /// codec/driver waits. Excludes startup/pinning/IPC; not physical NVDEC time.
    pub codec_call_nanoseconds: u64,
    /// Completed requests' helper copy() wall time through CUDA stream sync and
    /// import release, excluding ACK transport. Not isolated GPU-copy time.
    pub copy_ready_nanoseconds: u64,
    /// Completed destination NV12 bytes including stride padding, once per
    /// request. Not measured bus traffic or the sum of clear/copy transactions.
    pub gpu_local_copy_bytes: u64,
}
/// Two LRU range cursors (possibly for the same source), not a frame cache.
/// Failed/cancelled requests kill/reap before caller allocations can be released.
pub struct NativeDecoder {
    sessions: VecDeque<Session>,
    helper: PathBuf,
    timeout: Duration,
    statistics: NativeStatistics,
}
impl NativeDecoder {
    pub fn new() -> Result<Self, String> {
        let helper = std::env::var_os("FOLD_VIDEO_HELPER")
            .map(PathBuf::from)
            .unwrap_or(
                std::env::current_exe()
                    .map_err(|e| e.to_string())?
                    .with_file_name("fold-video-helper"),
            );
        if !helper.is_file() {
            return Err(format!("native NVDEC helper missing: {}", helper.display()));
        }
        Ok(Self {
            sessions: VecDeque::new(),
            helper,
            timeout: Duration::from_secs(30),
            statistics: NativeStatistics::default(),
        })
    }
    pub fn statistics(&self) -> NativeStatistics {
        self.statistics
    }
    /// Writes only GPU memory. On any failure the active helper is killed/reaped
    /// before returning, so the caller can safely destroy the export allocation.
    /// The opaque receipt proves the matching request acknowledged CUDA stream
    /// completion AND import destruction. It is not an external GPU semaphore.
    pub fn decode_into(
        &mut self,
        source: &VideoSource,
        time: Time,
        ticket: fold_native_video::WriteTicket<'_>,
        cancel: &Cancel,
    ) -> Result<fold_native_video::WriteComplete, String> {
        cancel.check()?;
        let layout = Nv12Layout::for_source(source)?;
        let frame = source.info.frame_at(time)?;
        if ticket.bytes() != layout.bytes || ticket.allocation_bytes() < layout.bytes {
            return Err("native target buffer layout mismatch".into());
        }
        let uuid = ticket.device_uuid();
        let deadline = Instant::now() + self.timeout;
        let matches = |s: &Session| {
            s.pin.source.fingerprint == source.fingerprint
                && s.pin.source.info == source.info
                && s.uuid == uuid
        };
        let nearby = |s: &Session| s.next.is_some_and(|n| n <= frame && frame - n <= 32);
        let index = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| matches(s) && nearby(s))
            .min_by_key(|(_, s)| frame - s.next.unwrap())
            .map(|(i, _)| i);
        // A free second slot is a second retained RANGE, even for one source.
        // Once full, seek the least-recent same-source cursor rather than evict
        // the other video. A third source evicts the overall least-recent slot.
        let index = index.or_else(|| {
            (self.sessions.len() == 2)
                .then(|| self.sessions.iter().position(matches))
                .flatten()
        });
        let mut session = if let Some(i) = index {
            self.sessions.remove(i).unwrap()
        } else {
            let pinned = self
                .sessions
                .iter()
                .find(|s| matches(s))
                .map(|s| s.pin.clone());
            if self.sessions.len() == 2 {
                // Release the evicted source before reserving a third copy.
                self.sessions.pop_front();
            }
            let pin = match pinned {
                Some(pin) => pin,
                None => source_pin::acquire(source, cancel)?,
            };
            let session = Session::open(pin, uuid, &self.helper, cancel, deadline)?;
            self.statistics.launches = self.statistics.launches.saturating_add(1);
            session
        };
        if nearby(&session) {
            self.statistics.forward_reuses = self.statistics.forward_reuses.saturating_add(1);
        } else {
            self.statistics.seeks = self.statistics.seeks.saturating_add(1);
        }
        let mut pending = ticket.send(&session.socket, frame)?;
        let complete = loop {
            session.check_wait(cancel, deadline)?;
            if let Some(complete) = pending.receive()? {
                break complete;
            }
        };
        cancel.check()?;
        session.next = Some(frame + 1);
        self.statistics.requests = self.statistics.requests.saturating_add(1);
        self.statistics.codec_call_nanoseconds = self
            .statistics
            .codec_call_nanoseconds
            .saturating_add(complete.codec_call_nanoseconds());
        self.statistics.copy_ready_nanoseconds = self
            .statistics
            .copy_ready_nanoseconds
            .saturating_add(complete.copy_ready_nanoseconds());
        self.statistics.gpu_local_copy_bytes = self
            .statistics
            .gpu_local_copy_bytes
            .saturating_add(layout.bytes);
        self.sessions.push_back(session);
        Ok(complete)
    }
}
