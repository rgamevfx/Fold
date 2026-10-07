//! Worker-owned BC7 encoder; only baked blocks are returned for disk storage.
use super::{Host, host::Reservation};
use crate::baked::{Format, Texture};
use fold_media::Cancel;
use std::sync::Arc;

pub struct BakeEncoder {
    host: Host,
    compressor: block_compression::GpuBlockCompressor,
    _parameters: Arc<Reservation>,
}
impl BakeEncoder {
    pub fn new(host: Host) -> Result<Self, String> {
        let limits = host.device().limits();
        let parameters = host.reserve(
            16 * (u64::from(limits.min_uniform_buffer_offset_alignment)
                + 128u64.div_ceil(u64::from(limits.min_storage_buffer_offset_alignment))
                    * u64::from(limits.min_storage_buffer_offset_alignment)),
        )?;
        let compressor =
            block_compression::GpuBlockCompressor::new(host.device().clone(), host.queue().clone());
        host.check()?;
        Ok(Self {
            host,
            compressor,
            _parameters: parameters,
        })
    }
    pub fn encode(
        &mut self,
        frame: &crate::Frame,
        format: Format,
        cancel: &Cancel,
    ) -> Result<Texture, String> {
        Texture::encode_with(frame, format, cancel, |rgba, w, h, cancel| {
            self.blocks(rgba, w, h, cancel)
        })
    }
    fn blocks(&mut self, rgba: &[u8], w: u32, h: u32, cancel: &Cancel) -> Result<Vec<u8>, String> {
        cancel.check()?;
        let size = u64::from(w) * u64::from(h);
        // Input, queue upload staging, compressed blocks and readback.
        let reservation = self.host.reserve(size * 10)?;
        let input = self.host.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Bake encoder input"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.host.queue().write_texture(
            input.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            input.size(),
        );
        let blocks = self.host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Baked BC7 blocks"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bake block readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let opaque = rgba.chunks_exact(4).all(|p| p[3] == 255);
        self.compressor.add_compression_task(
            block_compression::CompressionVariant::BC7(if opaque {
                block_compression::BC7Settings::opaque_ultra_fast()
            } else {
                block_compression::BC7Settings::alpha_fast()
            }),
            &input.create_view(&Default::default()),
            w,
            h,
            &blocks,
            None,
            None,
        );
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            self.compressor.compress(&mut pass);
        }
        encoder.copy_buffer_to_buffer(&blocks, 0, &readback, 0, size);
        self.host.submit(encoder, vec![], vec![reservation]);
        let (send, receive) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = send.send(r);
        });
        let start = std::time::Instant::now();
        loop {
            cancel.check()?;
            self.host.poll()?;
            match receive.try_recv() {
                Ok(result) => {
                    result.map_err(|e| e.to_string())?;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("Bake readback disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if start.elapsed().as_secs() > 30 {
                return Err("Bake compression timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mut bytes = readback.slice(..).get_mapped_range().to_vec();
        readback.unmap();
        if opaque {
            // Same correction as cache_opaque.wgsl: exact opaque coverage.
            // Shared p-bits change RGB endpoints by at most one SDR code.
            for block in bytes.chunks_exact_mut(16) {
                if block[0] & 127 == 64 {
                    let y = u32::from_le_bytes(block[4..8].try_into().unwrap()) | 0xfffe0000;
                    let z = u32::from_le_bytes(block[8..12].try_into().unwrap()) | 1;
                    block[4..8].copy_from_slice(&y.to_le_bytes());
                    block[8..12].copy_from_slice(&z.to_le_bytes());
                }
            }
        }
        Ok(bytes)
    }
}
