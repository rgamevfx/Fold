//! Trusted compiled-worker FD ingress. The pinned native renderer completes and
//! releases its immutable image before transfer; its process is retained until
//! the receiver completes its copy. Rust scripts run as trusted native code.
use fold_foundation::Failure;
use fold_gpu_sharing::{Description, Imported};
use fold_render::gpu::Host;
use std::os::fd::OwnedFd;

pub(super) fn import(
    host: &Host,
    description: Description,
    fd: OwnedFd,
) -> Result<Imported, Failure> {
    let limit = description.allocation_bytes;
    unsafe { Imported::new(host.device(), description, fd, limit) }
}
