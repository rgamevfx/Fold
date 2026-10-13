use ash::vk;
use fold_foundation::Failure;
use std::{
    ffi::{CStr, CString, c_char, c_void},
    os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd},
    path::PathBuf,
    sync::Arc,
};
#[repr(C)]
#[derive(Clone, Copy)]
struct Descriptor {
    bytes: u64,
    allocation: u64,
    memory_type: u32,
    fd: i32,
}
type Create = unsafe extern "C" fn(*const u8, *const c_char) -> *mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
type Prepare = unsafe extern "C" fn(*mut c_void, u32, u32, *mut Descriptor) -> i32;
type Run = unsafe extern "C" fn(*mut c_void) -> i32;
type Error = unsafe extern "C" fn() -> *const c_char;
struct Api {
    create: Create,
    destroy: Destroy,
    prepare: Prepare,
    run: Run,
    error: Error,
    _library: libloading::Library,
}
impl Api {
    fn load() -> Result<(Arc<Self>, PathBuf), Failure> {
        let root = std::env::var_os("FOLD_RIFE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .unwrap_or_default()
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join("..")
            });
        let path = root.join("lib/fold/libfold_rife.so");
        unsafe {
            let library=libloading::Library::new(&path).map_err(|e|format!("RIFE runtime unavailable at {}: {e}. Install Fold's RIFE runtime or set FOLD_RIFE_ROOT.",path.display()))?;
            let abi = library
                .get::<unsafe extern "C" fn() -> u32>(b"fold_rife_abi\0")
                .map_err(|e| e.to_string())?;
            if abi() != 1 {
                return Err("Unsupported RIFE runtime ABI".into());
            }
            let api = Self {
                create: *library
                    .get(b"fold_rife_create\0")
                    .map_err(|e| e.to_string())?,
                destroy: *library
                    .get(b"fold_rife_destroy\0")
                    .map_err(|e| e.to_string())?,
                prepare: *library
                    .get(b"fold_rife_prepare\0")
                    .map_err(|e| e.to_string())?,
                run: *library.get(b"fold_rife_run\0").map_err(|e| e.to_string())?,
                error: *library
                    .get(b"fold_rife_error\0")
                    .map_err(|e| e.to_string())?,
                _library: library,
            };
            Ok((Arc::new(api), root.join("share/fold/rife")))
        }
    }
    fn error(&self) -> Failure {
        unsafe {
            CStr::from_ptr((self.error)())
                .to_string_lossy()
                .into_owned()
                .into()
        }
    }
}
struct Imported {
    raw: ash::Device,
    raw_buffer: vk::Buffer,
    buffer: wgpu::Buffer,
    bytes: u64,
}
impl Imported {
    unsafe fn new(device: &wgpu::Device, d: Descriptor, fd: OwnedFd) -> Result<Self, Failure> {
        unsafe {
            let hal = device
                .as_hal::<wgpu::hal::api::Vulkan>()
                .ok_or("RIFE requires Vulkan")?;
            let raw = hal.raw_device();
            if !hal
                .enabled_device_extensions()
                .contains(&ash::khr::external_memory_fd::NAME)
            {
                return Err("RIFE requires Vulkan external-memory FD support".into());
            }
            let mut external = vk::ExternalMemoryBufferCreateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
            let buffer = raw
                .create_buffer(
                    &vk::BufferCreateInfo::default()
                        .size(d.bytes)
                        .usage(
                            vk::BufferUsageFlags::STORAGE_BUFFER
                                | vk::BufferUsageFlags::TRANSFER_SRC
                                | vk::BufferUsageFlags::TRANSFER_DST,
                        )
                        .push_next(&mut external),
                    None,
                )
                .map_err(err)?;
            let req = raw.get_buffer_memory_requirements(buffer);
            if req.size != d.allocation
                || d.memory_type >= 32
                || req.memory_type_bits & (1 << d.memory_type) == 0
            {
                raw.destroy_buffer(buffer, None);
                return Err("RIFE GPU allocation does not match the renderer".into());
            }
            let mut import = vk::ImportMemoryFdInfoKHR::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD)
                .fd(fd.as_raw_fd());
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().buffer(buffer);
            let memory = match raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(d.allocation)
                    .memory_type_index(d.memory_type)
                    .push_next(&mut import)
                    .push_next(&mut dedicated),
                None,
            ) {
                Ok(m) => m,
                Err(e) => {
                    raw.destroy_buffer(buffer, None);
                    return Err(err(e));
                }
            };
            let _ = fd.into_raw_fd();
            if let Err(e) = raw.bind_buffer_memory(buffer, memory, 0) {
                raw.destroy_buffer(buffer, None);
                raw.free_memory(memory, None);
                return Err(err(e));
            }
            let raw_device = raw.clone();
            drop(hal);
            let native = wgpu::hal::vulkan::Buffer::from_raw_managed(buffer, memory, 0, d.bytes);
            let owned = device.create_buffer_from_hal::<wgpu::hal::api::Vulkan>(
                native,
                &wgpu::BufferDescriptor {
                    label: Some("RIFE shared tensor"),
                    size: d.bytes,
                    usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            );
            Ok(Self {
                raw: raw_device,
                raw_buffer: buffer,
                buffer: owned,
                bytes: d.bytes,
            })
        }
    }
}
fn err(e: impl std::fmt::Display) -> Failure {
    e.to_string().into()
}
/// Serialized worker-owned session. Every exchange completes before returning.
pub struct Runtime {
    api: Arc<Api>,
    handle: *mut c_void,
    device: wgpu::Device,
    queue: wgpu::Queue,
    imports: Vec<Imported>,
    size: [u32; 2],
}
// Native state is only touched through &mut self; ncnn device initialization is locked.
unsafe impl Send for Runtime {}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.imports.clear();
        unsafe {
            (self.api.destroy)(self.handle);
        }
    }
}
impl Runtime {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, Failure> {
        let uuid = {
            let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }
                .ok_or("RIFE GPU interpolation currently requires Linux Vulkan")?;
            let mut id = vk::PhysicalDeviceIDProperties::default();
            let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
            unsafe {
                hal.shared_instance()
                    .raw_instance()
                    .get_physical_device_properties2(hal.raw_physical_device(), &mut props);
            }
            id.device_uuid
        };
        let (api, model) = Api::load()?;
        use sha2::Digest;
        for (name, expected) in [
            (
                "flownet.bin",
                "f334ed2260149ce0188a6dcf049844e8b0cdd912e01cbcfb63553157d2508958",
            ),
            (
                "flownet.param",
                "724569596bcd1e7b9fa50455c604777ebed99746d2ef40aa86e31b5725f1053c",
            ),
        ] {
            let bytes = std::fs::read(model.join(name))
                .map_err(|e| format!("Cannot read RIFE model {name}: {e}"))?;
            if format!("{:x}", sha2::Sha256::digest(&bytes)) != expected {
                return Err(
                    format!("RIFE model {name} does not match the pinned v4.6 runtime").into(),
                );
            }
        }
        let model = CString::new(model.to_string_lossy().as_bytes()).map_err(err)?;
        let handle = unsafe { (api.create)(uuid.as_ptr(), model.as_ptr()) };
        if handle.is_null() {
            return Err(api.error());
        }
        Ok(Self {
            api,
            handle,
            device: device.clone(),
            queue: queue.clone(),
            imports: vec![],
            size: [0, 0],
        })
    }
    pub fn prepare(&mut self, size: [u32; 2]) -> Result<(), Failure> {
        if self.size == size {
            return Ok(());
        }
        self.imports.clear();
        self.size = [0, 0];
        let mut descriptors = [Descriptor {
            bytes: 0,
            allocation: 0,
            memory_type: 0,
            fd: -1,
        }; 5];
        let result =
            unsafe { (self.api.prepare)(self.handle, size[0], size[1], descriptors.as_mut_ptr()) };
        let fds: Vec<_> = descriptors
            .iter()
            .map(|d| {
                if d.fd >= 0 {
                    Some(unsafe { OwnedFd::from_raw_fd(d.fd) })
                } else {
                    None
                }
            })
            .collect();
        if result != 0 {
            return Err(self.api.error());
        }
        for (d, fd) in descriptors.into_iter().zip(fds) {
            self.imports.push(unsafe {
                Imported::new(&self.device, d, fd.ok_or("Missing RIFE buffer FD")?)?
            });
        }
        self.size = size;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for i in 0..5 {
            self.barrier(&mut encoder, i, true, true)?;
            encoder.clear_buffer(&self.imports[i].buffer, 0, None);
            self.barrier(&mut encoder, i, false, true)?;
        }
        self.queue.submit([encoder.finish()]);
        self.finish()?;
        Ok(())
    }
    pub fn upload_and_run(
        &mut self,
        mut encoder: wgpu::CommandEncoder,
        inputs: [&wgpu::Buffer; 3],
        reuse_inputs: bool,
    ) -> Result<(), Failure> {
        if !reuse_inputs {
            self.copy(&mut encoder, &[(0, inputs[0]), (1, inputs[1])], true)?;
        }
        self.copy(&mut encoder, &[(2, inputs[2])], true)?;
        self.queue.submit([encoder.finish()]);
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(err)?;
        if unsafe { (self.api.run)(self.handle) } != 0 {
            return Err(self.api.error());
        }
        Ok(())
    }
    /// Copy flow/mask to renderer-owned GPU storage. No tensor reaches host RAM.
    pub fn download_gpu(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        flow: &wgpu::Buffer,
        mask: &wgpu::Buffer,
    ) -> Result<(), Failure> {
        self.copy(encoder, &[(3, flow), (4, mask)], false)
    }
    fn copy(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        buffers: &[(usize, &wgpu::Buffer)],
        upload: bool,
    ) -> Result<(), Failure> {
        for (i, buffer) in buffers {
            let external = &self.imports[*i];
            if buffer.size() != external.bytes {
                return Err("RIFE tensor extent mismatch".into());
            }
            self.barrier(encoder, *i, true, upload)?;
            let (src, dst) = if upload {
                (*buffer, &external.buffer)
            } else {
                (&external.buffer, *buffer)
            };
            encoder.copy_buffer_to_buffer(src, 0, dst, 0, external.bytes);
            self.barrier(encoder, *i, false, upload)?;
        }
        Ok(())
    }
    fn barrier(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        index: usize,
        acquire: bool,
        upload: bool,
    ) -> Result<(), Failure> {
        let family = {
            let hal = unsafe { self.device.as_hal::<wgpu::hal::api::Vulkan>() }
                .ok_or("RIFE requires Vulkan")?;
            hal.queue_family_index()
        };
        let external = &self.imports[index];
        let access = if upload {
            vk::AccessFlags::TRANSFER_WRITE
        } else {
            vk::AccessFlags::TRANSFER_READ
        };
        unsafe {
            encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|cmd| {
                let cmd = cmd.ok_or("Missing Vulkan encoder")?.raw_handle();
                let barrier = vk::BufferMemoryBarrier::default()
                    .buffer(external.raw_buffer)
                    .size(vk::WHOLE_SIZE)
                    .src_queue_family_index(if acquire {
                        vk::QUEUE_FAMILY_EXTERNAL
                    } else {
                        family
                    })
                    .dst_queue_family_index(if acquire {
                        family
                    } else {
                        vk::QUEUE_FAMILY_EXTERNAL
                    })
                    .src_access_mask(if acquire {
                        vk::AccessFlags::empty()
                    } else {
                        access
                    })
                    .dst_access_mask(if acquire {
                        access
                    } else {
                        vk::AccessFlags::empty()
                    });
                external.raw.cmd_pipeline_barrier(
                    cmd,
                    if acquire {
                        vk::PipelineStageFlags::TOP_OF_PIPE
                    } else {
                        vk::PipelineStageFlags::TRANSFER
                    },
                    if acquire {
                        vk::PipelineStageFlags::TRANSFER
                    } else {
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE
                    },
                    vk::DependencyFlags::empty(),
                    &[],
                    &[barrier],
                    &[],
                );
                Ok::<_, Failure>(())
            })?;
        }
        Ok(())
    }
    pub fn finish(&self) -> Result<(), Failure> {
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(err)?;
        Ok(())
    }
}
