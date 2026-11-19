//! Same native cold probe, with strict independent-adapter provenance.
#[path = "hardware_probe.rs"]
#[allow(dead_code)]
mod native_probe;
use mhd_workbench::gpu::GpuContext;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let out = std::path::Path::new(args.get(3).ok_or("Expected PROJECT STEPS OUTPUT ZLO ZHI")?);
    if out.exists() {
        return Err("Existing output; inspect before repeating".into());
    }
    let manifest = std::env::var("VK_DRIVER_FILES")?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapters = instance.enumerate_adapters(wgpu::Backends::VULKAN);
    if adapters.len() != 1 || adapters[0].get_info().vendor != 0x1002 {
        return Err("The private manifest must expose only the AMD device".into());
    }
    let info = adapters[0].get_info();
    let selected = GpuContext::headless()?;
    if selected.name != info.name {
        return Err("Default native probe did not select the isolated AMD adapter".into());
    }
    let expected = info.name.clone();
    let provenance = serde_json::json!({
        "name": info.name, "vendor": info.vendor, "device": info.device,
        "driver": info.driver, "driver_info": info.driver_info,
        "backend": format!("{:?}",info.backend), "manifest": manifest,
        "method": "Delegate to the unchanged hardware_probe body and normal RunState; no alternate field solver. One compute job per physical adapter."
    });
    eprintln!("Independent compute adapter: {provenance}");
    drop(selected);
    let result = native_probe::run_with_expected_adapter(Some(&expected));
    if out.exists() {
        std::fs::write(
            out.join("adapter-provenance.json"),
            serde_json::to_vec_pretty(&provenance)?,
        )?;
    }
    result
}
