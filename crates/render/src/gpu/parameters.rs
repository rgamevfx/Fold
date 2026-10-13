//! One immutable uniform upload per encoded graph. Record while mapped; unmap
//! before submission. Each binding has its own aligned range. Command buffers
//! retain storage and the submission retains the accounting lease.
use super::{Host, host::Reservation};
use std::sync::Arc;

pub(super) struct Arena {
    buffer: wgpu::Buffer,
    cursor: u64,
    alignment: u64,
}
impl Arena {
    pub fn stride(host: &Host, bytes: u64) -> u64 {
        let alignment =
            u64::from(host.device().limits().min_uniform_buffer_offset_alignment).max(8);
        bytes.div_ceil(alignment) * alignment
    }
    pub fn new(
        host: &Host,
        bytes: u64,
    ) -> Result<(Self, Arc<Reservation>), fold_foundation::Failure> {
        let lease = host.reserve(bytes)?;
        let buffer = host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fold graph parameters"),
            size: bytes,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        Ok((
            Self {
                buffer,
                cursor: 0,
                alignment: Self::stride(host, 1),
            },
            lease,
        ))
    }
    pub fn write(&mut self, bytes: &[u8]) -> wgpu::BindingResource<'_> {
        let offset = self.cursor;
        let length = bytes.len() as u64;
        self.cursor += length.div_ceil(self.alignment) * self.alignment;
        assert!(
            self.cursor <= self.buffer.size(),
            "parameter plan exceeds arena"
        );
        self.buffer
            .slice(offset..offset + length)
            .get_mapped_range_mut()
            .copy_from_slice(bytes);
        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &self.buffer,
            offset,
            size: std::num::NonZeroU64::new(length),
        })
    }
    pub fn finish(self) {
        self.buffer.unmap();
    }
}
