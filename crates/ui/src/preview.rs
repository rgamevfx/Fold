//! Display-transformed GPU hot cache: immediate RGBA8, background BC7 retention.
//! No working-frame retention/export reuse. Submitted leases stay protected.
use crate::{cache::Cache, shell::Preview};
use dear_imgui_wgpu::{ExternalTextureId, WgpuRenderer, wgpu};
use fold_platform::workspace::PanelInstanceId;
use fold_platform::{
    DisplayFrame,
    desktop::{DesktopClient, PreviewDemand, PreviewKey},
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

#[path = "preview_compression.rs"]
mod compression;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn realtime_next(
    key: &PreviewKey,
    transport: &fold_platform::desktop::ViewerTransport,
    rate: [u32; 2],
    frames: u32,
) -> Option<PreviewKey> {
    if !transport.playing
        || transport.mode != fold_platform::desktop::PlaybackMode::RealTime
        || frames == 0
    {
        return None;
    }
    let (start, end) = transport.range.bounds(frames);
    let frame = if key.frame >= end {
        if transport.looping {
            start
        } else {
            return None;
        }
    } else {
        key.frame + 1
    };
    let (document, _) = key.target?;
    Some(PreviewKey {
        frame,
        target: Some((
            document,
            fold_foundation::Time::new(i64::from(frame) * i64::from(rate[1]), rate[0]).ok()?,
        )),
        ..key.clone()
    })
}
struct Texture {
    registration: ExternalTextureId,
    _texture: wgpu::Texture,
    gpu_lease: Option<fold_platform::gpu::Display>,
    dimensions: [u32; 2],
    compression_attempted: bool,
    in_flight: Arc<AtomicUsize>,
}
enum Prepared {
    Cpu(DisplayFrame),
    Gpu(Box<fold_platform::gpu::Display>),
}
impl Prepared {
    fn dimensions(&self) -> [u32; 2] {
        match self {
            Self::Cpu(f) => f.dimensions(),
            Self::Gpu(f) => f.dimensions(),
        }
    }
}
pub(crate) struct PreviewHost {
    pub state: Preview,
    render_host: Option<u64>,
    compression: Option<compression::Compression>,
    compression_error: Option<String>,
    result_key: Option<PreviewKey>,
    displayed: Option<PreviewKey>,
    cache: Cache<PreviewKey, Texture>,
    pending: Option<Prepared>,
    requested: bool,
    consumers: Vec<PreviewKey>,
    /// Exact immutable key is the consumer's presentation generation. A repeated
    /// key intentionally reuses identical content, independent of request age.
    demands: std::collections::BTreeMap<fold_platform::workspace::PanelInstanceId, PreviewKey>,
    submitted_demands: Vec<PreviewDemand>,
    reviews: BTreeMap<PanelInstanceId, crate::review::Plan>,
    generations: BTreeMap<PanelInstanceId, u64>,
    admitted: BTreeMap<PanelInstanceId, u64>,
    completed: BTreeMap<PanelInstanceId, PreviewKey>,
    held: BTreeMap<PanelInstanceId, PreviewKey>,
    failures: Vec<(PreviewKey, String)>,
    #[cfg(feature = "native-probe")]
    pub probe_records: Vec<String>,
    #[cfg(feature = "native-probe")]
    pub probe_upload: Option<(std::time::Instant, std::time::Instant, usize)>,
}
impl PreviewHost {
    pub fn new() -> Self {
        Self {
            state: Preview::Pending,
            render_host: None,
            compression: None,
            compression_error: None,
            result_key: None,
            displayed: None,
            cache: Cache::new(256 * 1024 * 1024),
            pending: None,
            requested: false,
            consumers: vec![],
            demands: Default::default(),
            submitted_demands: vec![],
            reviews: Default::default(),
            generations: BTreeMap::new(),
            admitted: BTreeMap::new(),
            completed: BTreeMap::new(),
            held: BTreeMap::new(),
            failures: vec![],
            #[cfg(feature = "native-probe")]
            probe_upload: None,
            #[cfg(feature = "native-probe")]
            probe_records: Vec::new(),
        }
    }
    pub fn attach_host(&mut self, host: &fold_platform::gpu::Host) {
        self.render_host = Some(host.id());
        if host
            .device()
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        {
            self.compression = Some(compression::Compression::new(host.clone()));
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
            "GPU RGBA8/BC7 cache: {:.1}/256 MiB | hits {} | misses {} | displayed frame {:?} | {}",
            self.cache.bytes as f64 / 1048576.0,
            self.cache.hits,
            self.cache.misses,
            self.displayed.as_ref().map(|key| key.frame),
            self.compression_error
                .as_deref()
                .unwrap_or(if self.compression.is_some() {
                    "BC7 enabled"
                } else {
                    "RGBA8 fallback"
                })
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
                            && key.channels == held.channels
                    })
            })
        });
        self.demands = demands
            .iter()
            .filter_map(|(id, key)| key.clone().map(|key| (*id, key)))
            .collect();
        let previous = std::mem::take(&mut self.consumers);
        for key in self.demands.values() {
            if !self.consumers.contains(key) {
                if !previous.contains(key) {
                    let _ = self.cache.get(key);
                }
                self.consumers.push(key.clone());
            }
        }
        self.failures.retain(|(key, _)| {
            self.consumers.contains(key)
                || self
                    .reviews
                    .values()
                    .any(|p| (0..p.count).any(|n| p.key(n) == *key))
        });
        self.update_reviews(client);
        self.send_demands(client);
    }
    fn preparing_or_resident(&self, key: &PreviewKey) -> bool {
        self.cache.peek(key).is_some()
            || (self.pending.is_some() && self.result_key.as_ref() == Some(key))
    }
    fn send_demands(&mut self, client: &mut dyn DesktopClient) {
        let mut missing: Vec<_> = self
            .demands
            .iter()
            .filter(|(id, key)| {
                !self.reviews.get(id).is_some_and(|plan| plan.cached_playing)
                    && !self.preparing_or_resident(key)
                    && !self.failures.iter().any(|(failed, _)| failed == *key)
            })
            .map(|(&id, key)| PreviewDemand {
                consumer: id,
                generation: self.generations[&id],
                key: key.clone(),
                background: false,
            })
            .collect();
        // Up to six fresh frames ahead of the clock, only after its current
        // frame is resident or already submitted. This bounded pipeline is separate
        // from explicit range review.
        // Present_viewer still rejects early frames and retired generations.
        for (&id, key) in &self.demands {
            if !self.preparing_or_resident(key)
                || self.reviews.get(&id).is_some_and(|p| p.cached_playing)
            {
                continue;
            }
            let Some(transport) = client.viewer_transport(id) else {
                continue;
            };
            if !transport.playing
                || transport.mode != fold_platform::desktop::PlaybackMode::RealTime
            {
                continue;
            }
            let state = client.preview_state(&transport.output, transport.time);
            let mut ahead = key.clone();
            for _ in 0..6 {
                let Some(next) = realtime_next(&ahead, &transport, state.rate, state.frames) else {
                    break;
                };
                if !self.preparing_or_resident(&next)
                    && !self.failures.iter().any(|(failed, _)| failed == &next)
                {
                    missing.push(PreviewDemand {
                        consumer: id,
                        generation: self.generations[&id],
                        key: next,
                        background: false,
                    });
                    break;
                }
                ahead = next;
            }
        }
        for (&id, plan) in &mut self.reviews {
            if let Some(key) = plan.next(|key| self.cache.peek(key).is_some()) {
                if let Some((_, error)) = self.failures.iter().find(|(failed, _)| failed == &key) {
                    plan.message = Some(format!("Review preparation failed: {error}"));
                } else {
                    missing.push(PreviewDemand {
                        consumer: id,
                        generation: plan.generation,
                        key,
                        background: true,
                    });
                }
            }
        }
        self.requested = !missing.is_empty();
        if missing != self.submitted_demands {
            client.preview_demands(missing.clone());
            self.submitted_demands = missing;
        }
    }

    pub fn review_state(&self, id: PanelInstanceId) -> (Option<String>, bool) {
        self.reviews.get(&id).map_or((None, false), |plan| {
            let resident = plan.resident(|key| self.cache.peek(key).is_some());
            (Some(plan.status(resident)), plan.can_replay(resident))
        })
    }
    pub fn review_action(
        &mut self,
        id: PanelInstanceId,
        action: crate::review::Action,
        client: &mut dyn DesktopClient,
    ) {
        use crate::review::{Action, Plan};
        let Some(mut transport) = client.viewer_transport(id) else {
            return;
        };
        match action {
            Action::Cancel => {
                if self
                    .reviews
                    .get(&id)
                    .is_some_and(|plan| plan.cached_playing)
                    && transport.playing
                {
                    transport.playing = false;
                    client.command(fold_platform::desktop::DesktopCommand::ViewerTransport {
                        viewer: id,
                        transport,
                    });
                }
                self.reviews.remove(&id);
            }
            Action::Prepare => {
                if transport.playing {
                    return;
                }
                let Some(template) = self.demands.get(&id).cloned() else {
                    return;
                };
                let state = client.preview_state(&transport.output, transport.time);
                if state.transient {
                    return;
                }
                let generation = client.viewer_request(id).map_or(0, |r| r.0);
                self.reviews.insert(
                    id,
                    Plan::new(
                        generation,
                        template,
                        state.rate,
                        transport.range,
                        state.frames,
                        self.cache.budget,
                    ),
                );
                let available = self.cache.budget / self.reviews.len();
                for plan in self.reviews.values_mut() {
                    let bounded = Plan::new(
                        plan.generation,
                        plan.template.clone(),
                        plan.rate,
                        plan.range,
                        plan.start + plan.total,
                        available,
                    );
                    plan.count = plan.count.min(bounded.count);
                }
            }
            Action::PlayCached => {
                if transport.mode != fold_platform::desktop::PlaybackMode::RealTime {
                    return;
                }
                let Some(plan) = self.reviews.get_mut(&id) else {
                    return;
                };
                if !plan.can_replay(plan.resident(|key| self.cache.peek(key).is_some())) {
                    return;
                }
                transport.time = plan.key(0).target.unwrap().1;
                transport.playing = true;
                client.command(fold_platform::desktop::DesktopCommand::ViewerTransport {
                    viewer: id,
                    transport,
                });
                plan.generation = client.viewer_request(id).map_or(0, |r| r.0);
                plan.cached_playing = true;
            }
        }
    }
    fn update_reviews(&mut self, client: &mut dyn DesktopClient) {
        self.reviews.retain(|id, _| self.demands.contains_key(id));
        for (&id, plan) in &mut self.reviews {
            if plan.message.is_some() {
                continue;
            }
            let Some(mut transport) = client.viewer_transport(id) else {
                continue;
            };
            let key = &self.demands[&id];
            let valid = plan.compatible(self.generations[&id], key, transport.range);
            let miss = plan.cached_playing && transport.playing && self.cache.peek(key).is_none();
            if plan.cached_playing && !transport.playing {
                plan.cached_playing = false;
            }
            if !valid || miss {
                if plan.cached_playing && transport.playing {
                    transport.playing = false;
                    client.command(fold_platform::desktop::DesktopCommand::ViewerTransport {
                        viewer: id,
                        transport,
                    });
                    if let Some((generation, _)) = client.viewer_request(id) {
                        self.generations.insert(id, generation);
                    }
                    self.completed.remove(&id);
                }
                plan.cached_playing = false;
                plan.message = Some(
                    if miss {
                        "Cached preview stopped: frame evicted"
                    } else {
                        "Review preparation interrupted"
                    }
                    .into(),
                );
            }
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
                        && key.region == demand.region
                        && key.view == demand.view
                        && key.channels == demand.channels
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
    pub fn state_for(&self, key: Option<&PreviewKey>) -> Preview {
        let Some(key) = key else {
            return Preview::Failed("Choose an available document output".into());
        };
        if let Some(texture) = self.cache.peek(key) {
            Preview::Ready {
                texture: texture.registration.texture_id(),
                dimensions: texture.dimensions,
                uv_max: texture
                    .gpu_lease
                    .as_ref()
                    .map_or([1., 1.], |frame| frame.uv_max()),
            }
        } else if let Some((_, error)) = self.failures.iter().find(|(failed, _)| failed == key) {
            Preview::Failed(error.clone())
        } else {
            Preview::Pending
        }
    }
    fn evict_unused(&mut self) -> Option<Texture> {
        let unused = |key: &PreviewKey, texture: &Texture| {
            (self.consumers.is_empty() && Some(key) != self.displayed.as_ref()
                || !self.consumers.is_empty() && !self.consumers.contains(key))
                && !self.held.values().any(|held| held == key)
                && texture.in_flight.load(Ordering::Acquire) == 0
        };
        let outside_review = self.cache.evict(|key, texture| {
            unused(key, texture) && !self.reviews.values().any(|plan| plan.preparing_key(key))
        });
        // Preparation must not evict an earlier resident frame in its own range.
        // Foreground pressure may interrupt that promise, never block interaction.
        outside_review.or_else(|| {
            (!self.admitted.is_empty())
                .then(|| self.cache.evict(unused))
                .flatten()
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
            self.probe_records.clear();
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let playing = self.demands.keys().any(|&id| {
            client
                .viewer_transport(id)
                .is_some_and(|transport| transport.playing)
        });
        self.poll_compression(renderer, !playing)?;
        // One host-pending frame and one worker result can already be ready.
        // Drain both without imposing another UI refresh interval on the worker.
        for _ in 0..2 {
            if !self.poll_result(client, device, queue, renderer)? {
                break;
            }
        }
        Ok(())
    }
    fn poll_result(
        &mut self,
        client: &mut dyn DesktopClient,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut WgpuRenderer,
    ) -> Result<bool> {
        client.begin_preview_update();
        if self.pending.is_none() {
            if let Some(result) = client.take_preview() {
                self.admitted = result.consumers.into_iter().collect();
                self.result_key = Some(result.key.clone());
                match result.frame {
                    Ok(frame) => self.pending = Some(Prepared::Cpu(frame)),
                    Err(error) => {
                        self.failures.push((result.key, error.clone()));
                        self.state = Preview::Failed(error);
                    }
                }
            }
            if let Some(result) = client.take_gpu_preview() {
                self.admitted = result.consumers.into_iter().collect();
                self.result_key = Some(result.key.clone());
                match result.frame {
                    Ok(frame) => self.pending = Some(Prepared::Gpu(Box::new(frame))),
                    Err(error) => {
                        self.failures.push((result.key, error.clone()));
                        self.state = Preview::Failed(error);
                    }
                }
            }
        }
        // A submitted result owns its leases while decode of the next demand
        // runs. Readiness below remains mandatory before cache/presentation.
        self.send_demands(client);
        client.end_preview_update();
        if let Some(Prepared::Gpu(frame)) = self.pending.as_ref() {
            let readiness = if self.render_host == Some(frame.owner()) {
                frame.is_ready()
            } else {
                Err("Preview texture belongs to a different GPU device".into())
            };
            match readiness {
                Ok(false) => return Ok(false),
                Ok(true) => {}
                Err(error) => {
                    if let Some(key) = &self.result_key {
                        self.failures.push((key.clone(), error.clone()));
                    }
                    self.pending = None;
                    self.state = Preview::Failed(error);
                    return Ok(false);
                }
            }
        }
        if let Some(key) = self.result_key.clone()
            && self.pending.is_some()
            && self.cache.peek(&key).is_some()
        {
            self.pending = None;
            self.completed_frame(&key);
        }
        let Some(frame) = self.pending.as_ref() else {
            return Ok(false);
        };
        let [width, height] = frame.dimensions();
        let bytes = width as usize * height as usize * 4;
        if bytes > self.cache.budget
            || width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            self.pending = None;
            let error = "Preview exceeds GPU texture/cache budget".to_owned();
            if let Some(key) = &self.result_key {
                self.failures.push((key.clone(), error.clone()));
            }
            self.state = Preview::Failed(error);
            return Ok(false);
        }
        while self.cache.needs_room(bytes) {
            let old = self.evict_unused();
            let Some(old) = old else {
                if self.admitted.is_empty() {
                    for plan in self.reviews.values_mut() {
                        plan.message =
                            Some("Review preparation paused: cache capacity in use".into());
                    }
                    self.pending = None;
                }
                return Ok(false);
            }; // Retry foreground after GPU completion, never wait on UI.
            renderer.unregister_external_texture(old.registration)?;
        }
        let frame = self.pending.take().unwrap();
        #[cfg(feature = "native-probe")]
        if let Prepared::Gpu(gpu) = &frame {
            self.probe_records.push(format!(
                "GPU preview: frame={} owners={:?} unix_ms={} gpu_ms={:?} transfers={:?}",
                self.result_key.as_ref().unwrap().frame,
                self.admitted,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
                gpu.gpu_nanoseconds()?.map(|v| v / 1e6),
                gpu.statistics
            ));
        }
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        #[cfg(feature = "native-probe")]
        let upload_start = std::time::Instant::now();
        let (texture, gpu_lease) = match frame {
            Prepared::Gpu(frame) => (frame.texture().clone(), Some(*frame)),
            Prepared::Cpu(frame) => {
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
                (texture, None)
            }
        };
        let registration = renderer.register_external_texture(
            &texture.create_view(&wgpu::TextureViewDescriptor::default()),
        )?;
        #[cfg(feature = "native-probe")]
        {
            self.probe_upload = Some((
                upload_start,
                std::time::Instant::now(),
                if gpu_lease.is_some() { 0 } else { bytes },
            ));
        }
        self.state = Preview::Ready {
            texture: registration.texture_id(),
            dimensions: [width, height],
            uv_max: [1., 1.],
        };
        let key = self.result_key.clone().unwrap();
        self.cache.insert(
            key.clone(),
            Texture {
                registration,
                _texture: texture,
                gpu_lease,
                dimensions: [width, height],
                compression_attempted: false,
                in_flight: Arc::new(AtomicUsize::new(0)),
            },
            bytes,
        );
        self.completed_frame(&key);
        self.displayed = Some(key);
        self.send_demands(client);
        Ok(true)
    }
    fn poll_compression(&mut self, renderer: &mut WgpuRenderer, idle: bool) -> Result<()> {
        let Some(compression) = &mut self.compression else {
            return Ok(());
        };
        if let Some(result) = compression.take() {
            match result {
                Ok((key, frame)) => {
                    // Do not replace a displayed registration or unfinished UI use.
                    if let Some(old) = self.cache.evict(|candidate, texture| {
                        candidate == &key
                            && !self.consumers.contains(candidate)
                            && !self.held.values().any(|held| held == candidate)
                            && self.displayed.as_ref() != Some(candidate)
                            && texture.in_flight.load(Ordering::Acquire) == 0
                    }) {
                        let registration = renderer.register_external_texture(
                            &frame.texture().create_view(&Default::default()),
                        )?;
                        renderer.unregister_external_texture(old.registration)?;
                        let bytes = frame.storage_bytes() as usize;
                        self.cache.insert(
                            key,
                            Texture {
                                registration,
                                _texture: frame.texture().clone(),
                                dimensions: frame.dimensions(),
                                gpu_lease: Some(frame),
                                compression_attempted: true,
                                in_flight: Arc::new(AtomicUsize::new(0)),
                            },
                            bytes,
                        );
                    }
                }
                Err(error) => self.compression_error = Some(format!("RGBA8 retained: {error}")),
            }
        }
        // A full lookahead is deadline headroom, not idle GPU time. Start cache
        // compression only while paused; collect already submitted work above.
        if idle && compression.can_accept() && !self.requested && self.pending.is_none() {
            for entry in self.cache.entries_mut() {
                let texture = &mut entry.value;
                if texture.compression_attempted
                    || self.consumers.contains(&entry.key)
                    || self.held.values().any(|key| key == &entry.key)
                    || self.displayed.as_ref() == Some(&entry.key)
                    || texture.in_flight.load(Ordering::Acquire) != 0
                {
                    continue;
                }
                if let Some(frame) = &texture.gpu_lease {
                    texture.compression_attempted = true;
                    let [width, height] = frame.dimensions();
                    if u64::from(width.div_ceil(4)) * u64::from(height.div_ceil(4)) * 16
                        >= frame.storage_bytes()
                    {
                        continue;
                    }
                    compression.request(entry.key.clone(), frame.clone());
                    break;
                }
            }
        }
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
                let lease = texture.gpu_lease.clone();
                queue.on_submitted_work_done(move || {
                    drop(lease);
                    in_flight.fetch_sub(1, Ordering::AcqRel);
                });
            }
        }
    }
    pub fn release(&mut self, renderer: &mut WgpuRenderer) -> Result<()> {
        self.compression.take();
        while let Some(texture) = self.cache.evict(|_, _| true) {
            renderer.unregister_external_texture(texture.registration)?;
        }
        Ok(())
    }
}
