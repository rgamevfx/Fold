//! Worker-owned BC7 encoding of already display-transformed images. This service
//! never evaluates scenes, applies color, or reads image pixels back to the CPU.
use super::{Display, Host, host::Reservation, validation::Status};
use block_compression::{BC7Settings, CompressionVariant, GpuBlockCompressor};
use std::sync::Arc;
#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;

pub struct PresentationCompressor {
    host: Host,
    compressor: GpuBlockCompressor,
    pad: wgpu::ComputePipeline,
    opaque: wgpu::ComputePipeline,
    quality: wgpu::ComputePipeline,
    _parameters: Arc<Reservation>,
}
impl PresentationCompressor {
    /// Pipeline creation is expensive: construct on a worker, not the UI thread.
    pub fn new(host: Host) -> Result<Self, String> {
        host.check()?;
        if !host
            .device()
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        {
            return Err("BC7 texture sampling is unavailable on this device".into());
        }
        // The pinned compressor allocates sixteen aligned uniform/settings slots.
        // We submit one task at a time, so its buffers never grow.
        let limits = host.device().limits();
        let parameters = host.reserve(
            16 * (u64::from(limits.min_uniform_buffer_offset_alignment)
                + 128u64.div_ceil(u64::from(limits.min_storage_buffer_offset_alignment))
                    * u64::from(limits.min_storage_buffer_offset_alignment)),
        )?;
        let compressor = GpuBlockCompressor::new(host.device().clone(), host.queue().clone());
        let shader = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("cache_pad.wgsl"));
        let pad = host
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Presentation block-edge padding"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let shader = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("cache_opaque.wgsl"));
        let opaque = host
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("BC7 mode-6 opaque endpoints"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let shader = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("cache_quality.wgsl"));
        let quality = host
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("BC7 display quality gate"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        host.check()?;
        Ok(Self {
            host,
            compressor,
            pad,
            opaque,
            quality,
            _parameters: parameters,
        })
    }
    /// Input must already be ready. The caller decides when background work may
    /// run; returned storage cannot be published until `is_ready()` succeeds.
    pub fn compress(&mut self, source: &Display) -> Result<Display, String> {
        if source.owner() != self.host.id() || source.is_compressed() || !source.is_ready()? {
            return Err("BC7 requires a ready RGBA8 display from the same host".into());
        }
        let dimensions = source.dimensions().map(|v| v.div_ceil(4) * 4);
        let output = self
            .host
            .image_format(dimensions, wgpu::TextureFormat::Bc7RgbaUnorm)?;
        let scratch = self.host.reserve(output.bytes)?;
        let buffer = self.host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("BC7 blocks"),
            size: output.bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        let mut status = Status::new(&self.host)?;
        status.failure = "BC7 quality threshold exceeded; retain RGBA8";
        status.start(&mut encoder);
        let input = if dimensions == source.dimensions() {
            source.image.clone()
        } else {
            let padded = self
                .host
                .image_format(dimensions, wgpu::TextureFormat::Rgba8Unorm)?;
            let bindings = self
                .host
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.pad.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&source.image.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&padded.view),
                        },
                    ],
                });
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.pad);
                pass.set_bind_group(0, &bindings, &[]);
                pass.dispatch_workgroups(dimensions[0].div_ceil(8), dimensions[1].div_ceil(8), 1);
            }
            padded
        };
        self.compressor.add_compression_task(
            CompressionVariant::BC7(BC7Settings::opaque_ultra_fast()),
            &input.view,
            dimensions[0],
            dimensions[1],
            &buffer,
            None,
            None,
        );
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            self.compressor.compress(&mut pass);
        }
        let bindings = self
            .host
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.opaque.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.opaque);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups((dimensions[0] / 4 * (dimensions[1] / 4)).div_ceil(64), 1, 1);
        }
        // Single block-row copies avoid a 256-byte row-pitch padding buffer.
        let row_bytes = dimensions[0] / 4 * 16;
        for row in 0..dimensions[1] / 4 {
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: u64::from(row) * u64::from(row_bytes),
                        bytes_per_row: None,
                        rows_per_image: None,
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: row * 4,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: dimensions[0],
                    height: 4,
                    depth_or_array_layers: 1,
                },
            );
        }
        let bindings = self
            .host
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.quality.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source.image.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&output.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: status.gpu.as_entire_binding(),
                    },
                ],
            });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.quality);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(
                source.dimensions()[0].div_ceil(8),
                source.dimensions()[1].div_ceil(8),
                1,
            );
        }
        status.encode(&mut encoder);
        let status_bytes = status.readback_bytes();
        self.host.submit(
            encoder,
            vec![source.image.clone(), input, output.clone()],
            vec![scratch, self._parameters.clone()],
        );
        let mut result = source.clone();
        result.image = output;
        result.compression = Some(status.submitted());
        result.statistics.compute_passes += 3 + u32::from(dimensions != source.dimensions());
        result.statistics.status_readback_bytes += status_bytes;
        Ok(result)
    }
}
