//! Rasterized textured geometry with GPU vertex sampling. Retains immutable
//! meshes; each draw transfers only a compact transform/response uniform.
use super::{
    Host,
    host::{Image, Reservation},
    validation::Status,
};
use crate::textured::{Draw, Mesh};
use std::{collections::VecDeque, sync::Arc};
use wgpu::util::DeviceExt;
struct Resident {
    mesh: Arc<Mesh>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    lease: Arc<Reservation>,
}
pub(super) struct Pipeline {
    raster: wgpu::RenderPipeline,
    copy: wgpu::ComputePipeline,
    validate: wgpu::ComputePipeline,
    meshes: VecDeque<Resident>,
    bytes: u64,
}
impl Pipeline {
    pub fn new(host: &Host) -> Self {
        let device = host.device();
        let shader = device.create_shader_module(wgpu::include_wgsl!("textured.wgsl"));
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Textured geometry"),
            entries: &[
                texture(0),
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
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
        let raster = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Textured triangles and image responses"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: host.textured_format(),
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let validate = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Image response validation"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("validate"),
            compilation_options: Default::default(),
            cache: None,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("textured_copy.wgsl"));
        let copy = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Textured working output"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            raster,
            copy,
            validate,
            meshes: VecDeque::new(),
            bytes: 0,
        }
    }
    pub fn clear(&mut self) {
        self.meshes.clear();
        self.bytes = 0;
    }
    pub fn encode(
        &mut self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        source: &Image,
        driver: &Image,
        output: &Image,
        surface: &Image,
        draw: &Draw,
        status: &Status,
        aces: bool,
    ) -> Result<(Vec<Arc<Reservation>>, u64), String> {
        draw.validate()?;
        let size = (draw.mesh.vertices.len() * 32 + draw.mesh.indices.len() * 4) as u64;
        let mut uploaded = 0;
        let index =
            if let Some(index) = self.meshes.iter().position(|r| {
                Arc::ptr_eq(&r.mesh, &draw.mesh) || r.mesh.as_ref() == draw.mesh.as_ref()
            }) {
                index
            } else {
                while self.bytes + size > (host.memory().budget / 8).min(32 * 1024 * 1024)
                    || self.meshes.len() >= 128
                {
                    if let Some(old) = self.meshes.pop_front() {
                        self.bytes -=
                            (old.mesh.vertices.len() * 32 + old.mesh.indices.len() * 4) as u64;
                    } else {
                        break;
                    }
                }
                let lease = host.reserve(size.max(8))?;
                let vertices =
                    host.device()
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Image mesh vertices"),
                            contents: bytemuck::cast_slice(&draw.mesh.vertices),
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
                        });
                let indices = host
                    .device()
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Image mesh indices"),
                        contents: bytemuck::cast_slice(&draw.mesh.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                self.meshes.push_back(Resident {
                    mesh: draw.mesh.clone(),
                    vertices,
                    indices,
                    lease,
                });
                self.bytes += size;
                uploaded = size;
                self.meshes.len() - 1
            };
        let resident = &self.meshes[index];
        let [w, h] = output.dimensions;
        let instance_bytes = (draw.instances.len() * 32) as u64;
        let lease = host.reserve(96 + instance_bytes)?;
        let instances = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Image instances"),
                contents: bytemuck::cast_slice(&draw.instances),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let view = &surface.view;
        let m = draw.transform;
        let mut values = vec![
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5],
            w as f32,
            h as f32,
            draw.displacement[0],
            draw.displacement[1],
            draw.scale,
            draw.color,
            draw.remap[0],
            draw.remap[1],
            draw.alpha,
            draw.opacity,
            draw.atlas[0],
            draw.atlas[1],
            draw.atlas[2],
            draw.rotation,
        ];
        values.extend([
            f32::from_bits(draw.channel as u32),
            f32::from_bits(u32::from(aces)),
            f32::from_bits(u32::from(draw.document_samples)),
            f32::from_bits(
                u32::from(source.texture.format() == wgpu::TextureFormat::Bc7RgbaUnorm)
                    | (u32::from(driver.texture.format() == wgpu::TextureFormat::Bc7RgbaUnorm)
                        << 1),
            ),
        ]);
        let uniform = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&values),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let array_view = |image: &Image| {
            image.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let source_view = array_view(source);
        let driver_view = array_view(driver);
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.raster.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&driver_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: instances.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: status.gpu.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: resident.vertices.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.validate);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(
                ((draw.mesh.vertices.len() * draw.instances.len()) as u32).div_ceil(64),
                1,
                1,
            );
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Image cards/grid"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.raster);
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, resident.vertices.slice(..));
            pass.set_index_buffer(resident.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(
                0..draw.mesh.indices.len() as u32,
                0,
                0..draw.instances.len() as u32,
            );
        }
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.copy.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
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
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.copy);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        Ok((
            vec![lease, resident.lease.clone()],
            uploaded + 96 + instance_bytes,
        ))
    }
}
