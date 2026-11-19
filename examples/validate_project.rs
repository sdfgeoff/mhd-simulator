//! CPU-only validation through the same project loader as the GUI and solver.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        return Err("Expected one or more project files".into());
    }
    for path in paths {
        let project = mhd_workbench::project::load(std::path::Path::new(&path))?;
        println!(
            "{path}: {} bodies, {} CAD curves; fluid={}, thermal={}",
            project.model.bodies.len(),
            project.model.curves.len(),
            project.run.fluid_enabled,
            project.run.thermal_enabled
        );
    }
    Ok(())
}
