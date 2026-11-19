//! Native cold probe, requiring the runner's verified default-adapter selection.
#[path = "hardware_probe.rs"]
#[allow(dead_code)]
mod native_probe;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expected = std::env::var("MHD_EXPECT_ADAPTER")?;
    let context = mhd_workbench::gpu::GpuContext::headless()?;
    if context.name != expected || !expected.contains("NVIDIA") {
        return Err("Expected the verified NVIDIA adapter for this queue".into());
    }
    drop(context);
    native_probe::run_with_expected_adapter(Some(&expected))
}
