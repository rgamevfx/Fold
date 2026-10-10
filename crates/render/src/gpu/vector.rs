//! Retained source geometry, compact instances, GPU transforms and ordered
//! visibility bitsets. All allocations share completion-owned host reservations.
use super::{
    Host,
    host::{Image, Reservation},
    validation::Status,
};
use crate::{
    vector::Drawing,
    vector_geometry::Triangle,
    vector_instances::{Instance, Job, Node, Scene, Source, Sources},
};
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Weak},
};
use wgpu::util::DeviceExt;
#[cfg(test)]
#[path = "vector_tests.rs"]
mod tests;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_TILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Report {
    pub upload_bytes: u64,
    pub geometry_upload_bytes: u64,
    pub instance_upload_bytes: u64,
    pub prepare_nanoseconds: u128,
    pub instances: u64,
    pub unique_triangles: u64,
    pub execution_triangles: u64,
    pub passes: u32,
}
struct Atlas {
    // Identity only: retained GPU buffers must not pin evicted CPU hierarchies.
    // Weak handles also prevent allocation-address reuse while a key is cached.
    sources: Vec<Weak<Source>>,
    triangles: wgpu::Buffer,
    nodes: wgpu::Buffer,
    reservation: Arc<Reservation>,
    bytes: usize,
}
struct Packet {
    atlas: Arc<Atlas>,
    // instances, jobs, screen triangles, screen nodes, tile bitsets, params
    buffers: [wgpu::Buffer; 6],
    reservation: Arc<Reservation>,
    instances: u32,
    jobs: u32,
    words: u32,
    unique_triangles: usize,
    execution_triangles: usize,
}
struct Entry {
    drawings: Vec<Drawing>,
    size: [u32; 2],
    packet: Arc<Packet>,
    bytes: usize,
}
pub(super) struct VectorPipeline {
    prepare: wgpu::ComputePipeline,
    bin: wgpu::ComputePipeline,
    render: wgpu::ComputePipeline,
    scalar: Option<wgpu::ComputePipeline>,
    host: Host,
    sources: Sources,
    atlases: VecDeque<Arc<Atlas>>,
    atlas_bytes: usize,
    cache: VecDeque<Entry>,
    bytes: usize,
}
impl VectorPipeline {
    pub fn new(host: &Host) -> Self {
        let module = host
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("vector"),
                source: wgpu::ShaderSource::Wgsl(
                    concat!(
                        include_str!("vector_coverage.wgsl"),
                        "\n",
                        include_str!("vector.wgsl")
                    )
                    .into(),
                ),
            });
        let pipeline = |entry| {
            host.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: None,
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
        };
        Self {
            prepare: pipeline("prepare"),
            bin: pipeline("bin"),
            render: pipeline("main"),
            scalar: None,
            host: host.clone(),
            sources: Sources::default(),
            atlases: VecDeque::new(),
            atlas_bytes: 0,
            cache: VecDeque::new(),
            bytes: 0,
        }
    }
    pub fn clear(&mut self) {
        self.cache.clear();
        self.bytes = 0;
        self.atlases.clear();
        self.atlas_bytes = 0;
        self.sources = Sources::default();
    }
    fn reserve(&mut self, bytes: u64) -> Result<Arc<Reservation>, fold_foundation::Failure> {
        match self
            .host
            .reserve_as(bytes, super::host::AllocationKind::Geometry)
        {
            Ok(r) => Ok(r),
            Err(_) => {
                self.cache.clear();
                self.bytes = 0;
                self.atlases.clear();
                self.atlas_bytes = 0;
                self.host
                    .reserve_as(bytes, super::host::AllocationKind::Geometry)
            }
        }
    }
    fn check_buffer(&self, size: u64) -> Result<(), fold_foundation::Failure> {
        let limits = self.host.device().limits();
        let limit = limits
            .max_buffer_size
            .min(u64::from(limits.max_storage_buffer_binding_size));
        if size > limit {
            return Err(format!(
                "vector buffer budget exceeded: {size} bytes, device limit {limit}"
            )
            .into());
        }
        Ok(())
    }
    fn upload(&self, bytes: &[u8], minimum: usize, uniform: bool) -> wgpu::Buffer {
        let empty = vec![0; minimum];
        self.host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Immutable vector data"),
                contents: if bytes.is_empty() { &empty } else { bytes },
                usage: if uniform {
                    wgpu::BufferUsages::UNIFORM
                } else {
                    wgpu::BufferUsages::STORAGE
                },
            })
    }
    fn storage(&self, bytes: u64) -> wgpu::Buffer {
        self.host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU prepared vector data"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
    }
    fn atlas(&mut self, scene: &Scene) -> Result<(Arc<Atlas>, u64), fold_foundation::Failure> {
        if let Some(index) = self.atlases.iter().position(|a| {
            a.sources.len() == scene.sources.len()
                && a.sources
                    .iter()
                    .zip(&scene.sources)
                    .all(|(a, b)| a.as_ptr() == Arc::as_ptr(b))
        }) {
            let atlas = self.atlases.remove(index).unwrap();
            self.atlases.push_back(atlas.clone());
            return Ok((atlas, 0));
        }
        let triangles: Vec<Triangle> = scene
            .sources
            .iter()
            .flat_map(|s| s.triangles.iter().copied())
            .collect();
        let nodes: Vec<Node> = scene
            .sources
            .iter()
            .flat_map(|s| s.nodes.iter().copied())
            .collect();
        let triangle_bytes = triangles.len().max(1) * size_of::<Triangle>();
        let node_bytes = nodes.len().max(1) * size_of::<Node>();
        self.check_buffer(triangle_bytes as u64)?;
        self.check_buffer(node_bytes as u64)?;
        let bytes = triangle_bytes + node_bytes;
        while !self.atlases.is_empty()
            && (self.atlas_bytes + bytes > CACHE_BYTES || self.atlases.len() >= 8)
        {
            self.atlas_bytes -= self.atlases.pop_front().unwrap().bytes;
        }
        let reservation = self.reserve(bytes as u64)?;
        let atlas = Arc::new(Atlas {
            sources: scene.sources.iter().map(Arc::downgrade).collect(),
            triangles: self.upload(
                bytemuck::cast_slice(&triangles),
                size_of::<Triangle>(),
                false,
            ),
            nodes: self.upload(bytemuck::cast_slice(&nodes), size_of::<Node>(), false),
            reservation,
            bytes,
        });
        self.atlas_bytes += bytes;
        self.atlases.push_back(atlas.clone());
        Ok((atlas, bytes as u64))
    }
    fn packet(
        &mut self,
        drawings: &[Drawing],
        size: [u32; 2],
        cancel: &fold_media::Cancel,
    ) -> Result<(Arc<Packet>, Report), fold_foundation::Failure> {
        cancel.check()?;
        if let Some(index) = self
            .cache
            .iter()
            .position(|e| e.size == size && e.drawings == drawings)
        {
            let entry = self.cache.remove(index).unwrap();
            let packet = entry.packet.clone();
            self.cache.push_back(entry);
            return Ok((packet, Report::default()));
        }
        let scene = self.sources.prepare(drawings, size, cancel)?;
        let (atlas, geometry_upload_bytes) = self.atlas(&scene)?;
        let words = (scene.instances.len() as u32).div_ceil(32);
        let tile_bytes = (u64::from(size[0].div_ceil(16))
            * u64::from(size[1].div_ceil(16))
            * u64::from(words)
            * 4)
        .max(4);
        if tile_bytes > MAX_TILE_BYTES {
            return Err(format!(
                "vector visibility budget exceeded: {tile_bytes} bytes, limit {MAX_TILE_BYTES}"
            )
            .into());
        }
        let lengths = [
            (scene.instances.len().max(1) * size_of::<Instance>()) as u64,
            (scene.jobs.len().max(1) * size_of::<Job>()) as u64,
            (scene.triangles.max(1) * size_of::<Triangle>()) as u64,
            (scene.nodes.max(1) * size_of::<Node>()) as u64,
            tile_bytes,
            32,
        ];
        for bytes in lengths {
            self.check_buffer(bytes)?;
        }
        let allocation: u64 = lengths.iter().sum();
        let mut paths = HashSet::new();
        let path_bytes: usize = drawings
            .iter()
            .filter(|d| paths.insert(Arc::as_ptr(&d.path)))
            .map(|d| d.path.capacity() * size_of::<crate::vector::Segment>())
            .sum();
        let bytes = allocation as usize + std::mem::size_of_val(drawings) + path_bytes;
        while !self.cache.is_empty() && (self.bytes + bytes > CACHE_BYTES || self.cache.len() >= 8)
        {
            self.bytes -= self.cache.pop_front().unwrap().bytes;
        }
        let reservation = self.reserve(allocation)?;
        let instances = scene.instances.len() as u32;
        let jobs = scene.jobs.len() as u32;
        let params = [
            size[0],
            size[1],
            size[0].div_ceil(16),
            words,
            instances,
            jobs,
            0,
            0,
        ];
        let buffers = [
            self.upload(
                bytemuck::cast_slice(&scene.instances),
                size_of::<Instance>(),
                false,
            ),
            self.upload(bytemuck::cast_slice(&scene.jobs), size_of::<Job>(), false),
            self.storage(lengths[2]),
            self.storage(lengths[3]),
            self.storage(lengths[4]),
            self.upload(bytemuck::cast_slice(&params), 32, true),
        ];
        let packet = Arc::new(Packet {
            atlas,
            buffers,
            reservation,
            instances,
            jobs,
            words,
            unique_triangles: scene.unique_triangles,
            execution_triangles: scene.triangles,
        });
        cancel.check()?;
        if bytes <= CACHE_BYTES {
            self.bytes += bytes;
            self.cache.push_back(Entry {
                drawings: drawings.to_vec(),
                size,
                packet: packet.clone(),
                bytes,
            });
        }
        let instance_upload_bytes = lengths[0] + lengths[1] + lengths[5];
        Ok((
            packet,
            Report {
                upload_bytes: geometry_upload_bytes + instance_upload_bytes,
                geometry_upload_bytes,
                instance_upload_bytes,
                ..Default::default()
            },
        ))
    }
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        drawings: &[Drawing],
        output: &Image,
        status: &Status,
        reservations: &mut Vec<Arc<Reservation>>,
        cancel: &fold_media::Cancel,
    ) -> Result<Report, fold_foundation::Failure> {
        let begin = std::time::Instant::now();
        if output.texture.format() == wgpu::TextureFormat::R32Float && self.scalar.is_none() {
            let source = concat!(
                include_str!("vector_coverage.wgsl"),
                "\n",
                include_str!("vector.wgsl")
            )
            .replace(
                "texture_storage_2d<rgba32float,write>",
                "texture_storage_2d<r32float,write>",
            );
            let module = self
                .host
                .device()
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Scalar native vector coverage"),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
            self.scalar = Some(self.host.device().create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("Scalar native vector coverage"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            ));
        }
        let (packet, mut report) = self.packet(drawings, output.dimensions, cancel)?;
        reservations.push(packet.reservation.clone());
        reservations.push(packet.atlas.reservation.clone());
        let buffers = [
            &packet.atlas.triangles,
            &packet.atlas.nodes,
            &packet.buffers[0],
            &packet.buffers[1],
            &packet.buffers[2],
            &packet.buffers[3],
            &packet.buffers[4],
            &packet.buffers[5],
        ];
        let encode = |encoder: &mut wgpu::CommandEncoder,
                      pipeline: &wgpu::ComputePipeline,
                      bindings: &[usize],
                      render: bool,
                      groups: [u32; 3]| {
            let mut entries: Vec<_> = bindings
                .iter()
                .map(|&i| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: buffers[i].as_entire_binding(),
                })
                .collect();
            if render {
                entries.push(wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&output.view),
                });
                entries.push(wgpu::BindGroupEntry {
                    binding: 9,
                    resource: status.gpu.as_entire_binding(),
                });
            }
            let group = self
                .host
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &entries,
                });
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
        };
        // Prepare on every encode, including cache hits. An earlier encoder may
        // have been cancelled before submission; retained packets are not proof
        // that GPU writes occurred. Identical writes are ordered by the host queue.
        if packet.jobs != 0 {
            encode(
                encoder,
                &self.prepare,
                &[0, 1, 2, 3, 4, 5, 7],
                false,
                [packet.jobs.min(65535), packet.jobs.div_ceil(65535), 1],
            );
            report.passes += 1;
        }
        if packet.words != 0 {
            let tiles = output.dimensions[0].div_ceil(16) * output.dimensions[1].div_ceil(16);
            encode(
                encoder,
                &self.bin,
                &[2, 5, 6, 7],
                false,
                [
                    packet.words.div_ceil(64),
                    tiles.min(65535),
                    tiles.div_ceil(65535),
                ],
            );
            report.passes += 1;
        }
        encode(
            encoder,
            if output.texture.format() == wgpu::TextureFormat::R32Float {
                self.scalar.as_ref().unwrap()
            } else {
                &self.render
            },
            &[2, 4, 5, 6, 7],
            true,
            [
                output.dimensions[0].div_ceil(8),
                output.dimensions[1].div_ceil(8),
                1,
            ],
        );
        report.passes += 1;
        report.instances = u64::from(packet.instances);
        report.unique_triangles = packet.unique_triangles as u64;
        report.execution_triangles = packet.execution_triangles as u64;
        report.prepare_nanoseconds = begin.elapsed().as_nanos();
        Ok(report)
    }
}
