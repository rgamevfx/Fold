//! Image operation encoding and command-stream resource lifetimes.
use super::*;
use wgpu::util::DeviceExt;
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

pub(super) struct Encoded {
    pub encoder: wgpu::CommandEncoder,
    pub leases: Vec<Arc<Image>>,
    pub buffers: Vec<Arc<host::Reservation>>,
    pub status: validation::Status,
    pub image: Arc<Image>,
    pub dependencies: Vec<validation::Completion>,
    pub retained: Vec<(still::Key, Arc<Image>)>,
    pub stats: Statistics,
    pub aces: bool,
    pub evaluate_begin: std::time::Instant,
    pub uploads: Vec<upload::Upload>,
}
impl Renderer {
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
    pub(super) fn encode_request(
        &mut self,
        request: request::Prepared,
        config: Option<&fold_color::Config>,
        decoder: &mut fold_media::Decoder,
        cancel: &fold_media::Cancel,
    ) -> Result<Encoded, fold_foundation::Failure> {
        let request::Prepared {
            graph,
            aces,
            processors,
            needed,
            mut uses,
            fused,
            mut stats,
            mut sources,
            dimensions,
            mut keys,
            mut resident,
            mut exr,
            dependencies,
            mut video,
            evaluate_begin,
        } = request;
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
        let mut uploads = Vec::new();
        let mut immutable = vec![false; graph.nodes.len()];
        let mut retained = Vec::new();
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        let status = validation::Status::new(&self.host)?;
        status.start(&mut encoder);
        stats.status_readback_bytes += status.readback_bytes();
        for (id, op) in graph.nodes.iter().enumerate() {
            if !needed[id] {
                continue;
            }
            cancel.check()?;
            if !aces && matches!(op, ImageOp::Spatial { .. }) {
                let source = sources[id]
                    .take()
                    .ok_or("Missing prepared spatial source")?;
                leases.push(source.image.clone());
                frames[id] = Some(source.image);
                immutable[id] = true;
                continue;
            }
            let still_key = keys[id].take();
            if let Some(image) = resident[id].take() {
                leases.push(image.clone());
                frames[id] = Some(image);
                immutable[id] = true;
                stats.resident_still_nodes += 1;
                continue;
            }
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
                ImageOp::Spatial { .. } => {
                    let source = sources[id]
                        .take()
                        .ok_or("Missing prepared spatial source")?;
                    let (pipeline, uploaded) =
                        self.color_pipeline(Some(&processors[fold_color::LINEAR_SRGB]), false)?;
                    stats.lut_upload_bytes += uploaded;
                    let intermediate = pipeline.encode(
                        &self.host,
                        &mut encoder,
                        &source.image,
                        &output,
                        false,
                        &status,
                    )?;
                    leases.push(source.image);
                    leases.push(intermediate.clone());
                    scratch.push(intermediate);
                    buffers.push(pipeline.reservation.clone());
                    stats.compute_passes += 2;
                    compute = false;
                }
                ImageOp::Shader { inputs, program } => {
                    let images: Vec<_> = inputs
                        .iter()
                        .map(|id| frames[*id].as_ref().unwrap().as_ref())
                        .collect();
                    let before = self.shaders.compilations;
                    buffers.push(self.shaders.encode(
                        &self.host,
                        &mut encoder,
                        program,
                        &images,
                        &output,
                        &status,
                    )?);
                    stats.shader_compilations += self.shaders.compilations - before;
                    stats.compute_passes += 1;
                    compute = false;
                }

                ImageOp::WindowedSource {
                    region, dimensions, ..
                } => {
                    let source = sources[id].take().ok_or("missing prepared region source")?;
                    if source.dimensions() == *dimensions {
                        let mut copy = source.image.texture.as_image_copy();
                        copy.origin = wgpu::Origin3d {
                            x: region.x,
                            y: region.y,
                            z: 0,
                        };
                        encoder.copy_texture_to_texture(
                            copy,
                            output.texture.as_image_copy(),
                            output.texture.size(),
                        );
                    } else {
                        let rows =
                            crate::sampling::rows(source.dimensions()[1], dimensions[1], *region);
                        let intermediate = self.host.image([region.width, rows[1] - rows[0]])?;
                        buffers.push(self.sampling.encode(
                            &self.host,
                            &mut encoder,
                            [&source.image, &intermediate, &output],
                            *dimensions,
                            *region,
                            &status,
                        )?);
                        leases.push(intermediate);
                        stats.compute_passes += 2;
                    }
                    leases.push(source.image);
                    compute = false;
                }
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
                    params.rect[1] = u32::from(settings.invert);
                    params.rect[2] = u32::from(settings.black_clamp);
                    params.rect[3] = u32::from(settings.white_clamp);
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
                    if let ImageOp::Resample {
                        filter,
                        edges,
                        clamp,
                        ..
                    } = op
                    {
                        params.header[0] = 14;
                        params.rect[0] = *filter as u32;
                        params.rect[1] = *edges as u32;
                        params.rect[2] = u32::from(*clamp);
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
                ImageOp::FilteredBlur { .. } | ImageOp::Blur { .. } => {
                    let intermediate = match scratch.pop() {
                        Some(image) => image,
                        None => {
                            let image = self.host.image(dimensions)?;
                            leases.push(image.clone());
                            image
                        }
                    };
                    if let ImageOp::FilteredBlur {
                        size,
                        edges,
                        filter,
                        ..
                    } = op
                    {
                        for (axis, images) in [
                            [first, intermediate.as_ref()],
                            [intermediate.as_ref(), output.as_ref()],
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            buffers.push(self.filtered_blur.encode(
                                &mut encoder,
                                images,
                                axis,
                                size[axis],
                                *filter,
                                *edges,
                                &status,
                            )?);
                        }
                    } else if let ImageOp::Blur { radius, .. } = op {
                        params.header[0] = 7;
                        params.rect[0] = *radius;
                        self.pass(
                            &mut encoder,
                            [first, &dummy, &dummy],
                            &intermediate,
                            params,
                            &status,
                        );
                        params.header[0] = 8;
                        self.pass(
                            &mut encoder,
                            [&intermediate, &dummy, &dummy],
                            &output,
                            params,
                            &status,
                        );
                    }
                    stats.compute_passes += 2;
                    scratch.push(intermediate);
                    compute = false;
                }
                ImageOp::Baked(texture) => {
                    if texture.aces != aces
                        || texture.config.as_deref()
                            != config.map(|c| c.identity().content_sha256.as_str())
                    {
                        return Err("Baked texture color configuration changed; rebuild".into());
                    }
                    let (source, reservations, uploaded) =
                        self.baked.acquire(&self.host, texture)?;
                    stats.upload_bytes += uploaded;
                    buffers.extend(reservations);
                    buffers.push(self.baked_sampling.encode(
                        &self.host,
                        &mut encoder,
                        &source,
                        &output,
                        texture.format,
                        &status,
                    )?);
                    leases.push(source);
                    stats.upload_bytes += 16;
                    stats.compute_passes += 1;
                    compute = false;
                }
                ImageOp::Vector(drawings) => {
                    let report = self.vector.encode(
                        &mut encoder,
                        drawings,
                        &output,
                        &status,
                        &mut buffers,
                        cancel,
                    )?;
                    stats.upload_bytes += report.upload_bytes;
                    stats.vector_upload_bytes += report.upload_bytes;
                    stats.vector_geometry_upload_bytes += report.geometry_upload_bytes;
                    stats.vector_instance_upload_bytes += report.instance_upload_bytes;
                    stats.vector_instances += report.instances;
                    stats.vector_unique_triangles += report.unique_triangles;
                    stats.vector_execution_triangles += report.execution_triangles;
                    stats.vector_prepare_nanoseconds += report.prepare_nanoseconds;
                    stats.compute_passes += report.passes;
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
                | ImageOp::VideoInput { .. } => {
                    let begin = std::time::Instant::now();
                    let mut input_space = None;
                    let mut native_planes = None;
                    let exr_pixels;
                    let pixels: std::borrow::Cow<'_, [[f32; 4]]> =
                        if let ImageOp::Exr { space, .. } = op {
                            exr_pixels = exr.take(id, cancel)?;
                            input_space = space.as_deref();
                            std::borrow::Cow::Borrowed(&exr_pixels.pixels)
                        } else if aces {
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
                            if source.native_gbr_planes().is_some() && !source.has_alpha() {
                                native_planes = Some(source.clone());
                                std::borrow::Cow::Owned(Vec::new())
                            } else {
                                source.encoded_pixels().collect()
                            }
                        } else {
                            std::borrow::Cow::Owned(
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
                                .pixels,
                            )
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
                    if let Some(planes) = native_planes.as_ref() {
                        buffers.push(self.host.reserve(upload_bytes)?);
                        let upload = self.host.device().create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Fold native planes upload"),
                                contents: bytemuck::cast_slice(planes.native_gbr_planes().unwrap()),
                                usage: wgpu::BufferUsages::STORAGE,
                            },
                        );
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
                        let (upload, reused) = self.uploads.write(
                            &self.host,
                            &pixels,
                            dimensions,
                            &mut encoder,
                            &encoded.as_ref().unwrap_or(&output).texture,
                        )?;
                        stats.upload_allocations += u32::from(!reused);
                        stats.upload_reuses += u32::from(reused);
                        uploads.push(upload);
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
            if let Some(key) = still_key {
                immutable[id] = true;
                retained.push((key, output.clone()));
            }
            frames[id] = Some(output);
            for input in inputs.into_iter().flatten() {
                uses[input] -= 1;
                if uses[input] == 0 {
                    let image = frames[input].take().unwrap();
                    if !immutable[input] {
                        scratch.push(image);
                    }
                }
            }
        }
        Ok(Encoded {
            encoder,
            leases,
            buffers,
            status,
            image: frames[graph.output].take().unwrap(),
            dependencies,
            retained,
            stats,
            aces,
            evaluate_begin,
            uploads,
        })
    }
}
