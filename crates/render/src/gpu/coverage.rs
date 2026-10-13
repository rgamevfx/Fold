//! Bounded scalar tiles and completion-gated matte reuse. The cache key contains
//! evaluated coverage only: changing footage never invalidates a static matte.
use super::{
    Host, Statistics,
    host::{Image, Reservation},
    validation::{Completion, Status},
    vector::VectorPipeline,
};
use crate::{
    coverage::{Combine, Op, Program},
    vector::{Drawing, Segment},
};
use fold_media::Cancel;
use std::{collections::VecDeque, sync::Arc};
use wgpu::util::DeviceExt;
#[derive(Clone)]
struct Tile {
    rect: [u32; 4],
    outside: f32,
    image: Option<Arc<Image>>,
}
impl Tile {
    fn constant(outside: f32) -> Self {
        Self {
            rect: [0; 4],
            outside,
            image: None,
        }
    }
}
struct Entry {
    program: Arc<Program>,
    size: [u32; 2],
    image: Arc<Image>,
    ready: Completion,
}
#[derive(Default)]
pub(super) struct Coverage {
    pipeline: Option<wgpu::ComputePipeline>,
    entries: VecDeque<Entry>,
    bytes: u64,
}
pub(super) struct Retained {
    pub program: Arc<Program>,
    pub size: [u32; 2],
    pub image: Arc<Image>,
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    output: [u32; 4],
    first: [u32; 4],
    second: [u32; 4],
    values: [f32; 4],
    mode: [u32; 4],
}
impl Coverage {
    pub fn evict_one(&mut self) -> bool {
        if let Some(entry) = self.entries.pop_front() {
            self.bytes -= entry.image.bytes;
            true
        } else {
            false
        }
    }
    pub fn get(&mut self, program: &Program, size: [u32; 2]) -> Option<Arc<Image>> {
        let index = self.entries.iter().position(|e| {
            e.size == size && e.program.as_ref() == program && e.ready.ready() == Ok(true)
        })?;
        let entry = self.entries.remove(index).unwrap();
        let image = entry.image.clone();
        self.entries.push_back(entry);
        Some(image)
    }
    pub fn retain(&mut self, value: Retained, ready: Completion, budget: u64) {
        let limit = (budget / 8).min(64 * 1024 * 1024);
        if value.image.bytes > limit {
            return;
        }
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.size == value.size && e.program == value.program)
        {
            self.bytes -= self.entries.remove(i).unwrap().image.bytes;
        }
        while self.bytes + value.image.bytes > limit || self.entries.len() >= 8 {
            self.bytes -= self.entries.pop_front().unwrap().image.bytes;
        }
        self.bytes += value.image.bytes;
        self.entries.push_back(Entry {
            program: value.program,
            size: value.size,
            image: value.image,
            ready,
        });
    }
    fn pipeline(&mut self, host: &Host) -> &wgpu::ComputePipeline {
        self.pipeline.get_or_insert_with(|| {
            let module = host
                .device()
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Scalar coverage"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("coverage.wgsl").into()),
                });
            host.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Scalar coverage"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                })
        })
    }
    fn pass(
        &mut self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        op: &Op,
        a: &Tile,
        b: &Tile,
        rect: [u32; 4],
        dummy: &Image,
        status: &Status,
        buffers: &mut Vec<Arc<Reservation>>,
    ) -> Result<Arc<Image>, fold_foundation::Failure> {
        let (mode, value, invert) = match *op {
            Op::Combine { mode, .. } => (
                match mode {
                    Combine::Over => 1,
                    Combine::Add => 2,
                    Combine::Subtract => 3,
                    Combine::Intersect => 4,
                    Combine::Replace => 5,
                },
                0.,
                false,
            ),
            Op::Adjust {
                opacity, invert, ..
            } => (6, opacity, invert),
            Op::Mix { amount, .. } => (7, amount, false),
            _ => (0, 0., false),
        };
        let params = Params {
            output: rect,
            first: a.rect,
            second: b.rect,
            values: [a.outside, b.outside, value, u32::from(invert) as f32],
            mode: [
                mode,
                u32::from(a.image.is_some()),
                u32::from(b.image.is_some()),
                0,
            ],
        };
        buffers.push(host.reserve(std::mem::size_of::<Params>() as u64)?);
        let uniform = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Coverage tile"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let output = host.image_format([rect[2], rect[3]], wgpu::TextureFormat::R32Float)?;
        let pipeline = self.pipeline(host);
        let bind = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Coverage tiles"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &a.image.as_deref().unwrap_or(dummy).view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &b.image.as_deref().unwrap_or(dummy).view,
                    ),
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(rect[2].div_ceil(8), rect[3].div_ceil(8), 1);
        drop(pass);
        Ok(output)
    }
    pub fn encode(
        &mut self,
        program: &Program,
        size: [u32; 2],
        host: &Host,
        vector: &mut VectorPipeline,
        encoder: &mut wgpu::CommandEncoder,
        status: &Status,
        leases: &mut Vec<Arc<Image>>,
        buffers: &mut Vec<Arc<Reservation>>,
        stats: &mut Statistics,
        cancel: &Cancel,
    ) -> Result<Arc<Image>, fold_foundation::Failure> {
        let dummy = host.image([1, 1])?;
        leases.push(dummy.clone());
        let mut tiles = Vec::<Tile>::with_capacity(program.nodes.len());
        let needed = program.needed();
        for (id, op) in program.nodes.iter().enumerate() {
            if !needed[id] {
                tiles.push(Tile::constant(0.));
                continue;
            }
            cancel.check()?;
            let tile = match op {
                Op::Constant(value) => Tile::constant(*value),
                Op::Draw(drawings) => {
                    let rect = bounds(drawings, size);
                    if rect[2] == 0 || rect[3] == 0 {
                        Tile::constant(0.)
                    } else {
                        let mut local = drawings.as_ref().clone();
                        for d in &mut local {
                            d.transform[4] -= rect[0] as f64;
                            d.transform[5] -= rect[1] as f64;
                        }
                        let image =
                            host.image_format([rect[2], rect[3]], wgpu::TextureFormat::R32Float)?;
                        let r = vector.encode(encoder, &local, &image, status, buffers, cancel)?;
                        stats.upload_bytes += r.upload_bytes;
                        stats.vector_upload_bytes += r.upload_bytes;
                        stats.vector_geometry_upload_bytes += r.geometry_upload_bytes;
                        stats.vector_instance_upload_bytes += r.instance_upload_bytes;
                        stats.vector_instances += r.instances;
                        stats.vector_unique_triangles += r.unique_triangles;
                        stats.vector_execution_triangles += r.execution_triangles;
                        stats.vector_prepare_nanoseconds += r.prepare_nanoseconds;
                        stats.compute_passes += r.passes;
                        stats.coverage_pixels += rect[2] as u64 * rect[3] as u64;
                        Tile {
                            rect,
                            outside: 0.,
                            image: Some(image),
                        }
                    }
                }
                _ => {
                    let inputs = op.inputs();
                    let a = &tiles[inputs[0].unwrap()];
                    let zero = Tile::constant(0.);
                    let b = inputs[1].map_or(&zero, |i| &tiles[i]);
                    let rect = union(a.rect, b.rect);
                    let outside = op.apply(a.outside, b.outside);
                    if rect[2] == 0 || rect[3] == 0 {
                        Tile::constant(outside)
                    } else {
                        let image =
                            self.pass(host, encoder, op, a, b, rect, &dummy, status, buffers)?;
                        stats.compute_passes += 1;
                        stats.coverage_pixels += rect[2] as u64 * rect[3] as u64;
                        Tile {
                            rect,
                            outside,
                            image: Some(image),
                        }
                    }
                }
            };
            if let Some(image) = &tile.image {
                leases.push(image.clone());
            }
            tiles.push(tile);
        }
        let final_tile = &tiles[program.output];
        if final_tile.rect == [0, 0, size[0], size[1]] {
            if let Some(image) = &final_tile.image {
                return Ok(image.clone());
            }
        }
        let image = self.pass(
            host,
            encoder,
            &Op::Constant(0.),
            final_tile,
            &Tile::constant(0.),
            [0, 0, size[0], size[1]],
            &dummy,
            status,
            buffers,
        )?;
        stats.compute_passes += 1;
        stats.coverage_pixels += size[0] as u64 * size[1] as u64;
        leases.push(image.clone());
        Ok(image)
    }
}
fn union(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    if a[2] == 0 || a[3] == 0 {
        return b;
    }
    if b[2] == 0 || b[3] == 0 {
        return a;
    }
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    [
        x,
        y,
        (a[0] + a[2]).max(b[0] + b[2]) - x,
        (a[1] + a[3]).max(b[1] + b[3]) - y,
    ]
}
fn bounds(drawings: &[Drawing], size: [u32; 2]) -> [u32; 4] {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for d in drawings {
        let [a, b, c, e, x, y] = d.transform;
        for segment in d
            .path
            .iter()
            .chain(d.feather.iter().flat_map(|f| f.path.iter()))
        {
            let points: &[[f64; 2]] = match segment {
                Segment::Move(p) | Segment::Line(p) => std::slice::from_ref(p),
                Segment::Quad(p, q) => &[*p, *q],
                Segment::Cubic(p, q, r) => &[*p, *q, *r],
                Segment::Close => &[],
            };
            for p in points {
                let p = [a * p[0] + c * p[1] + x, b * p[0] + e * p[1] + y];
                for i in 0..2 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
        }
    }
    if !lo[0].is_finite() {
        return [0; 4];
    }
    let lo =
        std::array::from_fn::<_, 2, _>(|i| (lo[i].floor() - 1.).clamp(0., size[i] as f64) as u32);
    let hi =
        std::array::from_fn::<_, 2, _>(|i| (hi[i].ceil() + 1.).clamp(0., size[i] as f64) as u32);
    [
        lo[0],
        lo[1],
        hi[0].saturating_sub(lo[0]),
        hi[1].saturating_sub(lo[1]),
    ]
}
