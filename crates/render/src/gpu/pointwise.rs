//! Bounded pointwise chains. Spatial operations and shared results remain boundaries.
use super::{Host, Image, encoding::Params, validation::Status};
use crate::RenderGraph;
use wgpu::util::DeviceExt;

pub(super) struct Chain {
    pub input: usize,
    pub steps: Vec<usize>,
}
pub(super) fn plan(
    graph: &RenderGraph,
    needed: &mut [bool],
    uses: &mut [usize],
) -> Vec<Option<Chain>> {
    let mut chains: Vec<_> = (0..graph.nodes.len()).map(|_| None).collect();
    for id in (0..graph.nodes.len()).rev() {
        if !needed[id] || Params::pointwise(&graph.nodes[id], [1, 1], false).is_none() {
            continue;
        }
        let mut steps = vec![id];
        let mut input = graph.nodes[id].inputs().next().unwrap();
        while steps.len() < 16
            && uses[input] == 1
            && needed[input]
            && Params::pointwise(&graph.nodes[input], [1, 1], false).is_some()
        {
            steps.push(input);
            needed[input] = false;
            uses[input] = 0;
            input = graph.nodes[input].inputs().next().unwrap();
        }
        if steps.len() > 1 {
            steps.reverse();
            chains[id] = Some(Chain { input, steps });
        }
    }
    chains
}

pub(super) struct Pipeline(wgpu::ComputePipeline);
impl Pipeline {
    pub fn new(host: &Host) -> Self {
        let module = host
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Pointwise chains"),
                source: wgpu::ShaderSource::Wgsl(
                    concat!(
                        include_str!("pointwise.wgsl"),
                        include_str!("pointwise_chain.wgsl")
                    )
                    .into(),
                ),
            });
        Self(
            host.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Pointwise chains"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                }),
        )
    }
    pub fn encode(
        &self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        input: &Image,
        output: &Image,
        steps: &[Params],
        status: &Status,
    ) {
        let mut values = [Params::zeroed(); 16];
        values[..steps.len()].copy_from_slice(steps);
        let mut data = Vec::with_capacity(16 + std::mem::size_of_val(&values));
        data.extend_from_slice(bytemuck::cast_slice(&[steps.len() as u32, 0, 0, 0]));
        data.extend_from_slice(bytemuck::cast_slice(&values));
        let buffer = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Pointwise chain parameters"),
                contents: &data,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.0.get_bind_group_layout(0),
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
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: status.gpu.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.0);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            output.dimensions[0].div_ceil(8),
            output.dimensions[1].div_ceil(8),
            1,
        );
    }
}
use bytemuck::Zeroable;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageOp;
    #[test]
    fn shared_pointwise_shaders_validate_without_a_device() {
        for suffix in [
            include_str!("operators.wgsl"),
            include_str!("pointwise_chain.wgsl"),
        ] {
            let source = format!("{}\n{suffix}", include_str!("pointwise.wgsl"));
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }
    #[test]
    fn shared_results_and_spatial_samples_bound_chains() {
        let graph = RenderGraph {
            width: 1,
            height: 1,
            nodes: vec![
                ImageOp::Solid { rgba: [0.5; 4] },
                ImageOp::Opacity {
                    input: 0,
                    opacity: 0.8,
                },
                ImageOp::Grade {
                    input: 1,
                    gain: [0.5; 3],
                },
                ImageOp::Opacity {
                    input: 2,
                    opacity: 0.5,
                },
                ImageOp::Over {
                    foreground: 3,
                    background: 1,
                },
            ],
            output: 4,
        };
        let (mut needed, mut uses) = crate::graph::dependencies(&graph);
        let chains = plan(&graph, &mut needed, &mut uses);
        assert_eq!(chains[3].as_ref().unwrap().steps, [2, 3]);
        assert_eq!(chains[3].as_ref().unwrap().input, 1);
        assert!(needed[1]);
        assert!(!needed[2]);
        assert_eq!(uses[1], 2);
    }
}
