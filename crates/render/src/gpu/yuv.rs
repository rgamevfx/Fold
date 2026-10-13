//! Native-plane ingress and GPU reconstruction. Retained immutable buffers are
//! shared across sizes and never overwritten while queued consumers can read.
use super::{
    Host,
    host::{Image, Reservation},
};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use wgpu::util::DeviceExt;
#[cfg(test)]
#[path = "yuv_layout_tests.rs"]
mod layout_tests;
#[cfg(test)]
#[path = "yuv_tests.rs"]
mod tests;
#[derive(Clone, Copy, Debug)]
pub(super) struct Layout {
    pub dimensions: [u32; 2],
    pub luma_stride: u32,
    pub chroma_stride: u32,
    pub chroma_offsets: [u32; 2],
    pub chroma_step: u32,
    pub format: fold_media::VideoFormat,
    pub sample_shift: u32,
}
impl Layout {
    fn parameters(self, size: u64) -> Result<[u32; 20], fold_foundation::Failure> {
        self.format_parameters(size, self.format, 0)
    }
    fn format_parameters(
        self,
        size: u64,
        format: fold_media::VideoFormat,
        alpha: u32,
    ) -> Result<[u32; 20], fold_foundation::Failure> {
        format.validate()?;
        let step = format.sample_bytes();
        let [w, h] = self.dimensions;
        let [cw, ch] = [
            w.div_ceil(format.subsampling[0]),
            h.div_ceil(format.subsampling[1]),
        ];
        if w == 0
            || h == 0
            || u64::from(w) * u64::from(h) > fold_media::MAX_PIXELS as u64
            || !(1..=4).contains(&self.chroma_step)
            || self.luma_stride < w * step
            || u64::from(self.chroma_stride) < u64::from(cw) * u64::from(self.chroma_step)
        {
            return Err("invalid GPU video plane layout".into());
        }
        let y_end = u64::from(h - 1) * u64::from(self.luma_stride) + u64::from(w * step);
        let chroma_end = u64::from(ch - 1) * u64::from(self.chroma_stride)
            + u64::from(cw - 1) * u64::from(self.chroma_step)
            + u64::from(step);
        if y_end > size
            || (format.alpha && u64::from(alpha) + y_end > size)
            || self
                .chroma_offsets
                .iter()
                .any(|v| u64::from(*v) + chroma_end > size)
        {
            return Err("GPU video plane exceeds its allocation".into());
        }
        Ok([
            w,
            h,
            self.chroma_stride,
            self.chroma_step,
            self.chroma_offsets[0],
            self.chroma_offsets[1],
            self.luma_stride,
            alpha,
            step,
            format.depth,
            format.subsampling[0],
            format.subsampling[1],
            u32::from(format.alpha),
            u32::from(format.full_range),
            format.chroma_origin[0],
            format.chroma_origin[1],
            cw,
            ch,
            self.sample_shift,
            u32::from(format.rgb),
        ])
    }
}
struct Entry {
    submitted: Arc<AtomicBool>,
    retention: Option<super::host::VideoRetention>,
    identity: u64,
    params: [u32; 20],
    bytes: u64,
    buffer: wgpu::Buffer,
    uniform: wgpu::Buffer,
    reservation: Arc<Reservation>,
}
pub(super) struct YuvPipeline {
    pipeline: wgpu::ComputePipeline,
    uploads: super::upload::Pool,
    cache: VecDeque<Arc<Entry>>,
    bytes: u64,
    external_layouts: VecDeque<([u32; 20], wgpu::Buffer, Arc<Reservation>)>,
}
impl YuvPipeline {
    pub fn new(host: &Host) -> Self {
        let shader = host
            .device()
            .create_shader_module(wgpu::include_wgsl!("yuv.wgsl"));
        Self {
            uploads: Default::default(),
            pipeline: host
                .device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Native YUV reconstruction"),
                    layout: None,
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                }),
            cache: VecDeque::new(),
            bytes: 0,
            external_layouts: VecDeque::new(),
        }
    }
    pub fn evict_one(&mut self) -> bool {
        if let Some(entry) = self.cache.pop_front() {
            self.bytes -= entry.bytes;
            true
        } else if self.external_layouts.pop_front().is_some() {
            true
        } else {
            self.uploads.evict_one()
        }
    }
    pub fn clear(&mut self) {
        self.cache.clear();
        self.uploads.clear();
        self.bytes = 0;
        self.external_layouts.clear();
    }
    pub fn encode(
        &mut self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        frame: &fold_media::YuvFrame,
        output: &Image,
        reservations: &mut Vec<Arc<Reservation>>,
        uploads: &mut Vec<super::upload::Upload>,
    ) -> Result<u64, fold_foundation::Failure> {
        let (entry, uploaded) = if let Some(index) = self
            .cache
            .iter()
            .position(|v| v.identity == frame.identity() && v.submitted.load(Ordering::Acquire))
        {
            let entry = self.cache.remove(index).unwrap();
            self.cache.push_back(entry.clone());
            (entry, 0)
        } else {
            let planes = frame.planes();
            let layout = Layout {
                dimensions: frame.dimensions(),
                luma_stride: planes[0].stride,
                chroma_stride: planes[1].stride,
                chroma_offsets: [planes[1].offset as u32, planes[2].offset as u32],
                chroma_step: frame.format().sample_bytes(),
                format: frame.format(),
                sample_shift: 0,
            };
            let bytes = (frame.bytes().len() as u64).div_ceil(4) * 4;
            let params =
                layout.format_parameters(bytes, frame.format(), planes[3].offset as u32)?;
            let limit = host.video_cache_budget();
            let mut reusable = None;
            while !self.cache.is_empty()
                && (self.bytes + bytes + 80 > limit || self.cache.len() >= 512)
            {
                let old = self.cache.pop_front().unwrap();
                self.bytes -= old.bytes;
                // Every encoded/submitted use retains this reservation. A cache
                // eviction alone does not authorize overwriting queued readers.
                if old.params == params && Arc::strong_count(&old.reservation) == 1 {
                    reusable = Arc::try_unwrap(old).ok();
                    if let Some(entry) = &mut reusable {
                        entry.retention = None;
                    }
                }
            }
            let mut entry = if let Some(mut entry) = reusable {
                entry.submitted = Arc::new(AtomicBool::new(false));
                entry
            } else {
                let reservation =
                    match host.reserve_as(bytes + 80, super::host::AllocationKind::Decoded) {
                        Ok(r) => r,
                        Err(_) => {
                            self.clear();
                            host.reserve_as(bytes + 80, super::host::AllocationKind::Decoded)?
                        }
                    };
                let buffer = host.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Retained native YUV"),
                    size: bytes,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let uniform = host
                    .device()
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Native video layout"),
                        contents: bytemuck::cast_slice(&params),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                Entry {
                    submitted: Arc::new(AtomicBool::new(false)),
                    retention: None,
                    identity: frame.identity(),
                    params,
                    bytes: bytes + 80,
                    buffer,
                    uniform,
                    reservation,
                }
            };
            entry.identity = frame.identity();
            let (mut upload, _) =
                self.uploads
                    .write_buffer(host, frame.bytes(), encoder, &entry.buffer)?;
            upload.publish_on_submit(entry.submitted.clone());
            uploads.push(upload);
            if entry.bytes <= limit {
                entry.retention = host.retain_video(entry.bytes);
            }
            let entry = Arc::new(entry);
            if entry.retention.is_some() {
                self.bytes += entry.bytes;
                self.cache.push_back(entry.clone());
            }
            (entry, bytes)
        };
        reservations.push(entry.reservation.clone());
        self.dispatch(host, encoder, &entry.buffer, &entry.uniform, output);
        Ok(uploaded)
    }
    /// Input ownership/readiness is supplied by the native decoder module; this
    /// records only reconstruction. At most sixteen immutable 80-byte layouts
    /// are retained; repeated frames/sizes require no new layout allocation.
    #[cfg(any(test, all(feature = "native-video", target_os = "linux")))]
    pub fn encode_external(
        &mut self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        buffer: &wgpu::Buffer,
        layout: Layout,
        output: &Image,
        reservations: &mut Vec<Arc<Reservation>>,
    ) -> Result<(), fold_foundation::Failure> {
        let params = layout.parameters(buffer.size())?;
        let entry = if let Some(index) = self
            .external_layouts
            .iter()
            .position(|(key, _, _)| key == &params)
        {
            self.external_layouts.remove(index).unwrap()
        } else {
            if self.external_layouts.len() >= 16 {
                self.external_layouts.pop_front();
            }
            let reservation = host.reserve(80)?;
            let uniform = host
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("External video layout"),
                    contents: bytemuck::cast_slice(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            (params, uniform, reservation)
        };
        reservations.push(entry.2.clone());
        self.dispatch(host, encoder, buffer, &entry.1, output);
        self.external_layouts.push_back(entry);
        Ok(())
    }
    fn dispatch(
        &self,
        host: &Host,
        encoder: &mut wgpu::CommandEncoder,
        buffer: &wgpu::Buffer,
        uniform: &wgpu::Buffer,
        output: &Image,
    ) {
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
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
                    resource: uniform.as_entire_binding(),
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
    }
}
