//! Feature-independent immutable image ingress. The native adapter establishes
//! FD provenance; this module owns host accounting, device binding and readiness.
use super::{GpuFrame, Host, Statistics, host, validation};
use crate::WorkingSpace;
use fold_foundation::Failure;
use fold_gpu_sharing::{Description, Imported};
use std::sync::{Arc, atomic::Ordering};

/// Reserve before importing an external allocation. The shared producer/import
/// memory is counted once; the compositor copy is a separate allocation.
pub struct SharedImage {
    host: Host,
    description: Description,
    source: Arc<host::Reservation>,
    destination: Arc<host::Reservation>,
    status: validation::Status,
}
/// The external producer's allocation remains counted until release is
/// acknowledged or its process is reaped, independently of the import/copy.
#[derive(Clone)]
pub struct SharedSource {
    _source: Arc<host::Reservation>,
}
impl Host {
    pub fn prepare_shared_image(&self, dimensions: [u32; 2]) -> Result<SharedImage, Failure> {
        let description = fold_gpu_sharing::describe(self.device(), dimensions)?;
        self.reserve_shared_image(&description)
    }
    pub fn reserve_shared_image(&self, description: &Description) -> Result<SharedImage, Failure> {
        self.check()?;
        let [width, height] = description.dimensions;
        let limit = self.device().limits().max_texture_dimension_2d.min(16_384);
        if width == 0 || height == 0 || width > limit || height > limit {
            return Err("Unsupported shared image dimensions".into());
        }
        if fold_gpu_sharing::identity(self.device())? != (description.device, description.driver) {
            return Err("Shared image belongs to a different GPU or driver".into());
        }
        let pixels = u64::from(width) * u64::from(height) * 8;
        if description.allocation_bytes < pixels {
            return Err("Shared image allocation is smaller than its pixels".into());
        }
        let source =
            self.reserve_as(description.allocation_bytes, host::AllocationKind::Working)?;
        let destination = self.reserve_as(pixels, host::AllocationKind::Working)?;
        let status = validation::Status::new(self)?;
        Ok(SharedImage {
            host: self.clone(),
            description: description.clone(),
            source,
            destination,
            status,
        })
    }
}
impl SharedImage {
    pub fn description(&self) -> &Description {
        &self.description
    }
    pub fn retain_source(&self) -> SharedSource {
        SharedSource {
            _source: self.source.clone(),
        }
    }
    /// Copies an already completed native allocation. Source and destination
    /// reservations survive submitted work even if this frame is dropped.
    /// The source must contain premultiplied scene pixels in `working_space`.
    /// Color is the native adapter's explicit contract; it is not inferred from
    /// an FD. GPU status validation rejects nonfinite RGB and invalid alpha
    /// before readiness; only status bytes leave the GPU.
    pub fn receive(
        self,
        imported: Imported,
        working_space: WorkingSpace,
        config_identity: Option<String>,
    ) -> Result<GpuFrame, Failure> {
        if imported.description() != &self.description || !imported.belongs_to(self.host.device()) {
            return Err("Shared image does not match its host reservation".into());
        }
        let dimensions = self.description.dimensions;
        let bytes = u64::from(dimensions[0]) * u64::from(dimensions[1]) * 8;
        let destination = self.destination;
        let (texture, _) =
            imported.copy_reserved(self.host.queue(), (self.source, destination.clone()))?;
        let view = texture.create_view(&Default::default());
        let image = Arc::new(host::Image {
            texture,
            view,
            dimensions,
            bytes,
            _reservation: Some(destination),
        });
        let status = self.status;
        let mut encoder = self
            .host
            .device()
            .create_command_encoder(&Default::default());
        status.start(&mut encoder);
        self.host
            .shared_validation()
            .encode(self.host.device(), &image, &status, &mut encoder);
        let status_bytes = status.readback_bytes();
        status.encode(&mut encoder);
        self.host.submit(encoder, vec![image.clone()], vec![]);
        let ready = status.submitted();
        self.host.check()?;
        Ok(GpuFrame {
            image,
            host: self.host,
            ready,
            dependencies: vec![],
            working_space,
            config_identity,
            timing: None,
            lease_id: host::NEXT_ID.fetch_add(1, Ordering::Relaxed),
            statistics: Statistics {
                shared_gpu_copy_bytes: bytes,
                status_readback_bytes: status_bytes,
                ..Default::default()
            },
        })
    }
}

pub(super) struct Validation(wgpu::ComputePipeline);
impl Validation {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fold shared image validation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shared.wgsl").into()),
        });
        Self(
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Fold shared image validation"),
                layout: None,
                module: &shader,
                entry_point: Some("validate"),
                compilation_options: Default::default(),
                cache: None,
            }),
        )
    }
    fn encode(
        &self,
        device: &wgpu::Device,
        image: &host::Image,
        status: &validation::Status,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.0.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&image.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: status.gpu.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.0);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(
            image.dimensions[0].div_ceil(8),
            image.dimensions[1].div_ceil(8),
            1,
        );
    }
}
