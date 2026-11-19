//! Separate executable preserves historical binaries while using the same client.
#[path = "conduction_flow.rs"]
#[allow(dead_code)]
mod experiment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    experiment::main()
}
