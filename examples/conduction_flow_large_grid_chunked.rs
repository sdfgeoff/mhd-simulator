//! Large-grid dispatch with iteration-sized fenced command submissions.
#[path = "conduction_flow.rs"]
#[allow(dead_code)]
mod experiment;
fn main() -> Result<(), Box<dyn std::error::Error>> { experiment::main() }
