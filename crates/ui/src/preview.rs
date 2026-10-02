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
#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;

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
    requested: bool,
    consumers: Vec<PreviewKey>,
    next_consumer: usize,
    failures: Vec<(PreviewKey, String)>,
    #[cfg(feature = "native-probe")]
    pub probe_upload: Option<(std::time::Instant, std::time::Instant, usize)>,
}
impl PreviewHost {
    pub fn new() -> Self {
        Self {
            state: Preview::Pending,
            wanted: None,
            displayed: None,
            cache: Cache::new(256 * 1024 * 1024),
            pending: None,
            requested: false,
            consumers: vec![],
            next_consumer: 0,
            failures: vec![],
            #[cfg(feature = "native-probe")]
            probe_upload: None,
        }
    }
    #[cfg(feature = "native-probe")]
    pub fn probe_frame(&self) -> (Option<u32>, bool) {
        (
            self.displayed.as_ref().map(|key| key.frame),
            matches!(self.state, Preview::Ready { .. }),
        )
    }
    pub fn statistics(&self) -> String {
        format!(
            "GPU RGBA8 cache: {:.1}/256 MiB | hits {} | misses {} | displayed frame {:?}",
            self.cache.bytes as f64 / 1048576.0,
            self.cache.hits,
            self.cache.misses,
            self.displayed.as_ref().map(|key| key.frame)
        )
    }
    /// Phase 14 serial demand adapter over the existing single render worker.
    /// One cache/budget serves all visible consumers. Per-consumer request/audio
    /// lifetimes are introduced in phase 15, not by cloning decoders here.
    pub fn select_many(&mut self, keys: Vec<PreviewKey>, client: &mut dyn DesktopClient) {
        let previous = std::mem::take(&mut self.consumers);
        for key in keys {
            if !self.consumers.contains(&key) {
                if !previous.contains(&key) && self.cache.peek(&key).is_some() {
                    let _ = self.cache.get(&key);
                }
                self.consumers.push(key);
            }
        }
        self.failures
            .retain(|(key, _)| self.consumers.contains(key));
        if (self.requested || self.pending.is_some())
            && self
                .wanted
                .as_ref()
                .is_some_and(|key| self.consumers.contains(key))
        {
            return;
        }
        let mut missing = None;
        for offset in 0..self.consumers.len() {
            let index = (self.next_consumer + offset) % self.consumers.len();
            let key = &self.consumers[index];
            if self.cache.peek(key).is_none()
                && !self.failures.iter().any(|(failed, _)| failed == key)
            {
                missing = Some(key.clone());
                self.next_consumer = index + 1;
                break;
            }
        }
        if missing.is_some() {
            self.select(missing, client);
        } else if self.requested || self.consumers.is_empty() {
            self.select(None, client);
        }
    }
    pub fn state_for(&self, key: Option<&PreviewKey>) -> Preview {
        let Some(key) = key else {
            return Preview::Failed("Choose an available document output".into());
        };
        if let Some(texture) = self.cache.peek(key) {
            Preview::Ready {
                texture: texture.registration.texture_id(),
                dimensions: texture.dimensions,
            }
        } else if let Some((_, error)) = self.failures.iter().find(|(failed, _)| failed == key) {
            Preview::Failed(error.clone())
        } else {
            Preview::Pending
        }
    }
    pub fn select(&mut self, wanted: Option<PreviewKey>, client: &mut dyn DesktopClient) {
        let playing = client.state().playing && !client.state().priming;
        if playing
            && self.requested
            && wanted
                .as_ref()
                .zip(self.wanted.as_ref())
                .is_some_and(|(a, b)| a.content == b.content && a.dimensions == b.dimensions)
        {
            return; // Finish current decode; drop intervening video demands, never stall audio.
        }
        if wanted == self.wanted {
            return;
        }
        client.cancel_preview();
        self.pending = None;
        self.wanted = wanted.clone();
        self.requested = false;
        if !playing {
            self.state = Preview::Pending;
        }
        if let Some(key) = wanted {
            if let Some(texture) = self.cache.get(&key) {
                self.state = Preview::Ready {
                    texture: texture.registration.texture_id(),
                    dimensions: texture.dimensions,
                };
                self.displayed = Some(key);
            } else {
                self.requested = true;
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
        #[cfg(feature = "native-probe")]
        {
            self.probe_upload = None;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        if let Some(result) = client.take_preview()
            && self.wanted.as_ref() == Some(&result.key)
        {
            self.requested = false;
            match result.frame {
                Ok(frame) => self.pending = Some(frame),
                Err(error) => {
                    self.failures.push((result.key, error.clone()));
                    self.state = Preview::Failed(error);
                }
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
            let error = "Preview exceeds GPU texture/cache budget".to_owned();
            if let Some(key) = &self.wanted {
                self.failures.push((key.clone(), error.clone()));
            }
            self.state = Preview::Failed(error);
            return Ok(());
        }
        while self.cache.needs_room(bytes) {
            let old = self.cache.evict(|key, texture| {
                (self.consumers.is_empty() && Some(key) != self.displayed.as_ref()
                    || !self.consumers.is_empty() && !self.consumers.contains(key))
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
        #[cfg(feature = "native-probe")]
        let upload_start = std::time::Instant::now();
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
        #[cfg(feature = "native-probe")]
        {
            self.probe_upload = Some((upload_start, std::time::Instant::now(), bytes));
        }
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
        let mut keys = self.consumers.clone();
        if matches!(self.state, Preview::Ready { .. })
            && let Some(key) = &self.displayed
            && !keys.contains(key)
        {
            keys.push(key.clone());
        }
        for key in keys {
            if let Some(texture) = self.cache.peek(&key) {
                let in_flight = texture.in_flight.clone();
                in_flight.fetch_add(1, Ordering::AcqRel);
                queue.on_submitted_work_done(move || {
                    in_flight.fetch_sub(1, Ordering::AcqRel);
                });
            }
        }
    }
    pub fn release(&mut self, renderer: &mut WgpuRenderer) -> Result<()> {
        while let Some(texture) = self.cache.evict(|_, _| true) {
            renderer.unregister_external_texture(texture.registration)?;
        }
        Ok(())
    }
}
