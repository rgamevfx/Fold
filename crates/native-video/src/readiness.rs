//! Request-bound readiness receipts. Only a matching response read from the
//! socket that received the FD can produce a completion accepted by GPU ingress.
use std::{
    os::{fd::BorrowedFd, unix::net::UnixDatagram},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
pub struct WriteTicket<'a> {
    pub(crate) id: u64,
    fd: BorrowedFd<'a>,
    uuid: [u8; 16],
    allocation: u64,
    bytes: u64,
}
pub struct PendingWrite {
    socket: UnixDatagram,
    id: u64,
    frame: u32,
}
#[derive(Debug)]
pub struct WriteComplete {
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub(crate) id: u64,
    codec_call_nanoseconds: u64,
    copy_ready_nanoseconds: u64,
}
impl WriteComplete {
    /// Inclusive helper decode() wall time: demux, seek/preroll, codec/driver waits.
    /// Excludes helper startup, source pinning and parent/socket wait time.
    pub fn codec_call_nanoseconds(&self) -> u64 {
        self.codec_call_nanoseconds
    }
    /// Inclusive helper copy() wall time until ready: CUDA import, clear/copy,
    /// stream synchronization and import teardown. NOT isolated GPU copy time.
    /// Excludes acknowledgment transport back to the parent.
    pub fn copy_ready_nanoseconds(&self) -> u64 {
        self.copy_ready_nanoseconds
    }
}
impl<'a> WriteTicket<'a> {
    /// A standalone descriptor request. GPU buffers issue their own bound ticket;
    /// a standalone ticket cannot authorize importing a different allocation.
    pub fn new(fd: BorrowedFd<'a>, uuid: [u8; 16], allocation: u64, bytes: u64) -> Self {
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            fd,
            uuid,
            allocation,
            bytes,
        }
    }
    pub fn device_uuid(&self) -> [u8; 16] {
        self.uuid
    }
    pub fn allocation_bytes(&self) -> u64 {
        self.allocation
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn send(self, socket: &UnixDatagram, frame: u32) -> Result<PendingWrite, String> {
        let socket = socket.try_clone().map_err(|e| e.to_string())?;
        crate::send_fd(
            &socket,
            &crate::request(frame, self.allocation, self.bytes),
            self.fd,
        )
        .map_err(|e| e.to_string())?;
        Ok(PendingWrite {
            socket,
            id: self.id,
            frame,
        })
    }
}
impl PendingWrite {
    /// Socket timeout means not ready. Receipt consumes the request identity, so
    /// it cannot be reused or manufactured from caller-provided response bytes.
    pub fn receive(&mut self) -> Result<Option<WriteComplete>, String> {
        if self.id == 0 {
            return Err("native request already completed".into());
        }
        let mut bytes = [0; 64];
        match self.socket.recv(&mut bytes) {
            Ok(n)
                if n == crate::ACK_BYTES
                    && &bytes[..8] == crate::ACK_MAGIC
                    && bytes[8..16] == u64::from(self.frame).to_le_bytes() =>
            {
                let id = std::mem::replace(&mut self.id, 0);
                let value =
                    |offset| u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                Ok(Some(WriteComplete {
                    id,
                    codec_call_nanoseconds: value(16),
                    copy_ready_nanoseconds: value(24),
                }))
            }
            Ok(_) => Err("native helper readiness/frame mismatch".into()),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(format!("native helper socket: {e}")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsFd;
    #[test]
    fn receipt_requires_exact_socket_response_and_is_one_shot() {
        let file = std::fs::File::open("/dev/null").unwrap();
        let valid = crate::acknowledgment(7, 123, u64::MAX).to_vec();
        let mut wrong_version = valid.clone();
        wrong_version[..8].copy_from_slice(b"FNVACK01");
        for response in [
            valid.clone(),
            crate::acknowledgment(8, 123, 456).to_vec(),
            wrong_version,
            7u64.to_le_bytes().to_vec(), // obsolete v1 acknowledgment
            valid[..24].to_vec(),        // truncated timing
            [valid.as_slice(), &[0]].concat(), // trailing bytes
            vec![0; 64],
        ] {
            let (parent, child) = UnixDatagram::pair().unwrap();
            let ticket = WriteTicket::new(file.as_fd(), [0; 16], 4096, 3072);
            let expected = ticket.id;
            let mut request = ticket.send(&parent, 7).unwrap();
            let mut bytes = [0; 24];
            let (_, fd) = crate::receive_fd(&child, &mut bytes).unwrap();
            drop(fd);
            child.send(&response).unwrap();
            if response == valid {
                let complete = request.receive().unwrap().unwrap();
                assert_eq!(complete.id, expected);
                assert_eq!(complete.codec_call_nanoseconds(), 123);
                assert_eq!(complete.copy_ready_nanoseconds(), u64::MAX);
                assert!(request.receive().unwrap_err().contains("already completed"));
            } else {
                assert!(request.receive().unwrap_err().contains("mismatch"));
            }
        }
    }
}
