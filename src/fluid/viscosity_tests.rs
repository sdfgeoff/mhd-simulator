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

#[test]
fn material_bound_covers_gpu_stencil_with_fraction_roundoff_and_walls() {
    let _lock = crate::gpu_test_lock();
    let n = 8u32;
    let tolerance = crate::surface::FRACTION_TOLERANCE;
    for (rho_l, rho_g) in [(7000., 1.), (1., 7000.), (1., 1.)] {
        for periodic in [[true, false, true], [false; 3]] {
            let mut materials = default_materials();
            materials[3].density_kg_m3 = Some(rho_l);
            materials[3].dynamic_viscosity_pa_s = Some(0.2);
            materials[3].surface_tension_n_m = Some(0.);
            materials[0].density_kg_m3 = Some(rho_g);
            materials[0].dynamic_viscosity_pa_s = Some(0.01);
            let (gpu, volume, _, _) = crate::surface::tests::box_fixture(
                DomainSettings {
                    min_mm: [0.; 3],
                    max_mm: [1000.; 3],
                    cell_mm: 125.,
                },
                &materials,
            );
            let (mut surface, _) =
                SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, periodic)
                    .unwrap();
            let values: Vec<[f32; 4]> = (0..n.pow(3))
                .map(|i| {
                    let a = if rho_l == rho_g {
                        1. + tolerance
                    } else {
                        match i % n % 4 {
                            0 | 1 => 1. + tolerance,
                            2 => -tolerance,
                            _ => 0.4,
                        }
                    };
                    [a, 0., 0., 0.]
                })
                .collect();
            gpu.context
                .queue
                .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
            surface.refresh(&volume).unwrap();
            let mut state = FluidState::new(
                gpu.context.clone(),
                &volume,
                FluidConfig {
                    surface: crate::surface::SurfaceConfig { enabled: true },
                    periodic,
                    advection: false,
                    acceleration_m_s2: [0.; 3],
                    dt_s: 1e-6,
                    ..Default::default()
                },
                None,
            )
            .unwrap();
            state
                .update_surface(&volume, surface.field(), None)
                .unwrap();
            let upper = state.global_viscous_bound().unwrap();
            state.dispatch(&[
                ("viscosity_bound", groups(state.params.faces[3])),
                ("viscosity_bound_reduce", [1, 1, 1]),
            ]);
            let bytes = read_bytes(&gpu.context, &state.scalars, 0, 4).unwrap();
            let measured = bytemuck::pod_read_unaligned::<f32>(&bytes) as f64;
            assert!(
                measured <= upper,
                "GPU bound {measured} exceeds material bound {upper}, densities {rho_l}/{rho_g}, periodic {periodic:?}"
            );
            // Exercise a step just below the proof's upper timestep. Zero velocity
            // isolates stability selection from the independent interface CFL.
            state.config.dt_s = 1.8 / upper * (1. - 1e-5);
            state.step(&volume).unwrap();
            assert_eq!(state.diagnostics.viscous_substeps, 1);
            assert!(state.diagnostics.max_speed_m_s < 1e-6);
        }
    }
}

#[test]
fn fluid_rejects_nonfinite_or_unbounded_interface_fractions() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut surface, _) = crate::surface::tests::fixture(8);
    let mut state = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            surface: crate::surface::SurfaceConfig { enabled: true },
            acceleration_m_s2: [0.; 3],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let n = 8u32.pow(3) as usize;
    let good = vec![[0.5f32, 0., 0., 0.]; n];
    for bad in [f32::NAN, f32::INFINITY, -0.001, 1.001] {
        let mut data = good.clone();
        data[0][0] = bad;
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&data));
        let error = state
            .update_surface(&volume, surface.field(), None)
            .unwrap_err();
        assert!(error.contains("fractions must be finite"), "{error}");
        assert_eq!(state.time_s, 0.);
    }
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&good));
    surface.refresh(&volume).unwrap();
    state
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    state.step(&volume).unwrap();
}

#[test]
fn stabilized_stress_diffusion_matches_shear_decay_and_time_refines() {
    let _lock = crate::gpu_test_lock();
    let n = 32u32;
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(1.);
    materials[3].dynamic_viscosity_pa_s = Some(1.);
    materials[3].surface_tension_n_m = Some(0.);
    let (gpu, volume, _, _) = crate::surface::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000., 1000., 125.],
            cell_mm: 1000. / n as f32,
        },
        &materials,
    );
    let total_time = 0.016;
    let lambda = 4. * (n as f64).powi(2) * (std::f64::consts::PI / n as f64).sin().powi(2);
    let exact = (-lambda * total_time).exp();
    let mut errors = Vec::new();
    for ticks in [1, 2, 4, 8] {
        let mut state = FluidState::new(
            gpu.context.clone(),
            &volume,
            FluidConfig {
                surface: crate::surface::SurfaceConfig { enabled: true },
                periodic: [true; 3],
                advection: false,
                acceleration_m_s2: [0.; 3],
                dt_s: total_time / ticks as f64,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let f = state.params.faces;
        let initial: Vec<[f32; 4]> = (0..f[3])
            .map(|i| {
                let y = (i / f[0]) % f[1];
                [
                    (std::f64::consts::TAU * (y as f64 + 0.5) / n as f64).cos() as f32,
                    0.,
                    0.,
                    0.,
                ]
            })
            .collect();
        gpu.context
            .queue
            .write_buffer(&state.scratch, 0, bytemuck::cast_slice(&initial));
        gpu.context
            .queue
            .write_buffer(&state.velocity, 0, bytemuck::cast_slice(&initial));
        for _ in 0..ticks {
            state.diffuse_surface_stress().unwrap();
        }
        assert!(state.diagnostics.viscous_substeps > 1);
        let bytes = read_bytes(&gpu.context, &state.scratch, 0, state.scratch.size()).unwrap();
        let result = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        let mut error = 0.;
        let mut norm = 0.;
        for z in 0..4 {
            for y in 0..n {
                for x in 0..n {
                    let i = (x + f[0] * (y + f[1] * z)) as usize;
                    error += (result[i][0] as f64 - exact * initial[i][0] as f64).powi(2);
                    norm += (initial[i][0] as f64).powi(2);
                    assert!(result[i][1].abs() < 1e-6 && result[i][2].abs() < 1e-6);
                }
            }
        }
        let error = (error / norm).sqrt();
        eprintln!(
            "RKC shear ticks={ticks}: relative L2={error:.7e}, stages={}",
            state.diagnostics.viscous_substeps
        );
        errors.push(error);
    }
    assert!(errors.windows(2).all(|w| w[1] < 0.3 * w[0]), "{errors:?}");
    assert!(*errors.last().unwrap() < 0.01);
}

#[test]
fn stabilized_variable_density_stress_dissipates_energy_and_preserves_momentum_and_live_lanes() {
    let _lock = crate::gpu_test_lock();
    let n = 12u32;
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(7000.);
    materials[3].dynamic_viscosity_pa_s = Some(1.4);
    materials[3].surface_tension_n_m = Some(0.);
    materials[0].density_kg_m3 = Some(1.);
    materials[0].dynamic_viscosity_pa_s = Some(0.);
    let (gpu, v, _, _) = crate::surface::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &materials,
    );
    let mut state = FluidState::new(
        gpu.context.clone(),
        &v,
        FluidConfig {
            surface: crate::surface::SurfaceConfig { enabled: true },
            periodic: [true; 3],
            advection: false,
            acceleration_m_s2: [0.; 3],
            dt_s: 0.2,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let packets: Vec<FluidCell> = (0..n * n * n)
        .map(|i| FluidCell {
            pressure: [i as f32 * 0.01, 2., 3., 4.],
            reserved: [1., 1., f32::from(i % n < n / 2), 5.],
            ..Default::default()
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(&state.result, 0, bytemuck::cast_slice(&packets));
    let f = state.params.faces;
    let initial: Vec<[f32; 4]> = (0..f[3])
        .map(|i| {
            let q = [i % f[0], (i / f[0]) % f[1], i / (f[0] * f[1])].map(|x| x % n);
            let at = (q[0] + n * (q[1] + n * q[2])) as f32;
            [
                (at * 0.391).sin(),
                (at * 0.717).cos(),
                (at * 0.923).sin(),
                0.,
            ]
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(&state.scratch, 0, bytemuck::cast_slice(&initial));
    gpu.context
        .queue
        .write_buffer(&state.velocity, 0, bytemuck::cast_slice(&initial));
    state.diffuse_surface_stress().unwrap();
    assert!(state.diagnostics.viscous_substeps > 8);
    let bytes = read_bytes(&gpu.context, &state.scratch, 0, state.scratch.size()).unwrap();
    let result = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
    let measure = |field: &[[f32; 4]]| {
        let mut energy = 0.;
        let mut p = [0.; 3];
        let mut scale = 0.;
        let rho = |q: [u32; 3]| if q[0] < n / 2 { 7000. } else { 1. };
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    let i = (x + f[0] * (y + f[1] * z)) as usize;
                    for a in 0..3 {
                        let mut left = [x, y, z];
                        left[a] = (left[a] + n - 1) % n;
                        let mass = 0.5 * (rho(left) + rho([x, y, z]));
                        let u = field[i][a] as f64;
                        energy += 0.5 * mass * u * u;
                        p[a] += mass * u;
                        scale += mass * u.abs();
                    }
                }
            }
        }
        (energy, p, scale)
    };
    let before = measure(&initial);
    let after = measure(result);
    assert!(after.0 < before.0, "{} -> {}", before.0, after.0);
    for a in 0..3 {
        assert!(
            (after.1[a] - before.1[a]).abs() / before.2 < 2e-6,
            "momentum {a}: {} -> {}",
            before.1[a],
            after.1[a]
        );
    }
    let bytes = read_bytes(&gpu.context, &state.result, 0, state.result.size()).unwrap();
    for (c, old) in bytemuck::cast_slice::<u8, FluidCell>(&bytes)
        .iter()
        .zip(&packets)
    {
        assert_eq!(c.pressure, old.pressure);
        assert_eq!(c.reserved, old.reserved);
    }
    eprintln!(
        "RKC density contrast: stages={}, kinetic ratio={:.8}",
        state.diagnostics.viscous_substeps,
        after.0 / before.0
    );
}

#[test]
fn failed_projection_after_stabilized_stress_restores_packets_and_retry_rebuilds_display() {
    let _lock = crate::gpu_test_lock();
    let n = 12u32;
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(1.);
    materials[3].dynamic_viscosity_pa_s = Some(1.);
    materials[3].surface_tension_n_m = Some(0.);
    let (gpu, v, _, _) = crate::surface::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &materials,
    );
    let mut s = FluidState::new(
        gpu.context.clone(),
        &v,
        FluidConfig {
            surface: crate::surface::SurfaceConfig { enabled: true },
            periodic: [true; 3],
            acceleration_m_s2: [0.; 3],
            advection: false,
            dt_s: 0.01,
            relative_tolerance: 1e-7,
            absolute_divergence_tolerance: 0.,
            max_iterations: 1,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let f = s.params.faces;
    let initial: Vec<[f32; 4]> = (0..f[3])
        .map(|i| {
            let q = [i % f[0], (i / f[0]) % f[1], i / (f[0] * f[1])].map(|x| x % n);
            let at = (q[0] + n * (q[1] + n * q[2])) as f32;
            [
                0.01 * (at * 0.391).sin(),
                0.01 * (at * 0.717).cos(),
                0.01 * (at * 0.923).sin(),
                0.,
            ]
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(&s.velocity, 0, bytemuck::cast_slice(&initial));
    s.dispatch(&[("fields", groups(n.pow(3))), ("diagnostics", [1, 1, 1])]);
    s.read_diagnostics(0, 0., 0.).unwrap();
    let accepted = read_bytes(&gpu.context, &s.result, 0, s.result.size()).unwrap();
    let velocity = read_bytes(&gpu.context, &s.velocity, 0, s.velocity.size()).unwrap();
    let error = s.step(&v).unwrap_err();
    assert!(error.contains("pressure solve"), "{error}");
    assert_eq!(s.time_s, 0.);
    assert_eq!(
        read_bytes(&gpu.context, &s.result, 0, s.result.size()).unwrap(),
        accepted
    );
    assert_eq!(
        read_bytes(&gpu.context, &s.velocity, 0, s.velocity.size()).unwrap(),
        velocity
    );
    s.config.max_iterations = 1000;
    s.step(&v).unwrap();
    assert!(s.diagnostics.viscous_substeps > 1);
    assert_eq!(s.time_s, 0.01);
    let bytes = read_bytes(&gpu.context, &s.velocity, 0, s.velocity.size()).unwrap();
    let faces = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
    let bytes = read_bytes(&gpu.context, &s.result, 0, s.result.size()).unwrap();
    let cells = bytemuck::cast_slice::<u8, FluidCell>(&bytes);
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let cell = &cells[(x + n * (y + n * z)) as usize];
                for a in 0..3 {
                    let q = [x, y, z];
                    let mut upper = q;
                    upper[a] = (upper[a] + 1) % n;
                    let idx = |q: [u32; 3]| (q[0] + f[0] * (q[1] + f[1] * q[2])) as usize;
                    let average = 0.5 * (faces[idx(q)][a] + faces[idx(upper)][a]);
                    assert!((cell.velocity[a] - average).abs() < 1e-7);
                }
                assert_eq!(cell.force, [0.; 4]);
                assert_eq!(&cell.reserved[..3], &[1., 1., 1.]);
                assert_eq!(cell.reserved[3], 0.);
            }
        }
    }
}
