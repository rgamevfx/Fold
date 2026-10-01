//! Display-transformed RGBA8 GPU hot cache. No image-file compression, no linear
//! frame retention, no export reuse. Submitted textures are protected until done.
use crate::{cache::Cache, shell::Preview};
use dear_imgui_wgpu::{ExternalTextureId, WgpuRenderer, wgpu};
use fold_platform::{
    DisplayFrame,
    desktop::{DesktopClient, PreviewKey},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Texture {
    registration: ExternalTextureId,
    _texture: wgpu::Texture,
    dimensions: [u32; 2],
    in_flight: Arc<AtomicUsize>,
}
pub(crate) struct PreviewHost {
    pub state: Preview,
    wanted: Option<PreviewKey>,
    displayed: Option<PreviewKey>,
    cache: Cache<PreviewKey, Texture>,
    pending: Option<DisplayFrame>,
}
impl PreviewHost {
    pub fn new() -> Self {
        Self {
            state: Preview::Pending,
            wanted: None,
            displayed: None,
            cache: Cache::new(256 * 1024 * 1024),
            pending: None,
        }
    }
    pub fn statistics(&self) -> String {
        format!(
            "GPU RGBA8 cache: {:.1}/256 MiB | hits {} | misses {}",
            self.cache.bytes as f64 / 1048576.0,
            self.cache.hits,
            self.cache.misses
        )
    }
    pub fn select(&mut self, wanted: Option<PreviewKey>, client: &mut dyn DesktopClient) {
        if wanted == self.wanted {
            return;
        }
        client.cancel_preview();
        self.pending = None;
        self.wanted = wanted.clone();
        self.state = Preview::Pending;
        if let Some(key) = wanted {
            if let Some(texture) = self.cache.get(&key) {
                self.state = Preview::Ready {
                    texture: texture.registration.texture_id(),
                    dimensions: texture.dimensions,
                };
                self.displayed = Some(key);
            } else {
                client.request_preview(key);
            }
        } else {
            self.displayed = None;
        }
    }
    pub fn poll(
        &mut self,
        client: &mut dyn DesktopClient,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut WgpuRenderer,
    ) -> Result<()> {
        let _ = device.poll(wgpu::PollType::Poll);
        if let Some(result) = client.take_preview()
            && self.wanted.as_ref() == Some(&result.key)
        {
            match result.frame {
                Ok(frame) => self.pending = Some(frame),
                Err(error) => self.state = Preview::Failed(error),
            }
        }
        let Some(frame) = self.pending.as_ref() else {
            return Ok(());
        };
        let [width, height] = frame.dimensions();
        let bytes = frame.rgba().len();
        if bytes > self.cache.budget
            || width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            self.pending = None;
            self.state = Preview::Failed("Preview exceeds GPU texture/cache budget".into());
            return Ok(());
        }
        while self.cache.needs_room(bytes) {
            let old = self.cache.evict(|key, texture| {
                Some(key) != self.displayed.as_ref()
                    && texture.in_flight.load(Ordering::Acquire) == 0
            });
            let Some(old) = old else {
                return Ok(());
            }; // Retry after GPU completion, never wait on UI.
            renderer.unregister_external_texture(old.registration)?;
        }
        let frame = self.pending.take().unwrap();
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Fold cached SDR sRGB bytes"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Display transform is already applied; target is also non-sRGB.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            frame.rgba(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let registration = renderer.register_external_texture(
            &texture.create_view(&wgpu::TextureViewDescriptor::default()),
        )?;
        self.state = Preview::Ready {
            texture: registration.texture_id(),
            dimensions: [width, height],
        };
        let key = self.wanted.clone().unwrap();
        self.cache.insert(
            key.clone(),
            Texture {
                registration,
                _texture: texture,
                dimensions: [width, height],
                in_flight: Arc::new(AtomicUsize::new(0)),
            },
            bytes,
        );
        self.displayed = Some(key);
        Ok(())
    }
    /// Call immediately after submission; callbacks protect resources against
    /// cache eviction while the GPU is reading them. No explicit texture destroy.
    pub fn submitted(&self, queue: &wgpu::Queue) {
        if !matches!(self.state, Preview::Ready { .. }) {
            return;
        }
        if let Some(texture) = self.displayed.as_ref().and_then(|key| self.cache.peek(key)) {
            let in_flight = texture.in_flight.clone();
            in_flight.fetch_add(1, Ordering::AcqRel);
            queue.on_submitted_work_done(move || {
                in_flight.fetch_sub(1, Ordering::AcqRel);
            });
        }
    }
    pub fn release(&mut self, renderer: &mut WgpuRenderer) -> Result<()> {
        while let Some(texture) = self.cache.evict(|_, _| true) {
            renderer.unregister_external_texture(texture.registration)?;
        }
        Ok(())
    }
}
