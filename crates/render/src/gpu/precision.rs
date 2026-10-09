//! Explicit scene precision conversion. Color and premultiplied alpha are
//! unchanged; values outside finite half-float range fail before publication.
use super::{GpuFrame, Renderer, host, validation::Status};
use std::sync::atomic::Ordering;

impl Renderer {
    /// Convert a scene to RGBA16F on its owning GPU. This is a storage boundary,
    /// not an output/color transform. Completion retains all input dependencies.
    pub fn rgba16(
        &mut self,
        frame: GpuFrame,
        cancel: &fold_media::Cancel,
    ) -> Result<GpuFrame, fold_foundation::Failure> {
        convert(&self.host, &mut self.precision, frame, cancel, false)
    }
}

pub(super) fn convert(
    host: &super::Host,
    cache: &mut Option<(bool, wgpu::ComputePipeline)>,
    mut frame: GpuFrame,
    cancel: &fold_media::Cancel,
    validate_existing: bool,
) -> Result<GpuFrame, fold_foundation::Failure> {
    cancel.check()?;
    host.check()?;
    if frame.host.id() != host.id() {
        return Err("GPU scene belongs to a different host".into());
    }
    if !validate_existing && frame.image.texture.format() == wgpu::TextureFormat::Rgba16Float {
        return Ok(frame);
    }
    if cache.as_ref().map(|(strict, _)| *strict) != Some(validate_existing) {
        *cache = Some((validate_existing, pipeline(host, validate_existing)?));
        frame.statistics.shader_compilations += 1;
    }
    let (_, pipeline) = cache.as_ref().unwrap();
    let output = host.image_format(frame.dimensions(), wgpu::TextureFormat::Rgba16Float)?;
    let mut status = Status::new(host)?;
    status.failure =
        "RGBA16F scene contains nonfinite/out-of-range or saturated RGB, or invalid alpha";
    let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Scene precision conversion"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&frame.image.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&output.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: status.gpu.as_entire_binding(),
            },
        ],
    });
    let mut encoder = host.device().create_command_encoder(&Default::default());
    status.start(&mut encoder);
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            frame.dimensions()[0].div_ceil(8),
            frame.dimensions()[1].div_ceil(8),
            1,
        );
    }
    status.encode(&mut encoder);
    cancel.check()?;
    let begin = std::time::Instant::now();
    host.submit(encoder, vec![frame.image.clone(), output.clone()], vec![]);
    frame.statistics.queue_submit_nanoseconds += begin.elapsed().as_nanos();
    frame.statistics.compute_passes += 1;
    frame.statistics.status_readback_bytes += status.readback_bytes();
    frame.dependencies.push(frame.ready);
    frame.ready = status.submitted();
    frame.image = output;
    frame.lease_id = host::NEXT_ID.fetch_add(1, Ordering::Relaxed);
    cancel.check()?;
    Ok(frame)
}

fn pipeline(
    host: &super::Host,
    stored_half: bool,
) -> Result<wgpu::ComputePipeline, fold_foundation::Failure> {
    let _guard = super::shader::COMPILE_LOCK
        .lock()
        .map_err(|_| "Shader compiler lock poisoned")?;
    let device = host.device();
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty,
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scene precision inputs"),
        entries: &[
            entry(
                0,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                1,
                wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba16Float,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
            ),
            entry(
                2,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Scene RGBA16F conversion"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var<storage, read_write> failed: atomic<u32>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(source)) { return; }
    let value = textureLoad(source, vec2<i32>(id.xy), 0);
    if !all(VALID_RGB) || !(value.a >= 0.0 && value.a <= 1.0) {
        atomicStore(&failed, 1u);
        textureStore(destination, vec2<i32>(id.xy), vec4<f32>(0.0));
        return;
    }
    textureStore(destination, vec2<i32>(id.xy), value);
}
"#
            .replace(
                "VALID_RGB",
                if stored_half {
                    // An attachment may saturate finite overflow at half's maximum.
                    // Once stored, equality cannot distinguish saturation from an
                    // authored endpoint; refuse publication rather than clip silently.
                    "abs(value.rgb) < vec3<f32>(65504.0)"
                } else {
                    "abs(value.rgb) <= vec3<f32>(65504.0)"
                },
            )
            .into(),
        ),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Scene RGBA16F conversion"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    if let Some(error) = pollster::block_on(device.pop_error_scope()) {
        return Err(format!("Scene precision pipeline: {error}").into());
    }
    Ok(pipeline)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a native Vulkan adapter"]
    fn rgba16_signed_hdr_range_and_overflow() {
        let (host, _) = pollster::block_on(super::super::Host::headless(1024 * 1024)).unwrap();
        let mut renderer = Renderer::new(host.clone()).unwrap();
        let cancel = fold_media::Cancel::default();
        let mut decoder = fold_media::Decoder::default();
        for pixel in [
            [-16., 65504., 2., 0.5],
            [65536., 0., 0., 1.],
            [-65536., 0., 0., 1.],
            [0., 0., 0., 1.1],
            [f32::NAN, 0., 0., 1.],
            [f32::INFINITY, 0., 0., 1.],
        ] {
            let frame = renderer
                .evaluate(
                    crate::RenderGraph {
                        width: 1,
                        height: 1,
                        nodes: vec![crate::ImageOp::Solid { rgba: [0.; 4] }],
                        output: 0,
                    },
                    None,
                    &mut decoder,
                    &cancel,
                )
                .unwrap();
            // Direct texture input isolates storage conversion from SDR operators.
            host.queue().write_texture(
                frame.image.texture.as_image_copy(),
                bytemuck::cast_slice(&pixel),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(16),
                    rows_per_image: Some(1),
                },
                frame.image.texture.size(),
            );
            let mut half = renderer.rgba16(frame, &cancel).unwrap();
            let result = half.readback(&cancel);
            if pixel[0] == -16. {
                assert_eq!(result.unwrap().pixels()[0], pixel);
            } else {
                assert!(result.err().unwrap().message.contains("RGBA16F"));
            }
        }
    }
}
