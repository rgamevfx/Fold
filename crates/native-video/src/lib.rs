//! Linux native-video transport. No codec libraries are linked into the application.
//! Pixel bytes cross only GPU-local copies; Unix messages carry descriptors/readiness.
#![cfg(target_os = "linux")]
mod ranges;
mod readiness;
pub use ranges::Ranges;
pub use readiness::{PendingWrite, WriteComplete, WriteTicket};
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "gpu")]
pub use gpu::{PendingBuffer, ReadyBuffer};
use std::os::{
    fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
    unix::net::UnixDatagram,
};

pub const PROTOCOL_NAME: &str = "fold-native-v2";
pub const READY_MESSAGE: &[u8] = b"READY2";
pub const ACK_BYTES: usize = 32;
pub(crate) const ACK_MAGIC: &[u8; 8] = b"FNVACK02";
/// Completed-request acknowledgment: version, frame, decode-call wall time,
/// copy-to-ready wall time. These inclusive host/API waits are NOT GPU timings.
pub fn acknowledgment(
    frame: u32,
    codec_call_nanoseconds: u64,
    copy_ready_nanoseconds: u64,
) -> [u8; ACK_BYTES] {
    let mut out = [0; ACK_BYTES];
    out[..8].copy_from_slice(ACK_MAGIC);
    out[8..16].copy_from_slice(&u64::from(frame).to_le_bytes());
    out[16..24].copy_from_slice(&codec_call_nanoseconds.to_le_bytes());
    out[24..].copy_from_slice(&copy_ready_nanoseconds.to_le_bytes());
    out
}
pub const REQUEST_BYTES: usize = 24;
pub fn request(frame: u32, allocation: u64, bytes: u64) -> [u8; REQUEST_BYTES] {
    let mut out = [0; REQUEST_BYTES];
    out[..8].copy_from_slice(&u64::from(frame).to_le_bytes());
    out[8..16].copy_from_slice(&allocation.to_le_bytes());
    out[16..].copy_from_slice(&bytes.to_le_bytes());
    out
}
/// Send exactly one owned allocation capability. sendmsg duplicates the descriptor;
/// the sender retains its own FD and allocation until the helper is reaped/ready.
pub fn send_fd(socket: &UnixDatagram, data: &[u8], fd: BorrowedFd<'_>) -> std::io::Result<()> {
    unsafe {
        let mut iov = libc::iovec {
            iov_base: data.as_ptr().cast_mut().cast(),
            iov_len: data.len(),
        };
        let mut control = [0usize; 8];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = libc::CMSG_SPACE(std::mem::size_of::<i32>() as u32) as usize;
        let c = libc::CMSG_FIRSTHDR(&msg);
        (*c).cmsg_level = libc::SOL_SOCKET;
        (*c).cmsg_type = libc::SCM_RIGHTS;
        (*c).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<i32>() as u32) as usize;
        std::ptr::write_unaligned(libc::CMSG_DATA(c).cast::<i32>(), fd.as_raw_fd());
        let result = libc::sendmsg(socket.as_raw_fd(), &msg, libc::MSG_NOSIGNAL);
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if result as usize != data.len() {
            return Err(std::io::Error::other("short native request"));
        }
        Ok(())
    }
}
/// Receive a bounded request and one CLOEXEC descriptor. Unexpected descriptors
/// are closed even on malformed/truncated messages.
pub fn receive_fd(socket: &UnixDatagram, data: &mut [u8]) -> std::io::Result<(usize, OwnedFd)> {
    unsafe {
        let mut iov = libc::iovec {
            iov_base: data.as_mut_ptr().cast(),
            iov_len: data.len(),
        };
        let mut control = [0usize; 8];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = std::mem::size_of_val(&control);
        let n = libc::recvmsg(socket.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC);
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut fds = Vec::new();
        let mut c = libc::CMSG_FIRSTHDR(&msg);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let count =
                    ((*c).cmsg_len - libc::CMSG_LEN(0) as usize) / std::mem::size_of::<i32>();
                for index in 0..count {
                    fds.push(OwnedFd::from_raw_fd(std::ptr::read_unaligned(
                        libc::CMSG_DATA(c).cast::<i32>().add(index),
                    )));
                }
            }
            c = libc::CMSG_NXTHDR(&msg, c);
        }
        if msg.msg_flags & (libc::MSG_CTRUNC | libc::MSG_TRUNC) != 0 || fds.len() != 1 {
            return Err(std::io::Error::other("invalid native descriptor message"));
        }
        Ok((n as usize, fds.pop().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsFd;
    #[test]
    fn descriptor_transport_duplicates_ownership() {
        let (a, b) = UnixDatagram::pair().unwrap();
        let file = std::fs::File::open("/dev/null").unwrap();
        send_fd(&a, &request(7, 4096, 3072), file.as_fd()).unwrap();
        drop(file);
        let mut bytes = [0; 24];
        let (n, fd) = receive_fd(&b, &mut bytes).unwrap();
        assert_eq!(n, 24);
        assert_eq!(bytes, request(7, 4096, 3072));
        assert!(std::fs::File::from(fd).metadata().is_ok());
    }
}
