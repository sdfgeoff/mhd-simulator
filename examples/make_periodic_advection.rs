//! Recreate a source-free moving-interface project for periodic transport checks.
use glam::Vec3;
use mhd_workbench::{
    fluid::FluidConfig,
    model::Model,
    plane::SectionPlane,
    project,
    run::RunConfig,
    simulation::{DomainSettings, default_materials},
    surface::SurfaceConfig,
};
fn main() -> Result<(), String> {
    let mut model = Model::load(std::path::Path::new("assets/annulus.gltf"))?;
    for body in &mut model.bodies {
        body.simulation.material_id = 3;
    }
    let mut materials = default_materials();
    // Isolate translation: no capillary force, acceleration or electrical sources.
    materials[3].surface_tension_n_m = Some(0.);
    let fluid = FluidConfig {
        surface: SurfaceConfig { enabled: true },
        periodic: [true; 3],
        initial_velocity_m_s: [0.1, 0.05, -0.025],
        acceleration_m_s2: [0.; 3],
        dt_s: 0.001,
        ..Default::default()
    };
    project::save(
        std::path::Path::new("assets/periodic-advection.mhd.json"),
        &model,
        &DomainSettings {
            min_mm: [-12.; 3],
            max_mm: [12.; 3],
            cell_mm: 1.,
        },
        &materials,
        SectionPlane::new(Vec3::ZERO),
        &Default::default(),
        &fluid,
        &RunConfig {
            dt_s: fluid.dt_s,
            ..Default::default()
        },
    )
}
