//! Native cold probe with explicit zero-current decay timestep schedule.
#[path = "amd_hardware_probe.rs"]
#[allow(dead_code)]
mod experiment;
fn main() -> Result<(), Box<dyn std::error::Error>> { experiment::main() }
