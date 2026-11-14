use super::*;
use crate::{
    domain::GpuDomain,
    model::{Body, Model},
    simulation::{DomainSettings, default_materials},
};
use glam::Vec3;
fn channel(n: u32, config: FluidConfig) -> (GpuDomain, Volume, FluidState) {
    let h = 0.008 / n as f32;
    let dims = [4, n, 4];
    let hi = Vec3::new(4., n as f32, 4.) * h * 1000.;
    let points = [
        Vec3::ZERO,
        Vec3::new(hi.x, 0., 0.),
        Vec3::new(hi.x, hi.y, 0.),
        Vec3::new(0., hi.y, 0.),
        Vec3::new(0., 0., hi.z),
        Vec3::new(hi.x, 0., hi.z),
        hi,
        Vec3::new(0., hi.y, hi.z),
    ];
    let faces = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [3, 7, 6],
        [3, 6, 2],
        [0, 4, 7],
        [0, 7, 3],
        [1, 2, 6],
        [1, 6, 5],
    ];
    let mut model = Model::default();
    let mut body = Body::new(
        "Fluid channel".into(),
        faces.map(|t| t.map(|i| points[i])).to_vec(),
    )
    .unwrap();
    body.simulation.material_id = 3;
    model.bodies.push(body);
    let settings = DomainSettings {
        min_mm: [0.; 3],
        max_mm: hi.to_array(),
        cell_mm: h * 1000.,
    };
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(1000.);
    materials[3].dynamic_viscosity_pa_s = Some(1.);
    let gpu = GpuDomain::new().unwrap();
    let volume = gpu.build(&model, &settings, &materials).unwrap();
    assert_eq!(volume.dims, dims);
    let state = FluidState::new(gpu.context.clone(), &volume, config, None).unwrap();
    (gpu, volume, state)
}
#[test]
fn periodic_uniform_flow_and_closed_hydrostatic_balance() {
    let _lock = crate::gpu_test_lock();
    let (_, volume, mut state) = channel(
        8,
        FluidConfig {
            acceleration_m_s2: [0.; 3],
            initial_velocity_m_s: [0.03, 0.02, -0.01],
            periodic: [true; 3],
            dt_s: 1e-4,
            ..Default::default()
        },
    );
    for _ in 0..5 {
        state.step(&volume).unwrap();
    }
    let cell = state.snapshot().probe([2., 4., 2.]).unwrap().unwrap();
    for (actual, expected) in cell.velocity[..3].iter().zip([0.03, 0.02, -0.01]) {
        assert!((actual - expected).abs() < 1e-6);
    }
    assert!(state.diagnostics.max_divergence_s_inv < 1e-4);
    let (_, volume, mut closed) = channel(
        8,
        FluidConfig {
            dt_s: 1e-4,
            acceleration_m_s2: [0., -9.81, 0.],
            ..Default::default()
        },
    );
    closed.step(&volume).unwrap();
    assert!(
        closed.diagnostics.max_speed_m_s < 1e-5,
        "{:?}",
        closed.diagnostics
    );
    assert!(
        closed.diagnostics.max_divergence_s_inv < 0.01,
        "{:?}",
        closed.diagnostics
    );
    let low = closed
        .snapshot()
        .probe([2., 1.5, 2.])
        .unwrap()
        .unwrap()
        .pressure[0];
    let high = closed
        .snapshot()
        .probe([2., 6.5, 2.])
        .unwrap()
        .unwrap()
        .pressure[0];
    assert!(
        ((low - high) / (1000. * 9.81 * 0.005) - 1.).abs() < 0.005,
        "{low} {high}"
    );
}
#[test]
fn poiseuille_channel_converges_under_grid_refinement() {
    let _lock = crate::gpu_test_lock();
    let mut errors = Vec::new();
    for n in [8, 16] {
        let h = 0.008 / n as f32;
        let dt = (0.12 * h * h / 0.001) as f64;
        let (_, volume, mut state) = channel(
            n,
            FluidConfig {
                dt_s: dt,
                acceleration_m_s2: [1., 0., 0.],
                periodic: [true, false, true],
                ..Default::default()
            },
        );
        for _ in 0..(0.06 / dt).ceil() as u32 {
            state.step(&volume).unwrap();
        }
        let snapshot = state.snapshot();
        let mut error = 0.;
        let mut norm = 0.;
        for y in 0..n {
            let position = (y as f32 + 0.5) * h;
            let exact = position * (0.008 - position) / (2. * 0.001);
            let cell = snapshot
                .probe([h * 1500., position * 1000., h * 1500.])
                .unwrap()
                .unwrap();
            error += (cell.velocity[0] - exact).powi(2);
            norm += exact * exact;
        }
        let relative = (error / norm).sqrt();
        errors.push(relative);
        eprintln!(
            "Poiseuille Ny={n}: velocity relative L2={relative:.4e}, div={:.3e}",
            state.diagnostics.max_divergence_s_inv
        );
        assert!(state.diagnostics.max_divergence_s_inv < 1e-4);
    }
    assert!(
        errors[1] < 0.015 && errors[1] < errors[0] * 0.45,
        "{errors:?}"
    );
}
#[test]
fn pressure_failure_rolls_back_and_disconnected_components_get_gauges() {
    let _lock = crate::gpu_test_lock();
    let (_, volume, mut state) = channel(
        8,
        FluidConfig {
            dt_s: 1e-4,
            max_iterations: 1,
            ..Default::default()
        },
    );
    let before = read_bytes(&state.context, &state.result, 0, state.result.size()).unwrap();
    assert!(
        state
            .step(&volume)
            .unwrap_err()
            .contains("did not converge")
    );
    assert_eq!(state.time_s, 0.);
    assert_eq!(
        before,
        read_bytes(&state.context, &state.result, 0, state.result.size()).unwrap()
    );
    let mut mask = vec![1, 0, 1];
    assert_eq!(anchors(&mut mask, [3, 1, 1], [false; 3]), 2);
    assert_eq!(
        mask.iter().map(|v| v & 3).collect::<Vec<_>>(),
        vec![2, 0, 2]
    );
    assert_eq!(mask[0] >> 2, 0);
    assert_eq!(mask[2] >> 2, 2);
    let mut mask = vec![1, 0, 1];
    assert_eq!(anchors(&mut mask, [3, 1, 1], [true, false, false]), 1);
}

#[test]
fn frozen_em_force_accelerates_velocity_and_ignores_later_source_changes() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, base) = channel(
        8,
        FluidConfig {
            acceleration_m_s2: [0.; 3],
            periodic: [true; 3],
            dt_s: 1e-4,
            ..Default::default()
        },
    );
    let n = base
        .grid
        .dims
        .iter()
        .map(|&v| v as usize)
        .product::<usize>();
    let cells = vec![
        FluidCell {
            force: [1000., 0., 0., 0.],
            ..Default::default()
        };
        n
    ];
    let force = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&cells),
            usage: wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE,
        });
    let mut state = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            use_em_force: true,
            ..base.config().clone()
        },
        Some(ForceField {
            buffer: &force,
            grid: &base.grid,
        }),
    )
    .unwrap();
    let mut shared = FluidState::new_coupled(
        gpu.context.clone(),
        &volume,
        state.config().clone(),
        Some(ForceField {
            buffer: &force,
            grid: &base.grid,
        }),
    )
    .unwrap();
    assert_eq!(state.bytes - shared.bytes, n as u64 * 64);
    gpu.context.queue.write_buffer(
        &force,
        0,
        bytemuck::cast_slice(&vec![FluidCell::default(); n]),
    );
    shared
        .update_force(ForceField {
            buffer: &force,
            grid: &base.grid,
        })
        .unwrap();
    shared.step(&volume).unwrap();
    let live = shared.probe([2., 4., 2.]).unwrap().unwrap();
    assert_eq!(live.force[0], 0.);
    assert_eq!(live.velocity[0], 0.);

    state.step(&volume).unwrap();
    let p = state.snapshot().probe([2., 4., 2.]).unwrap().unwrap();
    assert!((p.velocity[0] - 1e-4).abs() < 1e-8, "{:?}", p.velocity);
    assert_eq!(p.force[0], 1000.);
}

#[test]
fn viscous_shear_matches_discrete_decay_and_enforces_cfl() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut state) = channel(
        16,
        FluidConfig {
            dt_s: 3e-5,
            acceleration_m_s2: [0.; 3],
            periodic: [true; 3],
            ..Default::default()
        },
    );
    let d = state.params.faces;
    let nodes = (0..d[3])
        .map(|i| {
            let y = (i / d[0]) % d[1];
            let value = (std::f32::consts::TAU * (y as f32 + 0.5) / 16.).sin();
            [value, 0., 0., 0.]
        })
        .collect::<Vec<_>>();
    gpu.context
        .queue
        .write_buffer(&state.velocity, 0, bytemuck::cast_slice(&nodes));
    state.dispatch(&[
        ("fields", groups(state.params.dims[3])),
        ("diagnostics", [1, 1, 1]),
    ]);
    state.read_diagnostics(0, 0., 0.).unwrap();
    let initial_energy = state.diagnostics.kinetic_energy_j;
    for _ in 0..20 {
        state.step(&volume).unwrap();
    }
    let factor = (1.
        - 4. * state.params.physical[2] * state.params.physical[1] / state.grid.cell_m.powi(2)
            * (std::f32::consts::PI / 16.).sin().powi(2))
    .powi(20);
    let y = 5;
    let expected = (std::f32::consts::TAU * (y as f32 + 0.5) / 16.).sin() * factor;
    let point = [0.75, (y as f32 + 0.5) * 0.5, 0.75];
    let value = state.probe(point).unwrap().unwrap().velocity[0];
    assert!((value - expected).abs() < 2e-5, "{value} vs {expected}");
    assert!((state.diagnostics.kinetic_energy_j / initial_energy - factor * factor).abs() < 2e-5);
    let (_, volume, mut fast) = channel(
        16,
        FluidConfig {
            dt_s: 3e-5,
            acceleration_m_s2: [0.; 3],
            initial_velocity_m_s: [20., 0., 0.],
            periodic: [true; 3],
            ..Default::default()
        },
    );
    assert!(fast.step(&volume).unwrap_err().contains("CFL"));
    assert_eq!(fast.time_s, 0.);
}

#[test]
fn thermal_phase_topology_and_weighted_projection_preserve_hydrostatic_rest() {
    let _lock = crate::gpu_test_lock();
    let (gpu, v, mut s) = channel(
        8,
        FluidConfig {
            dt_s: 1e-4,
            acceleration_m_s2: [0., -9.81, 0.],
            periodic: [true, false, true],
            ..Default::default()
        },
    );
    let count = s.grid.dims.iter().map(|n| *n as usize).product::<usize>();
    let mut data = vec![crate::thermal::ThermalCell::default(); count];
    for (i, c) in data.iter_mut().enumerate() {
        let y = (i / s.grid.dims[0] as usize) % s.grid.dims[1] as usize;
        c.state[1] = if y < 4 {
            0.
        } else {
            0.25 + 0.75 * (y - 4) as f32 / 3.
        };
    }
    let buffer = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Known thermal phase stratification"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
    s.update_phase(&v, &buffer).unwrap();
    assert_eq!(s.fluid_cells, count as u64 / 2);
    for _ in 0..5 {
        s.step(&v).unwrap();
    }
    assert!(s.diagnostics.max_speed_m_s < 5e-6, "{:?}", s.diagnostics);
    assert!(s.diagnostics.rms_divergence_s_inv < 5e-5);
    let expected_mass: f32 =
        data.iter().map(|c| c.state[1]).sum::<f32>() * 1000. * s.grid.cell_m.powi(3);
    assert!((s.diagnostics.fluid_mass_kg / expected_mass - 1.).abs() < 1e-5);
    data.fill(crate::thermal::ThermalCell::default());
    gpu.context
        .queue
        .write_buffer(&buffer, 0, bytemuck::cast_slice(&data));
    s.update_phase(&v, &buffer).unwrap();
    s.step(&v).unwrap();
    assert_eq!(s.fluid_cells, 0);
    assert_eq!(s.diagnostics.fluid_mass_kg, 0.);
    assert_eq!(s.diagnostics.max_speed_m_s, 0.);
}

#[test]
fn interface_coefficients_reference_liquid_and_ambient_materials() {
    let _lock = crate::gpu_test_lock();
    let (gpu, mut volume, _) = channel(
        8,
        FluidConfig {
            acceleration_m_s2: [0.; 3],
            ..Default::default()
        },
    );
    let mut snapshot = volume.inputs.snapshot().clone();
    snapshot.materials[0].density_kg_m3 = Some(2.7);
    snapshot.materials[3].surface_tension_n_m = Some(0.23);
    volume.inputs =
        crate::inputs::GpuInputs::new(&gpu.context, snapshot, volume.build_serial).unwrap();
    let config = FluidConfig {
        surface: crate::surface::SurfaceConfig { enabled: true },
        acceleration_m_s2: [0.; 3],
        ..Default::default()
    };
    let state = FluidState::new(gpu.context.clone(), &volume, config, None).unwrap();
    assert_eq!(state.params.surface[1], 2.7);
    assert_eq!(state.params.surface[2], 0.23);
}

#[test]
fn physical_pressure_check_accepts_solution_without_discarding_cg_direction() {
    let _lock = crate::gpu_test_lock();
    let (_, _, state) = channel(
        8,
        FluidConfig {
            acceleration_m_s2: [0.; 3],
            ..Default::default()
        },
    );
    let dims = state.grid.dims;
    let faces = dims.map(|n| n + 1);
    let h = state.grid.cell_m;
    let index = |q: [u32; 3], d: [u32; 3]| (q[0] + d[0] * (q[1] + d[1] * q[2])) as usize;
    let phi: Vec<f32> = (0..state.params.dims[3])
        .map(|i| ((i * 17) % 31) as f32 * 1e-7)
        .collect();
    let work: Vec<_> = phi
        .iter()
        .enumerate()
        .map(|(i, p)| [*p, 0., i as f32 * 0.001, 0.])
        .collect();
    let mut predicted = vec![[0f32; 4]; state.params.faces[3] as usize];
    for z in 0..faces[2] {
        for y in 0..faces[1] {
            for x in 0..faces[0] {
                let q = [x, y, z];
                for axis in 0..3 {
                    if q[axis] > 0 && (0..3).all(|a| q[a] < dims[a]) {
                        let mut left = q;
                        left[axis] -= 1;
                        predicted[index(q, faces)][axis] =
                            (phi[index(q, dims)] - phi[index(left, dims)]) / h;
                    }
                }
            }
        }
    }
    state
        .context
        .queue
        .write_buffer(&state._work, 0, bytemuck::cast_slice(&work));
    state
        .context
        .queue
        .write_buffer(&state.scratch, 0, bytemuck::cast_slice(&predicted));
    state.dispatch(&[
        ("pressure_refresh", groups(state.params.dims[3])),
        ("initial_reduce", [1, 1, 1]),
    ]);
    // Model the over-strict recursive stop flag while preserving an exact manufactured solution.
    state
        .context
        .queue
        .write_buffer(&state.scalars, 24, bytemuck::bytes_of(&0f32));
    state
        .context
        .queue
        .write_buffer(&state._work, 0, bytemuck::cast_slice(&work));
    let before = read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
    state.dispatch(&[
        ("pressure_check", groups(state.params.dims[3])),
        ("check_reduce", [1, 1, 1]),
    ]);
    assert!(state.convergence().unwrap().1);
    assert_eq!(
        before,
        read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap()
    );
    state.dispatch(&[
        ("correct", groups(state.params.faces[3])),
        ("fields", groups(state.params.dims[3])),
        ("diagnostics", [1, 1, 1]),
    ]);
    let bytes = read_bytes(&state.context, &state.result, 0, state.result.size()).unwrap();
    assert!(
        bytemuck::cast_slice::<u8, FluidCell>(&bytes)
            .iter()
            .all(|c| c.velocity[3] < 1e-7)
    );
}
