//! Existing native electrode client with complete energy/profiler diagnostics.
#[path = "conduction_flow.rs"]
#[allow(dead_code)]
mod experiment;
fn main() -> Result<(), Box<dyn std::error::Error>> { experiment::main() }
