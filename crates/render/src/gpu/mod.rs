//! GPU execution of the existing logical graph. Decoder-native GBR packing,
//! OCIO input/output processing and image operators are GPU-resident. CPU decode,
//! YUV reconstruction and vector coverage remain explicit measured adapters.
//! No renderer or device is initialized in CPU-only/headless builds by default.
mod color;
mod display;
mod host;
pub use display::Display;
mod validation;
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
    pub cpu_adapter_nodes: u32,
    pub cpu_adapter_nanoseconds: u128,
    pub compute_passes: u32,
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
}

/// Worker-owned pipeline state, sharing host allocations with other workers.
/// Explicit selection: constructing this renderer never changes CPU entry points.
pub struct Renderer {
    host: Host,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    colors: std::collections::BTreeMap<(String, bool), Arc<color::ColorPipeline>>,
    planes: wgpu::ComputePipeline,
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
        host.check()?;
        Ok(Self {
            host,
            pipeline,
            layout,
            planes,
            colors: Default::default(),
        })
    }
    fn pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        first: &Image,
        second: &Image,
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

    /// Evaluate without working-image readback. Source decode/reconstruction and
    /// vector rasterization use bounded CPU adapters; native float-plane packing
    /// and OCIO input processing run on the GPU. Unsupported work fails.
    pub fn evaluate(
        &mut self,
        graph: RenderGraph,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
    ) -> Result<GpuFrame, String> {
        cancel.check()?;
        self.host.poll()?;
        let aces = config.is_some();
        graph::validate(&graph, aces)?;
        let processors = graph::input_processors(&graph, config)?;
        let (needed, mut uses) = graph::dependencies(&graph);
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
        let mut stats = Statistics {
            status_readback_bytes: status.readback_bytes(),
            ..Default::default()
        };
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
            let mut inputs = op.inputs();
            let first = inputs
                .next()
                .map_or(&dummy, |i| frames[i].as_ref().unwrap());
            let second = inputs
                .next()
                .map_or(&dummy, |i| frames[i].as_ref().unwrap());
            let mut compute = true;
            match op {
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
                ImageOp::Over { .. } => params.header[0] = 2,
                ImageOp::Crop { rect, .. } => {
                    params.header[0] = 3;
                    params.rect = *rect;
                }
                ImageOp::Grade { gain, .. } => {
                    params.header[0] = 4;
                    params.color[..3].copy_from_slice(gain);
                }
                ImageOp::Mask { .. } => params.header[0] = 5,
                ImageOp::Transform { transform, .. } => {
                    let m = transform.inverse()?;
                    params.header[0] = 6;
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
                ImageOp::Blur { radius, .. } => {
                    let intermediate = match scratch.pop() {
                        Some(image) => image,
                        None => {
                            let image = self.host.image(dimensions)?;
                            leases.push(image.clone());
                            image
                        }
                    };
                    params.header[0] = 7;
                    params.rect[0] = *radius;
                    self.pass(&mut encoder, first, &dummy, &intermediate, params, &status);
                    params.header[0] = 8;
                    self.pass(
                        &mut encoder,
                        &intermediate,
                        &dummy,
                        &output,
                        params,
                        &status,
                    );
                    stats.compute_passes += 2;
                    scratch.push(intermediate);
                    compute = false;
                }
                ImageOp::Media(_)
                | ImageOp::Video { .. }
                | ImageOp::VideoInput { .. }
                | ImageOp::Vector(_) => {
                    let begin = std::time::Instant::now();
                    let mut input_space = None;
                    let mut native_planes = None;
                    let pixels: Vec<[f32; 4]> = if aces && !matches!(op, ImageOp::Vector(_)) {
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
                self.pass(&mut encoder, first, second, &output, params, &status);
                stats.compute_passes += 1;
            }
            frames[id] = Some(output);
            for input in op.inputs() {
                uses[input] -= 1;
                if uses[input] == 0 {
                    scratch.push(frames[input].take().unwrap());
                }
            }
        }
        cancel.check()?;
        self.host.check()?;
        status.encode(&mut encoder);
        self.host.submit(encoder, leases, buffers);
        let ready = status.submitted();
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
