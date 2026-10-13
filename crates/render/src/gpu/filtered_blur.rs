//! Shared-sample separable filter execution; no approximation or hardware filtering.
use super::{
    Host,
    host::{Image, Reservation},
    validation::Status,
};
use crate::operations::{BlurFilter, Edges};
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub(super) struct FilteredBlur {
    host: Host,
    pipeline: wgpu::ComputePipeline,
    kernels: std::collections::VecDeque<Kernel>,
}
struct Kernel {
    key: (u32, BlurFilter),
    buffer: wgpu::Buffer,
    reservation: Arc<Reservation>,
    radius: u32,
    divisor: f32,
}
impl FilteredBlur {
    pub fn new(host: &Host) -> Self {
        let shader = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("filtered_blur.wgsl"));
        let pipeline = host
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("FilteredBlur shared samples"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        Self {
            host: host.clone(),
            pipeline,
            kernels: Default::default(),
        }
    }
    pub fn evict_one(&mut self) -> bool {
        self.kernels.pop_front().is_some()
    }
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.kernels.clear();
    }
    fn kernel(
        &mut self,
        size: f32,
        filter: BlurFilter,
    ) -> Result<&Kernel, fold_foundation::Failure> {
        let key = (size.to_bits(), filter);
        if let Some(index) = self.kernels.iter().position(|k| k.key == key) {
            let kernel = self.kernels.remove(index).unwrap();
            self.kernels.push_back(kernel);
        } else {
            let weights = filter.weights(size);
            let reservation = self.host.reserve((weights.len() * 4) as u64)?;
            let buffer = self
                .host
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("FilteredBlur cached weights"),
                    contents: bytemuck::cast_slice(&weights),
                    usage: wgpu::BufferUsages::STORAGE,
                });
            if self.kernels.len() == 16 {
                self.kernels.pop_front();
            }
            self.kernels.push_back(Kernel {
                key,
                buffer,
                reservation,
                radius: (weights.len() / 2) as u32,
                divisor: weights.iter().sum(),
            });
        }
        Ok(self.kernels.back().unwrap())
    }
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        images: [&Image; 2],
        axis: usize,
        sigma: f32,
        filter: BlurFilter,
        edges: Edges,
        status: &Status,
    ) -> Result<[Arc<Reservation>; 2], fold_foundation::Failure> {
        let kernel = self.kernel(sigma, filter)?;
        let kernel_buffer = kernel.buffer.clone();
        let kernel_reservation = kernel.reservation.clone();
        let radius = kernel.radius;
        let divisor = kernel.divisor;
        let host = &self.host;
        let [input, output] = images;
        let params = [
            output.dimensions[0],
            output.dimensions[1],
            axis as u32,
            edges as u32,
            radius,
            divisor.to_bits(),
            0,
            0,
        ];
        let reservation = host.reserve(std::mem::size_of_val(&params) as u64)?;
        let uniform = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("FilteredBlur parameters"),
                contents: bytemuck::cast_slice(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let pipeline = &self.pipeline;
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
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
                    resource: kernel_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: status.gpu.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            output.dimensions[axis].div_ceil(64),
            output.dimensions[1 - axis],
            1,
        );
        Ok([reservation, kernel_reservation])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires native Vulkan"]
    fn kernels_reuse_exact_keys_and_keep_evicted_leases_alive() {
        let (host, _) = pollster::block_on(Host::headless(1024 * 1024)).unwrap();
        let mut blur = FilteredBlur::new(&host);
        let first = blur.kernel(4., BlurFilter::Gaussian).unwrap();
        let buffer = first.buffer.clone();
        let lease = first.reservation.clone();
        assert_eq!(
            blur.kernel(4., BlurFilter::Gaussian).unwrap().buffer,
            buffer
        );
        assert_ne!(blur.kernel(4., BlurFilter::Box).unwrap().buffer, buffer);
        for size in 5..30 {
            blur.kernel(size as f32, BlurFilter::Gaussian).unwrap();
        }
        assert_eq!(blur.kernels.len(), 16);
        blur.clear();
        assert_eq!(host.memory().allocated, buffer.size());
        drop(lease);
        assert_eq!(host.memory().allocated, 0);
    }
}
