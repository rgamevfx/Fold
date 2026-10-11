//! Read-only Vulkan heap telemetry. No device extensions or allocator ownership.
use ash::vk;

/// Largest device-local heap: physical bytes, current process budget and usage.
/// None means Vulkan or its memory-budget telemetry is unavailable.
pub fn device_memory(adapter: &wgpu::Adapter) -> Option<[u64; 3]> {
    // The adapter guard retains the instance and physical device for both queries.
    unsafe {
        let adapter = adapter.as_hal::<wgpu::hal::api::Vulkan>()?;
        let instance = adapter.shared_instance().raw_instance();
        let physical = adapter.raw_physical_device();
        let extensions = instance
            .enumerate_device_extension_properties(physical)
            .ok()?;
        if !extensions.iter().any(|extension| {
            extension
                .extension_name_as_c_str()
                .is_ok_and(|name| name == ash::ext::memory_budget::NAME)
        }) {
            return None;
        }
        let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
        let mut properties = vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
        instance.get_physical_device_memory_properties2(physical, &mut properties);
        let memory = properties.memory_properties;
        memory.memory_heaps[..memory.memory_heap_count as usize]
            .iter()
            .enumerate()
            .filter(|(_, heap)| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
            .max_by_key(|(_, heap)| heap.size)
            .map(|(index, heap)| {
                [
                    heap.size,
                    budget.heap_budget[index],
                    budget.heap_usage[index],
                ]
            })
    }
}
