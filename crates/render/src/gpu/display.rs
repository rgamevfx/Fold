//! Display-ready opaque RGBA8 leases. Only the host registers their texture with
//! the UI renderer; panels receive the resulting registry ID, never a device.
use super::{
    GpuFrame, Host, Renderer, Statistics,
    host::Image,
    validation::{Completion, Status},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct Display {
    pub(super) image: Arc<Image>,
    pub(super) host: Host,
    pub(super) parent: Completion,
    pub(super) ready: Completion,
    pub(super) compression: Option<Completion>,
    pub(super) dimensions: [u32; 2],
    identity: String,
    timing: Option<crate::frame::Timing>,
    pub statistics: Statistics,
}
impl std::fmt::Debug for Display {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuDisplay")
            .field("dimensions", &self.dimensions())
            .field("processor", &self.identity)
            .finish()
    }
}
impl Display {
    pub fn gpu_nanoseconds(&self) -> Result<Option<f64>, String> {
        let compression = match &self.compression {
            Some(completion) => completion.gpu_nanoseconds()?,
            None => Some(0.),
        };
        Ok(self
            .parent
            .gpu_nanoseconds()?
            .zip(self.ready.gpu_nanoseconds()?)
            .zip(compression)
            .map(|((scene, output), cache)| scene + output + cache))
    }
    pub fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }
    pub fn storage_bytes(&self) -> u64 {
        self.image.bytes
    }
    pub fn uv_max(&self) -> [f32; 2] {
        [
            self.dimensions[0] as f32 / self.image.dimensions[0] as f32,
            self.dimensions[1] as f32 / self.image.dimensions[1] as f32,
        ]
    }
    pub fn is_compressed(&self) -> bool {
        self.image.texture.format() == wgpu::TextureFormat::Bc7RgbaUnorm
    }
    pub fn is_ready(&self) -> Result<bool, String> {
        self.host.check()?;
        Ok(self.parent.ready()?
            && self.ready.ready()?
            && self
                .compression
                .as_ref()
                .map(Completion::ready)
                .transpose()?
                .unwrap_or(true))
    }
    pub fn texture(&self) -> &wgpu::Texture {
        &self.image.texture
    }
    /// Globally unique service identity. Raw wgpu IDs may collide between
    /// separate Instances, even when Device equality reports a match.
    pub fn owner(&self) -> u64 {
        self.host.id()
    }
    pub fn transform_identity(&self) -> &str {
        &self.identity
    }
    /// Explicit encoder/test boundary. Never used by normal viewer publication.
    pub fn readback(&mut self, cancel: &fold_media::Cancel) -> Result<crate::DisplayFrame, String> {
        if self.is_compressed() {
            return Err("BC7 presentation entries are not delivery/readback sources".into());
        }
        let begin = std::time::Instant::now();
        while !self.is_ready()? {
            self.host.poll()?;
            cancel.check()?;
            if begin.elapsed().as_secs() > 30 {
                return Err("GPU display completion timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        cancel.check()?;
        let [width, height] = self.dimensions();
        let stride = (width * 4).div_ceil(256) * 256;
        let size = u64::from(stride) * u64::from(height);
        let reservation = self.host.reserve(size)?;
        let buffer = self.host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Explicit output readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.image.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            self.image.texture.size(),
        );
        self.host
            .submit(encoder, vec![self.image.clone()], vec![reservation.clone()]);
        self.statistics.readback_bytes += size;
        let (send, receive) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |v| {
            let _ = send.send(v);
        });
        loop {
            self.host.poll()?;
            cancel.check()?;
            match receive.try_recv() {
                Ok(result) => {
                    result.map_err(|e| e.to_string())?;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("GPU output readback disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if begin.elapsed().as_secs() > 30 {
                return Err("GPU output readback timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mapped = buffer.slice(..).get_mapped_range();
        let storage = fold_media::budget::reserve_output(u64::from(width) * u64::from(height) * 4)?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(width as usize * height as usize * 4)
            .map_err(|_| "output CPU allocation failed")?;
        for row in mapped.chunks_exact(stride as usize) {
            rgba.extend_from_slice(&row[..width as usize * 4]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(crate::DisplayFrame {
            _storage: storage,
            width,
            height,
            rgba,
            transform_identity: self.identity.clone(),
            timing: self.timing,
        })
    }
}
impl Renderer {
    /// Data inspection bypasses OCIO/beauty display processing and retains only
    /// its opaque grayscale display image in the normal playback cache.
    pub fn output_data(
        &mut self,
        frame: &GpuFrame,
        range: crate::view::Range,
        cancel: &fold_media::Cancel,
    ) -> Result<Display, String> {
        use wgpu::util::DeviceExt;
        cancel.check()?;
        self.host.check()?;
        if frame.host.id() != self.host.id() {
            return Err("GPU scene belongs to a different host".into());
        }
        let output = self
            .host
            .image_format(frame.dimensions(), wgpu::TextureFormat::Rgba8Unorm)?;
        let status = Status::new(&self.host)?;
        let lease = self.host.reserve(16)?;
        let [black, white] = range.values();
        let uniform = self
            .host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Data display range"),
                contents: bytemuck::cast_slice(&[black, white, 0f32, 0f32]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self
            .host
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.data_display.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&frame.image.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&output.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: status.gpu.as_entire_binding(),
                    },
                ],
            });
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        status.start(&mut encoder);
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.data_display);
            pass.set_bind_group(0, &group, &[]);
            let [width, height] = frame.dimensions();
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        status.encode(&mut encoder);
        self.host.submit(
            encoder,
            vec![frame.image.clone(), output.clone()],
            vec![lease],
        );
        let mut statistics = frame.statistics;
        statistics.compute_passes += 1;
        statistics.status_readback_bytes += status.readback_bytes();
        Ok(Display {
            dimensions: frame.dimensions(),
            image: output,
            host: self.host.clone(),
            parent: frame.ready.clone(),
            ready: status.submitted(),
            compression: None,
            identity: format!("fold.data-display.v1:{black:?}:{white:?}"),
            timing: frame.timing,
            statistics,
        })
    }

    /// Explicit output transform and black matte. This never uses a viewer cache,
    /// and the same entry point serves preview and pinned delivery settings.
    pub fn output(
        &mut self,
        frame: &GpuFrame,
        processor: Option<&fold_color::Processor>,
        cancel: &fold_media::Cancel,
    ) -> Result<Display, String> {
        let output_begin = std::time::Instant::now();
        cancel.check()?;
        self.host.check()?;
        if frame.host.id() != self.host.id() {
            return Err("GPU scene belongs to a different host".into());
        }
        match processor {
            Some(p) => {
                let source = match frame.working_space {
                    crate::WorkingSpace::AcesCg => fold_color::WORKING_SPACE,
                    crate::WorkingSpace::LegacyLinearSrgb => fold_color::LINEAR_SRGB,
                };
                if p.source() != source
                    || frame
                        .config_identity
                        .as_deref()
                        .is_some_and(|id| id != p.config_identity())
                {
                    return Err("GPU output processor does not match scene identity".into());
                }
            }
            None if frame.working_space != crate::WorkingSpace::LegacyLinearSrgb => {
                return Err("ACEScg requires an explicit output processor".into());
            }
            None => {}
        }
        let (pipeline, bytes) = self.color_pipeline(processor, true)?;
        let output = self
            .host
            .image_format(frame.dimensions(), wgpu::TextureFormat::Rgba8Unorm)?;
        let status = Status::new(&self.host)?;
        let uniform = self.host.reserve(16)?;
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        status.start(&mut encoder);
        let intermediate = pipeline.encode(
            &self.host,
            &mut encoder,
            &frame.image,
            &output,
            true,
            &status,
        )?;
        status.encode(&mut encoder);
        let submit_begin = std::time::Instant::now();
        self.host.submit(
            encoder,
            vec![frame.image.clone(), output.clone(), intermediate],
            vec![uniform, pipeline.reservation.clone()],
        );
        let submit_nanoseconds = submit_begin.elapsed().as_nanos();
        let status_bytes = status.readback_bytes();
        let ready = status.submitted();
        let mut statistics = frame.statistics;
        statistics.output_cpu_nanoseconds = output_begin.elapsed().as_nanos();
        statistics.queue_submit_nanoseconds += submit_nanoseconds;
        statistics.lut_upload_bytes += bytes;
        statistics.compute_passes += 2;
        statistics.status_readback_bytes += status_bytes;
        cancel.check()?;
        Ok(Display {
            dimensions: frame.dimensions(),
            image: output,
            host: self.host.clone(),
            parent: frame.ready.clone(),
            ready,
            compression: None,
            identity: processor
                .map_or("fold.legacy-linear-srgb-to-srgb.v1", |p| p.identity())
                .into(),
            timing: frame.timing,
            statistics,
        })
    }
}
