//! Recreate the loadable nozzle benchmark with the same geometry as jet_tests.
use glam::Vec3;
use mhd_workbench::{
    fluid::FluidConfig,
    forcing::JetPulse,
    model::Model,
    plane::SectionPlane,
    project,
    run::RunConfig,
    simulation::{DomainSettings, default_materials},
    surface::SurfaceConfig,
};
use serde_json::json;
use std::{f32::consts::TAU, path::Path};
fn main() -> Result<(), String> {
    let profiles: [(&str, &[[f32; 2]]); 2] = [
        (
            "Nozzle wall",
            &[
                [0.75, -1.],
                [0.75, 0.15],
                [0.15, 0.15],
                [0.15, 0.],
                [0.6, 0.],
                [0.6, -1.],
            ],
        ),
        (
            "Liquid pool",
            &[
                [0., -0.75],
                [0.6, -0.75],
                [0.6, 0.],
                [0.15, 0.],
                [0.15, 0.15],
                [0., 0.15],
            ],
        ),
    ];
    let mut bytes = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    for (name, profile) in profiles {
        let point = |ring: usize, j: usize| {
            let [r, z] = profile[ring];
            let t = TAU * j as f32 / 96.;
            Vec3::new(r * t.cos(), r * t.sin(), z)
        };
        let mut vertices = Vec::<[f32; 3]>::new();
        for ring in 0..profile.len() {
            for j in 0..96 {
                let k = (j + 1) % 96;
                let next = (ring + 1) % profile.len();
                for t in [
                    [point(ring, j), point(next, j), point(next, k)],
                    [point(ring, j), point(next, k), point(ring, k)],
                ] {
                    if (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 1e-18 {
                        vertices.extend(t.map(|p| (p * 0.001).to_array()));
                    }
                }
            }
        }
        let lo = vertices
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |v, p| v.min(Vec3::from(*p)));
        let hi = vertices
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |v, p| v.max(Vec3::from(*p)));
        let data: &[u8] = bytemuck::cast_slice(&vertices);
        views.push(
            json!({"buffer":0,"byteOffset":bytes.len(),"byteLength":data.len(),"target":34962}),
        );
        bytes.extend_from_slice(data);
        accessors.push(json!({"bufferView":views.len()-1,"componentType":5126,"count":vertices.len(),"type":"VEC3","min":lo.to_array(),"max":hi.to_array()}));
        meshes.push(
            json!({"name":name,"primitives":[{"attributes":{"POSITION":accessors.len()-1}}]}),
        );
        nodes.push(json!({"name":name,"mesh":meshes.len()-1}));
    }
    let gltf = json!({"asset":{"version":"2.0","generator":"make_nozzle (mm geometry, metres in glTF)"},"scene":0,"scenes":[{"nodes":[0,1]}],"nodes":nodes,"meshes":meshes,"buffers":[{"uri":"nozzle.bin","byteLength":bytes.len()}],"bufferViews":views,"accessors":accessors});
    std::fs::write("assets/nozzle.bin", bytes).map_err(|e| e.to_string())?;
    std::fs::write(
        "assets/nozzle.gltf",
        serde_json::to_vec_pretty(&gltf).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    let mut model = Model::load(Path::new("assets/nozzle.gltf"))?;
    for (body, id) in model.bodies.iter_mut().zip([7, 3]) {
        body.simulation.material_id = id;
        body.simulation.temperature_c = Some(250.);
    }
    let mut materials = default_materials();
    materials[3].name = "Benchmark liquid (uncalibrated)".into();
    materials[3].density_kg_m3 = Some(7000.);
    materials[3].dynamic_viscosity_pa_s = Some(0.002);
    materials[3].surface_tension_n_m = Some(0.5);
    materials[3].notes="Synthetic nozzle regression: rho 7000 kg/m3, viscosity 0.002 Pa s, surface tension 0.5 N/m. Prescribed force; no calibrated solder or coil prediction. Thermal evolution disabled.".into();
    materials[3].reference = None;
    let dt = 0.002
        / (0.002
            / (0.1
                * (7001.2_f64 * (50e-6_f64).powi(3) / (2. * std::f64::consts::PI * 0.5)).sqrt()))
        .ceil();
    project::save(
        Path::new("assets/nozzle-pulse.mhd.json"),
        &model,
        &DomainSettings {
            min_mm: [-0.9, -0.9, -2.],
            max_mm: [0.9, 0.9, 1.5],
            cell_mm: 0.05,
        },
        &materials,
        SectionPlane::new(Vec3::ZERO),
        &Default::default(),
        &FluidConfig {
            material_id: 3,
            surface: SurfaceConfig { enabled: true },
            acceleration_m_s2: [0.; 3],
            dt_s: dt,
            relative_tolerance: 1e-5,
            absolute_divergence_tolerance: 1e-3,
            max_iterations: 10000,
            ..Default::default()
        },
        &RunConfig {
            dt_s: dt,
            prescribed_jet: Some(JetPulse::default()),
            ..Default::default()
        },
    )?;
    println!(
        "assets/nozzle-pulse.mhd.json: {} steps to 2 ms at {dt} s",
        (0.002 / dt).round()
    );
    Ok(())
}
