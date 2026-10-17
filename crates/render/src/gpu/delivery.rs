//! Alpha-preserving GPU output transform and packed readback for software encoders.
use super::{GpuFrame, Renderer, validation::Status};
use crate::{Rgba16Output, WorkingSpace};
use fold_media::Cancel;

impl Renderer {
    pub fn output_rgba16(
        &mut self,
        frame: &GpuFrame,
        processor: Option<&fold_color::Processor>,
        cancel: &Cancel,
    ) -> Result<Rgba16Output, fold_foundation::Failure> {
        self.output_rgba16_admitted(frame, processor, cancel, None)
    }

    pub fn output_rgba16_admitted(
        &mut self,
        frame: &GpuFrame,
        processor: Option<&fold_color::Processor>,
        cancel: &Cancel,
        class: Option<crate::scheduling::Class>,
    ) -> Result<Rgba16Output, fold_foundation::Failure> {
        self.output_rgba16_with_alpha(frame, processor, true, cancel, class)
    }

    /// Matte in working space before the output transform when alpha is disabled.
    pub fn output_rgba16_with_alpha(
        &mut self,
        frame: &GpuFrame,
        processor: Option<&fold_color::Processor>,
        preserve_alpha: bool,
        cancel: &Cancel,
        class: Option<crate::scheduling::Class>,
    ) -> Result<Rgba16Output, fold_foundation::Failure> {
        cancel.check()?;
        self.host.check()?;
        if frame.host.id() != self.host.id() {
            return Err("GPU scene belongs to a different host".into());
        }
        if let Some(processor) = processor {
            let source = match frame.working_space {
                WorkingSpace::LegacyLinearSrgb => fold_color::LINEAR_SRGB,
                WorkingSpace::AcesCg => fold_color::WORKING_SPACE,
            };
            if processor.source() != source
                || frame
                    .config_identity
                    .as_deref()
                    .is_some_and(|id| id != processor.config_identity())
            {
                return Err("output processor does not match scene color identity".into());
            }
        } else if frame.working_space != WorkingSpace::LegacyLinearSrgb {
            return Err("ACES delivery requires an output processor".into());
        }
        if self.delivery.is_none() {
            let shader = self
                .host
                .device()
                .create_shader_module(wgpu::include_wgsl!("delivery.wgsl"));
            self.delivery = Some(self.host.device().create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("Alpha delivery packing"),
                    layout: None,
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            ));
        }
        let (color, _) = self.color_pipeline(processor, false)?;
        let dimensions = frame.dimensions();
        let bytes = u64::from(dimensions[0]) * u64::from(dimensions[1]) * 8;
        if bytes > u64::from(self.host.device().limits().max_storage_buffer_binding_size) {
            return Err("packed delivery exceeds GPU storage buffer limit".into());
        }
        let reservation = self.host.reserve(bytes * 2 + 16)?;
        let output = self.host.image(dimensions)?;
        let device = self.host.device();
        let packed = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Packed RGBA16 delivery"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Packed delivery readback"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let status = Status::new(&self.host)?;
        let mut encoder = device.create_command_encoder(&Default::default());
        status.start(&mut encoder);
        color.encode(
            &self.host,
            &mut encoder,
            &frame.image,
            &output,
            !preserve_alpha,
            &status,
        )?;
        let pipeline = self.delivery.as_ref().unwrap();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Alpha delivery"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&output.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: packed.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: status.gpu.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(dimensions[0].div_ceil(8), dimensions[1].div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&packed, 0, &readback, 0, bytes);
        status.encode(&mut encoder);
        let admission = class
            .map(|class| crate::scheduling::Scheduler::shared().enter(class, cancel))
            .transpose()?;
        self.host.submit(
            encoder,
            vec![frame.image.clone(), output],
            vec![reservation.clone(), color.reservation.clone()],
        );
        drop(admission);
        let ready = status.submitted();
        let (send, receive) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        let begin = std::time::Instant::now();
        loop {
            self.host.poll()?;
            cancel.check()?;
            match receive.try_recv() {
                Ok(result) => {
                    result.map_err(|e| e.to_string())?;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("delivery readback disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if begin.elapsed().as_secs() >= 30 {
                return Err("delivery readback timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        // Pixel mapping alone does not authorize publication of invalid ancestors.
        while !frame.is_ready()? || !ready.ready()? {
            self.host.poll()?;
            cancel.check()?;
            if begin.elapsed().as_secs() >= 30 {
                return Err("delivery validation timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let storage = fold_media::budget::reserve_working(bytes)?;
        let mapped = readback.slice(..).get_mapped_range();
        let mut data = Vec::new();
        data.try_reserve_exact(bytes as usize)
            .map_err(|_| "delivery CPU allocation failed")?;
        data.extend_from_slice(&mapped);
        drop(mapped);
        readback.unmap();
        Ok(Rgba16Output {
            bytes: data,
            _storage: storage,
        })
    }
}
