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
