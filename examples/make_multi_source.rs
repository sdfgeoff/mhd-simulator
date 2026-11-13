//! Recreate the main-CAD multi-source example, retaining source bindings to curves/bodies.
use glam::Vec3;
use mhd_workbench::{
    em,
    model::Model,
    plane::SectionPlane,
    project,
    simulation::{DomainSettings, default_materials},
};
fn main() -> Result<(), String> {
    let mut model = Model::load(std::path::Path::new("assets/hotend-cad.gltf"))?;
    for (i, body) in model.bodies.iter_mut().enumerate() {
        body.simulation.material_id = match i {
            0 => 6,
            1 => 3,
            2 => 7,
            _ => 5,
        };
    }
    let mut config = em::EmConfig {
        wires: em::cad_wires(&model),
        ..Default::default()
    };
    config.wires[0].current = em::Waveform::Pulse {
        peak: 100.,
        delay_s: 0.,
        rise_s: 50e-6,
        hold_s: 50e-6,
        fall_s: 50e-6,
    };
    config.wires[1].current = em::Waveform::Pulse {
        peak: -35.,
        delay_s: 25e-6,
        rise_s: 25e-6,
        hold_s: 25e-6,
        fall_s: 50e-6,
    };
    for (body, x) in [(3, -8e5), (4, 8e5)] {
        config.magnets.push(em::Magnet {
            body,
            magnetization_a_m: [x, 0., 0.],
            subdivisions: 1,
            smoothing_mm: 0.05,
        });
    }
    let settings = DomainSettings {
        min_mm: [-16., -12., -8.],
        max_mm: [20., 12., 16.],
        cell_mm: 0.6,
    };
    project::save(
        std::path::Path::new("assets/hotend-cad.mhd.json"),
        &model,
        &settings,
        &default_materials(),
        SectionPlane::new(Vec3::new(0., 0., 4.)),
        &config,
        &Default::default(),
    )
}
