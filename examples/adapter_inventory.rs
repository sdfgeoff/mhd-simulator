//! Inspect headless adapter selection before scheduling independent devices.
use mhd_workbench::gpu::GpuContext;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapters: Vec<_> = instance
        .enumerate_adapters(wgpu::Backends::VULKAN)
        .into_iter()
        .map(|adapter| {
            let info = adapter.get_info();
            let limits = adapter.limits();
            serde_json::json!({
                "name": info.name, "vendor": info.vendor, "device": info.device,
                "backend": format!("{:?}", info.backend),
                "device_type": format!("{:?}", info.device_type),
                "driver": info.driver, "driver_info": info.driver_info,
                "max_storage_buffer_binding_size": limits.max_storage_buffer_binding_size,
                "max_buffer_size": limits.max_buffer_size
            })
        })
        .collect();
    let selected = GpuContext::headless()?;
    if std::env::args().any(|arg| arg == "--expect-amd")
        && (!selected.name.contains("AMD Radeon") || adapters.iter().any(|a| a["vendor"] != 0x1002))
    {
        return Err("Expected an isolated AMD adapter before compute work".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "vulkan_adapters": adapters,
            "default_headless_selected_name": selected.name,
            "driver_manifest": std::env::var("VK_DRIVER_FILES").ok(),
            "scope": "Device initialization/selection only; no simulation dispatches"
        }))?
    );
    Ok(())
}
