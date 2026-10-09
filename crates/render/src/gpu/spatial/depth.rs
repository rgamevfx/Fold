//! Opaque-surface reverse-Z readback from an immutable frame-owned attachment.
use super::{Binding, Frame, Projection};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Row-major reverse-Z samples in [0, 1], with zero denoting clear/background.
/// Transparent materials test against opaque depth but do not write this plane.
/// This is geometric visibility depth, not object picking or composited-alpha depth.
pub struct Depth {
    binding: Binding,
    near: f32,
    far: f32,
    projection: Projection,
    values: Vec<f32>,
    _storage: Arc<fold_media::budget::Lease>,
}
impl Depth {
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn values(&self) -> &[f32] {
        &self.values
    }
    /// Positive camera-space distance along the view axis, in scene units.
    /// Returns None for background or coordinates outside the frame.
    pub fn view_distance(&self, x: u32, y: u32) -> Option<f32> {
        let [width, height] = self.binding.dimensions;
        if x >= width || y >= height {
            return None;
        }
        let z = self.values[(y * width + x) as usize];
        if z == 0. {
            return None;
        }
        let distance = match self.projection {
            Projection::Perspective { .. } => self.near / z,
            Projection::Orthographic { .. } => self.far - z * (self.far - self.near),
        };
        distance.is_finite().then_some(distance)
    }
}
impl Frame {
    /// Worker-only, cancellation/timeout bounded. Requires the same binding as
    /// color publication, and waits for the color validation result as well.
    pub fn read_depth(
        &mut self,
        expected: &Binding,
        cancel: &fold_media::Cancel,
    ) -> Result<Depth, fold_foundation::Failure> {
        self.image(expected)?;
        cancel.check()?;
        let host = &self.image.host;
        let begin = Instant::now();
        while !self.image.is_ready()? {
            host.poll()?;
            cancel.check()?;
            if begin.elapsed() > Duration::from_secs(30) {
                return Err("Spatial depth completion timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let [width, height] = self.binding.dimensions;
        let stride = (width * 4).div_ceil(256) * 256;
        let size = u64::from(stride) * u64::from(height);
        let reservation = host.reserve(size)?;
        let storage =
            fold_media::budget::reserve_working(u64::from(width) * u64::from(height) * 4)?;
        let buffer = host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fold spatial depth readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = host.device().create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::DepthOnly,
                ..self.depth.texture.as_image_copy()
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            self.depth.texture.size(),
        );
        host.submit(encoder, vec![self.depth.clone()], vec![reservation.clone()]);
        self.image.statistics.readback_bytes += size;
        let (send, receive) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        let begin = Instant::now();
        loop {
            host.poll()?;
            cancel.check()?;
            match receive.try_recv() {
                Ok(result) => {
                    result.map_err(|error| error.to_string())?;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("Spatial depth readback disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if begin.elapsed() > Duration::from_secs(30) {
                return Err("Spatial depth readback timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let mapped = buffer.slice(..).get_mapped_range();
        let mut values = Vec::new();
        values
            .try_reserve_exact((width * height) as usize)
            .map_err(|_| "Depth readback CPU allocation failed")?;
        for row in mapped.chunks_exact(stride as usize) {
            for bytes in row[..width as usize * 4].chunks_exact(4) {
                let z = f32::from_le_bytes(bytes.try_into().unwrap());
                if !z.is_finite() || !(0.0..=1.).contains(&z) {
                    return Err("Invalid spatial reverse-Z depth".into());
                }
                values.push(z);
            }
        }
        drop(mapped);
        buffer.unmap();
        cancel.check()?;
        Ok(Depth {
            binding: self.binding.clone(),
            near: self.near,
            far: self.far,
            projection: self.projection,
            values,
            _storage: storage,
        })
    }
}
