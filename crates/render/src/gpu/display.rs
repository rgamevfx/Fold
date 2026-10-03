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
    image: Arc<Image>,
    host: Host,
    parent: Completion,
    ready: Completion,
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
        Ok(self
            .parent
            .gpu_nanoseconds()?
            .zip(self.ready.gpu_nanoseconds()?)
            .map(|(a, b)| a + b))
    }
    pub fn dimensions(&self) -> [u32; 2] {
        self.image.dimensions
    }
    pub fn is_ready(&self) -> Result<bool, String> {
        self.host.check()?;
        Ok(self.parent.ready()? && self.ready.ready()?)
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
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(width as usize * height as usize * 4)
            .map_err(|_| "output CPU allocation failed")?;
        for row in mapped.chunks_exact(stride as usize) {
            rgba.extend_from_slice(&row[..width as usize * 4]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(crate::DisplayFrame {
            width,
            height,
            rgba,
            transform_identity: self.identity.clone(),
            timing: self.timing,
        })
    }
}
impl Renderer {
    /// Explicit output transform and black matte. This never uses a viewer cache,
    /// and the same entry point serves preview and pinned delivery settings.
    pub fn output(
        &mut self,
        frame: &GpuFrame,
        processor: Option<&fold_color::Processor>,
        cancel: &fold_media::Cancel,
    ) -> Result<Display, String> {
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
        self.host.submit(
            encoder,
            vec![frame.image.clone(), output.clone(), intermediate],
            vec![uniform, pipeline.reservation.clone()],
        );
        let status_bytes = status.readback_bytes();
        let ready = status.submitted();
        let mut statistics = frame.statistics;
        statistics.lut_upload_bytes += bytes;
        statistics.compute_passes += 2;
        statistics.status_readback_bytes += status_bytes;
        cancel.check()?;
        Ok(Display {
            image: output,
            host: self.host.clone(),
            parent: frame.ready.clone(),
            ready,
            identity: processor
                .map_or("fold.legacy-linear-srgb-to-srgb.v1", |p| p.identity())
                .into(),
            timing: frame.timing,
            statistics,
        })
    }
}
