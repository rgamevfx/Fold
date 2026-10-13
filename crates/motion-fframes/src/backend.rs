//! Trusted Skia/Vulkan boundary. Device, queue and allocation lifetimes stay
//! with Renderer/Target; Skia borrows them and never owns the exported memory.
use ash::{vk, vk::Handle};
use fframes_skia_renderer::skia_safe as skia;
use fold_gpu_sharing::{Device, Export, Image};
use std::ffi::c_void;

pub struct Target {
    pub surface: skia::Surface,
    image: Image,
}
impl Target {
    pub fn allocation_bytes(&self) -> u64 {
        self.image.allocation_bytes()
    }
}
pub fn queue<R>(device: &mut Device, f: impl FnOnce() -> R) -> R {
    unsafe { device.with_raw(|_, _, _, _, _, _| f()) }
}
pub fn context(device: &mut Device) -> Result<skia::gpu::DirectContext, String> {
    unsafe {
        device.with_raw(|entry, instance, physical, raw, queue, family| {
            let get_proc = |of| {
                let function = match of {
                    skia::gpu::vk::GetProcOf::Instance(instance, name) => {
                        entry.get_instance_proc_addr(vk::Instance::from_raw(instance as _), name)
                    }
                    skia::gpu::vk::GetProcOf::Device(device, name) => {
                        instance.get_device_proc_addr(vk::Device::from_raw(device as _), name)
                    }
                };
                function.map_or(std::ptr::null(), |f| f as *const c_void)
            };
            let backend = skia::gpu::vk::BackendContext::new_builder(
                instance.handle().as_raw() as _,
                physical.as_raw() as _,
                raw.handle().as_raw() as _,
                (queue.as_raw() as _, family as usize),
                &get_proc,
                Some(skia::gpu::vk::Version::new(1, 3, 0)),
            )
            .with_extensions(&[], &["VK_KHR_external_memory_fd"])
            .build();
            // Improve curved-stroke coverage while keeping the shared target single-sample.
            // Skia selects supported internal sampling for the device.
            let mut options = skia::gpu::ContextOptions::default();
            options.internal_multisample_count = 8;
            skia::gpu::direct_contexts::make_vulkan(&backend, Some(&options))
                .ok_or("Skia Vulkan context is unavailable".into())
        })
    }
}
pub fn target(
    device: &mut Device,
    gpu: &mut skia::gpu::DirectContext,
    dimensions: [u32; 2],
    allocation_limit: u64,
) -> Result<Target, String> {
    unsafe {
        let mut image = device
            .image(dimensions, allocation_limit)
            .map_err(|e| e.to_string())?;
        let (raw, memory) = image.raw();
        let mut allocation = skia::gpu::vk::Alloc::default();
        allocation.memory = memory.as_raw() as _;
        allocation.size = image.allocation_bytes();
        let family = device.with_raw(|_, _, _, _, _, family| family);
        let mut info = skia::gpu::vk::ImageInfo::new(
            raw.as_raw() as _,
            allocation,
            skia::gpu::vk::ImageTiling::OPTIMAL,
            skia::gpu::vk::ImageLayout::UNDEFINED,
            skia::gpu::vk::Format::R16G16B16A16_SFLOAT,
            1,
            family,
            None,
            None,
            None,
        );
        info.image_usage_flags = (vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST)
            .as_raw();
        let backend = skia::gpu::ganesh::vk::backend_render_targets::make_vk(
            (dimensions[0] as i32, dimensions[1] as i32),
            &info,
        );
        let surface = queue(device, || {
            skia::gpu::surfaces::wrap_backend_render_target(
                gpu,
                &backend,
                skia::gpu::SurfaceOrigin::TopLeft,
                skia::ColorType::RGBAF16,
                skia::ColorSpace::new_srgb_linear(),
                None,
            )
        })
        .ok_or("Skia cannot wrap the shared float target")?;
        Ok(Target { surface, image })
    }
}
pub fn publish(
    mut target: Target,
    gpu: &mut skia::gpu::DirectContext,
    device: &mut Device,
) -> Result<Export, String> {
    let layout = queue(device, || -> Result<_, String> {
        gpu.flush_and_submit();
        let backend = skia::gpu::surfaces::get_backend_render_target(
            &mut target.surface,
            skia::surface::BackendHandleAccess::FlushRead,
        )
        .ok_or("Missing Skia Vulkan target")?;
        let info = skia::gpu::ganesh::vk::backend_render_targets::get_vk_image_info(&backend)
            .ok_or("Missing Skia Vulkan image state")?;
        Ok(vk::ImageLayout::from_raw(info.layout as i32))
    });
    let layout = match layout {
        Ok(layout) => layout,
        Err(error) => {
            dispose(target, device);
            return Err(error);
        }
    };
    queue(device, || drop(target.surface)); // release drawing access before external ownership
    unsafe { target.image.publish(layout).map_err(|e| e.to_string()) }
}
pub fn dispose(target: Target, device: &mut Device) {
    queue(device, || drop(target.surface));
    drop(target.image);
}
pub fn wait(device: &mut Device) {
    unsafe {
        device.with_raw(|_, _, _, raw, _, _| {
            let _ = raw.device_wait_idle();
        });
    }
}
