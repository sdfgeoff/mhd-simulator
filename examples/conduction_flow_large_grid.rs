//! Same flow experiment with multi-row conduction dispatch for large grids.
#[path = "conduction_flow.rs"]
#[allow(dead_code)]
mod experiment;
fn main() -> Result<(), Box<dyn std::error::Error>> { experiment::main() }
