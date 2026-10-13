//! OCIO-generated GLSL and pinned LUTs; no sampled approximation of the processor.
use super::{
    Host,
    host::{Image, Reservation},
    validation::Status,
};
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub(super) struct ColorPipeline {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    views: Vec<wgpu::TextureView>,
    samplers: Vec<wgpu::Sampler>,
    pub reservation: Arc<Reservation>,
    pub upload_bytes: u64,
}

impl ColorPipeline {
    pub fn new(
        host: &Host,
        processor: Option<&fold_color::Processor>,
        display: bool,
    ) -> Result<Self, fold_foundation::Failure> {
        let desc = match processor {
            Some(p) => p.gpu_shader()?,
            None => fold_color::GpuShader { shader: "vec4 fold_ocio(vec4 p) { return vec4(mix(12.92*p.rgb, 1.055*pow(max(p.rgb,vec3(0.0)),vec3(1.0/2.4))-0.055, greaterThan(p.rgb,vec3(0.0031308))),p.a); }".into(), textures: vec![] },
        };
        let mut text = desc.shader;
        for c in ['r', 'g', 'b'] {
            text = text.replace(&format!(".rgb.{c}"), &format!(".{c}"));
        }
        let bytes = desc
            .textures
            .iter()
            .map(|t| t.size.iter().map(|&v| u64::from(v)).product::<u64>() * 16)
            .sum::<u64>();
        // Conservatively retain a second copy allowance for queue upload staging.
        let reservation = host.reserve(bytes * 2)?;
        let device = host.device();
        let mut entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: if display {
                        wgpu::TextureFormat::Rgba8Unorm
                    } else {
                        wgpu::TextureFormat::Rgba32Float
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        let mut views = vec![];
        let mut samplers = vec![device.create_sampler(&Default::default())];
        for (i, lut) in desc.textures.iter().enumerate() {
            if lut.linear
                && !device
                    .features()
                    .contains(wgpu::Features::FLOAT32_FILTERABLE)
            {
                return Err("OCIO LUT requires FLOAT32_FILTERABLE GPU support".into());
            }
            let dim = if lut.size[2] > 1 { "3D" } else { "2D" };
            let declaration = format!("uniform sampler{dim} {};", lut.sampler);
            if !text.contains(&declaration) {
                return Err("unsupported OCIO texture declaration".into());
            }
            text = text.replace(&declaration, &format!("layout(set=0,binding={}) uniform texture{dim} lut{i};\nlayout(set=0,binding={}) uniform sampler sample{i};", 5+i*2, 6+i*2));
            // OCIO uses texture() with explicit sampler uniforms. Separate Vulkan
            // texture/sampler bindings, and explicit LOD for compute derivatives.
            text = text.replace(
                &format!("texture({},", lut.sampler),
                &format!("textureLod(sampler{dim}(lut{i},sample{i}),"),
            );
            // Find the balanced call end, not the next ')' in the coordinates.
            let needle = format!("textureLod(sampler{dim}(lut{i},sample{i}),");
            let mut from = 0;
            while let Some(start) = text[from..].find(&needle).map(|v| v + from) {
                let mut depth = 1;
                let mut end = start + needle.len();
                for (offset, c) in text[end..].char_indices() {
                    if c == '(' {
                        depth += 1;
                    }
                    if c == ')' {
                        depth -= 1;
                    }
                    if depth == 0 {
                        end += offset;
                        break;
                    }
                }
                text.insert_str(end, ", 0.0");
                from = end + 6;
            }
            let [w, h, d] = lut.size;
            if ![1, 3].contains(&lut.channels)
                || lut.values.len() != w as usize * h as usize * d as usize * lut.channels
            {
                return Err("invalid OCIO LUT layout".into());
            }
            let rgba: Vec<f32> = lut
                .values
                .chunks_exact(lut.channels)
                .flat_map(|v| {
                    if lut.channels == 1 {
                        [v[0], 0., 0., 1.]
                    } else {
                        [v[0], v[1], v[2], 1.]
                    }
                })
                .collect();
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("OCIO LUT"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: d,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: if d > 1 {
                    wgpu::TextureDimension::D3
                } else {
                    wgpu::TextureDimension::D2
                },
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            host.queue().write_texture(
                texture.as_image_copy(),
                bytemuck::cast_slice(&rgba),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 16),
                    rows_per_image: Some(h),
                },
                texture.size(),
            );
            views.push(texture.create_view(&Default::default()));
            let filter = if lut.linear {
                wgpu::FilterMode::Linear
            } else {
                wgpu::FilterMode::Nearest
            };
            samplers.push(device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            }));
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 5 + i as u32 * 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float {
                        filterable: lut.linear,
                    },
                    view_dimension: if d > 1 {
                        wgpu::TextureViewDimension::D3
                    } else {
                        wgpu::TextureViewDimension::D2
                    },
                    multisampled: false,
                },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 6 + i as u32 * 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(if lut.linear {
                    wgpu::SamplerBindingType::Filtering
                } else {
                    wgpu::SamplerBindingType::NonFiltering
                }),
                count: None,
            });
        }
        let format = if display { "rgba8" } else { "rgba32f" };
        let shader = format!(
            "#version 450\nlayout(local_size_x=8,local_size_y=8) in;\nlayout(set=0,binding=0) uniform texture2D source_image;\nlayout(set=0,binding=1) uniform sampler source_sampler;\nlayout(set=0,binding=2,{format}) uniform writeonly image2D destination;\nlayout(set=0,binding=3,std430) buffer Validation {{ uint invalid; }};\nlayout(set=0,binding=4,std140) uniform Settings {{ uvec4 settings; }};\n{text}\nvoid fold_flag_invalid() {{ invalid=1u; }}\nvoid main() {{\n ivec2 xy=ivec2(gl_GlobalInvocationID.xy); if(any(greaterThanEqual(xy,imageSize(destination)))) return;\n vec4 p=texelFetch(sampler2D(source_image,source_sampler),xy,0);\n if(settings.x != 0u) p.a=1.0;\n vec4 v=vec4(0.0);\n if(p.a>0.0) {{ vec3 straight=p.rgb/p.a; v=vec4(fold_ocio(vec4(straight,p.a)).rgb*p.a,p.a); if(any(isinf(straight))||any(isnan(straight))) v=vec4(uintBitsToFloat(0x7f800000u)); }}\n if(any(isinf(v))||any(isnan(v))||v.a<0.0||v.a>1.0) fold_flag_invalid();\n imageStore(destination,xy,v);\n}}\n"
        );
        let shader = validated_shader(&shader)?;
        let _compile_guard = super::shader::COMPILE_LOCK
            .lock()
            .map_err(|_| "Shader compiler lock poisoned")?;
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("OCIO GPU"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("OCIO GLSL"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(shader)),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("OCIO GPU"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
            return Err(format!("OCIO GPU shader: {error}").into());
        }
        host.check()?;
        Ok(Self {
            pipeline,
            layout,
            views,
            samplers,
            reservation,
            upload_bytes: bytes,
        })
    }
    pub fn encode(
        &self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        input: &Image,
        output: &Image,
        matte: bool,
        status: &Status,
    ) -> Result<(), fold_foundation::Failure> {
        // The generated shader validates the float result before UNORM storage.
        // No float display intermediate or separate validation scan is needed.
        let uniform = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[u32::from(matte), 0, 0, 0]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&input.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&self.samplers[0]),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&output.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: status.gpu.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: uniform.as_entire_binding(),
            },
        ];
        for (i, view) in self.views.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: 5 + i as u32 * 2,
                resource: wgpu::BindingResource::TextureView(view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 6 + i as u32 * 2,
                resource: wgpu::BindingResource::Sampler(&self.samplers[i + 1]),
            });
        }
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &entries,
        });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            output.dimensions[0].div_ceil(8),
            output.dimensions[1].div_ceil(8),
            1,
        );
        Ok(())
    }
}

/// Naga's GLSL frontend does not support storage atomicOr. Promote the one
/// generated validation flag/store in typed IR instead of using a racy store,
/// another full-image pass, or rewriting arbitrary OCIO shader text.
fn validated_shader(source: &str) -> Result<naga::Module, fold_foundation::Failure> {
    let mut module = naga::front::glsl::Frontend::default()
        .parse(
            &naga::front::glsl::Options::from(naga::ShaderStage::Compute),
            source,
        )
        .map_err(|error| error.to_string())?;
    let atomic = module.types.insert(
        naga::Type {
            name: None,
            inner: naga::TypeInner::Atomic(naga::Scalar {
                kind: naga::ScalarKind::Uint,
                width: 4,
            }),
        },
        naga::Span::UNDEFINED,
    );
    let (_, global) = module
        .global_variables
        .iter_mut()
        .find(|(_, global)| {
            global.binding
                == Some(naga::ResourceBinding {
                    group: 0,
                    binding: 3,
                })
        })
        .ok_or("missing color validation binding")?;
    let mut ty = module.types[global.ty].clone();
    let naga::TypeInner::Struct { members, .. } = &mut ty.inner else {
        return Err("invalid color validation type".into());
    };
    if members.len() != 1 {
        return Err("invalid color validation members".into());
    }
    members[0].ty = atomic;
    global.ty = module.types.insert(ty, naga::Span::UNDEFINED);
    let (_, function) = module
        .functions
        .iter_mut()
        .find(|(_, function)| function.name.as_deref() == Some("fold_flag_invalid"))
        .ok_or("missing color validation function")?;
    let mut stores = 0;
    for statement in function.body.iter_mut() {
        if let naga::Statement::Store { pointer, value } = *statement {
            *statement = naga::Statement::Atomic {
                pointer,
                fun: naga::AtomicFunction::InclusiveOr,
                value,
                result: None,
            };
            stores += 1;
        }
    }
    if stores != 1 {
        return Err("invalid color validation store count".into());
    }
    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires native Vulkan"]
    fn display_conversion_needs_no_float_intermediate() {
        let dimensions = [64, 32];
        let bytes = 64 * 32 * (16 + 4);
        let (host, _) = pollster::block_on(Host::headless(bytes + 4096)).unwrap();
        let input = host.image(dimensions).unwrap();
        let output = host
            .image_format(dimensions, wgpu::TextureFormat::Rgba8Unorm)
            .unwrap();
        let pipeline = ColorPipeline::new(&host, None, true).unwrap();
        let status = Status::new(&host).unwrap();
        let mut encoder = host.device().create_command_encoder(&Default::default());
        status.start(&mut encoder);
        pipeline
            .encode(&host, &mut encoder, &input, &output, true, &status)
            .unwrap();
        status.encode(&mut encoder);
        host.submit(
            encoder,
            vec![input, output],
            vec![pipeline.reservation.clone()],
        );
        let ready = status.submitted();
        let start = std::time::Instant::now();
        while !ready.ready().unwrap() {
            host.poll().unwrap();
            assert!(start.elapsed().as_secs() < 10);
        }
        assert!(host.memory().peak <= bytes + 4096);
    }

    #[test]
    #[ignore = "requires native Vulkan"]
    fn working_conversion_does_not_allocate_a_copy_of_its_output() {
        let dimensions = [64, 32];
        let image_bytes = 64 * 32 * 16;
        let (host, _) = pollster::block_on(Host::headless(2 * image_bytes + 4096)).unwrap();
        let input = host.image(dimensions).unwrap();
        let output = host.image(dimensions).unwrap();
        let pipeline = ColorPipeline::new(&host, None, false).unwrap();
        let status = Status::new(&host).unwrap();
        let mut encoder = host.device().create_command_encoder(&Default::default());
        status.start(&mut encoder);
        pipeline
            .encode(&host, &mut encoder, &input, &output, false, &status)
            .unwrap();
        status.encode(&mut encoder);
        host.submit(
            encoder,
            vec![input, output],
            vec![pipeline.reservation.clone()],
        );
        let ready = status.submitted();
        let begin = std::time::Instant::now();
        while !ready.ready().unwrap() {
            host.poll().unwrap();
            assert!(begin.elapsed().as_secs() < 10);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(host.memory().peak <= 2 * image_bytes + 4096);
    }

    #[test]
    #[ignore = "requires native Vulkan"]
    fn direct_working_output_still_rejects_conversion_overflow() {
        let (host, _) = pollster::block_on(Host::headless(1024 * 1024)).unwrap();
        let input = host.image([1, 1]).unwrap();
        let output = host.image([1, 1]).unwrap();
        let pipeline = ColorPipeline::new(&host, None, false).unwrap();
        for (pixel, valid) in [
            ([0.1, -0.1, 2., 0.5], true),
            ([f32::MAX, 0., 0., 0.1], false),
        ] {
            host.queue().write_texture(
                input.texture.as_image_copy(),
                bytemuck::cast_slice(&pixel),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(16),
                    rows_per_image: Some(1),
                },
                input.texture.size(),
            );
            let status = Status::new(&host).unwrap();
            let mut encoder = host.device().create_command_encoder(&Default::default());
            status.start(&mut encoder);
            pipeline
                .encode(&host, &mut encoder, &input, &output, false, &status)
                .unwrap();
            status.encode(&mut encoder);
            host.submit(
                encoder,
                vec![input.clone(), output.clone()],
                vec![pipeline.reservation.clone()],
            );
            let ready = status.submitted();
            let begin = std::time::Instant::now();
            loop {
                host.poll().unwrap();
                match ready.ready() {
                    Ok(false) => {}
                    result => {
                        assert_eq!(result.is_ok(), valid);
                        break;
                    }
                }
                assert!(begin.elapsed().as_secs() < 10);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
}
