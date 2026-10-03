//! Shared host resources, deliberately independent of windowing and ImGui.
//! Pool ownership alone never authorizes reuse: submitted command buffers retain
//! explicit leases until the queue completion callback releases them.
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicU64, Ordering},
};

pub(crate) static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Image {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub dimensions: [u32; 2],
    pub bytes: u64,
}
struct Pool {
    images: Vec<Arc<Image>>,
    bytes: u64,
    peak: u64,
}
struct Shared {
    id: u64,
    device: wgpu::Device,
    queue: wgpu::Queue,
    budget: u64,
    pool: Mutex<Pool>,
    stopped: Arc<Mutex<Option<String>>>,
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
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(host) = self.host.upgrade() {
            host.pool.lock().unwrap().bytes -= self.bytes;
        }
    }
}
impl Pool {
    fn make_room(&mut self, bytes: u64, budget: u64) -> Result<(), String> {
        if bytes > budget {
            return Err("GPU allocation exceeds host budget".into());
        }
        while self.bytes > budget - bytes {
            let Some(index) = self
                .images
                .iter()
                .position(|image| Arc::strong_count(image) == 1)
            else {
                return Err(
                    "GPU working budget exhausted; release completed frames or retry later".into(),
                );
            };
            self.bytes -= self.images.swap_remove(index).bytes;
        }
        Ok(())
    }
    fn allocated(&mut self, bytes: u64) {
        self.bytes += bytes;
        self.peak = self.peak.max(self.bytes);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Memory {
    pub allocated: u64,
    pub peak: u64,
    pub budget: u64,
}

impl Host {
    /// Construct off the UI thread. No surface or Dear ImGui dependency.
    pub async fn headless(budget: u64) -> Result<(Self, wgpu::AdapterInfo), String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&Default::default())
            .await
            .map_err(|e| e.to_string())?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: adapter.features()
                    & (wgpu::Features::FLOAT32_FILTERABLE
                        | wgpu::Features::TIMESTAMP_QUERY
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                        | wgpu::Features::TEXTURE_COMPRESSION_BC),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok((Self::from_device(device, queue, budget)?, info))
    }

    /// The application host transfers error/device-loss handling to this service.
    /// Call once for a device; clones, rather than independent hosts, share it.
    pub fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        budget: u64,
    ) -> Result<Self, String> {
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
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            device,
            queue,
            budget,
            pool: Mutex::new(Pool {
                images: vec![],
                bytes: 0,
                peak: 0,
            }),
            stopped,
        })))
    }
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn device(&self) -> &wgpu::Device {
        &self.0.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.0.queue
    }
    pub fn check(&self) -> Result<(), String> {
        match self.0.stopped.lock().unwrap().as_ref() {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
    /// Nonblocking completion pump, usable by the desktop event loop.
    pub fn poll(&self) -> Result<(), String> {
        self.device()
            .poll(wgpu::PollType::Poll)
            .map_err(|e| e.to_string())?;
        self.check()
    }
    pub fn memory(&self) -> Memory {
        let pool = self.0.pool.lock().unwrap();
        Memory {
            allocated: pool.bytes,
            peak: pool.peak,
            budget: self.0.budget,
        }
    }
    pub(crate) fn reserve(&self, bytes: u64) -> Result<Arc<Reservation>, String> {
        self.check()?;
        let mut pool = self.0.pool.lock().unwrap();
        pool.make_room(bytes, self.0.budget)?;
        pool.allocated(bytes);
        Ok(Arc::new(Reservation {
            host: Arc::downgrade(&self.0),
            bytes,
        }))
    }
    pub(crate) fn image(&self, dimensions: [u32; 2]) -> Result<Arc<Image>, String> {
        self.image_format(dimensions, wgpu::TextureFormat::Rgba32Float)
    }
    pub(crate) fn image_format(
        &self,
        dimensions: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Result<Arc<Image>, String> {
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
        pool.allocated(bytes);
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
                | if compressed {
                    wgpu::TextureUsages::empty()
                } else {
                    wgpu::TextureUsages::STORAGE_BINDING
                },
            view_formats: &[],
        });
        if let Err(error) = self.check() {
            self.0.pool.lock().unwrap().bytes -= bytes;
            return Err(error);
        }
        let view = texture.create_view(&Default::default());
        let image = Arc::new(Image {
            texture,
            view,
            dimensions,
            bytes,
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
        self.queue().submit([encoder.finish()]);
        self.queue().on_submitted_work_done(move || {
            drop(leases);
            drop(buffers);
        });
    }
}
