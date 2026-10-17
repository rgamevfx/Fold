//! One fframes SVG-tree path for native artwork and procedural script objects.
//! Construct and use the retained renderer on a render worker.
#[allow(unsafe_code)]
mod backend;
pub mod capture;
mod native;
pub mod worker;
pub use fframes;
pub use fold_foundation::ObjectId;
pub use fold_motion_canvas::motion::{Context as MotionContext, Pose as MotionPose};
pub use fold_motion_canvas::scripts::Context;
pub use native::{Scripts, document, script_output};

use fframes_skia_renderer::{render, skia_safe as skia};
use fold_gpu_sharing::{Device, Export};
use std::time::Instant;
use std::{collections::HashMap, sync::Arc};

/// Resolved immutable raster data. Keys are explicit resource names in SVG hrefs;
/// project asset resolution and decoding belong to the calling worker.
pub type Images = HashMap<String, Arc<fframes::usvgr::PreloadedImageData>>;

/// CPU phase timings. GPU completion is included by the later readback demand.
#[derive(Default, serde::Serialize)]
pub struct DrawTimings {
    pub conversion_ms: f64,
    pub commands_ms: f64,
    pub submit_ms: f64,
}

/// The evaluated upstream tree is shared by rendering and editable capture.
/// Conversion requires explicit font data, but does not create a GPU device.
pub struct Conversion {
    cache: fframes::usvgr::Cache,
    fonts: fframes::usvgr::fontdb::Database,
    images: Images,
    image_bytes: u64,
}
impl Conversion {
    pub fn new(fonts: impl IntoIterator<Item = Vec<u8>>) -> Self {
        let mut database = fframes::usvgr::fontdb::Database::new();
        for font in fonts {
            database.load_font_data(font);
        }
        Self {
            cache: fframes::usvgr::Cache::new_with_text_cache(
                fframes_skia_renderer::SkiaCacheConfig::default().text_capacity,
            ),
            fonts: database,
            images: Images::new(),
            image_bytes: 0,
        }
    }

    /// Publish a complete resource snapshot atomically. Invalid snapshots leave
    /// the currently retained resources and conversion cache unchanged.
    pub fn set_images(&mut self, images: Images, byte_limit: u64) -> Result<bool, String> {
        // Worker demands clone Arc handles, not pixels. Reuse already validated
        // immutable storage, but enforce each demand's current allowance.
        if images.len() == self.images.len()
            && images.iter().all(|(name, image)| {
                self.images
                    .get(name)
                    .is_some_and(|retained| Arc::ptr_eq(image, retained))
            })
        {
            if self.image_bytes > byte_limit {
                return Err("Motion images exceed their allocation allowance".into());
            }
            return Ok(false);
        }
        let mut bytes = 0u64;
        let mut identities = std::collections::BTreeSet::new();
        for (name, image) in &images {
            let size = u64::from(image.width)
                .checked_mul(u64::from(image.height))
                .and_then(|size| size.checked_mul(4))
                .ok_or("Motion image size overflow")?;
            if name.is_empty()
                || image.id.is_empty()
                || image.width == 0
                || image.height == 0
                || image.width > 16_384
                || image.height > 16_384
                || size != image.data.len() as u64
                || !identities.insert(&image.id)
                || image
                    .data
                    .chunks_exact(4)
                    .any(|rgba| rgba[0] > rgba[3] || rgba[1] > rgba[3] || rgba[2] > rgba[3])
            {
                return Err("Invalid resolved Motion image".into());
            }
            bytes = bytes
                .checked_add(size)
                .ok_or("Motion image size overflow")?;
        }
        if bytes > byte_limit {
            return Err("Motion images exceed their allocation allowance".into());
        }
        if self.images == images {
            return Ok(false);
        }
        self.images = images;
        self.image_bytes = bytes;
        self.cache = fframes::usvgr::Cache::new_with_text_cache(
            fframes_skia_renderer::SkiaCacheConfig::default().text_capacity,
        );
        Ok(true)
    }

    pub fn evaluate(&mut self, frame: fframes::Svgr<'_>) -> Result<fframes::usvgr::Tree, String> {
        frame
            .into_svg_tree(
                &fframes::usvgr::Options {
                    fast_shapes: true,
                    image_data: Some(&self.images),
                    ..Default::default()
                },
                &mut self.cache,
                &self.fonts,
            )
            .map_err(|error| error.to_string())
    }
}

/// Retains the Vulkan device, Skia surface, SVG conversion and drawing caches.
/// Struct field order releases the surface/context before its owning device.
pub struct Renderer {
    state: Option<State>,
    backend: Device,
    dimensions: [u32; 2],
    allocation_limit: u64,
    cache_limit: u64,
}
struct State {
    cache: render::RenderCache,
    conversion: Conversion,
    target: Option<backend::Target>,
    gpu: skia::gpu::DirectContext,
}
impl Renderer {
    /// Retain a complete image snapshot and release drawing-cache references to
    /// replaced resources. Unchanged snapshots preserve the warm caches.
    pub fn set_images(&mut self, images: Images, byte_limit: u64) -> Result<(), String> {
        let state = self.state.as_mut().expect("live renderer");
        if state.conversion.set_images(images, byte_limit)? {
            state.cache = render::RenderCache::new();
        }
        Ok(())
    }
    pub fn new(
        device_uuid: [u8; 16],
        dimensions: [u32; 2],
        fonts: impl IntoIterator<Item = Vec<u8>>,
        allocation_limit: u64,
        cache_limit: u64,
    ) -> Result<Self, String> {
        let [width, height] = dimensions;
        if width == 0 || height == 0 || width > 16_384 || height > 16_384 || allocation_limit == 0 {
            return Err("Invalid Motion render dimensions".into());
        }
        let mut backend = Device::new(device_uuid).map_err(|e| e.to_string())?;
        let mut gpu = backend::context(&mut backend)?;
        if cache_limit == 0 {
            return Err("Motion resource cache needs a nonzero allowance".into());
        }
        gpu.set_resource_cache_limit(
            cache_limit
                .try_into()
                .map_err(|_| "Cache allowance overflow")?,
        );
        Ok(Self {
            state: Some(State {
                target: None,
                gpu,
                cache: render::RenderCache::new(),
                conversion: Conversion::new(fonts),
            }),
            backend,
            dimensions,
            allocation_limit,
            cache_limit,
        })
    }
    /// Skia reports budgeted cache resources, excluding unbudgeted resources and
    /// driver/device overhead. Shared targets have a separate host reservation.
    pub fn cache_bytes(&self) -> u64 {
        self.state
            .as_ref()
            .expect("live renderer")
            .gpu
            .resource_cache_usage()
            .resource_bytes as u64
    }
    pub fn set_cache_limit(&mut self, limit: u64) -> Result<(), String> {
        if limit == 0 {
            return Err("Motion resource cache needs a nonzero allowance".into());
        }
        self.state
            .as_mut()
            .expect("live renderer")
            .gpu
            .set_resource_cache_limit(limit.try_into().map_err(|_| "Cache allowance overflow")?);
        self.cache_limit = limit;
        Ok(())
    }
    pub fn set_allocation_limit(&mut self, limit: u64) -> Result<(), String> {
        if limit == 0
            || self
                .state
                .as_ref()
                .and_then(|s| s.target.as_ref())
                .is_some_and(|target| target.allocation_bytes() > limit)
        {
            return Err("Motion target exceeds its allocation allowance".into());
        }
        self.allocation_limit = limit;
        Ok(())
    }
    pub fn draw(&mut self, frame: fframes::Svgr<'_>) -> Result<DrawTimings, String> {
        let state = self.state.as_mut().expect("live renderer");
        let start = Instant::now();
        let tree = state.conversion.evaluate(frame)?;
        let conversion_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        if state.target.is_none() {
            state.target = Some(backend::target(
                &mut self.backend,
                &mut state.gpu,
                self.dimensions,
                self.allocation_limit,
            )?);
        }
        let (commands_ms, submit_ms) = backend::queue(&mut self.backend, || {
            let surface = &mut state.target.as_mut().expect("created target").surface;
            surface.canvas().clear(skia::Color::TRANSPARENT);
            surface.canvas().save();
            let scale = (
                surface.width() as f32 / tree.size().width(),
                surface.height() as f32 / tree.size().height(),
            );
            surface.canvas().scale(scale);
            render::render_tree(&tree, surface.canvas(), &mut state.cache);
            surface.canvas().restore();
            let commands_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            state.gpu.flush_and_submit();
            (commands_ms, start.elapsed().as_secs_f64() * 1000.)
        });
        // Skia's limit purges unlocked resources. Locked resources may exceed it;
        // never publish such a frame. The worker discards the renderer on error.
        if self.cache_bytes() > self.cache_limit {
            if let Some(target) = self.state.as_mut().expect("live renderer").target.take() {
                backend::wait(&mut self.backend);
                backend::dispose(target, &mut self.backend);
            }
            return Err("Motion GPU resource cache exceeds its allowance".into());
        }
        Ok(DrawTimings {
            conversion_ms,
            commands_ms,
            submit_ms,
        })
    }
    /// Publish the current immutable target. The next draw allocates a new target
    /// while retaining the device, context, SVG conversion and drawing caches.
    pub fn publish(&mut self) -> Result<Export, String> {
        let state = self.state.as_mut().expect("live renderer");
        let target = state
            .target
            .take()
            .ok_or("Render a frame before publishing")?;
        backend::publish(target, &mut state.gpu, &mut self.backend)
    }

    /// Explicit qualification/readback path. Final preview interop must remain GPU-resident.
    pub fn readback(&mut self) -> Result<Vec<[f32; 4]>, String> {
        let state = self.state.as_mut().expect("live renderer");
        let surface = &mut state
            .target
            .as_mut()
            .ok_or("Render a frame before readback")?
            .surface;
        let width = surface.width();
        let info = skia::ImageInfo::new(
            (surface.width(), surface.height()),
            skia::ColorType::RGBAF32,
            skia::AlphaType::Premul,
            skia::ColorSpace::new_srgb_linear(),
        );
        let mut pixels = vec![[0f32; 4]; surface.width() as usize * surface.height() as usize];
        if !backend::queue(&mut self.backend, || {
            surface.read_pixels(
                &info,
                bytemuck::cast_slice_mut(&mut pixels),
                width as usize * 16,
                (0, 0),
            )
        }) {
            return Err("Motion float readback failed".into());
        }
        if pixels.iter().flatten().any(|v| !v.is_finite())
            || pixels.iter().any(|p| !(0. ..=1.).contains(&p[3]))
        {
            return Err("Motion output has invalid float color or alpha".into());
        }
        Ok(pixels)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(mut state) = self.state.take() {
            backend::queue(&mut self.backend, || state.gpu.flush_and_submit());
            backend::wait(&mut self.backend);
            if let Some(target) = state.target.take() {
                backend::dispose(target, &mut self.backend);
            }
            backend::queue(&mut self.backend, || drop(state));
        }
    }
}

pub mod presets;
