use ash::vk;
use fold_foundation::Failure;
use serde::{Deserialize, Serialize};
#[cfg(feature = "consumer")]
use std::os::fd::{AsRawFd, IntoRawFd};
use std::{
    ffi::CStr,
    os::fd::{FromRawFd, OwnedFd},
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub dimensions: [u32; 2],
    pub device: [u8; 16],
    pub driver: [u8; 16],
    pub allocation_bytes: u64,
    pub memory_type: u32,
}
struct Instance {
    _entry: ash::Entry,
    raw: ash::Instance,
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            self.raw.destroy_instance(None);
        }
    }
}
struct Owner {
    queue_lock: Mutex<()>,
    instance: Arc<Instance>,
    raw: ash::Device,
    physical: vk::PhysicalDevice,
    queue: vk::Queue,
    family: u32,
    device: [u8; 16],
    driver: [u8; 16],
}
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = self.raw.device_wait_idle();
            self.raw.destroy_device(None);
        }
    }
}
/// Single render-thread producer. Selection must match the receiving device UUID.
pub struct Device {
    owner: Arc<Owner>,
}
pub struct Image {
    owner: Arc<Owner>,
    raw: vk::Image,
    memory: vk::DeviceMemory,
    description: Description,
}
impl Drop for Image {
    fn drop(&mut self) {
        // Foreign render backends may have submitted before returning an error
        // or unwinding. Never free their backing image while work is pending.
        let _queue = self
            .owner
            .queue_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        unsafe {
            let _ = self.owner.raw.queue_wait_idle(self.owner.queue);
            self.owner.raw.destroy_image(self.raw, None);
            self.owner.raw.free_memory(self.memory, None);
        }
    }
}
/// The producer allocation stays alive until the exported frame is released.
pub struct Export {
    pub description: Description,
    pub fd: OwnedFd,
    _image: Image,
}

fn error(error: impl std::fmt::Display) -> Failure {
    error.to_string().into()
}
fn ids(instance: &ash::Instance, physical: vk::PhysicalDevice) -> ([u8; 16], [u8; 16]) {
    let mut id = vk::PhysicalDeviceIDProperties::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
    unsafe {
        instance.get_physical_device_properties2(physical, &mut properties);
    }
    (id.device_uuid, id.driver_uuid)
}
#[cfg(feature = "consumer")]
pub fn identity(device: &wgpu::Device) -> Result<([u8; 16], [u8; 16]), Failure> {
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }
        .ok_or("GPU sharing requires Vulkan")?;
    Ok(ids(
        hal.shared_instance().raw_instance(),
        hal.raw_physical_device(),
    ))
}
/// Query the exact dedicated-image footprint without allocating GPU memory.
/// Matches the producer's image format, usage and device-local type selection.
#[cfg(feature = "consumer")]
pub fn describe(device: &wgpu::Device, dimensions: [u32; 2]) -> Result<Description, Failure> {
    unsafe {
        let hal = device
            .as_hal::<wgpu::hal::api::Vulkan>()
            .ok_or("GPU sharing requires Vulkan")?;
        let instance = hal.shared_instance().raw_instance();
        let physical = hal.raw_physical_device();
        let raw = hal.raw_device();
        let mut external = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let image = raw
            .create_image(&image_info(dimensions)?.push_next(&mut external), None)
            .map_err(error)?;
        let requirements = raw.get_image_memory_requirements(image);
        raw.destroy_image(image, None);
        let properties = instance.get_physical_device_memory_properties(physical);
        let memory_type = (0..properties.memory_type_count)
            .find(|i| {
                requirements.memory_type_bits & (1 << i) != 0
                    && properties.memory_types[*i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            })
            .ok_or("Shared image has no device-local memory")?;
        let (device, driver) = ids(instance, physical);
        Ok(Description {
            dimensions,
            device,
            driver,
            allocation_bytes: requirements.size,
            memory_type,
        })
    }
}
fn image_info(dimensions: [u32; 2]) -> Result<vk::ImageCreateInfo<'static>, Failure> {
    if dimensions.contains(&0) || dimensions.iter().any(|n| *n > 16384) {
        return Err("Invalid shared image dimensions".into());
    }
    Ok(vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::R16G16B16A16_SFLOAT)
        .extent(vk::Extent3D {
            width: dimensions[0],
            height: dimensions[1],
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
        )
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED))
}
fn range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

impl Device {
    /// Explicit standalone qualification selection. Application sharing must
    /// instead use the receiving device UUID with `new`.
    pub fn first_uuid() -> Result<[u8; 16], Failure> {
        unsafe {
            let entry = ash::Entry::load().map_err(error)?;
            let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_3);
            let raw = entry
                .create_instance(
                    &vk::InstanceCreateInfo::default().application_info(&app),
                    None,
                )
                .map_err(error)?;
            let instance = Instance { _entry: entry, raw };
            let physical = instance
                .raw
                .enumerate_physical_devices()
                .map_err(error)?
                .into_iter()
                .next()
                .ok_or("No Vulkan device")?;
            Ok(ids(&instance.raw, physical).0)
        }
    }
    pub fn new(device_uuid: [u8; 16]) -> Result<Self, Failure> {
        unsafe {
            let entry = ash::Entry::load().map_err(error)?;
            let app = vk::ApplicationInfo::default()
                .application_name(c"Fold GPU sharing")
                .api_version(vk::API_VERSION_1_3);
            let raw = entry
                .create_instance(
                    &vk::InstanceCreateInfo::default().application_info(&app),
                    None,
                )
                .map_err(error)?;
            let instance = Arc::new(Instance { _entry: entry, raw });
            let physical = instance
                .raw
                .enumerate_physical_devices()
                .map_err(error)?
                .into_iter()
                .find(|p| ids(&instance.raw, *p).0 == device_uuid)
                .ok_or("Shared GPU device is unavailable")?;
            let extensions = instance
                .raw
                .enumerate_device_extension_properties(physical)
                .map_err(error)?;
            if !extensions.iter().any(|e| {
                CStr::from_ptr(e.extension_name.as_ptr()) == ash::khr::external_memory_fd::NAME
            }) {
                return Err("GPU does not support Vulkan memory FD exchange".into());
            }
            let family = instance
                .raw
                .get_physical_device_queue_family_properties(physical)
                .iter()
                .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .ok_or("Shared GPU has no graphics queue")? as u32;
            let priorities = [1.0];
            let queues = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&priorities)];
            let extensions = [ash::khr::external_memory_fd::NAME.as_ptr()];
            let raw = instance
                .raw
                .create_device(
                    physical,
                    &vk::DeviceCreateInfo::default()
                        .queue_create_infos(&queues)
                        .enabled_extension_names(&extensions),
                    None,
                )
                .map_err(error)?;
            let queue = raw.get_device_queue(family, 0);
            let (device, driver) = ids(&instance.raw, physical);
            Ok(Self {
                owner: Arc::new(Owner {
                    queue_lock: Mutex::new(()),
                    instance,
                    raw,
                    physical,
                    queue,
                    family,
                    device,
                    driver,
                }),
            })
        }
    }
    pub fn image(&mut self, dimensions: [u32; 2], allocation_limit: u64) -> Result<Image, Failure> {
        unsafe {
            let mut external = vk::ExternalMemoryImageCreateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
            let info = image_info(dimensions)?.push_next(&mut external);
            let raw = self.owner.raw.create_image(&info, None).map_err(error)?;
            let requirements = self.owner.raw.get_image_memory_requirements(raw);
            let properties = self
                .owner
                .instance
                .raw
                .get_physical_device_memory_properties(self.owner.physical);
            let memory_type = (0..properties.memory_type_count).find(|i| {
                requirements.memory_type_bits & (1 << i) != 0
                    && properties.memory_types[*i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            });
            let Some(memory_type) = memory_type.filter(|_| requirements.size <= allocation_limit)
            else {
                self.owner.raw.destroy_image(raw, None);
                return Err(
                    "Shared image exceeds allocation allowance or has no device-local memory"
                        .into(),
                );
            };
            let mut export = vk::ExportMemoryAllocateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(raw);
            let allocation = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type)
                .push_next(&mut export)
                .push_next(&mut dedicated);
            let memory = match self.owner.raw.allocate_memory(&allocation, None) {
                Ok(memory) => memory,
                Err(e) => {
                    self.owner.raw.destroy_image(raw, None);
                    return Err(error(e));
                }
            };
            let image = Image {
                owner: self.owner.clone(),
                raw,
                memory,
                description: Description {
                    dimensions,
                    device: self.owner.device,
                    driver: self.owner.driver,
                    allocation_bytes: requirements.size,
                    memory_type,
                },
            };
            self.owner
                .raw
                .bind_image_memory(raw, memory, 0)
                .map_err(error)?;
            Ok(image)
        }
    }
    /// Vulkan handles are borrowed; a backend must never destroy these resources.
    /// # Safety
    /// The callback must obey Vulkan lifetimes and externally synchronize queue use.
    pub unsafe fn with_raw<R>(
        &mut self,
        f: impl FnOnce(
            &ash::Entry,
            &ash::Instance,
            vk::PhysicalDevice,
            &ash::Device,
            vk::Queue,
            u32,
        ) -> R,
    ) -> R {
        let _queue = self
            .owner
            .queue_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        f(
            &self.owner.instance._entry,
            &self.owner.instance.raw,
            self.owner.physical,
            &self.owner.raw,
            self.owner.queue,
            self.owner.family,
        )
    }
}
impl Image {
    pub fn allocation_bytes(&self) -> u64 {
        self.description.allocation_bytes
    }
    /// # Safety
    /// Borrow for backend rendering only. No destruction, export, or concurrent use.
    pub unsafe fn raw(&mut self) -> (vk::Image, vk::DeviceMemory) {
        (self.raw, self.memory)
    }
    /// Qualification operation; the same image can instead be drawn by Skia.
    pub fn clear(&mut self, color: [f32; 4]) -> Result<(), Failure> {
        if color.iter().any(|v| !v.is_finite()) {
            return Err("Invalid shared clear color".into());
        }
        self.submit(|command| unsafe {
            let barrier = vk::ImageMemoryBarrier::default()
                .image(self.raw)
                .subresource_range(range())
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            self.owner.raw.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
            self.owner.raw.cmd_clear_color_image(
                command,
                self.raw,
                vk::ImageLayout::GENERAL,
                &vk::ClearColorValue { float32: color },
                &[range()],
            );
        })
    }
    /// # Safety
    /// `layout` must be the final layout left by the completed producer backend.
    /// No backend may reference this image after calling publish.
    pub unsafe fn publish(self, layout: vk::ImageLayout) -> Result<Export, Failure> {
        self.submit(|command| unsafe {
            let release = vk::ImageMemoryBarrier::default()
                .image(self.raw)
                .subresource_range(range())
                .old_layout(layout)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(self.owner.family)
                .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                .src_access_mask(vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::MEMORY_READ);
            self.owner.raw.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[release],
            );
        })?;
        let extension =
            ash::khr::external_memory_fd::Device::new(&self.owner.instance.raw, &self.owner.raw);
        let fd = unsafe {
            extension.get_memory_fd(
                &vk::MemoryGetFdInfoKHR::default()
                    .memory(self.memory)
                    .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD),
            )
        }
        .map_err(error)?;
        Ok(Export {
            description: self.description.clone(),
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
            _image: self,
        })
    }
    fn submit(&self, record: impl FnOnce(vk::CommandBuffer)) -> Result<(), Failure> {
        let _queue = self
            .owner
            .queue_lock
            .lock()
            .map_err(|_| "producer queue lock poisoned")?;
        unsafe {
            let pool = self
                .owner
                .raw
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default().queue_family_index(self.owner.family),
                    None,
                )
                .map_err(error)?;
            let result = (|| {
                let command = self
                    .owner
                    .raw
                    .allocate_command_buffers(
                        &vk::CommandBufferAllocateInfo::default()
                            .command_pool(pool)
                            .level(vk::CommandBufferLevel::PRIMARY)
                            .command_buffer_count(1),
                    )
                    .map_err(error)?[0];
                self.owner
                    .raw
                    .begin_command_buffer(
                        command,
                        &vk::CommandBufferBeginInfo::default()
                            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                    )
                    .map_err(error)?;
                record(command);
                self.owner.raw.end_command_buffer(command).map_err(error)?;
                let fence = self
                    .owner
                    .raw
                    .create_fence(&vk::FenceCreateInfo::default(), None)
                    .map_err(error)?;
                let commands = [command];
                let result = self
                    .owner
                    .raw
                    .queue_submit(
                        self.owner.queue,
                        &[vk::SubmitInfo::default().command_buffers(&commands)],
                        fence,
                    )
                    .and_then(|_| {
                        self.owner
                            .raw
                            .wait_for_fences(&[fence], true, 30_000_000_000)
                    });
                if result.is_err() {
                    let _ = self.owner.raw.device_wait_idle();
                }
                self.owner.raw.destroy_fence(fence, None);
                result.map_err(error)
            })();
            self.owner.raw.destroy_command_pool(pool, None);
            result
        }
    }
}

#[cfg(feature = "consumer")]
pub struct Imported {
    device: wgpu::Device,
    raw: ash::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
    description: Description,
}
#[cfg(feature = "consumer")]
impl Drop for Imported {
    fn drop(&mut self) {
        unsafe {
            self.raw.destroy_image(self.image, None);
            self.raw.free_memory(self.memory, None);
        }
    }
}
#[cfg(feature = "consumer")]
impl Imported {
    pub fn description(&self) -> &Description {
        &self.description
    }
    pub fn belongs_to(&self, device: &wgpu::Device) -> bool {
        self.device == *device
    }
    /// Takes one already-completed, immutable producer image. The producer must
    /// retain its Export until this consumer completes the copy.
    /// # Safety
    /// The FD must name the immutable image described by this descriptor,
    /// exported after producer completion and queue-family release.
    pub unsafe fn new(
        device: &wgpu::Device,
        description: Description,
        fd: OwnedFd,
        allocation_limit: u64,
    ) -> Result<Self, Failure> {
        unsafe {
            let hal = device
                .as_hal::<wgpu::hal::api::Vulkan>()
                .ok_or("GPU sharing requires Vulkan")?;
            if ids(
                hal.shared_instance().raw_instance(),
                hal.raw_physical_device(),
            ) != (description.device, description.driver)
            {
                return Err("Shared image device/driver does not match receiving GPU".into());
            }
            if !hal
                .enabled_device_extensions()
                .contains(&ash::khr::external_memory_fd::NAME)
            {
                return Err("Receiving GPU has no memory FD extension".into());
            }
            let mut external = vk::ExternalMemoryImageCreateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
            let info = image_info(description.dimensions)?.push_next(&mut external);
            let raw = hal.raw_device();
            let image = raw.create_image(&info, None).map_err(error)?;
            let requirements = raw.get_image_memory_requirements(image);
            if description.allocation_bytes != requirements.size
                || description.allocation_bytes > allocation_limit
                || description.memory_type >= 32
                || requirements.memory_type_bits & (1 << description.memory_type) == 0
            {
                raw.destroy_image(image, None);
                return Err("Invalid shared memory description or allowance".into());
            }
            let mut import = vk::ImportMemoryFdInfoKHR::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD)
                .fd(fd.as_raw_fd());
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
            let memory = match raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(description.memory_type)
                    .push_next(&mut import)
                    .push_next(&mut dedicated),
                None,
            ) {
                Ok(memory) => memory,
                Err(e) => {
                    raw.destroy_image(image, None);
                    return Err(error(e));
                }
            };
            let _ = fd.into_raw_fd(); // Vulkan consumed the FD on successful import.
            let imported = Self {
                device: device.clone(),
                raw: raw.clone(),
                image,
                memory,
                description,
            };
            raw.bind_image_memory(image, memory, 0).map_err(error)?;
            Ok(imported)
        }
    }
    /// Records a GPU copy into an initialized compositor-owned RGBA16F target.
    /// Retains imported storage and the target until queue completion. No raw
    /// queue submission occurs outside wgpu's synchronization.
    pub fn copy(
        self,
        queue: &wgpu::Queue,
    ) -> Result<(wgpu::Texture, wgpu::SubmissionIndex), Failure> {
        self.copy_reserved(queue, ())
    }
    /// The caller reserves the destination and shared source before allocation.
    /// Both initialization and copy retain the reservation through completion,
    /// including cancellation or a failure after initialization submission.
    pub fn copy_reserved<L: Clone + Send + 'static>(
        self,
        queue: &wgpu::Queue,
        reservation: L,
    ) -> Result<(wgpu::Texture, wgpu::SubmissionIndex), Failure> {
        let destination = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Fold shared image copy"),
            size: wgpu::Extent3d {
                width: self.description.dimensions[0],
                height: self.description.dimensions[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        // HAL guards hold wgpu's resource lock. Extract handles one at a time,
        // then release the guards before calling another wgpu operation.
        let family = {
            let hal = unsafe { self.device.as_hal::<wgpu::hal::api::Vulkan>() }
                .ok_or("GPU sharing requires Vulkan")?;
            hal.queue_family_index()
        };
        let target = {
            let target = unsafe { destination.as_hal::<wgpu::hal::api::Vulkan>() }
                .ok_or("Destination is not Vulkan")?;
            unsafe { target.raw_handle() }
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Fold shared GPU image"),
            });
        // Initializes wgpu's memory tracker, then establishes COPY_DST before
        // native recording. No private raw queue submission is needed.
        let view = destination.create_view(&wgpu::TextureViewDescriptor::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Initialize shared image destination"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        // Commit initialization before recording an external write, so wgpu's
        // deferred initialization cannot erase pixels written outside its tracker.
        queue.submit([encoder.finish()]);
        let initializing = reservation.clone();
        queue.on_submitted_work_done(move || drop(initializing));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Copy shared Vulkan image"),
            });
        encoder.transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: &destination,
                selector: None,
                state: wgpu::TextureUses::COPY_DST,
            }),
        );
        unsafe {
            encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|command| {
                let command = command.ok_or("Missing Vulkan encoder")?.raw_handle();
                let acquire = vk::ImageMemoryBarrier::default()
                    .image(self.image)
                    .subresource_range(range())
                    .old_layout(vk::ImageLayout::GENERAL)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                    .dst_queue_family_index(family)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
                self.raw.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[acquire],
                );
                let layers = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                let copy = vk::ImageCopy::default()
                    .src_subresource(layers)
                    .dst_subresource(layers)
                    .extent(vk::Extent3D {
                        width: self.description.dimensions[0],
                        height: self.description.dimensions[1],
                        depth: 1,
                    });
                self.raw.cmd_copy_image(
                    command,
                    self.image,
                    vk::ImageLayout::GENERAL,
                    target,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy],
                );
                Ok::<_, Failure>(())
            })?;
        }
        let submission = queue.submit([encoder.finish()]);
        let target = destination.clone();
        queue.on_submitted_work_done(move || {
            drop(target);
            drop(self);
            drop(reservation);
        });
        Ok((destination, submission))
    }
}
