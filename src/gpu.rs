//! GPU ownership and limits shared by the native app and headless execution.
#[derive(Clone)]
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub name: String,
}
pub fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    let limits = adapter.limits();
    wgpu::DeviceDescriptor {
        label: Some("Rendering and simulation device"),
        required_limits: wgpu::Limits {
            max_texture_dimension_2d: limits.max_texture_dimension_2d,
            max_texture_array_layers: limits.max_texture_array_layers,
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_buffer_size: limits.max_buffer_size,
            ..Default::default()
        },
        ..Default::default()
    }
}
impl GpuContext {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, name: String) -> Self {
        Self {
            device,
            queue,
            name,
        }
    }
    pub fn headless() -> Result<Self, String> {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&Default::default())
                .await
                .ok_or("No compatible GPU adapter")?;
            let (device, queue) = adapter
                .request_device(&device_descriptor(&adapter), None)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Self::new(device, queue, adapter.get_info().name))
        })
    }
}

pub(crate) fn read_bytes(
    context: &GpuContext,
    source: &wgpu::Buffer,
    offset: u64,
    bytes: u64,
) -> Result<Vec<u8>, String> {
    let output = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("GPU scalar/probe readback"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = context.device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(source, offset, &output, 0, bytes);
    let submission = context.queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    output.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    context
        .device
        .poll(wgpu::Maintain::WaitForSubmissionIndex(submission));
    rx.recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let data = output.slice(..).get_mapped_range().to_vec();
    output.unmap();
    Ok(data)
}

/// Driver-reported process budget and current allocation usage, not total board VRAM.
#[derive(Clone, Debug)]
pub struct MemoryStatus {
    pub budget_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub source: &'static str,
}
impl GpuContext {
    pub fn memory_status(&self) -> Option<MemoryStatus> {
        // Release completed deferred allocations before asking the driver for current usage.
        self.device.poll(wgpu::Maintain::Poll);
        #[cfg(target_os = "linux")]
        // SAFETY: handles stay within wgpu's locked callback; queries are read-only.
        // The extension and Vulkan 1.1 entry point are checked before use.
        unsafe {
            self.device
                .as_hal::<wgpu::hal::api::Vulkan, _, _>(|device| {
                    let device = device?;
                    let shared = device.shared_instance();
                    if shared.instance_api_version() < ash::vk::API_VERSION_1_1 {
                        return None;
                    }
                    let instance = shared.raw_instance();
                    let physical = device.raw_physical_device();
                    let extensions = instance
                        .enumerate_device_extension_properties(physical)
                        .ok()?;
                    if !extensions.iter().any(|e| {
                        std::ffi::CStr::from_ptr(e.extension_name.as_ptr())
                            == ash::ext::memory_budget::NAME
                    }) {
                        return None;
                    }
                    let mut budget = ash::vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
                    let mut properties =
                        ash::vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
                    instance.get_physical_device_memory_properties2(physical, &mut properties);
                    let heaps = properties.memory_properties;
                    let mut total = 0u64;
                    let mut used = 0u64;
                    let mut available = 0u64;
                    for i in 0..heaps.memory_heap_count as usize {
                        if heaps.memory_heaps[i]
                            .flags
                            .contains(ash::vk::MemoryHeapFlags::DEVICE_LOCAL)
                        {
                            total += budget.heap_budget[i];
                            used += budget.heap_usage[i];
                            available += budget.heap_budget[i].saturating_sub(budget.heap_usage[i]);
                        }
                    }
                    (total > 0).then_some(MemoryStatus {
                        budget_bytes: total,
                        used_bytes: used,
                        available_bytes: available,
                        source: "Vulkan driver memory budget",
                    })
                })
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
    /// Check only additional allocations: existing geometry/history already count in usage.
    pub fn ensure_memory(&self, additional_bytes: u64, purpose: &str) -> Result<(), String> {
        if let Some(m) = self.memory_status() {
            if additional_bytes > m.available_bytes {
                return Err(format!(
                    "{purpose} needs {:.0} MiB additional GPU memory; {:.0} MiB currently available ({}, process usage {:.0} / budget {:.0} MiB). Reduce resolution or retained history",
                    additional_bytes as f64 / 1048576.,
                    m.available_bytes as f64 / 1048576.,
                    m.source,
                    m.used_bytes as f64 / 1048576.,
                    m.budget_bytes as f64 / 1048576.
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;
    #[test]
    fn live_budget_reports_available_memory_and_refuses_an_impossible_allocation() {
        let _lock = crate::gpu_test_lock();
        let context = GpuContext::headless().unwrap();
        if let Some(m) = context.memory_status() {
            eprintln!(
                "{}: available {:.0} MiB, used {:.0}, budget {:.0}",
                m.source,
                m.available_bytes as f64 / 1048576.,
                m.used_bytes as f64 / 1048576.,
                m.budget_bytes as f64 / 1048576.
            );
            assert!(m.budget_bytes > 0);
            assert!(m.available_bytes <= m.budget_bytes);
            assert!(
                context
                    .ensure_memory(u64::MAX, "Impossible allocation")
                    .unwrap_err()
                    .contains("currently available")
            );
        }
    }
}
