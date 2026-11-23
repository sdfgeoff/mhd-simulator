//! Large-grid DC and pressure submission boundaries; unchanged physical experiment.
#[path = "conduction_flow.rs"]
#[allow(dead_code)]
mod experiment;
fn main() -> Result<(), Box<dyn std::error::Error>> { experiment::main() }
