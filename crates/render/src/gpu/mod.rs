//! GPU execution of the logical graph. Native YUV reconstruction, OCIO,
//! float vector compositing and image operators remain GPU-resident. Native
//! sample transport remains a measured CPU adapter where required.
//! No renderer or device is initialized in CPU-only/headless builds by default.
mod encoding;
mod frame;
mod request;
mod statistics;
mod submission;
pub use frame::GpuFrame;
pub use statistics::Statistics;
mod presentation;
pub use presentation::PresentationTransfer;
mod cache;
pub use cache::PresentationCompressor;
mod baked;
mod color;
mod delivery;
mod display;
mod filtered_blur;
mod fusion;
mod host;
mod pointwise;
mod precision;
mod preparation;
mod sampling;
mod shader;
#[cfg(feature = "spatial")]
pub mod spatial;
mod still;
mod upload;
pub use display::Display;
mod coverage;
#[cfg(all(feature = "native-video", target_os = "linux"))]
mod native_video;
mod validation;
mod vector;
mod yuv;
use crate::{ImageOp, RenderGraph, WorkingSpace, graph};
use host::Image;
pub use host::{Host, Memory};
use std::sync::Arc;

/// Worker-owned pipeline state, sharing host allocations with other workers.
/// Explicit selection: constructing this renderer never changes CPU entry points.
pub struct Renderer {
    uploads: upload::Pool,
    host: Host,
    pipeline: wgpu::ComputePipeline,
    data_display: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    colors: std::collections::BTreeMap<(String, bool), Arc<color::ColorPipeline>>,
    planes: wgpu::ComputePipeline,
    yuv: yuv::YuvPipeline,
    vector: vector::VectorPipeline,
    coverage: coverage::Coverage,
    filtered_blur: filtered_blur::FilteredBlur,
    sampling: sampling::Sampling,
    baked_sampling: baked::Sampling,
    baked: baked::Cache,
    shaders: shader::Cache,
    precision: Option<(bool, wgpu::ComputePipeline)>,
    delivery: Option<wgpu::ComputePipeline>,
    secondary_decoder: Option<fold_media::Decoder>,
    pointwise: pointwise::Pipeline,
    #[cfg(feature = "spatial")]
    spatial: Option<spatial::Adapter>,
    #[cfg(all(feature = "native-video", target_os = "linux"))]
    native_video: Option<native_video::NativeVideo>,
}
impl Renderer {
    pub fn new(host: Host) -> Result<Self, fold_foundation::Failure> {
        host.check()?;
        let device = host.device();
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fold image operators"),
            entries: &[
                texture(0),
                texture(1),
                texture(5),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fold image operators"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("pointwise.wgsl"),
                    include_str!("operators.wgsl")
                )
                .into(),
            ),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Fold image operators"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let plane_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native GBR planes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("planes.wgsl").into()),
        });
        let planes = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Native GBR planes"),
            layout: None,
            module: &plane_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let filtered_blur = filtered_blur::FilteredBlur::new(&host);
        let sampling = sampling::Sampling::new(&host);
        let baked_sampling = baked::Sampling::new(&host);
        let yuv = yuv::YuvPipeline::new(&host);
        let vector = vector::VectorPipeline::new(&host);
        host.check()?;
        let data_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Data channel visualization"),
            source: wgpu::ShaderSource::Wgsl(include_str!("data_display.wgsl").into()),
        });
        let data_display = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Data channel visualization"),
            layout: None,
            module: &data_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            uploads: Default::default(),
            data_display,
            filtered_blur,
            sampling,
            baked_sampling,
            baked: Default::default(),
            shaders: Default::default(),
            precision: None,
            delivery: None,
            secondary_decoder: None,
            pointwise: pointwise::Pipeline::new(&host),
            #[cfg(feature = "spatial")]
            spatial: None,
            host,
            pipeline,
            yuv,
            vector,
            coverage: Default::default(),
            #[cfg(all(feature = "native-video", target_os = "linux"))]
            native_video: None,
            layout,
            planes,
            colors: Default::default(),
        })
    }
    fn color_pipeline(
        &mut self,
        processor: Option<&fold_color::Processor>,
        display: bool,
    ) -> Result<(Arc<color::ColorPipeline>, u64), fold_foundation::Failure> {
        let key = (
            processor
                .map_or("legacy-srgb-v1", |p| p.identity())
                .to_owned(),
            display,
        );
        if let Some(pipeline) = self.colors.get(&key) {
            return Ok((pipeline.clone(), 0));
        }
        if self.colors.len() >= 16 {
            self.colors.clear();
        }
        let pipeline = Arc::new(color::ColorPipeline::new(&self.host, processor, display)?);
        let bytes = pipeline.upload_bytes;
        self.colors.insert(key, pipeline.clone());
        Ok((pipeline, bytes))
    }
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// Evaluate the dependency footprint and extract the requested scene region.
    pub fn evaluate_region(
        &mut self,
        graph: RenderGraph,
        region: crate::region::Region,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
        class: Option<crate::scheduling::Class>,
    ) -> Result<GpuFrame, fold_foundation::Failure> {
        let plan = crate::region::Plan::new(graph, region, config.is_some())?;
        let mut frame = self.evaluate_admitted(plan.graph, config, decoder, cancel, class)?;
        if plan.output == crate::region::Region::full(frame.dimensions()) {
            return Ok(frame);
        }
        let _permit = class
            .map(|c| crate::scheduling::Scheduler::shared().enter(c, cancel))
            .transpose()?;
        cancel.check()?;
        let output = self
            .host
            .image_format(plan.output.dimensions(), frame.image.texture.format())?;
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        let mut source = frame.image.texture.as_image_copy();
        source.origin = wgpu::Origin3d {
            x: plan.output.x,
            y: plan.output.y,
            z: 0,
        };
        // Append a completion after the copy; readiness covers both operator
        // validation and extraction without waiting on the UI thread.
        let status = validation::Status::new(&self.host)?;
        status.start(&mut encoder);
        encoder.copy_texture_to_texture(
            source,
            output.texture.as_image_copy(),
            output.texture.size(),
        );
        status.encode(&mut encoder);
        frame.statistics.status_readback_bytes += status.readback_bytes();
        self.host
            .submit(encoder, vec![frame.image.clone(), output.clone()], vec![]);
        frame.dependencies.push(frame.ready.clone());
        frame.ready = status.submitted();
        frame.image = output;
        Ok(frame)
    }

    /// Evaluate without working-image readback. ACES video preserves native YUV
    /// until GPU reconstruction; drawing coverage is composed into float color on
    /// the GPU. Legacy CPU image adapters remain explicit. Unsupported work fails.
    pub fn evaluate(
        &mut self,
        graph: RenderGraph,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
    ) -> Result<GpuFrame, fold_foundation::Failure> {
        self.evaluate_admitted(graph, config, decoder, cancel, None)
    }

    /// Source preparation and recording hold no graphics permit. Admission
    /// orders submission only; queued/held inputs retain their allocation leases.
    pub fn evaluate_scheduled(
        &mut self,
        graph: RenderGraph,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
        class: crate::scheduling::Class,
    ) -> Result<GpuFrame, fold_foundation::Failure> {
        self.evaluate_admitted(graph, config, decoder, cancel, Some(class))
    }

    pub(super) fn evaluate_admitted(
        &mut self,
        graph: RenderGraph,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
        class: Option<crate::scheduling::Class>,
    ) -> Result<GpuFrame, fold_foundation::Failure> {
        let request = self.prepare_request(graph, config, decoder, cancel, class)?;
        let encoded = self.encode_request(request, config, decoder, cancel)?;
        self.submit_request(encoded, config, cancel, class)
    }
}

mod capture;
