//! Regenerate the explicitly illustrative, runnable hotend EM example.
use mhd_workbench::{em, model::Model, plane::SectionPlane, project, simulation::DomainSettings};
fn main() -> Result<(), String> {
    let mut model = Model::load(std::path::Path::new("assets/hotend.gltf"))?;
    let (materials, config) = em::hotend_example(&mut model)?;
    let settings = DomainSettings {
        min_mm: [-12., -12., -8.],
        max_mm: [12., 12., 16.],
        cell_mm: 0.4,
    };
    project::save_with_em(
        std::path::Path::new("assets/hotend-em.mhd.json"),
        &model,
        &settings,
        &materials,
        SectionPlane::new(glam::Vec3::new(0., 0., 4.)),
        &config,
    )
}
