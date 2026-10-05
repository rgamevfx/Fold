//! Two bounded reconstruction passes. No scene readback or display-space resize.
use super::{
    Host,
    host::{Image, Reservation},
    validation::Status,
};
use crate::region::Region;
use std::sync::Arc;
use wgpu::util::DeviceExt;
pub(super) struct Sampling {
    pipeline: wgpu::ComputePipeline,
}
impl Sampling {
    pub fn new(host: &Host) -> Self {
        let shader = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("sampling.wgsl"));
        Self {
            pipeline: host
                .device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Working image reconstruction"),
                    layout: None,
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                }),
        }
    }
    pub fn encode(
        &self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        images: [&Image; 3],
        dimensions: [u32; 2],
        region: Region,
        status: &Status,
    ) -> Result<Arc<Reservation>, String> {
        let [source, scratch, output] = images;
        let rows = crate::sampling::rows(source.dimensions[1], dimensions[1], region);
        let reservation = host.reserve(64)?;
        for (axis, pair) in [[source, scratch], [scratch, output]]
            .into_iter()
            .enumerate()
        {
            let params = [
                source.dimensions[0],
                source.dimensions[1],
                dimensions[0],
                dimensions[1],
                region.x,
                region.y,
                rows[0],
                axis as u32,
            ];
            let uniform = host
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Reconstruction footprint"),
                    contents: bytemuck::cast_slice(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&pair[0].view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&pair[1].view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: status.gpu.as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(
                pair[1].dimensions[0].div_ceil(8),
                pair[1].dimensions[1].div_ceil(8),
                1,
            );
        }
        Ok(reservation)
    }
}
