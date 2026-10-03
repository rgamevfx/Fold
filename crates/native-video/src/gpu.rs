//! Dedicated Vulkan allocations: pending external writer -> immutable wgpu buffer.
//! No allocation is reused. HAL owns both VkBuffer and memory after import, so
//! clones, bind groups and in-flight submissions retain the allocation correctly.
//! wgpu-hal 27's optional debug memory counter subtracts managed imports on
//! destruction although add_raw_buffer does not increment that byte counter.
//! Do not use it as external-memory accounting; Fold's exact allocation-size
//! reservations are authoritative. No vendor patch/counter workaround is applied.
use ash::vk;
use std::os::fd::{AsFd, FromRawFd, OwnedFd};

pub struct PendingBuffer {
    device: wgpu::Device,
    raw: ash::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    fd: OwnedFd,
    bytes: u64,
    allocation: u64,
    family: u32,
    uuid: [u8; 16],
    ticket: Option<u64>,
}
pub struct ReadyBuffer {
    pub buffer: wgpu::Buffer,
    pub allocation_bytes: u64,
    pub device_uuid: [u8; 16],
}
impl PendingBuffer {
    /// Must be allocated on the consuming device. Query its actual UUID rather
    /// than assuming CUDA ordinal zero refers to the same physical adapter.
    pub fn new(device: &wgpu::Device, bytes: u64) -> Result<Self, String> {
        Self::new_reserved(device, bytes, |_| Ok(())).map(|(buffer, ())| buffer)
    }
    /// Reserve the exact driver allocation requirement before allocating VRAM.
    pub fn new_reserved<R>(
        device: &wgpu::Device,
        bytes: u64,
        reserve: impl FnOnce(u64) -> Result<R, String>,
    ) -> Result<(Self, R), String> {
        if bytes == 0 || bytes > 8 * 1024 * 1024 || !bytes.is_multiple_of(4) {
            return Err("invalid native buffer size".into());
        }
        unsafe {
            let hal = device
                .as_hal::<wgpu::hal::api::Vulkan>()
                .ok_or("native video requires Vulkan")?;
            let instance = hal.shared_instance().raw_instance().clone();
            let raw = hal.raw_device().clone();
            let family = hal.queue_family_index();
            if !hal
                .enabled_device_extensions()
                .contains(&ash::khr::external_memory_fd::NAME)
            {
                return Err("Vulkan opaque FD extension unavailable".into());
            }
            let mut id = vk::PhysicalDeviceIDProperties::default();
            let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
            instance.get_physical_device_properties2(hal.raw_physical_device(), &mut props);
            let handles = vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;
            let mut support = vk::ExternalBufferProperties::default();
            instance.get_physical_device_external_buffer_properties(
                hal.raw_physical_device(),
                &vk::PhysicalDeviceExternalBufferInfo::default()
                    .usage(
                        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
                    )
                    .handle_type(handles),
                &mut support,
            );
            if !support
                .external_memory_properties
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE)
            {
                return Err("device cannot export native video storage".into());
            }
            let mut external = vk::ExternalMemoryBufferCreateInfo::default().handle_types(handles);
            let buffer = raw
                .create_buffer(
                    &vk::BufferCreateInfo::default()
                        .size(bytes)
                        .usage(
                            vk::BufferUsageFlags::STORAGE_BUFFER
                                | vk::BufferUsageFlags::TRANSFER_SRC,
                        )
                        .sharing_mode(vk::SharingMode::EXCLUSIVE)
                        .push_next(&mut external),
                    None,
                )
                .map_err(|e| e.to_string())?;
            let req = raw.get_buffer_memory_requirements(buffer);
            let properties =
                instance.get_physical_device_memory_properties(hal.raw_physical_device());
            let kind = (0..properties.memory_type_count).find(|i| {
                req.memory_type_bits & (1 << i) != 0
                    && properties.memory_types[*i as usize]
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            });
            let Some(kind) = kind else {
                raw.destroy_buffer(buffer, None);
                return Err("no device-local export memory".into());
            };
            // A budget callback may evict wgpu resources. Never run it while
            // holding wgpu's HAL/snatch guard. `device` keeps the raw device alive.
            drop(hal);
            let reservation = match reserve(req.size) {
                Ok(r) => r,
                Err(e) => {
                    raw.destroy_buffer(buffer, None);
                    return Err(e);
                }
            };
            let mut export = vk::ExportMemoryAllocateInfo::default().handle_types(handles);
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().buffer(buffer);
            let memory = match raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(kind)
                    .push_next(&mut export)
                    .push_next(&mut dedicated),
                None,
            ) {
                Ok(memory) => memory,
                Err(e) => {
                    raw.destroy_buffer(buffer, None);
                    return Err(e.to_string());
                }
            };
            let result = raw.bind_buffer_memory(buffer, memory, 0).and_then(|_| {
                ash::khr::external_memory_fd::Device::new(&instance, &raw).get_memory_fd(
                    &vk::MemoryGetFdInfoKHR::default()
                        .memory(memory)
                        .handle_type(handles),
                )
            });
            let fd = match result {
                Ok(fd) => OwnedFd::from_raw_fd(fd),
                Err(e) => {
                    raw.destroy_buffer(buffer, None);
                    raw.free_memory(memory, None);
                    return Err(e.to_string());
                }
            };
            Ok((
                Self {
                    device: device.clone(),
                    raw: raw.clone(),
                    buffer,
                    memory,
                    fd,
                    bytes,
                    allocation: req.size,
                    family,
                    uuid: id.device_uuid,
                    ticket: None,
                },
                reservation,
            ))
        }
    }
    pub fn allocation_bytes(&self) -> u64 {
        self.allocation
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn device_uuid(&self) -> [u8; 16] {
        self.uuid
    }
    /// Issue exactly one allocation-bound request. The supervised media service
    /// must return its opaque completion receipt before this buffer can be used.
    pub fn write_ticket(&mut self) -> Result<crate::WriteTicket<'_>, String> {
        if self.ticket.is_some() {
            return Err("native allocation already issued to a writer".into());
        }
        let ticket =
            crate::WriteTicket::new(self.fd.as_fd(), self.uuid, self.allocation, self.bytes);
        self.ticket = Some(ticket.id);
        Ok(ticket)
    }
    /// Submit ownership acquire before publishing storage. The raw HAL barrier
    /// does NOT track buffer usage in wgpu, so this method explicitly retains a
    /// buffer clone AND the caller's reservation through queue completion.
    /// Readiness can only come from this allocation's one-shot socket request.
    pub fn submit<R: Send + 'static>(
        self,
        queue: &wgpu::Queue,
        complete: crate::WriteComplete,
        reservation: R,
    ) -> Result<ReadyBuffer, String> {
        if self.ticket != Some(complete.id) {
            return Err("native readiness belongs to another allocation".into());
        }
        let (ready, commands) = self.acquire()?;
        let buffer = ready.buffer.clone();
        queue.submit([commands]);
        queue.on_submitted_work_done(move || {
            drop(buffer);
            drop(reservation);
        });
        Ok(ready)
    }
    // Private: no safe public path can import uninitialized or unacknowledged
    // external storage, or separate a raw-barrier submission from its lifetime.
    fn acquire(mut self) -> Result<(ReadyBuffer, wgpu::CommandBuffer), String> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("NVDEC ownership acquire"),
            });
        unsafe {
            encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|hal| {
                let hal = hal.ok_or("native acquire requires Vulkan encoder")?;
                self.raw.cmd_pipeline_barrier(
                    hal.raw_handle(),
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[vk::BufferMemoryBarrier::default()
                        .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                        .dst_queue_family_index(self.family)
                        .dst_access_mask(
                            vk::AccessFlags::SHADER_READ | vk::AccessFlags::TRANSFER_READ,
                        )
                        .buffer(self.buffer)
                        .offset(0)
                        .size(self.bytes)],
                    &[],
                );
                Ok::<(), String>(())
            })?;
            let buffer = self
                .device
                .create_buffer_from_hal::<wgpu::hal::api::Vulkan>(
                    wgpu::hal::vulkan::Buffer::from_raw_managed(
                        self.buffer,
                        self.memory,
                        0,
                        self.allocation,
                    ),
                    &wgpu::BufferDescriptor {
                        label: Some("NVDEC immutable NV12"),
                        size: self.bytes,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    },
                );
            self.buffer = vk::Buffer::null();
            self.memory = vk::DeviceMemory::null();
            // Also register the native use with wgpu's tracker. In particular,
            // explicit Buffer::destroy must defer freeing until this submission,
            // even though the external ownership barrier itself is raw HAL work.
            encoder.transition_resources(
                std::iter::once(wgpu::BufferTransition {
                    buffer: &buffer,
                    state: wgpu::BufferUses::STORAGE_READ_ONLY,
                }),
                std::iter::empty(),
            );
            Ok((
                ReadyBuffer {
                    buffer,
                    allocation_bytes: self.allocation,
                    device_uuid: self.uuid,
                },
                encoder.finish(),
            ))
        }
    }
}
impl Drop for PendingBuffer {
    fn drop(&mut self) {
        unsafe {
            if self.buffer != vk::Buffer::null() {
                self.raw.destroy_buffer(self.buffer, None);
            }
            if self.memory != vk::DeviceMemory::null() {
                self.raw.free_memory(self.memory, None);
            }
        }
    }
}
