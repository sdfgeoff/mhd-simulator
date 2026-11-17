use super::*;
use crate::{
    simulation::{DomainSettings, default_materials},
    surface::SurfaceState,
};

#[test]
fn manufactured_variable_viscosity_symmetric_stress_refines() {
    let _lock = crate::gpu_test_lock();
    let mut errors = Vec::new();
    for n in [16u32, 32] {
        let h = 1. / n as f32;
        let nz = 4;
        let mut materials = default_materials();
        materials[3].density_kg_m3 = Some(1.);
        materials[3].dynamic_viscosity_pa_s = Some(1.);
        materials[3].surface_tension_n_m = Some(0.);
        materials[0].density_kg_m3 = Some(1.);
        materials[0].dynamic_viscosity_pa_s = Some(0.);
        let (gpu, volume, _, _) = crate::surface::tests::box_fixture(
            DomainSettings {
                min_mm: [0.; 3],
                max_mm: [1000., 1000., 1000. * nz as f32 / n as f32],
                cell_mm: 1000. / n as f32,
            },
            &materials,
        );
        let (mut surface, _) =
            SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, [true; 3])
                .unwrap();
        let alpha: Vec<[f32; 4]> = (0..n * n * nz)
            .map(|i| {
                let x = (i % n) as f64 + 0.5;
                [
                    (0.5 + 0.25 * (std::f64::consts::TAU * x / n as f64).cos()) as f32,
                    0.,
                    0.,
                    0.,
                ]
            })
            .collect();
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&alpha));
        surface.refresh(&volume).unwrap();
        let dt = 0.01 * h * h;
        let mut state = FluidState::new(
            gpu.context.clone(),
            &volume,
            FluidConfig {
                surface: crate::surface::SurfaceConfig { enabled: true },
                periodic: [true; 3],
                advection: false,
                acceleration_m_s2: [0.; 3],
                dt_s: dt as f64,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        state
            .update_surface(&volume, surface.field(), None)
            .unwrap();
        let f = state.params.faces;
        let velocities: Vec<[f32; 4]> = (0..f[3])
            .map(|i| {
                let x = (i % f[0]) as f64;
                let y = ((i / f[0]) % f[1]) as f64;
                let k = std::f64::consts::TAU / n as f64;
                [
                    (2. * (k * x).sin() * (2. * k * (y + 0.5)).cos()) as f32,
                    (-(k * (x + 0.5)).cos() * (2. * k * y).sin()) as f32,
                    0.,
                    0.,
                ]
            })
            .collect();
        gpu.context
            .queue
            .write_buffer(&state.velocity, 0, bytemuck::cast_slice(&velocities));
        state.dispatch(&[("predict", groups(f[3])), ("stress_diffuse", groups(f[3]))]);
        let output = read_bytes(&gpu.context, &state.velocity, 0, state.velocity.size()).unwrap();
        let output = bytemuck::cast_slice::<u8, [f32; 4]>(&output);
        let mut error = 0.;
        let mut norm = 0.;
        let k = std::f64::consts::TAU;
        for z in 0..nz {
            for y in 0..n {
                for x in 0..n {
                    let i = (x + f[0] * (y + f[1] * z)) as usize;
                    for axis in 0..2 {
                        let xx = (x as f64 + if axis == 0 { 0. } else { 0.5 }) / n as f64;
                        let yy = (y as f64 + if axis == 1 { 0. } else { 0.5 }) / n as f64;
                        let sx = (k * xx).sin();
                        let cx = (k * xx).cos();
                        let sy = (2. * k * yy).sin();
                        let cy = (2. * k * yy).cos();
                        let mu = 0.5 + 0.25 * cx;
                        let mux = -0.25 * k * sx;
                        let exact = if axis == 0 {
                            -10. * mu * k * k * sx * cy + 4. * k * mux * cx * cy
                        } else {
                            5. * mu * k * k * cx * sy - 3. * k * mux * sx * sy
                        };
                        let actual =
                            (output[i][axis] as f64 - velocities[i][axis] as f64) / dt as f64;
                        error += (actual - exact).powi(2);
                        norm += exact.powi(2);
                    }
                }
            }
        }
        let relative = (error / norm).sqrt();
        eprintln!("Variable-viscosity symmetric stress n={n}: L2 relative error {relative:.6e}");
        errors.push(relative);
    }
    assert!(errors[1] < 0.02);
    assert!(errors[1] < 0.3 * errors[0]);
}

#[test]
fn closed_wall_stress_is_symmetric_and_dissipative() {
    let _lock = crate::gpu_test_lock();
    let n = 8u32;
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(1.);
    materials[3].dynamic_viscosity_pa_s = Some(1.);
    materials[3].surface_tension_n_m = Some(0.);
    materials[0].density_kg_m3 = Some(1.);
    materials[0].dynamic_viscosity_pa_s = Some(0.1);
    let (gpu, volume, _, _) = crate::surface::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &materials,
    );
    let (mut surface, _) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
    let alpha: Vec<[f32; 4]> = (0..n.pow(3))
        .map(|i| [0.25 + 0.5 * (i as f32 * 0.37).sin().powi(2), 0., 0., 0.])
        .collect();
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&alpha));
    surface.refresh(&volume).unwrap();
    let dt = 1e-4;
    let mut state = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            surface: crate::surface::SurfaceConfig { enabled: true },
            advection: false,
            acceleration_m_s2: [0.; 3],
            dt_s: dt,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    state
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    for internal_wall in [false, true] {
        // An isolated voxel block and a step exercise concave/convex CAD-like
        // edge stencils in addition to the six outer no-slip walls.
        let mask: Vec<u32> = (0..n.pow(3))
            .map(|i| {
                let x = i % n;
                let y = (i / n) % n;
                let z = i / (n * n);
                let blocked = (x >= 3 && x <= 5 && y >= 3 && y <= 5 && z >= 3 && z <= 5)
                    || (x >= 5 && y >= 1 && z >= 4);
                u32::from(!internal_wall || !blocked)
            })
            .collect();
        gpu.context
            .queue
            .write_buffer(&state._mask, 0, bytemuck::cast_slice(&mask));
        let mut fields = Vec::new();
        for seed in [7., 19.] {
            let initial: Vec<[f32; 4]> = (0..state.params.faces[3])
                .map(|i| {
                    [
                        ((i as f32 + seed) * 0.391).sin(),
                        ((i as f32 + seed) * 0.717).cos(),
                        ((i as f32 + seed) * 0.923).sin(),
                        0.,
                    ]
                })
                .collect();
            gpu.context
                .queue
                .write_buffer(&state.velocity, 0, bytemuck::cast_slice(&initial));
            state.dispatch(&[("phase_faces", groups(state.params.faces[3]))]);
            let initial =
                read_bytes(&gpu.context, &state.velocity, 0, state.velocity.size()).unwrap();
            let initial = bytemuck::cast_slice::<u8, [f32; 4]>(&initial).to_vec();
            state.dispatch(&[
                ("predict", groups(state.params.faces[3])),
                ("stress_diffuse", groups(state.params.faces[3])),
            ]);
            let result =
                read_bytes(&gpu.context, &state.velocity, 0, state.velocity.size()).unwrap();
            let result = bytemuck::cast_slice::<u8, [f32; 4]>(&result);
            let derivative: Vec<[f64; 3]> = result
                .iter()
                .zip(&initial)
                .map(|(v, u)| std::array::from_fn(|a| (v[a] as f64 - u[a] as f64) / dt))
                .collect();
            let power: f64 = initial
                .iter()
                .zip(&derivative)
                .map(|(u, l)| (0..3).map(|a| u[a] as f64 * l[a]).sum::<f64>())
                .sum();
            assert!(power < 0., "Viscous power {power}");
            let before: f64 = initial
                .iter()
                .flat_map(|u| &u[..3])
                .map(|u| (*u as f64).powi(2))
                .sum();
            let after: f64 = result
                .iter()
                .flat_map(|u| &u[..3])
                .map(|u| (*u as f64).powi(2))
                .sum();
            assert!(after < before, "Wall viscosity increased kinetic energy");
            fields.push((initial, derivative));
        }
        let xy: f64 = fields[0]
            .0
            .iter()
            .zip(&fields[1].1)
            .map(|(u, l)| (0..3).map(|a| u[a] as f64 * l[a]).sum::<f64>())
            .sum();
        let yx: f64 = fields[1]
            .0
            .iter()
            .zip(&fields[0].1)
            .map(|(u, l)| (0..3).map(|a| u[a] as f64 * l[a]).sum::<f64>())
            .sum();
        let defect = (xy - yx).abs() / xy.abs().max(yx.abs());
        eprintln!("Variable-mu wall stress internal={internal_wall}: symmetry defect {defect:.6e}");
        assert!(defect < 2e-5, "Stress symmetry: {xy} vs {yx}");
    }
}

#[test]
fn ambient_material_viscosity_matches_discrete_shear_decay_with_subcycling() {
    let _lock = crate::gpu_test_lock();
    let n = 16u32;
    let h = 1. / n as f32;
    let mut materials = default_materials();
    materials[3].surface_tension_n_m = Some(0.);
    materials[3].dynamic_viscosity_pa_s = Some(0.);
    materials[0].density_kg_m3 = Some(1.);
    materials[0].dynamic_viscosity_pa_s = Some(0.1);
    let (gpu, volume, _, _) = crate::surface::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &materials,
    );
    let (mut surface, _) =
        SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, [true; 3]).unwrap();
    gpu.context.queue.write_buffer(
        surface.field(),
        0,
        bytemuck::cast_slice(&vec![[0f32; 4]; n.pow(3) as usize]),
    );
    surface.refresh(&volume).unwrap();
    let mut state = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            surface: crate::surface::SurfaceConfig { enabled: true },
            advection: false,
            periodic: [true; 3],
            acceleration_m_s2: [0.; 3],
            dt_s: 0.01,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    state
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    let f = state.params.faces;
    let initial: Vec<[f32; 4]> = (0..f[3])
        .map(|i| {
            let y = ((i / f[0]) % f[1]) as f32;
            [
                0.1 * (std::f32::consts::TAU * (y + 0.5) / n as f32).sin(),
                0.,
                0.,
                0.,
            ]
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(&state.velocity, 0, bytemuck::cast_slice(&initial));
    let mut decay = 1f64;
    for _ in 0..20 {
        state.step(&volume).unwrap();
        let count = state.diagnostics.viscous_substeps;
        assert!(count > 1);
        let lambda =
            4. * 0.1 * (std::f64::consts::PI / n as f64).sin().powi(2) / (h as f64).powi(2);
        decay *= (1. - lambda * 0.01 / count as f64).powi(count as i32);
    }
    let actual = read_bytes(&gpu.context, &state.velocity, 0, state.velocity.size()).unwrap();
    let actual = bytemuck::cast_slice::<u8, [f32; 4]>(&actual);
    let mut error = 0.;
    let mut norm = 0.;
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let i = (x + f[0] * (y + f[1] * z)) as usize;
                let expected = initial[i][0] as f64 * decay;
                error += (actual[i][0] as f64 - expected).powi(2);
                norm += expected.powi(2);
            }
        }
    }
    let relative = (error / norm).sqrt();
    eprintln!(
        "Ambient viscous shear: relative L2={relative:.7e}, substeps={}",
        state.diagnostics.viscous_substeps
    );
    assert!(relative < 2e-5);
    assert!((state.time_s - 0.2).abs() < 1e-12);
}

#[test]
fn excessive_viscosity_substeps_restore_state_and_allow_retry() {
    let _lock = crate::gpu_test_lock();
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(1.);
    materials[3].dynamic_viscosity_pa_s = Some(1.);
    materials[3].surface_tension_n_m = Some(0.);
    let (gpu, volume, _, _) = crate::surface::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 125.,
        },
        &materials,
    );
    let (surface, _) =
        SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, [true; 3]).unwrap();
    let mut state = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            surface: crate::surface::SurfaceConfig { enabled: true },
            periodic: [true; 3],
            advection: false,
            acceleration_m_s2: [0.; 3],
            initial_velocity_m_s: [0.01, 0.02, -0.01],
            dt_s: 0.01,
            max_viscous_substeps: 1,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    state
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    let velocity = read_bytes(&gpu.context, &state.velocity, 0, state.velocity.size()).unwrap();
    let fields = read_bytes(&gpu.context, &state.result, 0, state.result.size()).unwrap();
    let error = state.step(&volume).unwrap_err();
    assert!(
        error.contains("Viscosity requires") && error.contains("substeps"),
        "{error}"
    );
    assert_eq!(state.time_s, 0.);
    assert_eq!(
        velocity,
        read_bytes(&gpu.context, &state.velocity, 0, state.velocity.size()).unwrap()
    );
    assert_eq!(
        fields,
        read_bytes(&gpu.context, &state.result, 0, state.result.size()).unwrap()
    );
    state.config.max_viscous_substeps = 4096;
    state.step(&volume).unwrap();
    assert_eq!(state.time_s, 0.01);
    assert!(state.diagnostics.viscous_substeps > 1);
}
