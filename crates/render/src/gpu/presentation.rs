//! Worker-only, bounded presentation retention. No color conversion or evaluation.
use super::{Display, Host, host::Reservation};
use crate::presentation::{Description, Format, Pixels};
use std::sync::Arc;

/// One reusable readback buffer. The caller serializes transfers on its worker.
#[derive(Default)]
pub struct PresentationTransfer {
    owner: Option<u64>,
    readback_bytes: u64,
    staging: Option<(wgpu::Buffer, Arc<Reservation>)>,
}
impl PresentationTransfer {
    pub fn readback_bytes(&self) -> u64 {
        self.readback_bytes
    }
    pub fn retain(
        &mut self,
        source: &Display,
        limit: usize,
        cancel: &fold_media::Cancel,
    ) -> Result<Pixels, fold_foundation::Failure> {
        cancel.check()?;
        if !source.is_ready()? {
            return Err("Presentation retention requires a ready frame".into());
        }
        let description = Description {
            dimensions: source.dimensions(),
            format: if source.is_compressed() {
                Format::Bc7
            } else {
                Format::Rgba8
            },
        };
        let (dimensions, row, rows) = description.layout()?;
        let stride = row.div_ceil(256) * 256;
        let size = u64::from(stride) * u64::from(rows);
        if size > limit as u64 || description.bytes()? > limit {
            return Err("Presentation exceeds transfer allowance".into());
        }
        let host = &source.host;
        if self.owner != Some(host.id()) {
            self.staging = None;
            self.owner = Some(host.id());
        }
        if self
            .staging
            .as_ref()
            .is_none_or(|(buffer, _)| buffer.size() < size)
        {
            self.staging = None;
            let reservation = host.reserve(size)?;
            let buffer = host.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("Playback cache staging"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.staging = Some((buffer, reservation));
        }
        let (buffer, reservation) = self.staging.as_ref().unwrap();
        let mut encoder = host.device().create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            source.texture().as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: dimensions[0],
                height: dimensions[1],
                depth_or_array_layers: 1,
            },
        );
        self.readback_bytes += size;
        host.submit(
            encoder,
            vec![source.image.clone()],
            vec![reservation.clone()],
        );
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        buffer
            .slice(..size)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        let start = std::time::Instant::now();
        let mapped = loop {
            if let Err(error) = cancel.check().and_then(|_| host.poll()) {
                break Err(error);
            }
            match receive.try_recv() {
                Ok(result) => {
                    break result.map_err(|e| fold_foundation::Failure::from(e.to_string()));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    break Err("Cache transfer disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if start.elapsed().as_secs() >= 10 {
                break Err("Cache transfer timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        if let Err(error) = mapped {
            buffer.unmap();
            self.staging = None;
            return Err(error);
        }
        let view = buffer.slice(..size).get_mapped_range();
        let mut data = Vec::with_capacity(description.bytes()?);
        for bytes in view.chunks_exact(stride as usize) {
            data.extend_from_slice(&bytes[..row as usize]);
        }
        drop(view);
        buffer.unmap();
        Ok(Pixels { description, data })
    }
}
impl Host {
    /// Worker-only upload of existing display bytes; preserves compression and
    /// registers completion before publication. No re-render/display transform.
    pub fn restore_presentation(
        &self,
        pixels: &Pixels,
    ) -> Result<Display, fold_foundation::Failure> {
        pixels.validate()?;
        let (dimensions, row, _) = pixels.description.layout()?;
        let format = match pixels.description.format {
            Format::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            Format::Bc7 => wgpu::TextureFormat::Bc7RgbaUnorm,
        };
        let image = self.image_format(dimensions, format)?;
        self.queue().write_texture(
            image.texture.as_image_copy(),
            &pixels.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
            image.texture.size(),
        );
        self.queue().submit([]);
        let ready = super::validation::Completion::submitted(self.queue(), image.clone());
        Ok(Display::restored(
            self.clone(),
            image,
            pixels.description.dimensions,
            ready,
            pixels.data.len() as u64,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready(host: &Host, frame: &Display) {
        let start = std::time::Instant::now();
        while !frame.is_ready().unwrap() {
            host.poll().unwrap();
            assert!(start.elapsed().as_secs() < 10);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    #[ignore = "requires native Vulkan BC7 support"]
    fn native_presentation_roundtrip_reuses_staging_and_preserves_bc7() {
        let (host, adapter) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
        eprintln!("Cache transfer adapter: {adapter:?}");
        let cancel = fold_media::Cancel::default();
        let mut transfer = PresentationTransfer::default();
        let mut compressor = super::super::PresentationCompressor::new(host.clone()).unwrap();
        for dimensions in [[64, 64], [479, 273], [1920, 1080]] {
            let pixels = Pixels {
                description: Description {
                    dimensions,
                    format: Format::Rgba8,
                },
                data: [80, 128, 160, 255].repeat((dimensions[0] * dimensions[1]) as usize),
            };
            let original = host.restore_presentation(&pixels).unwrap();
            ready(&host, &original);
            let raw = transfer
                .retain(&original, 64 * 1024 * 1024, &cancel)
                .unwrap();
            assert_eq!(raw.data, pixels.data);
            let compressed = compressor.compress(&original).unwrap();
            ready(&host, &compressed);
            let start = std::time::Instant::now();
            let stored = transfer
                .retain(&compressed, 64 * 1024 * 1024, &cancel)
                .unwrap();
            assert_eq!(stored.description.format, Format::Bc7);
            assert_eq!(stored.data.len() as u64, compressed.storage_bytes());
            let restored = host.restore_presentation(&stored).unwrap();
            ready(&host, &restored);
            let copied = transfer
                .retain(&restored, 64 * 1024 * 1024, &cancel)
                .unwrap();
            assert_eq!(copied.data, stored.data);
            assert_eq!(restored.dimensions(), dimensions);
            assert_eq!(restored.uv_max(), compressed.uv_max());
            let staging = transfer.staging.as_ref().unwrap().0.clone();
            transfer
                .retain(&restored, 64 * 1024 * 1024, &cancel)
                .unwrap();
            assert_eq!(transfer.staging.as_ref().unwrap().0, staging);
            eprintln!(
                "{dimensions:?}: RGBA8={} BC7={} transfer/restore/verify={:?}",
                pixels.data.len(),
                stored.data.len(),
                start.elapsed()
            );
            assert!(transfer.retain(&restored, 1, &cancel).is_err());
        }
        cancel.cancel();
        let pixels = Pixels {
            description: Description {
                dimensions: [1, 1],
                format: Format::Rgba8,
            },
            data: vec![0; 4],
        };
        let frame = host.restore_presentation(&pixels).unwrap();
        ready(&host, &frame);
        assert!(transfer.retain(&frame, 4096, &cancel).is_err());
        host.check().unwrap();
    }
}
