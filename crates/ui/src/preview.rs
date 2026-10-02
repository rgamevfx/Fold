//! Display-transformed RGBA8 GPU hot cache. No image-file compression, no linear
//! frame retention, no export reuse. Submitted textures are protected until done.
use crate::{cache::Cache, shell::Preview};
use dear_imgui_wgpu::{ExternalTextureId, WgpuRenderer, wgpu};
use fold_platform::workspace::PanelInstanceId;
use fold_platform::{
    DisplayFrame,
    desktop::{DesktopClient, PreviewKey},
};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[cfg(test)]
#[path = "preview_gpu_tests.rs"]
mod gpu_tests;
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
    /// Exact immutable key is the consumer's presentation generation. A repeated
    /// key intentionally reuses identical content, independent of request age.
    demands: std::collections::BTreeMap<fold_platform::workspace::PanelInstanceId, PreviewKey>,
    next_consumer: usize,
    generations: BTreeMap<PanelInstanceId, u64>,
    admitted: BTreeMap<PanelInstanceId, u64>,
    completed: BTreeMap<PanelInstanceId, PreviewKey>,
    held: BTreeMap<PanelInstanceId, PreviewKey>,
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
            demands: Default::default(),
            next_consumer: 0,
            generations: BTreeMap::new(),
            admitted: BTreeMap::new(),
            completed: BTreeMap::new(),
            held: BTreeMap::new(),
            failures: vec![],
            #[cfg(feature = "native-probe")]
            probe_upload: None,
        }
    }
    #[cfg(feature = "native-probe")]
    pub fn probe_frame(&self) -> (Option<u32>, bool) {
        // The diagnostic's ready flag means the requested image is presented,
        // not that the canvas is empty while a retained older image is visible.
        let Some((&id, key)) = self.held.first_key_value() else {
            return (None, false);
        };
        (Some(key.frame), self.demands.get(&id) == Some(key))
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
    pub fn select_viewers(
        &mut self,
        demands: &[(
            fold_platform::workspace::PanelInstanceId,
            Option<PreviewKey>,
        )],
        client: &mut dyn DesktopClient,
    ) {
        let generations: BTreeMap<_, _> = demands
            .iter()
            .filter_map(|(id, key)| {
                key.as_ref()
                    .map(|_| (*id, client.viewer_request(*id).map_or(0, |r| r.0)))
            })
            .collect();
        self.completed
            .retain(|id, _| generations.get(id) == self.generations.get(id));
        self.generations = generations;
        self.held.retain(|id, held| {
            demands.iter().any(|(wanted_id, key)| {
                wanted_id == id
                    && key.as_ref().is_some_and(|key| {
                        key.target.map(|t| t.0) == held.target.map(|t| t.0)
                            && key.output == held.output
                    })
            })
        });
        self.demands = demands
            .iter()
            .filter_map(|(id, key)| key.clone().map(|key| (*id, key)))
            .collect();
        let previous = self.wanted.clone();
        let was_requested = self.requested;
        self.select_many(self.demands.values().cloned().collect(), client);
        if self.wanted != previous || (!was_requested && self.requested) {
            self.admitted = self
                .demands
                .iter()
                .filter(|(_, key)| Some(*key) == self.wanted.as_ref())
                .map(|(id, _)| (*id, self.generations[id]))
                .collect();
        }
    }
    pub fn state_for_viewer(&self, id: PanelInstanceId) -> Preview {
        if let Some(key) = self.held.get(&id) {
            self.state_for(Some(key))
        } else if let Some(error) = self.viewer_error(id) {
            Preview::Failed(error.into())
        } else {
            Preview::Pending
        }
    }
    fn completed_frame(&mut self, key: &PreviewKey) {
        for (&id, generation) in &self.admitted {
            if self.generations.get(&id) == Some(generation) {
                self.completed.insert(id, key.clone());
            }
        }
    }
    pub fn presented_key(&self, id: PanelInstanceId) -> Option<&PreviewKey> {
        self.held.get(&id)
    }
    pub fn viewer_error(&self, id: PanelInstanceId) -> Option<&str> {
        let demand = self.demands.get(&id)?;
        self.failures
            .iter()
            .find(|(key, _)| key == demand)
            .map(|(_, error)| error.as_str())
    }
    /// Admission happens only with an uploaded/cache-resident image. The app owns
    /// pacing and playhead advancement; the host owns retained texture lifetimes.
    pub fn present_viewers(&mut self, client: &mut dyn DesktopClient) {
        for (&id, demand) in &self.demands {
            let candidate = if self.cache.peek(demand).is_some() {
                Some(demand)
            } else {
                self.completed.get(&id).filter(|key| {
                    self.cache.peek(key).is_some()
                        && key.content == demand.content
                        && key.dimensions == demand.dimensions
                        && key.view == demand.view
                        && key.output == demand.output
                        && key.target.map(|t| t.0) == demand.target.map(|t| t.0)
                })
            };
            if let Some(key) = candidate
                && client.present_viewer(id, self.generations[&id], key)
            {
                self.held.insert(id, key.clone());
                self.completed.remove(&id);
            }
        }
    }
    /// Bounded shared content scheduler: one in-flight render, latest demand per
    /// visible consumer, deduplicated by immutable content identity. Finish work
    /// already admitted rather than letting a continuously seeking consumer cancel
    /// another's work. Generation-tagged completions may be admitted by the app's
    /// playback policy; holding an image never authorizes a retired completion.
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
        if (self.requested || self.pending.is_some()) && !self.consumers.is_empty() {
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
    fn evict_unused(&mut self) -> Option<Texture> {
        self.cache.evict(|key, texture| {
            (self.consumers.is_empty() && Some(key) != self.displayed.as_ref()
                || !self.consumers.is_empty() && !self.consumers.contains(key))
                && !self.held.values().any(|held| held == key)
                && texture.in_flight.load(Ordering::Acquire) == 0
        })
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
            let old = self.evict_unused();
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
        self.completed_frame(&key);
        self.displayed = Some(key);
        Ok(())
    }
    /// Call immediately after submission; callbacks protect resources against
    /// cache eviction while the GPU is reading them. No explicit texture destroy.
    pub fn submitted(&self, queue: &wgpu::Queue) {
        let mut keys = self.consumers.clone();
        for key in self.held.values() {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
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
