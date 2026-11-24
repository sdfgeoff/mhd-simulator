//! GPU ownership and limits shared by the native app and headless execution.
#[derive(Clone)]
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub name: String,
    pub profiler: std::sync::Arc<crate::profile::Profiler>,
    bounded_queue: std::sync::Arc<std::sync::Mutex<BoundedQueue>>,
}
struct BoundedQueue {
    limit: usize,
    pending: std::collections::VecDeque<wgpu::SubmissionIndex>,
    submissions: u64,
    waits: u64,
}
pub fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    let limits = adapter.limits();
    wgpu::DeviceDescriptor {
        label: Some("Rendering and simulation device"),
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
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
        let profiler = std::sync::Arc::new(crate::profile::Profiler::new(&device, &queue));
        Self {
            device,
            queue,
            name,
            profiler,
            bounded_queue: std::sync::Arc::new(std::sync::Mutex::new(BoundedQueue {
                limit: 1,
                pending: Default::default(),
                submissions: 0,
                waits: 0,
            })),
        }
    }
    /// Pipeline short command buffers without enlarging watchdog-sensitive GPU
    /// submissions. Queue ordering preserves dependencies; readbacks still fence.
    /// A bounded window limits CPU lead. Set to 1 for the original fence policy.
    pub fn submit_bounded(&self, commands: wgpu::CommandBuffer) {
        let mut state = self.bounded_queue.lock().unwrap();
        let index = self.queue.submit(Some(commands));
        state.submissions += 1;
        state.pending.push_back(index);
        if state.pending.len() >= state.limit {
            let last = state.pending.pop_back().unwrap();
            self.device
                .poll(wgpu::Maintain::WaitForSubmissionIndex(last));
            state.pending.clear();
            state.waits += 1;
        }
    }
    pub fn set_bounded_submission_window(&self, limit: usize) -> Result<(), String> {
        if !(1..=8).contains(&limit) {
            return Err("Submission window must be between1 and8".into());
        }
        let mut state = self.bounded_queue.lock().unwrap();
        if let Some(last) = state.pending.pop_back() {
            self.device
                .poll(wgpu::Maintain::WaitForSubmissionIndex(last));
        }
        state.pending.clear();
        state.limit = limit;
        state.submissions = 0;
        state.waits = 0;
        Ok(())
    }
    /// Counts explicit bounded-submission waits, not required scalar map waits.
    pub fn bounded_submission_counts(&self) -> (u64, u64) {
        let state = self.bounded_queue.lock().unwrap();
        (state.submissions, state.waits)
    }
    pub fn compute_pass<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        label: Option<&'a str>,
    ) -> wgpu::ComputePass<'a> {
        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label,
            timestamp_writes: self.profiler.timestamps(),
        })
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

/// Copy a GPU buffer range to host bytes for headless analysis.
/// The caller must supply an aligned, in-bounds range of a COPY_SRC buffer.
pub fn read_bytes(
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
        if let Some(m) = self.memory_status()
            && additional_bytes > m.available_bytes
        {
            return Err(format!(
                "{purpose} needs {:.0} MiB additional GPU memory; {:.0} MiB currently available ({}, process usage {:.0} / budget {:.0} MiB). Reduce resolution or retained history",
                additional_bytes as f64 / 1048576.,
                m.available_bytes as f64 / 1048576.,
                m.source,
                m.used_bytes as f64 / 1048576.,
                m.budget_bytes as f64 / 1048576.
            ));
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

#[cfg(test)]
mod periodic_index_tests {
    use super::*;
    use wgpu::util::DeviceExt;
    #[test]
    fn shader_periodic_indices_match_euclidean_wrapping_including_non_power_of_two_sizes() {
        let _lock = crate::gpu_test_lock();
        let context = GpuContext::headless().unwrap();
        let mut cases = Vec::<[i32; 2]>::new();
        for size in [1, 2, 3, 7, 16, 24, 31, 48, 257, 768, 1024, 65535] {
            for value in [
                i32::MIN,
                -100000,
                -65,
                -49,
                -48,
                -25,
                -24,
                -23,
                -1,
                0,
                1,
                23,
                24,
                25,
                48,
                49,
                i32::MAX,
            ] {
                cases.push([value, size]);
            }
        }
        let input = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Signed periodic coordinate cases"),
                contents: bytemuck::cast_slice(&cases),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: cases.len() as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let source = format!(
            "{}\n@group(0) @binding(0) var<storage,read> cases:array<vec2<i32>>; @group(0) @binding(1) var<storage,read_write> output:array<i32>; @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) g:vec3<u32>){{if g.x>=arrayLength(&cases){{return;}}let c=cases[g.x];output[g.x]=wrap_periodic(c.x,u32(c.y));}}",
            include_str!("periodic.wgsl")
        );
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Portable periodic index regression"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline = context
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let bind = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: input.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: output.as_entire_binding(),
                    },
                ],
            });
        let mut enc = context.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((cases.len() as u32).div_ceil(64), 1, 1);
        }
        context.queue.submit(Some(enc.finish()));
        let bytes = read_bytes(&context, &output, 0, output.size()).unwrap();
        let actual = bytemuck::cast_slice::<u8, i32>(&bytes);
        for (case, result) in cases.iter().zip(actual) {
            assert_eq!(
                *result,
                case[0].rem_euclid(case[1]),
                "coordinate {} size {}",
                case[0],
                case[1]
            );
        }
    }
}
