//! GPU execution of the logical graph. Native YUV reconstruction, OCIO,
//! float vector compositing and image operators remain GPU-resident. Native
//! sample transport and reference-compatible coverage are measured CPU adapters.
//! No renderer or device is initialized in CPU-only/headless builds by default.
mod cache;
pub use cache::PresentationCompressor;
mod color;
mod display;
mod fusion;
mod host;
mod preparation;
pub use display::Display;
#[cfg(all(feature = "native-video", target_os = "linux"))]
mod native_video;
mod validation;
mod vector;
mod yuv;
use crate::{Frame, ImageOp, RenderGraph, WorkingSpace, graph};
use host::Image;
pub use host::{Host, Memory};
use std::sync::{Arc, atomic::Ordering};
use wgpu::util::DeviceExt;

/// Transfer and adapter costs for one evaluation, excluding decoder-owned caches.
#[derive(Clone, Copy, Debug, Default)]
pub struct Statistics {
    pub upload_bytes: u64,
    pub lut_upload_bytes: u64,
    pub readback_bytes: u64,
    pub status_readback_bytes: u64,
    /// Full-frame CPU RGB/RGBA adapters. Native decode and CPU coverage are
    /// measured separately below; zero here does not mean zero CPU work.
    pub cpu_adapter_nodes: u32,
    pub cpu_adapter_nanoseconds: u128,
    pub compute_passes: u32,
    pub fused_opacity_passes: u32,
    pub graph_prepare_nanoseconds: u128,
    pub evaluate_cpu_nanoseconds: u128,
    pub output_cpu_nanoseconds: u128,
    pub queue_submit_nanoseconds: u128,
    pub graphics_queue_nanoseconds: u128,
    pub native_video_nodes: u32,
    pub resident_video_nodes: u32,
    pub vector_prepare_nanoseconds: u128,
    pub vector_upload_bytes: u64,
    pub native_decode_nanoseconds: u128,
    /// Inclusive helper call intervals, not isolated GPU codec/copy timestamps.
    pub native_codec_call_nanoseconds: u64,
    pub native_allocate_nanoseconds: u128,
    pub native_submit_nanoseconds: u128,
    pub native_seeks: u64,
    pub native_forward_reuses: u64,
    pub native_copy_ready_nanoseconds: u64,
    pub native_gpu_local_copy_bytes: u64,
    pub native_pipe_bytes: u64,
    pub native_upload_encode_nanoseconds: u128,
}

/// Immutable scene image. Readiness is queue completion, not submission. A frame
/// cannot be used with a different host, even if that host uses the same adapter.
pub struct GpuFrame {
    image: Arc<Image>,
    host: Host,
    ready: validation::Completion,
    working_space: WorkingSpace,
    config_identity: Option<String>,
    timing: Option<crate::frame::Timing>,
    lease_id: u64,
    pub statistics: Statistics,
}
impl GpuFrame {
    pub fn gpu_nanoseconds(&self) -> Result<Option<f64>, String> {
        self.ready.gpu_nanoseconds()
    }
    pub fn with_timing(
        mut self,
        timestamp: fold_foundation::Time,
        duration: fold_foundation::Time,
    ) -> Result<Self, String> {
        if duration <= fold_foundation::Time::ZERO {
            return Err("Frame duration must be positive".into());
        }
        self.timing = Some(crate::frame::Timing {
            timestamp,
            duration,
        });
        Ok(self)
    }
    pub fn descriptor(&self) -> Result<crate::frame::Descriptor, String> {
        use crate::frame::*;
        Ok(Descriptor {
            dimensions: self.dimensions(),
            pixel_aspect: [1, 1],
            planes: vec![Plane {
                offset: 0,
                row_stride: u64::from(self.dimensions()[0]) * 16,
                channels: 4,
                component: Component::Float32,
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
    pub fn dimensions(&self) -> [u32; 2] {
        self.image.dimensions
    }
    pub fn is_ready(&self) -> Result<bool, String> {
        self.host.check()?;
        self.ready.ready()
    }
    /// Explicit worker-only reference/export boundary. Never call from a UI
    /// callback. Readback is accounted, timeout bounded, and cancellation checked
    /// on both sides of GPU completion (submitted GPU work is not preempted).
    pub fn readback(&mut self, cancel: &fold_media::Cancel) -> Result<Frame, String> {
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
        let stride = (width * 16).div_ceil(256) * 256;
        let size = u64::from(stride) * u64::from(height);
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
            for bytes in row[..width as usize * 16].chunks_exact(16) {
                let pixel = std::array::from_fn(|c| {
                    f32::from_le_bytes(bytes[c * 4..c * 4 + 4].try_into().unwrap())
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

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    header: [u32; 4],
    color: [f32; 4],
    matrix: [f32; 4],
    offset: [f32; 4],
    rect: [u32; 4],
    grade: [[f32; 4]; 7],
}

/// Worker-owned pipeline state, sharing host allocations with other workers.
/// Explicit selection: constructing this renderer never changes CPU entry points.
pub struct Renderer {
    host: Host,
    pipeline: wgpu::ComputePipeline,
    data_display: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    colors: std::collections::BTreeMap<(String, bool), Arc<color::ColorPipeline>>,
    planes: wgpu::ComputePipeline,
    yuv: yuv::YuvPipeline,
    vector: vector::VectorPipeline,
    #[cfg(all(feature = "native-video", target_os = "linux"))]
    native_video: Option<native_video::NativeVideo>,
}
impl Renderer {
    pub fn new(host: Host) -> Result<Self, String> {
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
            source: wgpu::ShaderSource::Wgsl(include_str!("operators.wgsl").into()),
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
            data_display,
            host,
            pipeline,
            yuv,
            vector,
            #[cfg(all(feature = "native-video", target_os = "linux"))]
            native_video: None,
            layout,
            planes,
            colors: Default::default(),
        })
    }
    fn pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        [first, second, mask]: [&Image; 3],
        output: &Image,
        params: Params,
        status: &validation::Status,
    ) {
        let uniform = self
            .host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Fold operator parameters"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self
            .host
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&first.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&second.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&mask.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&output.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: status.gpu.as_entire_binding(),
                    },
                ],
            });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            output.dimensions[0].div_ceil(8),
            output.dimensions[1].div_ceil(8),
            1,
        );
    }
    fn color_pipeline(
        &mut self,
        processor: Option<&fold_color::Processor>,
        display: bool,
    ) -> Result<(Arc<color::ColorPipeline>, u64), String> {
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

    /// Evaluate without working-image readback. ACES video preserves native YUV
    /// until GPU reconstruction; drawing coverage is composed into float color on
    /// the GPU. Legacy CPU image adapters remain explicit. Unsupported work fails.
    pub fn evaluate(
        &mut self,
        graph: RenderGraph,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
    ) -> Result<GpuFrame, String> {
        self.evaluate_admitted(graph, config, decoder, cancel, None)
    }

    /// Source preparation holds no graphics permit. Admission covers recording
    /// and submission only; queued/held inputs retain their allocation leases.
    pub fn evaluate_scheduled(
        &mut self,
        graph: RenderGraph,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
        class: crate::scheduling::Class,
    ) -> Result<GpuFrame, String> {
        self.evaluate_admitted(graph, config, decoder, cancel, Some(class))
    }

    fn evaluate_admitted(
        &mut self,
        graph: RenderGraph,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
        class: Option<crate::scheduling::Class>,
    ) -> Result<GpuFrame, String> {
        cancel.check()?;
        self.host.poll()?;
        // Retained inputs are expendable under host pressure; displayed and
        // submitted resources remain protected by their independent leases.
        let memory = self.host.memory();
        if memory.allocated > memory.budget.saturating_mul(3) / 4 {
            self.yuv.clear();
            self.vector.clear();
            #[cfg(all(feature = "native-video", target_os = "linux"))]
            if let Some(native) = self.native_video.as_mut() {
                native.clear();
            }
        }
        let evaluate_begin = std::time::Instant::now();
        let aces = config.is_some();
        graph::validate(&graph, aces)?;
        let processors = graph::input_processors(&graph, config)?;
        let (mut needed, mut uses) = graph::dependencies(&graph);
        let fused = fusion::opacity_over(&graph, &mut needed, &mut uses);
        let prepare_nanoseconds = evaluate_begin.elapsed().as_nanos();
        let mut stats = Statistics {
            graph_prepare_nanoseconds: prepare_nanoseconds,
            ..Default::default()
        };
        let mut video = if aces {
            self.prepare_video(&graph, &needed, decoder, cancel, &mut stats)?
        } else {
            Vec::new()
        };
        let begin = std::time::Instant::now();
        let _permit = class
            .map(|class| crate::scheduling::Scheduler::shared().enter(class, cancel))
            .transpose()?;
        stats.graphics_queue_nanoseconds = begin.elapsed().as_nanos();
        let dimensions = [graph.width, graph.height];
        // At most two parameter buffers per reachable node (separable blur).
        // Reserve before recording so even a long graph is aggregate bounded.
        let parameter_bytes =
            needed.iter().filter(|&&v| v).count() as u64 * 2 * std::mem::size_of::<Params>() as u64;
        let mut buffers = vec![self.host.reserve(parameter_bytes)?];
        let dummy = self.host.image([1, 1])?;
        let mut leases = vec![dummy.clone()];
        let mut frames: Vec<Option<Arc<Image>>> = vec![None; graph.nodes.len()];
        // Scratch reuse within ONE command stream is safe after its last logical
        // read: distinct passes provide barriers. It does not escape to the host
        // pool until the entire submission completes.
        let mut scratch = Vec::<Arc<Image>>::new();
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        let status = validation::Status::new(&self.host)?;
        status.start(&mut encoder);
        stats.status_readback_bytes = status.readback_bytes();
        for (id, op) in graph.nodes.iter().enumerate() {
            if !needed[id] {
                continue;
            }
            cancel.check()?;
            let output = match scratch.pop() {
                Some(image) => image,
                None => {
                    let image = self.host.image(dimensions)?;
                    leases.push(image.clone());
                    image
                }
            };
            let mut params = Params {
                header: [0, graph.width, graph.height, u32::from(aces)],
                ..Default::default()
            };
            let mut logical_inputs = op.inputs();
            let mut inputs = [
                logical_inputs.next(),
                logical_inputs.next(),
                logical_inputs.next(),
            ];
            if let Some((input, _)) = fused[id] {
                inputs[0] = Some(input);
            }
            let first = inputs[0].map_or(&dummy, |i| frames[i].as_ref().unwrap());
            let second = inputs[1].map_or(&dummy, |i| frames[i].as_ref().unwrap());
            let mask = inputs[2].map_or(&dummy, |i| frames[i].as_ref().unwrap());
            let mut compute = true;
            match op {
                ImageOp::Unary { operation, .. } => {
                    use crate::operations::Unary;
                    params.header[0] = 17;
                    params.rect[0] = match *operation {
                        Unary::Exposure { stops } => {
                            params.color[0] = stops.exp2();
                            0
                        }
                        Unary::Invert => 1,
                        Unary::Clamp { minimum, maximum } => {
                            params.color[0] = minimum;
                            params.color[1] = maximum;
                            2
                        }
                        Unary::Premult => 3,
                        Unary::Unpremult => 4,
                    };
                }
                ImageOp::Merge { mode, .. } => {
                    params.header[0] = 12;
                    params.rect[0] = *mode as u32;
                }
                ImageOp::ColorGrade { settings, .. } => {
                    params.header[0] = 13;
                    params.rect[0] = u32::from(settings.unpremultiply);
                    params.rect[1] = u32::from(settings == &crate::operations::Grade::default());
                    for (target, value) in params.grade.iter_mut().zip([
                        settings.black,
                        settings.white,
                        settings.lift,
                        settings.gain,
                        settings.multiply,
                        settings.offset,
                        settings.gamma,
                    ]) {
                        target[..3].copy_from_slice(&value);
                    }
                }
                ImageOp::Shuffle { mapping, .. } => {
                    params.header[0] = 10;
                    params.rect = mapping.map(u32::from);
                }
                ImageOp::Mix {
                    mask,
                    mask_channel,
                    invert,
                    amount,
                    channels,
                    ..
                } => {
                    params.header[0] = 11;
                    params.color[0] = *amount;
                    params.rect = [
                        u32::from(mask.is_some()),
                        u32::from(*mask_channel),
                        u32::from(*invert),
                        0,
                    ];
                    params.matrix = channels.map(|enabled| if enabled { 1. } else { 0. });
                }
                ImageOp::Solid { rgba } => {
                    params.color = if aces && rgba[3] == 0. {
                        [0.; 4]
                    } else {
                        *rgba
                    };
                }
                ImageOp::Opacity { opacity, .. } => {
                    params.header[0] = 1;
                    params.color[0] = *opacity;
                }
                ImageOp::Over { .. } => {
                    if let Some((_, opacity)) = fused[id] {
                        params.header[0] = 9;
                        params.color[0] = opacity;
                        stats.fused_opacity_passes += 1;
                    } else {
                        params.header[0] = 2;
                    }
                }
                ImageOp::Crop { rect, .. } => {
                    params.header[0] = 3;
                    params.rect = *rect;
                }
                ImageOp::Grade { gain, .. } => {
                    params.header[0] = 4;
                    params.color[..3].copy_from_slice(gain);
                }
                ImageOp::Mask { .. } => params.header[0] = 5,
                ImageOp::Resample { transform, .. } | ImageOp::Transform { transform, .. } => {
                    let m = transform.inverse()?;
                    params.header[0] = 6;
                    if let ImageOp::Resample { filter, .. } = op {
                        params.header[0] = 14;
                        params.rect[0] = *filter as u32;
                    }
                    params.matrix = [m.a as f32, m.b as f32, m.c as f32, m.d as f32];
                    params.offset = [m.tx as f32, m.ty as f32, 0., 0.];
                    if params
                        .matrix
                        .iter()
                        .chain(&params.offset)
                        .any(|x| !x.is_finite())
                    {
                        return Err(
                            "transform inverse exceeds GPU float32 range; select CPU backend"
                                .into(),
                        );
                    }
                }
                ImageOp::Gaussian { .. } | ImageOp::Blur { .. } => {
                    let intermediate = match scratch.pop() {
                        Some(image) => image,
                        None => {
                            let image = self.host.image(dimensions)?;
                            leases.push(image.clone());
                            image
                        }
                    };
                    params.header[0] = 7;
                    if let ImageOp::Blur { radius, .. } = op {
                        params.rect[0] = *radius;
                    }
                    if let ImageOp::Gaussian { size, edges, .. } = op {
                        params.header[0] = 15;
                        params.color[0] = size[0];
                        params.rect[1] = *edges as u32;
                    }
                    self.pass(
                        &mut encoder,
                        [first, &dummy, &dummy],
                        &intermediate,
                        params,
                        &status,
                    );
                    params.header[0] = 8;
                    if let ImageOp::Gaussian { size, .. } = op {
                        params.header[0] = 16;
                        params.color[0] = size[1];
                    }
                    self.pass(
                        &mut encoder,
                        [&intermediate, &dummy, &dummy],
                        &output,
                        params,
                        &status,
                    );
                    stats.compute_passes += 2;
                    scratch.push(intermediate);
                    compute = false;
                }
                ImageOp::Vector(drawings) if aces => {
                    let (bytes, elapsed) = self.vector.encode(
                        &mut encoder,
                        drawings,
                        &output,
                        &status,
                        &mut buffers,
                        cancel,
                    )?;
                    stats.upload_bytes += bytes;
                    stats.vector_upload_bytes += bytes;
                    stats.vector_prepare_nanoseconds += elapsed;
                    stats.compute_passes += 1;
                    compute = false;
                }
                ImageOp::Video { .. } | ImageOp::VideoInput { .. } if aces => {
                    stats.native_video_nodes += 1;
                    let encoded = match scratch.pop() {
                        Some(image) => image,
                        None => {
                            let image = self.host.image(dimensions)?;
                            leases.push(image.clone());
                            image
                        }
                    };
                    match video[id].take().ok_or("missing prepared video input")? {
                        #[cfg(all(feature = "native-video", target_os = "linux"))]
                        preparation::Video::Native(input) => {
                            stats.resident_video_nodes += 1;
                            buffers.push(input.reservation.clone());
                            let begin = std::time::Instant::now();
                            self.yuv.encode_external(
                                &self.host,
                                &mut encoder,
                                &input.buffer,
                                input.layout,
                                &encoded,
                                &mut buffers,
                            )?;
                            stats.native_upload_encode_nanoseconds += begin.elapsed().as_nanos();
                        }
                        preparation::Video::Planes(frame) => {
                            let begin = std::time::Instant::now();
                            stats.upload_bytes += self.yuv.encode(
                                &self.host,
                                &mut encoder,
                                &frame,
                                &encoded,
                                &mut buffers,
                            )?;
                            stats.native_upload_encode_nanoseconds += begin.elapsed().as_nanos();
                        }
                    }
                    let space = match op {
                        ImageOp::VideoInput { space, .. } => space.as_str(),
                        _ => fold_color::settings::VIDEO_INPUT,
                    };
                    let (pipeline, uploaded) =
                        self.color_pipeline(Some(&processors[space]), false)?;
                    stats.lut_upload_bytes += uploaded;
                    let intermediate = pipeline.encode(
                        &self.host,
                        &mut encoder,
                        &encoded,
                        &output,
                        false,
                        &status,
                    )?;
                    leases.push(intermediate.clone());
                    scratch.push(intermediate);
                    scratch.push(encoded);
                    buffers.push(pipeline.reservation.clone());
                    stats.compute_passes += 3;
                    compute = false;
                }
                ImageOp::Exr { .. }
                | ImageOp::Media(_)
                | ImageOp::Video { .. }
                | ImageOp::VideoInput { .. }
                | ImageOp::Vector(_) => {
                    let begin = std::time::Instant::now();
                    let mut input_space = None;
                    let mut native_planes = None;
                    let mut exr_pixels;
                    let pixels: Vec<[f32; 4]> = if let ImageOp::Exr {
                        source,
                        channels,
                        space,
                    } = op
                    {
                        exr_pixels = fold_media::exr::decode(
                            source,
                            channels.each_ref().map(|c| c.as_deref()),
                            dimensions,
                            cancel,
                        )?;
                        input_space = space.as_deref();
                        std::mem::take(&mut exr_pixels.pixels)
                    } else if aces && !matches!(op, ImageOp::Vector(_)) {
                        let decoded;
                        let source = match op {
                            ImageOp::Media(source) => source,
                            ImageOp::Video { source, time }
                            | ImageOp::VideoInput { source, time, .. } => {
                                decoded =
                                    decoder.decode_signal(source, *time, dimensions, cancel)?;
                                &decoded
                            }
                            _ => unreachable!(),
                        };
                        input_space = Some(match op {
                            ImageOp::VideoInput { space, .. } => space.as_str(),
                            _ if source.is_signal() => fold_color::settings::VIDEO_INPUT,
                            _ => fold_color::settings::SRGB_INPUT,
                        });
                        if source.native_gbr_planes().is_some() {
                            native_planes = Some(source.clone());
                            Vec::new()
                        } else {
                            source.encoded_pixels().collect()
                        }
                    } else {
                        graph::evaluate(
                            RenderGraph {
                                width: graph.width,
                                height: graph.height,
                                nodes: vec![op.clone()],
                                output: 0,
                            },
                            decoder,
                            cancel,
                            128 * 1024 * 1024,
                            config,
                        )?
                        .pixels
                    };
                    stats.cpu_adapter_nanoseconds += begin.elapsed().as_nanos();
                    stats.cpu_adapter_nodes += 1;
                    // Upload through an encoded copy, NOT queue.write_texture:
                    // queue writes run before the whole submission and would
                    // overwrite a scratch image still read by an earlier pass.
                    let stride = (graph.width * 16).div_ceil(256) * 256;
                    let upload_bytes = native_planes
                        .as_ref()
                        .map_or(u64::from(stride) * u64::from(graph.height), |p| {
                            p.storage_bytes()
                        });
                    buffers.push(self.host.reserve(upload_bytes)?);
                    let mut bytes = Vec::new();
                    if native_planes.is_none() {
                        bytes
                            .try_reserve_exact(upload_bytes as usize)
                            .map_err(|_| "source upload CPU allocation failed")?;
                        bytes.resize(upload_bytes as usize, 0u8);
                        for (row, pixels) in bytes
                            .chunks_exact_mut(stride as usize)
                            .zip(pixels.chunks_exact(graph.width as usize))
                        {
                            row[..graph.width as usize * 16]
                                .copy_from_slice(bytemuck::cast_slice(pixels));
                        }
                    }
                    let contents: &[u8] = match &native_planes {
                        Some(p) => bytemuck::cast_slice(p.native_gbr_planes().unwrap()),
                        None => &bytes,
                    };
                    let encoded = if input_space.is_some() {
                        let image = match scratch.pop() {
                            Some(image) => image,
                            None => {
                                let image = self.host.image(dimensions)?;
                                leases.push(image.clone());
                                image
                            }
                        };
                        Some(image)
                    } else {
                        None
                    };
                    let upload =
                        self.host
                            .device()
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Fold CPU adapter upload"),
                                contents,
                                usage: if native_planes.is_some() {
                                    wgpu::BufferUsages::STORAGE
                                } else {
                                    wgpu::BufferUsages::COPY_SRC
                                },
                            });
                    if native_planes.is_some() {
                        let target = encoded.as_ref().unwrap_or(&output);
                        let group =
                            self.host
                                .device()
                                .create_bind_group(&wgpu::BindGroupDescriptor {
                                    label: None,
                                    layout: &self.planes.get_bind_group_layout(0),
                                    entries: &[
                                        wgpu::BindGroupEntry {
                                            binding: 0,
                                            resource: upload.as_entire_binding(),
                                        },
                                        wgpu::BindGroupEntry {
                                            binding: 1,
                                            resource: wgpu::BindingResource::TextureView(
                                                &target.view,
                                            ),
                                        },
                                    ],
                                });
                        let mut pass = encoder.begin_compute_pass(&Default::default());
                        pass.set_pipeline(&self.planes);
                        pass.set_bind_group(0, &group, &[]);
                        pass.dispatch_workgroups(
                            graph.width.div_ceil(8),
                            graph.height.div_ceil(8),
                            1,
                        );
                        stats.compute_passes += 1;
                    } else {
                        encoder.copy_buffer_to_texture(
                            wgpu::TexelCopyBufferInfo {
                                buffer: &upload,
                                layout: wgpu::TexelCopyBufferLayout {
                                    offset: 0,
                                    bytes_per_row: Some(stride),
                                    rows_per_image: Some(graph.height),
                                },
                            },
                            encoded.as_ref().unwrap_or(&output).texture.as_image_copy(),
                            output.texture.size(),
                        );
                    }
                    if let Some(space) = input_space {
                        let (pipeline, uploaded) =
                            self.color_pipeline(Some(&processors[space]), false)?;
                        stats.lut_upload_bytes += uploaded;
                        let encoded = encoded.unwrap();
                        let intermediate = pipeline.encode(
                            &self.host,
                            &mut encoder,
                            &encoded,
                            &output,
                            false,
                            &status,
                        )?;
                        leases.push(intermediate.clone());
                        scratch.push(intermediate);
                        buffers.push(pipeline.reservation.clone());
                        stats.compute_passes += 2;
                        scratch.push(encoded);
                    }
                    stats.upload_bytes += upload_bytes;
                    compute = false;
                }
            }
            if compute {
                self.pass(
                    &mut encoder,
                    [first, second, mask],
                    &output,
                    params,
                    &status,
                );
                stats.compute_passes += 1;
            }
            frames[id] = Some(output);
            for input in inputs.into_iter().flatten() {
                uses[input] -= 1;
                if uses[input] == 0 {
                    scratch.push(frames[input].take().unwrap());
                }
            }
        }
        cancel.check()?;
        self.host.check()?;
        status.encode(&mut encoder);
        let submit_begin = std::time::Instant::now();
        self.host.submit(encoder, leases, buffers);
        stats.queue_submit_nanoseconds = submit_begin.elapsed().as_nanos();
        let ready = status.submitted();
        stats.evaluate_cpu_nanoseconds = evaluate_begin.elapsed().as_nanos();
        cancel.check()?;
        self.host.check()?;
        Ok(GpuFrame {
            image: frames[graph.output].take().unwrap(),
            host: self.host.clone(),
            ready,
            working_space: if aces {
                WorkingSpace::AcesCg
            } else {
                WorkingSpace::LegacyLinearSrgb
            },
            config_identity: config.map(|c| c.identity().content_sha256.clone()),
            timing: None,
            lease_id: host::NEXT_ID.fetch_add(1, Ordering::Relaxed),
            statistics: stats,
        })
    }
}
