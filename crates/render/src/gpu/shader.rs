//! Worker-owned validated pipeline cache; parameter changes do not compile WGSL.
use super::{
    Host,
    host::{Image, Reservation},
    validation::Status,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use wgpu::util::DeviceExt;
pub(super) static COMPILE_LOCK: Mutex<()> = Mutex::new(());
#[derive(Default)]
pub(super) struct Cache {
    pipelines: BTreeMap<String, Arc<Pipeline>>,
    pub compilations: u32,
}
struct Pipeline {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}
impl Cache {
    pub fn encode(
        &mut self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        program: &crate::shader::Program,
        inputs: &[&Image],
        output: &Image,
        status: &Status,
    ) -> Result<Arc<Reservation>, String> {
        let source = program.wgsl()?;
        if !self.pipelines.contains_key(&source) {
            program.validate_source()?;
            let mut entries = vec![
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ];
            for i in 0..inputs.len() {
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: 3 + i as u32,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                });
            }
            let _guard = COMPILE_LOCK
                .lock()
                .map_err(|_| "Shader compiler lock poisoned")?;
            let device = host.device();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Live shader inputs"),
                entries: &entries,
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Live image shader"),
                source: wgpu::ShaderSource::Wgsl(source.clone().into()),
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Live image shader"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("fold_main"),
                compilation_options: Default::default(),
                cache: None,
            });
            if let Some(error) = pollster::block_on(device.pop_error_scope()) {
                return Err(format!("Shader pipeline: {error}"));
            }
            self.compilations += 1;
            if self.pipelines.len() >= 64 {
                self.pipelines.pop_first();
            }
            self.pipelines
                .insert(source.clone(), Arc::new(Pipeline { pipeline, layout }));
        }
        let pipeline = &self.pipelines[&source];
        let mut params = [[0f32; 4]; 34];
        params[0] = [
            program.logical_size[0],
            program.logical_size[1],
            output.dimensions[0] as f32,
            output.dimensions[1] as f32,
        ];
        params[1] = [program.time, program.progress, 0., 0.];
        params[2..2 + program.values.len()].copy_from_slice(&program.values);
        let bytes = bytemuck::cast_slice(&params);
        let reservation = host.reserve(bytes.len() as u64)?;
        let buffer = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Shader parameters"),
                contents: bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&output.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: status.gpu.as_entire_binding(),
            },
        ];
        for (i, input) in inputs.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: 3 + i as u32,
                resource: wgpu::BindingResource::TextureView(&input.view),
            });
        }
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader images"),
            layout: &pipeline.layout,
            entries: &entries,
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Live image shader"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            output.dimensions[0].div_ceil(8),
            output.dimensions[1].div_ceil(8),
            1,
        );
        Ok(reservation)
    }
}
