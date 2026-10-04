//! Shared-sample Gaussian execution; no approximation or hardware filtering.
use super::{
    Host,
    host::{Image, Reservation},
    validation::Status,
};
use crate::operations::{Edges, gaussian_weights};
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub(super) struct Gaussian {
    host: Host,
    pipeline: wgpu::ComputePipeline,
}
impl Gaussian {
    pub fn new(host: &Host) -> Self {
        let shader = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("gaussian.wgsl"));
        let pipeline = host
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Gaussian shared samples"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        Self {
            host: host.clone(),
            pipeline,
        }
    }
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        images: [&Image; 2],
        axis: usize,
        sigma: f32,
        edges: Edges,
        status: &Status,
    ) -> Result<Arc<Reservation>, String> {
        let host = &self.host;
        let [input, output] = images;
        let weights = gaussian_weights(sigma);
        let params = [
            output.dimensions[0],
            output.dimensions[1],
            axis as u32,
            edges as u32,
            (weights.len() / 2) as u32,
            weights.iter().sum::<f32>().to_bits(),
            0,
            0,
        ];
        let reservation =
            host.reserve((std::mem::size_of_val(&params) + weights.len() * 4) as u64)?;
        let uniform = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Gaussian parameters"),
                contents: bytemuck::cast_slice(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let kernel = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Gaussian weights"),
                contents: bytemuck::cast_slice(&weights),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&output.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: kernel.as_entire_binding(),
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
            output.dimensions[axis].div_ceil(64),
            output.dimensions[1 - axis],
            1,
        );
        Ok(reservation)
    }
}
