//! Reusable mapped upload storage. Mapping after submission is the fence:
//! neither cancellation nor dropping a frame makes a submitted buffer reusable.
use super::{Host, host::Reservation};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Staging {
    buffer: wgpu::Buffer,
    _lease: Arc<Reservation>,
    mapped: AtomicBool,
}
#[derive(Default)]
pub(super) struct Pool {
    entries: Vec<Arc<Staging>>,
}
pub(super) struct Upload {
    staging: Arc<Staging>,
    publication: Option<Arc<AtomicBool>>,
}
impl Upload {
    pub fn publish_on_submit(&mut self, ready: Arc<AtomicBool>) {
        self.publication = Some(ready);
    }
    /// Call only after the encoder containing this upload has been submitted.
    /// Queue ordering then protects later cached reads, without a CPU fence.
    pub fn submitted(&self) {
        if let Some(ready) = &self.publication {
            ready.store(true, Ordering::Release);
        }
    }
}
impl Pool {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    fn acquire(
        &mut self,
        host: &Host,
        bytes: u64,
    ) -> Result<(Arc<Staging>, bool), fold_foundation::Failure> {
        let available = self
            .entries
            .iter()
            .find(|e| e.buffer.size() == bytes && e.mapped.load(Ordering::Acquire))
            .cloned();
        let reused = available.is_some();
        let entry = if let Some(entry) = available {
            entry
        } else {
            let limit = (host.memory().budget / 8).min(128 * 1024 * 1024);
            // Idle storage is expendable. Pending callbacks retain their own Arc.
            let mut retained: u64 = self.entries.iter().map(|e| e.buffer.size()).sum();
            self.entries.retain(|e| {
                if retained + bytes > limit && e.mapped.load(Ordering::Acquire) {
                    retained -= e.buffer.size();
                    false
                } else {
                    true
                }
            });
            let lease = host.reserve(bytes).or_else(|_| {
                self.entries.retain(|e| !e.mapped.load(Ordering::Acquire));
                host.reserve(bytes)
            })?;
            let entry = Arc::new(Staging {
                buffer: host.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Fold reusable image upload"),
                    size: bytes,
                    usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                }),
                _lease: lease,
                mapped: AtomicBool::new(true),
            });
            if retained + bytes <= limit {
                self.entries.push(entry.clone());
            }
            entry
        };
        entry.mapped.store(false, Ordering::Release);
        Ok((entry, reused))
    }
    pub fn write_buffer(
        &mut self,
        host: &Host,
        data: &[u8],
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Buffer,
    ) -> Result<(Upload, bool), fold_foundation::Failure> {
        let bytes = (data.len() as u64).div_ceil(4) * 4;
        let (entry, reused) = self.acquire(host, bytes)?;
        {
            let mut mapped = entry.buffer.slice(..).get_mapped_range_mut();
            mapped[..data.len()].copy_from_slice(data);
            mapped[data.len()..].fill(0);
        }
        entry.buffer.unmap();
        encoder.copy_buffer_to_buffer(&entry.buffer, 0, target, 0, bytes);
        Ok((
            Upload {
                staging: entry,
                publication: None,
            },
            reused,
        ))
    }
    pub fn write(
        &mut self,
        host: &Host,
        pixels: &[[f32; 4]],
        dimensions: [u32; 2],
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
    ) -> Result<(Upload, bool), fold_foundation::Failure> {
        let stride = (dimensions[0] * 16).div_ceil(256) * 256;
        let bytes = u64::from(stride) * u64::from(dimensions[1]);
        let (entry, reused) = self.acquire(host, bytes)?;
        {
            let mut mapped = entry.buffer.slice(..).get_mapped_range_mut();
            for (row, pixels) in mapped
                .chunks_exact_mut(stride as usize)
                .zip(pixels.chunks_exact(dimensions[0] as usize))
            {
                row[..dimensions[0] as usize * 16].copy_from_slice(bytemuck::cast_slice(pixels));
            }
        }
        entry.buffer.unmap();
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &entry.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(dimensions[1]),
                },
            },
            target.as_image_copy(),
            target.size(),
        );
        Ok((
            Upload {
                staging: entry,
                publication: None,
            },
            reused,
        ))
    }
}
impl Drop for Upload {
    fn drop(&mut self) {
        let entry = self.staging.clone();
        self.staging
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Write, move |result| {
                if result.is_ok() {
                    entry.mapped.store(true, Ordering::Release);
                }
                // A failed map never publishes reusable storage. Device errors are
                // reported through the shared host; the allocation lease stays live.
            });
    }
}
