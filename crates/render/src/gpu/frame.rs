//! Immutable frame leases, readiness and explicit CPU readback.
use super::{Host, Image, Statistics, validation};
use crate::{Frame, WorkingSpace};
use std::sync::Arc;
/// Immutable scene image. Readiness is queue completion, not submission. A frame
/// cannot be used with a different host, even if that host uses the same adapter.
pub struct GpuFrame {
    pub(super) image: Arc<Image>,
    pub(super) host: Host,
    pub(super) ready: validation::Completion,
    pub(super) dependencies: Vec<validation::Completion>,
    pub(super) working_space: WorkingSpace,
    pub(super) config_identity: Option<String>,
    pub(super) timing: Option<crate::frame::Timing>,
    pub(super) lease_id: u64,
    pub statistics: Statistics,
}
impl GpuFrame {
    pub fn gpu_nanoseconds(&self) -> Result<Option<f64>, fold_foundation::Failure> {
        let mut total = self.ready.gpu_nanoseconds()?;
        for ready in &self.dependencies {
            total = total.zip(ready.gpu_nanoseconds()?).map(|(a, b)| a + b);
        }
        Ok(total)
    }
    pub fn with_timing(
        mut self,
        timestamp: fold_foundation::Time,
        duration: fold_foundation::Time,
    ) -> Result<Self, fold_foundation::Failure> {
        if duration <= fold_foundation::Time::ZERO {
            return Err("Frame duration must be positive".into());
        }
        self.timing = Some(crate::frame::Timing {
            timestamp,
            duration,
        });
        Ok(self)
    }
    pub fn descriptor(&self) -> Result<crate::frame::Descriptor, fold_foundation::Failure> {
        use crate::frame::*;
        Ok(Descriptor {
            dimensions: self.dimensions(),
            pixel_aspect: [1, 1],
            planes: vec![Plane {
                offset: 0,
                row_stride: u64::from(self.dimensions()[0]) * self.pixel_bytes(),
                channels: 4,
                component: if self.pixel_bytes() == 8 {
                    Component::Float16
                } else {
                    Component::Float32
                },
            }],
            alpha: Alpha::Premultiplied,
            color: ColorEncoding::Scene {
                working: self.working_space,
                config_identity: self.config_identity.clone(),
            },
            timing: self.timing,
            storage: Storage::GpuLease {
                device: self.host.id(),
                lease: self.lease_id,
            },
            readiness: if self.is_ready()? {
                Readiness::Ready
            } else {
                Readiness::Completion {
                    owner: self.host.id(),
                    token: self.lease_id,
                }
            },
        })
    }
    fn pixel_bytes(&self) -> u64 {
        match self.image.texture.format() {
            wgpu::TextureFormat::Rgba16Float => 8,
            wgpu::TextureFormat::Rgba32Float => 16,
            _ => unreachable!("scene frames own floating-point RGBA images"),
        }
    }
    pub fn dimensions(&self) -> [u32; 2] {
        self.image.dimensions
    }
    pub fn is_ready(&self) -> Result<bool, fold_foundation::Failure> {
        self.host.check()?;
        for ready in &self.dependencies {
            if !ready.ready()? {
                return Ok(false);
            }
        }
        self.ready.ready()
    }
    /// Explicit worker-only reference/export boundary. Never call from a UI
    /// callback. Readback is accounted, timeout bounded, and cancellation checked
    /// on both sides of GPU completion (submitted GPU work is not preempted).
    pub fn readback(
        &mut self,
        cancel: &fold_media::Cancel,
    ) -> Result<Frame, fold_foundation::Failure> {
        cancel.check()?;
        self.host.check()?;
        let begin = std::time::Instant::now();
        while !self.is_ready()? {
            self.host.poll()?;
            cancel.check()?;
            if begin.elapsed() > std::time::Duration::from_secs(30) {
                return Err("GPU evaluation completion timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let [width, height] = self.dimensions();
        let pixel_bytes = self.pixel_bytes() as usize;
        let stride = (width * pixel_bytes as u32).div_ceil(256) * 256;
        let size = u64::from(stride) * u64::from(height);
        self.host.check_buffer_size(size)?;
        let reservation = self.host.reserve(size)?;
        let device = self.host.device();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fold explicit scene readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
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
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        let start = std::time::Instant::now();
        loop {
            self.host.poll()?;
            cancel.check()?;
            match receive.try_recv() {
                Ok(result) => {
                    result.map_err(|e| e.to_string())?;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("GPU readback disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if start.elapsed() > std::time::Duration::from_secs(30) {
                return Err("GPU readback timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mapped = buffer.slice(..).get_mapped_range();
        let storage =
            fold_media::budget::reserve_working(u64::from(width) * u64::from(height) * 16)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact((width * height) as usize)
            .map_err(|_| "readback CPU allocation failed")?;
        for row in mapped.chunks_exact(stride as usize) {
            for bytes in row[..width as usize * pixel_bytes].chunks_exact(pixel_bytes) {
                let pixel = std::array::from_fn(|c| {
                    if pixel_bytes == 8 {
                        half::f16::from_le_bytes(bytes[c * 2..c * 2 + 2].try_into().unwrap())
                            .to_f32()
                    } else {
                        f32::from_le_bytes(bytes[c * 4..c * 4 + 4].try_into().unwrap())
                    }
                });
                fold_color::validate_pixel(pixel)?;
                pixels.push(pixel);
            }
        }
        drop(mapped);
        buffer.unmap();
        Ok(Frame {
            _storage: storage,
            width,
            height,
            pixels,
            working_space: self.working_space,
            config_identity: self.config_identity.clone(),
            timing: self.timing,
        })
    }
}
