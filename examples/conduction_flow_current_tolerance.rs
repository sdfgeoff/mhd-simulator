//! Explicit dimensional-current criteria, preserving earlier executable versions.
#[path = "conduction_flow.rs"]
#[allow(dead_code)]
mod experiment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    experiment::main()
}
