//! One image description and one CLOEXEC memory FD; never CPU pixels.
//! Callers set socket deadlines and supervise the producer lifecycle.
use crate::Description;
use fold_foundation::Failure;
use rustix::net::{
    self, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags,
};
use std::{
    io::{IoSlice, IoSliceMut, Read, Write},
    mem::MaybeUninit,
    os::{
        fd::{BorrowedFd, OwnedFd},
        unix::net::UnixStream,
    },
};
const MAX_DESCRIPTION: usize = 4096;
pub fn send(
    mut socket: &UnixStream,
    description: &Description,
    fd: BorrowedFd<'_>,
) -> Result<(), Failure> {
    let body = serde_json::to_vec(description).map_err(|e| e.to_string())?;
    if body.len() > MAX_DESCRIPTION {
        return Err("Shared image description is too large".into());
    }
    let header = (body.len() as u32).to_le_bytes();
    let fds = [fd];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    if !control.push(SendAncillaryMessage::ScmRights(&fds)) {
        return Err("Missing FD control storage".into());
    }
    let written = net::sendmsg(
        socket,
        &[IoSlice::new(&header)],
        &mut control,
        SendFlags::NOSIGNAL,
    )
    .map_err(|e| e.to_string())?;
    if written == 0 {
        return Err("Shared image socket closed".into());
    }
    socket
        .write_all(&header[written..])
        .and_then(|_| socket.write_all(&body))
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn receive(mut socket: &UnixStream) -> Result<(Description, OwnedFd), Failure> {
    let mut header = [0u8; 4];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(2))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let received = net::recvmsg(
        socket,
        &mut [IoSliceMut::new(&mut header)],
        &mut control,
        RecvFlags::CMSG_CLOEXEC,
    )
    .map_err(|e| e.to_string())?;
    let mut fds = Vec::new();
    for message in control.drain() {
        if let RecvAncillaryMessage::ScmRights(rights) = message {
            fds.extend(rights);
        }
    }
    if received.bytes == 0
        || received
            .flags
            .intersects(ReturnFlags::CTRUNC | ReturnFlags::TRUNC)
        || fds.len() != 1
    {
        return Err("Invalid shared image FD message".into());
    }
    socket
        .read_exact(&mut header[received.bytes..])
        .map_err(|e| e.to_string())?;
    let length = u32::from_le_bytes(header) as usize;
    if length == 0 || length > MAX_DESCRIPTION {
        return Err("Invalid shared image description length".into());
    }
    let mut body = vec![0u8; length];
    socket.read_exact(&mut body).map_err(|e| e.to_string())?;
    let description = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok((description, fds.pop().expect("one FD")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::{AsFd, AsRawFd};
    #[test]
    fn transfers_owned_cloexec_fd_and_rejects_extra_rights() {
        let (sender, receiver) = UnixStream::pair().unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let file = tempfile::tempfile().unwrap();
        let description = Description {
            dimensions: [32, 16],
            device: [1; 16],
            driver: [2; 16],
            allocation_bytes: 4096,
            memory_type: 1,
        };
        send(&sender, &description, file.as_fd()).unwrap();
        let (received, fd) = receive(&receiver).unwrap();
        assert_eq!(received.dimensions, description.dimensions);
        assert_ne!(fd.as_raw_fd(), file.as_raw_fd());
        assert!(
            rustix::io::fcntl_getfd(&fd)
                .unwrap()
                .contains(rustix::io::FdFlags::CLOEXEC)
        );
        drop(fd);
        let fds = [file.as_fd(), file.as_fd()];
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(2))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        assert!(control.push(SendAncillaryMessage::ScmRights(&fds)));
        net::sendmsg(
            &sender,
            &[IoSlice::new(&[0; 4])],
            &mut control,
            SendFlags::NOSIGNAL,
        )
        .unwrap();
        assert!(
            receive(&receiver)
                .err()
                .unwrap()
                .message
                .contains("Invalid shared image FD")
        );
    }
}
