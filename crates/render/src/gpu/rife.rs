//! Retained native inference session, bounded tensor storage, and float reconstruction.
use super::{
    Host,
    host::{Image, Reservation},
};
use fold_foundation::Failure;
use std::sync::Arc;
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    size: [u32; 4],
    control: [f32; 4],
}
struct Tensors {
    size: [u32; 2],
    buffers: [wgpu::Buffer; 5],
    _reservation: Arc<Reservation>,
}
pub(super) struct Adapter {
    runtime: Option<fold_rife::Runtime>,
    native_reservation: Option<Arc<Reservation>>,
    tensors: Option<Tensors>,
    prepare: wgpu::ComputePipeline,
    reconstruct: wgpu::ComputePipeline,
    cut: wgpu::ComputePipeline,
    pair: Option<(usize, usize, [u32; 2], u32, bool)>,
    cut_value: Option<f32>,
}
impl Adapter {
    pub fn new(host: &Host) -> Self {
        let pipeline = |label, source: &str| {
            let module = host
                .device()
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(label),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
            host.device()
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(label),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                })
        };
        Self {
            runtime: None,
            native_reservation: None,
            tensors: None,
            prepare: pipeline("RIFE input preparation", include_str!("rife_prepare.wgsl")),
            reconstruct: pipeline(
                "RIFE float reconstruction",
                include_str!("rife_reconstruct.wgsl"),
            ),
            cut: pipeline("RIFE cut detection", include_str!("rife_cut.wgsl")),
            pair: None,
            cut_value: None,
        }
    }
    pub fn begin(&mut self) {
        self.pair = None;
        self.cut_value = None;
    }
    fn tensors(&mut self, host: &Host, size: [u32; 2]) -> Result<(), Failure> {
        if self.tensors.as_ref().is_some_and(|t| t.size == size) {
            return Ok(());
        }
        self.tensors = None;
        self.runtime = None;
        self.native_reservation = None;
        self.pair = None;
        self.cut_value = None;
        let bytes = u64::from(size[0]) * u64::from(size[1]) * 4;
        if bytes * 4 > u64::from(host.device().limits().max_storage_buffer_binding_size) {
            return Err("RIFE motion resolution exceeds GPU storage limits; select Half or Quarter motion resolution".into());
        }
        let reservation = host.reserve(bytes * 12)?;
        let buffers = [3, 3, 1, 4, 1].map(|c| {
            host.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("RIFE float tensor"),
                size: bytes * c,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        self.tensors = Some(Tensors {
            size,
            buffers,
            _reservation: reservation,
        });
        Ok(())
    }
    pub fn run(
        &mut self,
        host: &Host,
        first: &Image,
        second: &Image,
        output: &Image,
        ids: (usize, usize),
        fraction: f32,
        scale: u32,
        cut_threshold: f32,
        aces: bool,
        status: &super::validation::Status,
        cancel: &fold_media::Cancel,
    ) -> Result<(u128, u64, bool), Failure> {
        cancel.check()?;
        let [w, h] = output.dimensions;
        let size = [
            w.div_ceil(scale).div_ceil(32) * 32,
            h.div_ceil(scale).div_ceil(32) * 32,
        ];
        self.tensors(host, size)?;
        let key = (ids.0, ids.1, output.dimensions, scale, aces);
        let reuse = self.pair == Some(key);
        if !reuse {
            self.cut_value = None;
        }
        let params = Params {
            size: [w, h, size[0], size[1]],
            control: [fraction, f32::from(aces), scale as f32, f32::from(reuse)],
        };
        let uniform = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("RIFE parameters"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let mut readbacks = 0;
        let cut = if cut_threshold > 0. {
            let value = if let Some(value) = self.cut_value {
                value
            } else {
                let score = host.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("RIFE cut score"),
                    size: 4,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let read = host.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("RIFE cut score readback"),
                    size: 4,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.cut.get_bind_group_layout(0),
                    entries: &[
                        entry(0, uniform.as_entire_binding()),
                        entry(1, wgpu::BindingResource::TextureView(&first.view)),
                        entry(2, wgpu::BindingResource::TextureView(&second.view)),
                        entry(3, score.as_entire_binding()),
                    ],
                });
                let mut encoder = host.device().create_command_encoder(&Default::default());
                encoder.clear_buffer(&score, 0, None);
                {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&self.cut);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(4, 3, 1);
                }
                encoder.copy_buffer_to_buffer(&score, 0, &read, 0, 4);
                host.queue().submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r);
                });
                host.device()
                    .poll(wgpu::PollType::wait_indefinitely())
                    .map_err(|e| e.to_string())?;
                rx.recv()
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;
                let value =
                    u32::from_le_bytes(read.slice(..).get_mapped_range()[..4].try_into().unwrap())
                        as f32
                        / (576. * 65535.);
                read.unmap();
                readbacks += 4;
                self.cut_value = Some(value);
                value
            };
            value > cut_threshold
        } else {
            false
        };
        let started = std::time::Instant::now();
        if !cut {
            if self.runtime.is_none() {
                // Account model, tensor interchange and a conservative working-set ceiling.
                let reservation = host
                    .reserve(256 * 1024 * 1024 + u64::from(size[0]) * u64::from(size[1]) * 512)?;
                let mut runtime = fold_rife::Runtime::new(host.device(), host.queue())?;
                runtime.prepare(size)?;
                self.native_reservation = Some(reservation);
                self.runtime = Some(runtime);
            }
            let t = self.tensors.as_ref().unwrap();
            let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.prepare.get_bind_group_layout(0),
                entries: &[
                    entry(0, uniform.as_entire_binding()),
                    entry(1, wgpu::BindingResource::TextureView(&first.view)),
                    entry(2, wgpu::BindingResource::TextureView(&second.view)),
                    entry(3, t.buffers[0].as_entire_binding()),
                    entry(4, t.buffers[1].as_entire_binding()),
                    entry(5, t.buffers[2].as_entire_binding()),
                ],
            });
            let mut encoder = host.device().create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.prepare);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
            }
            let runtime = self.runtime.as_mut().unwrap();
            runtime.upload_and_run(
                encoder,
                [&t.buffers[0], &t.buffers[1], &t.buffers[2]],
                reuse,
            )?;
            let mut encoder = host.device().create_command_encoder(&Default::default());
            runtime.download_gpu(&mut encoder, &t.buffers[3], &t.buffers[4])?;
            host.queue().submit([encoder.finish()]);
            runtime.finish()?;
            self.pair = Some(key);
        }
        let elapsed = started.elapsed().as_nanos();
        cancel.check()?;
        let params = Params {
            control: [fraction, f32::from(aces), scale as f32, f32::from(cut)],
            ..params
        };
        let uniform = host
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("RIFE reconstruction parameters"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let t = self.tensors.as_ref().unwrap();
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.reconstruct.get_bind_group_layout(0),
            entries: &[
                entry(0, uniform.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(&first.view)),
                entry(2, wgpu::BindingResource::TextureView(&second.view)),
                entry(3, t.buffers[3].as_entire_binding()),
                entry(4, t.buffers[4].as_entire_binding()),
                entry(5, wgpu::BindingResource::TextureView(&output.view)),
                entry(6, status.gpu.as_entire_binding()),
            ],
        });
        let mut encoder = host.device().create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.reconstruct);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        host.queue().submit([encoder.finish()]);
        host.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        Ok((elapsed, readbacks, cut))
    }
}
fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}
