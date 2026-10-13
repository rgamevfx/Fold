//! Bounded native-sized mipmapped images and full-frame sampling.
use super::{
    Host,
    host::{AllocationKind, Image, Reservation},
};
use crate::baked::{Format, Texture};
use std::{collections::VecDeque, sync::Arc};
struct Entry {
    key: String,
    image: Arc<Image>,
    lease: Arc<Reservation>,
}
#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Entry>,
    bytes: u64,
}
impl Cache {
    pub fn evict_one(&mut self) -> bool {
        if let Some(entry) = self.entries.pop_front() {
            self.bytes -= entry.image.bytes;
            true
        } else {
            false
        }
    }
    pub fn acquire(
        &mut self,
        host: &Host,
        source: &Arc<Texture>,
    ) -> Result<(Arc<Image>, Vec<Arc<Reservation>>, u64), fold_foundation::Failure> {
        let key = source.identity.clone();
        if let Some(i) = self.entries.iter().position(|e| e.key == key) {
            let e = self.entries.remove(i).unwrap();
            let result = (e.image.clone(), vec![e.lease.clone()], 0);
            self.entries.push_back(e);
            return Ok(result);
        }
        let first = source;
        let [w, h] = first.dimensions();
        let format = match first.format {
            Format::Bc7 => wgpu::TextureFormat::Bc7RgbaUnorm,
            Format::Float16 => wgpu::TextureFormat::Rgba16Float,
            Format::Auto => return Err("unresolved baked format".into()),
        };
        if first.format == Format::Bc7
            && !host
                .device()
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        {
            return Err("BC7 unavailable on this GPU; rebuild as Float16".into());
        }
        let bytes = source.bytes();
        let limit = (host.memory().budget / 4).min(128 * 1024 * 1024);
        while self.bytes + bytes > limit || self.entries.len() >= 64 {
            if let Some(e) = self.entries.pop_front() {
                self.bytes -= e.image.bytes;
            } else {
                break;
            }
        }
        let lease = host.reserve_as(bytes, AllocationKind::Decoded)?;
        let upload = host.reserve(bytes)?;
        // Compressed base dimensions must be block aligned. Baking supplies a
        // block-aligned base; lower mip levels may end at a partial block.
        if first.format == Format::Bc7 && (!w.is_multiple_of(4) || !h.is_multiple_of(4)) {
            return Err("BC7 base dimensions must be multiples of four".into());
        }
        let texture = host.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Baked image"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: first.levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (mip, l) in source.levels.iter().enumerate() {
            let (width, height, row, rows) = if first.format == Format::Bc7 {
                (
                    l.width.div_ceil(4) * 4,
                    l.height.div_ceil(4) * 4,
                    l.width.div_ceil(4) * 16,
                    l.height.div_ceil(4),
                )
            } else {
                (l.width, l.height, l.width * 8, l.height)
            };
            host.queue().write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip as u32,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &l.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(rows),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });
        let image = Arc::new(Image {
            texture,
            view,
            dimensions: [w, h],
            bytes,
            _reservation: None,
        });
        if bytes <= limit {
            self.entries.push_back(Entry {
                key,
                image: image.clone(),
                lease: lease.clone(),
            });
            self.bytes += bytes;
        }
        Ok((image, vec![lease, upload], bytes))
    }
}

pub(super) struct Sampling {
    pipeline: wgpu::ComputePipeline,
}
impl Sampling {
    pub fn new(host: &Host) -> Self {
        let shader = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("baked.wgsl"));
        Self {
            pipeline: host
                .device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Baked image sampling"),
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
        source: &Image,
        output: &Image,
        format: Format,
        status: &super::validation::Status,
    ) -> Result<Arc<Reservation>, fold_foundation::Failure> {
        use wgpu::util::DeviceExt;
        let reservation = host.reserve(16)?;
        let uniform = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Baked image format"),
                contents: bytemuck::cast_slice(&[u32::from(format == Format::Bc7), 0, 0, 0]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source.view),
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
                    resource: status.gpu.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            output.dimensions[0].div_ceil(8),
            output.dimensions[1].div_ceil(8),
            1,
        );
        Ok(reservation)
    }
}
