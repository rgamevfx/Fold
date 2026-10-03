//! GPU analytic coverage and float paint compositing, with bounded immutable
//! geometry and uploaded-packet retention. No CPU coverage image is produced.
use super::{
    Host,
    host::{Image, Reservation},
    validation::Status,
};
use crate::{
    vector::Drawing,
    vector_geometry::{self, GeometryCache},
};
use std::{collections::VecDeque, sync::Arc};
use wgpu::util::DeviceExt;
#[cfg(test)]
#[path = "vector_tests.rs"]
mod tests;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
struct Packet {
    buffers: Vec<wgpu::Buffer>,
    reservation: Arc<Reservation>,
}
struct Entry {
    drawings: Vec<Drawing>,
    size: [u32; 2],
    packet: Arc<Packet>,
    bytes: usize,
}
pub(super) struct VectorPipeline {
    pipeline: wgpu::ComputePipeline,
    host: Host,
    geometry: GeometryCache,
    cache: VecDeque<Entry>,
    bytes: usize,
}
impl VectorPipeline {
    pub fn new(host: &Host) -> Self {
        let module = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("vector.wgsl"));
        Self {
            host: host.clone(),
            geometry: GeometryCache::default(),
            cache: VecDeque::new(),
            bytes: 0,
            pipeline: host
                .device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Analytic float vector coverage"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                }),
        }
    }
    pub fn clear(&mut self) {
        self.cache.clear();
        self.bytes = 0;
    }
    fn packet(
        &mut self,
        drawings: &[Drawing],
        size: [u32; 2],
        cancel: &fold_media::Cancel,
    ) -> Result<(Arc<Packet>, u64), String> {
        cancel.check()?;
        if let Some(index) = self
            .cache
            .iter()
            .position(|e| e.size == size && e.drawings == drawings)
        {
            let entry = self.cache.remove(index).unwrap();
            let packet = entry.packet.clone();
            self.cache.push_back(entry);
            return Ok((packet, 0));
        }
        let prepared = vector_geometry::prepare(&mut self.geometry, drawings, size, cancel)?;
        let geometry = vector_geometry::bin(prepared, size, cancel)?;
        let params = [size[0], size[1], size[0].div_ceil(16), 0];
        let contents: [&[u8]; 4] = [
            bytemuck::cast_slice(&geometry.triangles),
            bytemuck::cast_slice(&geometry.paints),
            bytemuck::cast_slice(&geometry.tiles),
            bytemuck::cast_slice(&params),
        ];
        let upload = contents.iter().map(|v| v.len() as u64).sum();
        let bytes = upload as usize
            + std::mem::size_of_val(drawings)
            + drawings
                .iter()
                .map(|d| d.path.capacity() * std::mem::size_of::<crate::vector::Segment>())
                .sum::<usize>();
        while !self.cache.is_empty() && (self.bytes + bytes > CACHE_BYTES || self.cache.len() >= 8)
        {
            let old = self.cache.pop_front().unwrap();
            self.bytes -= old.bytes;
        }
        let reservation = match self
            .host
            .reserve_as(upload, super::host::AllocationKind::Geometry)
        {
            Ok(r) => r,
            Err(_) => {
                self.cache.clear();
                self.bytes = 0;
                self.host
                    .reserve_as(upload, super::host::AllocationKind::Geometry)?
            }
        };
        let buffers = contents
            .iter()
            .enumerate()
            .map(|(i, contents)| {
                self.host
                    .device()
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Immutable vector geometry"),
                        contents,
                        usage: if i == 3 {
                            wgpu::BufferUsages::UNIFORM
                        } else {
                            wgpu::BufferUsages::STORAGE
                        },
                    })
            })
            .collect();
        let packet = Arc::new(Packet {
            buffers,
            reservation,
        });
        if bytes <= CACHE_BYTES {
            self.bytes += bytes;
            self.cache.push_back(Entry {
                drawings: drawings.to_vec(),
                size,
                packet: packet.clone(),
                bytes,
            });
        }
        Ok((packet, upload))
    }
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        drawings: &[Drawing],
        output: &Image,
        status: &Status,
        reservations: &mut Vec<Arc<Reservation>>,
        cancel: &fold_media::Cancel,
    ) -> Result<(u64, u128), String> {
        let begin = std::time::Instant::now();
        let (packet, bytes) = self.packet(drawings, output.dimensions, cancel)?;
        let elapsed = begin.elapsed().as_nanos();
        reservations.push(packet.reservation.clone());
        let mut entries: Vec<_> = packet
            .buffers
            .iter()
            .enumerate()
            .map(|(binding, b)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::TextureView(&output.view),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 5,
            resource: status.gpu.as_entire_binding(),
        });
        let group = self
            .host
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.pipeline.get_bind_group_layout(0),
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
        Ok((bytes, elapsed))
    }
}
