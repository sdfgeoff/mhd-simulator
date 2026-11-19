//! Opt-in stationary endpoint control; PDE, timestep schedule and fields unchanged.
#[path = "hardware_probe.rs"]
#[allow(dead_code)]
mod native_probe;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expected = std::env::var("MHD_EXPECT_ADAPTER")?;
    let context = mhd_workbench::gpu::GpuContext::headless()?;
    if context.name != expected {
        return Err("Expected the explicitly verified physical adapter".into());
    }
    drop(context);
    native_probe::run_with_stationary_decay(Some(&expected))
}
