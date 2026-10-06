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
pub(super) struct Upload(Arc<Staging>);
impl Pool {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn write(
        &mut self,
        host: &Host,
        pixels: &[[f32; 4]],
        dimensions: [u32; 2],
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
    ) -> Result<(Upload, bool), String> {
        let stride = (dimensions[0] * 16).div_ceil(256) * 256;
        let bytes = u64::from(stride) * u64::from(dimensions[1]);
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
        Ok((Upload(entry), reused))
    }
}
impl Drop for Upload {
    fn drop(&mut self) {
        let entry = self.0.clone();
        self.0
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
