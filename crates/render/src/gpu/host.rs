//! Shared host resources, deliberately independent of windowing and ImGui.
//! Pool ownership alone never authorizes reuse: submitted command buffers retain
//! explicit leases until the queue completion callback releases them.
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicU64, Ordering},
};

// Native Vulkan instance startup can crash when entered concurrently on the
// qualified Mesa host. Keep this short bootstrap critical section shared by
// headless and desktop creation; no await or rendering occurs under the lock.
static INSTANCE_CREATION: Mutex<()> = Mutex::new(());

pub(crate) static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Image {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub dimensions: [u32; 2],
    pub bytes: u64,
    pub _reservation: Option<Arc<Reservation>>,
}
#[derive(Clone, Copy)]
pub(crate) enum AllocationKind {
    Working = 0,
    Presentation = 1,
    Decoded = 2,
    Geometry = 3,
    Scratch = 4,
}
fn image_kind(image: &Image) -> AllocationKind {
    if matches!(
        image.texture.format(),
        wgpu::TextureFormat::Rgba32Float
            | wgpu::TextureFormat::R32Float
            | wgpu::TextureFormat::Rgba16Float
            | wgpu::TextureFormat::Depth32Float
    ) {
        AllocationKind::Working
    } else {
        AllocationKind::Presentation
    }
}
struct Pool {
    by_kind: [u64; 5],
    images: Vec<Arc<Image>>,
    bytes: u64,
    peak: u64,
}
struct Shared {
    #[cfg(all(feature = "shared-images", target_os = "linux"))]
    shared_validation: std::sync::OnceLock<super::shared::Validation>,
    id: u64,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    budget: u64,
    video_retained: Mutex<u64>,
    pool: Mutex<Pool>,
    sources: Mutex<super::still::Cache>,
    stopped: Arc<Mutex<Option<String>>>,
    completion_poll: Arc<Mutex<()>>,
}

/// Retention entitlement only; the allocation has its separate completion lease.
pub(super) struct VideoRetention {
    owner: Weak<Shared>,
    bytes: u64,
}
impl Drop for VideoRetention {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            *owner.video_retained.lock().unwrap() -= self.bytes;
        }
    }
}

/// Host-only access to the render device. Panels should receive presentation
/// registrations, never this service. Clones share one allocation budget.
#[derive(Clone)]
pub struct Host(Arc<Shared>);

/// Reservation for staging, readback, and parameter buffers. The caller and
/// submission callback each retain it for their respective resource lifetimes.
pub(crate) struct Reservation {
    host: Weak<Shared>,
    bytes: u64,
    kind: AllocationKind,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(host) = self.host.upgrade() {
            let mut pool = host.pool.lock().unwrap();
            pool.bytes -= self.bytes;
            pool.by_kind[self.kind as usize] -= self.bytes;
        }
    }
}
impl Pool {
    fn make_room(&mut self, bytes: u64, budget: u64) -> Result<(), fold_foundation::Failure> {
        if bytes > budget {
            return Err(fold_foundation::Failure::new(
                fold_foundation::FailureKind::RequestTooLarge,
                "GPU allocation exceeds host budget",
            )
            .context(bytes));
        }
        while self.bytes > budget - bytes {
            let Some(index) = self
                .images
                .iter()
                .position(|image| Arc::strong_count(image) == 1)
            else {
                return Err(fold_foundation::Failure::new(
                    fold_foundation::FailureKind::ResourcePressure,
                    "GPU working budget exhausted; release completed frames or retry later",
                )
                .context(bytes));
            };
            let image = self.images.swap_remove(index);
            self.bytes -= image.bytes;
            self.by_kind[image_kind(&image) as usize] -= image.bytes;
        }
        Ok(())
    }
    fn allocated(&mut self, bytes: u64, kind: AllocationKind) {
        self.bytes += bytes;
        self.by_kind[kind as usize] += bytes;
        self.peak = self.peak.max(self.bytes);
    }
}

/// Budget held for resources owned by an external renderer. The owner must keep
/// this lease until those resources and all work that uses them are released.
/// This is an allowance, not a measurement of physical driver memory.
pub struct ResourceAllowance(Arc<Reservation>);
impl ResourceAllowance {
    pub fn bytes(&self) -> u64 {
        self.0.bytes
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Memory {
    pub allocated: u64,
    /// Idle pooled textures can be reclaimed without evicting source caches.
    pub reusable: u64,
    pub peak: u64,
    pub budget: u64,
    pub working: u64,
    pub presentation: u64,
    pub decoded: u64,
    pub geometry: u64,
    pub scratch: u64,
}

fn automatic_limit(physical: u64, budget: u64, usage: u64) -> u64 {
    const MIB: u64 = 1024 * 1024;
    (physical / 2)
        .min(budget.saturating_sub(usage).saturating_mul(3) / 4)
        .clamp(128 * MIB, 32 * 1024 * MIB)
        / MIB
        * MIB
}

impl Host {
    /// Shared engine allocations on the reference 8 GiB GPU. Codec/driver
    /// allocations are measured separately; this is not a physical VRAM cap.
    pub const DEFAULT_BUDGET: u64 = 1024 * 1024 * 1024;

    /// Startup policy based on actual available device-local memory. Keep half
    /// the physical heap and a quarter of current headroom outside Fold's cap.
    /// Unsupported telemetry uses the explicit conservative portable default.
    pub fn automatic_budget(adapter: &wgpu::Adapter) -> u64 {
        #[cfg(target_os = "linux")]
        if let Some([physical, budget, usage]) = fold_native_video::device_memory(adapter) {
            return automatic_limit(physical, budget, usage);
        }
        Self::DEFAULT_BUDGET
    }

    /// Expendable video retention per cache. All allocations still acquire
    /// the shared host reservation, including retained and submitted inputs.
    pub(super) fn video_cache_budget(&self) -> u64 {
        (self.0.budget / 8).min(512 * 1024 * 1024)
    }

    pub(super) fn retain_video(&self, bytes: u64) -> Option<VideoRetention> {
        let mut retained = self.0.video_retained.lock().unwrap();
        if bytes > (self.0.budget / 4).saturating_sub(*retained) {
            return None;
        }
        *retained += bytes;
        Some(VideoRetention {
            owner: Arc::downgrade(&self.0),
            bytes,
        })
    }

    /// Create the native GPU instance through Fold's shared bootstrap boundary.
    /// Keep adapter/device selection outside this short critical section.
    pub fn create_instance(
        descriptor: &wgpu::InstanceDescriptor,
    ) -> Result<wgpu::Instance, fold_foundation::Failure> {
        let _creation = INSTANCE_CREATION
            .lock()
            .map_err(|_| "GPU instance initialization lock poisoned")?;
        Ok(wgpu::Instance::new(descriptor))
    }

    /// Construct off the UI thread. No surface or Dear ImGui dependency.
    pub async fn headless(
        budget: u64,
    ) -> Result<(Self, wgpu::AdapterInfo), fold_foundation::Failure> {
        Self::headless_with_budget(Some(budget)).await
    }

    pub async fn headless_automatic() -> Result<(Self, wgpu::AdapterInfo), fold_foundation::Failure>
    {
        Self::headless_with_budget(None).await
    }

    async fn headless_with_budget(
        budget: Option<u64>,
    ) -> Result<(Self, wgpu::AdapterInfo), fold_foundation::Failure> {
        let instance = Self::create_instance(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        })?;
        let adapter = instance
            .request_adapter(&Default::default())
            .await
            .map_err(|e| e.to_string())?;
        let info = adapter.get_info();
        let budget = budget.unwrap_or_else(|| Self::automatic_budget(&adapter));
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: Self::image_device_limits(&adapter),
                required_features: adapter.features()
                    & (wgpu::Features::FLOAT32_FILTERABLE
                        | wgpu::Features::TIMESTAMP_QUERY
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                        | wgpu::Features::TEXTURE_COMPRESSION_BC),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok((
            Self::from_device(instance, adapter, device, queue, budget)?,
            info,
        ))
    }

    /// Shared desktop/headless device contract. An 8192-square RGBA32F copy
    /// needs a 1 GiB transfer buffer; request only limits the adapter supports.
    pub fn image_device_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
        let available = adapter.limits();
        let mut limits = wgpu::Limits::default();
        limits.max_buffer_size = available
            .max_buffer_size
            .min((fold_media::MAX_PIXELS as u64) * 16)
            .max(limits.max_buffer_size);
        limits
    }

    pub(crate) fn check_buffer_size(&self, bytes: u64) -> Result<(), fold_foundation::Failure> {
        if bytes > self.device().limits().max_buffer_size {
            return Err(fold_foundation::Failure::new(
                fold_foundation::FailureKind::UnsupportedCapability,
                "Image transfer exceeds this GPU device's buffer limit",
            ));
        }
        Ok(())
    }

    /// The application host transfers error/device-loss handling to this service.
    /// Call once for a device; clones, rather than independent hosts, share it.
    /// The caller supplies the instance/adapter that created this device and queue.
    /// Retaining that context lets offscreen adapters use the same GPU ownership.
    pub fn from_device(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        budget: u64,
    ) -> Result<Self, fold_foundation::Failure> {
        if budget == 0 {
            return Err("GPU budget must be positive".into());
        }
        let stopped = Arc::new(Mutex::new(None));
        let errors = stopped.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            errors
                .lock()
                .unwrap()
                .get_or_insert_with(|| format!("GPU rendering stopped: {error}"));
        }));
        let lost = stopped.clone();
        device.set_device_lost_callback(move |reason, message| {
            *lost.lock().unwrap() = Some(format!("GPU device lost ({reason:?}): {message}"));
        });
        Ok(Self(Arc::new(Shared {
            #[cfg(all(feature = "shared-images", target_os = "linux"))]
            shared_validation: std::sync::OnceLock::new(),
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            instance,
            adapter,
            device,
            queue,
            budget,
            video_retained: Mutex::new(0),
            pool: Mutex::new(Pool {
                images: vec![],
                by_kind: [0; 5],
                bytes: 0,
                peak: 0,
            }),
            sources: Mutex::new(Default::default()),
            stopped,
            completion_poll: Arc::new(Mutex::new(())),
        })))
    }
    pub(super) fn sources(&self) -> std::sync::MutexGuard<'_, super::still::Cache> {
        self.0.sources.lock().unwrap()
    }
    #[cfg(all(feature = "shared-images", target_os = "linux"))]
    pub(super) fn shared_validation(&self) -> &super::shared::Validation {
        self.0
            .shared_validation
            .get_or_init(|| super::shared::Validation::new(self.device()))
    }
    pub fn id(&self) -> u64 {
        self.0.id
    }
    /// Render adapters borrow this context from the same host as their device.
    /// No second instance, adapter selection, device or queue is created.
    pub fn instance(&self) -> &wgpu::Instance {
        &self.0.instance
    }
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.0.adapter
    }
    pub fn device(&self) -> &wgpu::Device {
        &self.0.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.0.queue
    }
    /// Completion callbacks observe failure without retaining the host/device.
    pub(super) fn failure_state(&self) -> Arc<Mutex<Option<String>>> {
        self.0.stopped.clone()
    }
    pub(super) fn completion_poll(&self) -> Arc<Mutex<()>> {
        self.0.completion_poll.clone()
    }
    pub fn check(&self) -> Result<(), fold_foundation::Failure> {
        match self.0.stopped.lock().unwrap().as_ref() {
            Some(error) => Err(error.clone().into()),
            None => Ok(()),
        }
    }
    /// Nonblocking completion pump, usable by the desktop event loop.
    pub fn poll(&self) -> Result<(), fold_foundation::Failure> {
        let _poll = self.0.completion_poll.lock().unwrap();
        self.device()
            .poll(wgpu::PollType::Poll)
            .map_err(|e| e.to_string())?;
        self.check()
    }
    pub fn memory(&self) -> Memory {
        let pool = self.0.pool.lock().unwrap();
        Memory {
            allocated: pool.bytes,
            reusable: pool
                .images
                .iter()
                .filter(|image| Arc::strong_count(image) == 1)
                .map(|image| image.bytes)
                .sum(),
            peak: pool.peak,
            budget: self.0.budget,
            working: pool.by_kind[0],
            presentation: pool.by_kind[1],
            decoded: pool.by_kind[2],
            geometry: pool.by_kind[3],
            scratch: pool.by_kind[4],
        }
    }
    /// Reserve retained external-renderer resources against the shared scratch
    /// budget before starting their owner. Drop only after the owner is reaped.
    pub fn reserve_external_resources(
        &self,
        bytes: u64,
    ) -> Result<ResourceAllowance, fold_foundation::Failure> {
        self.reserve(bytes).map(ResourceAllowance)
    }
    pub(crate) fn reserve(&self, bytes: u64) -> Result<Arc<Reservation>, fold_foundation::Failure> {
        self.reserve_as(bytes, AllocationKind::Scratch)
    }
    pub(crate) fn reserve_as(
        &self,
        bytes: u64,
        kind: AllocationKind,
    ) -> Result<Arc<Reservation>, fold_foundation::Failure> {
        self.check()?;
        let mut pool = self.0.pool.lock().unwrap();
        pool.make_room(bytes, self.0.budget)?;
        pool.allocated(bytes, kind);
        Ok(Arc::new(Reservation {
            host: Arc::downgrade(&self.0),
            bytes,
            kind,
        }))
    }
    pub(crate) fn image(
        &self,
        dimensions: [u32; 2],
    ) -> Result<Arc<Image>, fold_foundation::Failure> {
        self.image_format(dimensions, wgpu::TextureFormat::Rgba32Float)
    }
    pub(crate) fn image_format(
        &self,
        dimensions: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Result<Arc<Image>, fold_foundation::Failure> {
        self.check()?;
        let [width, height] = dimensions;
        let limit = self.device().limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > limit || height > limit {
            return Err("unsupported GPU image dimensions".into());
        }
        let compressed = format == wgpu::TextureFormat::Bc7RgbaUnorm;
        if compressed
            && (!width.is_multiple_of(4)
                || !height.is_multiple_of(4)
                || !self
                    .device()
                    .features()
                    .contains(wgpu::Features::TEXTURE_COMPRESSION_BC))
        {
            return Err("unsupported BC7 image dimensions or device".into());
        }
        let bytes = u64::from(width)
            * u64::from(height)
            * match format {
                wgpu::TextureFormat::Rgba32Float => 16,
                wgpu::TextureFormat::Rgba16Float => 8,
                wgpu::TextureFormat::Bc7RgbaUnorm => 1,
                _ => 4,
            };
        let mut pool = self.0.pool.lock().unwrap();
        if let Some(image) = pool.images.iter().find(|image| {
            image.dimensions == dimensions
                && image.texture.format() == format
                && Arc::strong_count(image) == 1
        }) {
            return Ok(image.clone());
        }
        // Drop only leases not held by callers, recording encoders, or submitted
        // work. Completion callbacks own submitted leases independently of frames.
        pool.make_room(bytes, self.0.budget)?;
        let kind = if matches!(
            format,
            wgpu::TextureFormat::Rgba32Float
                | wgpu::TextureFormat::R32Float
                | wgpu::TextureFormat::Rgba16Float
                | wgpu::TextureFormat::Depth32Float
        ) {
            AllocationKind::Working
        } else {
            AllocationKind::Presentation
        };
        pool.allocated(bytes, kind);
        // Completion callbacks may run on the UI thread. Never hold the pool
        // accounting lock across a driver allocation they might otherwise await.
        drop(pool);
        let texture = self.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Fold pooled image"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | if matches!(
                    format,
                    wgpu::TextureFormat::Rgba16Float
                        | wgpu::TextureFormat::Rgba32Float
                        | wgpu::TextureFormat::R32Float
                        | wgpu::TextureFormat::Depth32Float
                ) {
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                } else {
                    wgpu::TextureUsages::empty()
                }
                | if compressed || format == wgpu::TextureFormat::Depth32Float {
                    wgpu::TextureUsages::empty()
                } else {
                    wgpu::TextureUsages::STORAGE_BINDING
                },
            view_formats: &[],
        });
        if let Err(error) = self.check() {
            let mut pool = self.0.pool.lock().unwrap();
            pool.bytes -= bytes;
            pool.by_kind[kind as usize] -= bytes;
            return Err(error);
        }
        let view = texture.create_view(&Default::default());
        let image = Arc::new(Image {
            texture,
            view,
            dimensions,
            bytes,
            _reservation: None,
        });
        self.0.pool.lock().unwrap().images.push(image.clone());
        Ok(image)
    }
    pub(crate) fn submit(
        &self,
        encoder: wgpu::CommandEncoder,
        leases: Vec<Arc<Image>>,
        buffers: Vec<Arc<Reservation>>,
    ) {
        let _poll = self.0.completion_poll.lock().unwrap();
        self.queue().submit([encoder.finish()]);
        self.queue().on_submitted_work_done(move || {
            drop(leases);
            drop(buffers);
        });
    }
}

#[cfg(test)]
mod budget_tests {
    #[test]
    fn automatic_headroom_is_bounded_by_capacity_and_current_usage() {
        let gib = 1024 * 1024 * 1024;
        assert_eq!(super::automatic_limit(8 * gib, 7 * gib, gib), 4 * gib);
        assert_eq!(
            super::automatic_limit(8 * gib, 7 * gib, 5 * gib),
            3 * gib / 2
        );
        assert_eq!(
            super::automatic_limit(8 * gib, gib, 2 * gib),
            128 * 1024 * 1024
        );
    }
    use super::*;
    use fold_foundation::FailureKind;
    #[test]
    #[ignore = "requires native Vulkan"]
    fn video_retention_is_shared_and_released_independently_of_allocations() {
        let mib = 1024 * 1024;
        let (host, _) = pollster::block_on(Host::headless(64 * mib)).unwrap();
        let other = host.clone();
        let first = host.retain_video(8 * mib).unwrap();
        let second = other.retain_video(8 * mib).unwrap();
        assert!(host.retain_video(1).is_none());
        assert_eq!(
            host.memory().allocated,
            0,
            "retention grants are not allocations"
        );
        drop(first);
        let third = other.retain_video(8 * mib).unwrap();
        drop((second, third));
        assert_eq!(*host.0.video_retained.lock().unwrap(), 0);
    }

    #[test]
    fn oversized_reservations_are_permanent_and_held_leases_are_pressure() {
        let mut pool = Pool {
            by_kind: [0; 5],
            images: vec![],
            bytes: 12,
            peak: 12,
        };
        assert_eq!(
            pool.make_room(17, 16).unwrap_err().kind,
            FailureKind::RequestTooLarge
        );
        assert_eq!(
            pool.make_room(8, 16).unwrap_err().kind,
            FailureKind::ResourcePressure
        );
        pool.bytes = 0;
        pool.make_room(16, 16).unwrap();
    }
}
