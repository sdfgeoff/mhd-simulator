use super::*;
use crate::{
    domain::GpuDomain,
    simulation::{DomainSettings, default_materials},
};
fn setup(n: u32) -> (GpuDomain, Volume, Model) {
    let gpu = GpuDomain::new().unwrap();
    let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
    model.bodies[0].simulation.material_id = 6;
    let mut materials = default_materials();
    materials[6].relative_permeability = Some(1.);
    let settings = DomainSettings {
        min_mm: [-8.; 3],
        max_mm: [8.; 3],
        cell_mm: 16. / n as f32,
    };
    let volume = gpu.build(&model, &settings, &materials).unwrap();
    (gpu, volume, model)
}
#[test]
fn uniform_field_and_zero_dc_induction() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model) = setup(16);
    let mut config = EmConfig::default();
    config.uniform.push(UniformField {
        tesla: [0.003, -0.004, 0.005],
        scale: Waveform::Constant { value: 1. },
    });
    let mut state = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config,
        Mode::Induction,
    )
    .unwrap();
    let p = state.probe([0.; 3]).unwrap().unwrap();
    for (i, expected) in [0.003, -0.004, 0.005].iter().enumerate() {
        assert!((p.b[i] - expected).abs() < 1e-7, "{:?}", p.b);
    }
    state.step(&model, &volume).unwrap();
    assert!(
        state.diagnostics.maxima[1] < 1e-3,
        "{:?}",
        state.diagnostics
    );
    assert!(state.diagnostics.divergence_b_scaled < 1e-7);
}
#[test]
fn loop_field_matches_analytic_and_pulse_induces_conserved_currents() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut model) = setup(24);
    let radius = 5.;
    let n = 128;
    let points = (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [radius * a.cos(), radius * a.sin(), 0.]
        })
        .collect();
    model.curves.push(crate::model::CurvePath {
        name: "Loop".into(),
        points_mm: points,
    });
    let wire = Wire {
        cad_curve: 0,
        reverse: false,
        name: "Loop".into(),
        core_radius_mm: 0.01,
        current: Waveform::Constant { value: 100. },
    };
    let mut cfg = EmConfig::default();
    cfg.wires.push(wire);
    let applied = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        cfg.clone(),
        Mode::Applied,
    )
    .unwrap();
    let p = applied.probe([0.; 3]).unwrap().unwrap();
    let expect = 4e-7 * std::f32::consts::PI * 100. / (2. * radius * 1e-3);
    assert!(
        (p.b[2] / expect - 1.).abs() < 0.04,
        "B={} expected={expect}",
        p.b[2]
    );
    let mut rotated_model = model.clone();
    let axis = glam::Vec3::new(1., 2., 3.).normalize();
    let rotation = glam::Quat::from_rotation_arc(glam::Vec3::Z, axis);
    for point in &mut rotated_model.curves[0].points_mm {
        *point = (rotation * glam::Vec3::from(*point)).to_array();
    }
    let rotated_state = EmState::new(
        gpu.context.clone(),
        &volume,
        &rotated_model,
        cfg.clone(),
        Mode::Applied,
    )
    .unwrap();
    let b = rotated_state.probe([0.; 3]).unwrap().unwrap().b;
    let b = glam::Vec3::new(b[0], b[1], b[2]);
    assert!(
        (b - axis * expect).length() / expect < 0.06,
        "rotated loop {b:?}"
    );
    cfg.wires[0].current = Waveform::Pulse {
        peak: 100.,
        delay_s: 0.,
        rise_s: 50e-6,
        hold_s: 50e-6,
        fall_s: 50e-6,
    };
    cfg.relative_tolerance = 1e-5;
    cfg.max_iterations = 2000;
    let mut state =
        EmState::new(gpu.context.clone(), &volume, &model, cfg, Mode::Induction).unwrap();
    state.step(&model, &volume).unwrap();
    eprintln!("pulse {:?}", state.diagnostics);
    assert!(state.diagnostics.maxima[1] > 1.);
    assert!(state.diagnostics.power_w > 0.);
    assert!(
        (state.bodies.iter().map(|b| b.power_w).sum::<f32>() / state.diagnostics.power_w - 1.)
            .abs()
            < 1e-4
    );
    for axis in 0..3 {
        assert!(
            (state.bodies.iter().map(|b| b.force_n[axis]).sum::<f32>()
                - state.diagnostics.force_n[axis])
                .abs()
                < 1e-8
        );
    }
    assert!(state.diagnostics.reaction_max_t > 0.);
    assert!(
        state.diagnostics.divergence_j_scaled / state.diagnostics.max_edge_j < 1e-3,
        "{:?}",
        state.diagnostics
    );
    assert!(state.diagnostics.divergence_b_scaled / state.diagnostics.maxima[0] < 1e-4);
    assert!(
        state
            .all_fields()
            .iter()
            .all(|p| p.b.iter().all(|v| v.is_finite()))
    );
}

pub(super) fn sphere(radius: f32, longitude: usize, latitude: usize) -> Vec<[glam::Vec3; 3]> {
    let p = |j: usize, i: usize| {
        let theta = j as f32 / latitude as f32 * std::f32::consts::PI;
        let phi = (i % longitude) as f32 / longitude as f32 * std::f32::consts::TAU;
        glam::Vec3::new(
            theta.sin() * phi.cos(),
            theta.sin() * phi.sin(),
            theta.cos(),
        ) * radius
    };
    let mut t = Vec::new();
    for i in 0..longitude {
        t.push([glam::Vec3::Z * radius, p(1, i), p(1, i + 1)]);
        for j in 1..latitude - 1 {
            t.push([p(j, i), p(j + 1, i), p(j + 1, i + 1)]);
            t.push([p(j, i), p(j + 1, i + 1), p(j, i + 1)]);
        }
        t.push([
            -glam::Vec3::Z * radius,
            p(latitude - 1, i + 1),
            p(latitude - 1, i),
        ]);
    }
    t
}
fn box_triangles(h: f32) -> Vec<[glam::Vec3; 3]> {
    let p: Vec<_> = (0..8)
        .map(|i| {
            glam::Vec3::new(
                if i & 1 == 0 { -h } else { h },
                if i & 2 == 0 { -h } else { h },
                if i & 4 == 0 { -h } else { h },
            )
        })
        .collect();
    [
        [0, 4, 6, 2],
        [1, 3, 7, 5],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 2, 3, 1],
        [4, 5, 7, 6],
    ]
    .iter()
    .flat_map(|q| [[p[q[0]], p[q[1]], p[q[2]]], [p[q[0]], p[q[2]], p[q[3]]]])
    .collect()
}
#[test]
fn magnetized_cad_prism_matches_analytic_axis_field() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let mut model = Model::default();
    model
        .bodies
        .push(crate::model::Body::new("Magnet prism".into(), box_triangles(1.)).unwrap());
    let settings = DomainSettings {
        min_mm: [-5.15625; 3],
        max_mm: [4.84375; 3],
        cell_mm: 0.3125,
    };
    let volume = gpu.build(&model, &settings, &default_materials()).unwrap();
    let cfg = EmConfig {
        magnets: vec![Magnet {
            body: 0,
            magnetization_a_m: [0., 0., 1e6],
            subdivisions: 2,
            smoothing_mm: 0.01,
        }],
        ..Default::default()
    };
    let state = EmState::new(gpu.context.clone(), &volume, &model, cfg, Mode::Applied).unwrap();
    let sample = state.probe([0., 0., 3.125]).unwrap().unwrap();
    let a = 0.001f64;
    let z = 0.003125;
    let omega = |d: f64| (a * a / (d * (2. * a * a + d * d).sqrt())).atan();
    let expected =
        4e-7 * std::f64::consts::PI * 1e6 / std::f64::consts::PI * (omega(z - a) - omega(z + a));
    assert!(
        (sample.b[2] as f64 / expected - 1.).abs() < 0.025,
        "{} != {expected}",
        sample.b[2]
    );
    assert!(sample.b[0].abs() < expected as f32 * 0.01);
    assert!(sample.b[1].abs() < expected as f32 * 0.01);
}
#[test]
fn permeability_contrast_matches_sphere_polarization() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let mut model = Model::default();
    model.bodies.push(
        crate::model::Body::new("Linear magnetic sphere".into(), sphere(3., 48, 24)).unwrap(),
    );
    model.bodies[0].simulation.material_id = 6;
    let mut materials = default_materials();
    materials[6].relative_permeability = Some(2.);
    materials[6].conductivity_s_m = Some(0.);
    let settings = DomainSettings {
        min_mm: [-10.; 3],
        max_mm: [10.; 3],
        cell_mm: 0.5,
    };
    let volume = gpu.build(&model, &settings, &materials).unwrap();
    let cfg = EmConfig {
        uniform: vec![UniformField {
            tesla: [0., 0., 0.001],
            scale: Waveform::Constant { value: 1. },
        }],
        max_iterations: 3000,
        ..Default::default()
    };
    let state = EmState::new(gpu.context.clone(), &volume, &model, cfg, Mode::Induction).unwrap();
    let sample = state.probe([0.; 3]).unwrap().unwrap();
    eprintln!("polarized sphere B {:?}, {:?}", sample.b, state.diagnostics);
    let expected = 0.001 * 3. * 2. / (2. + 2.);
    assert!((sample.b[2] / expected - 1.).abs() < 0.07);
    assert_eq!(state.diagnostics.maxima[1], 0.);
}
#[test]
fn implicit_diffusion_matches_discrete_eigenmode_and_time_refines() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model) = setup(16);
    let mut cfg = EmConfig::default();
    cfg.max_iterations = 2000;
    let mut state =
        EmState::new(gpu.context.clone(), &volume, &model, cfg, Mode::Induction).unwrap();
    // Manufactured homogeneous conductor/electrode boundary case isolates the
    // discrete diffusion operator. Device simulations reject conductors at the box.
    gpu.context.queue.write_buffer(
        volume.material_ids(),
        0,
        bytemuck::cast_slice(&vec![6u32; state.params.dims[3] as usize]),
    );
    state.dispatch(&[("coefficients", state.edge_groups())]);
    let [nx, ny, nz, _] = state.params.dims;
    let mut initial = vec![[0f32; 4]; state.params.edges[3] as usize];
    for z in 0..=nz {
        for y in 0..=ny {
            for x in 0..nx {
                let i = x + nx * (y + (ny + 1) * z);
                initial[i as usize][0] = 1e-5
                    * (std::f32::consts::PI * y as f32 / ny as f32).sin()
                    * (std::f32::consts::PI * z as f32 / nz as f32).sin();
            }
        }
    }
    let lambda = 4. * (std::f64::consts::PI / (2. * ny as f64)).sin().powi(2)
        + 4. * (std::f64::consts::PI / (2. * nz as f64)).sin().powi(2);
    let mu_sigma = 4e-7
        * std::f64::consts::PI
        * volume.inputs.snapshot().materials[6]
            .conductivity_s_m
            .unwrap() as f64;
    let h = state.grid.cell_m as f64;
    let total = 2e-5;
    let continuous = (-lambda / (mu_sigma * h * h) * total).exp();
    let mut previous_error = f64::INFINITY;
    for steps in [1, 2, 4] {
        state
            .context
            .queue
            .write_buffer(&state._work, 0, bytemuck::cast_slice(&initial));
        state.time_s = 0.;
        state.config.dt_s = total / steps as f64;
        for _ in 0..steps {
            state.step(&model, &volume).unwrap();
        }
        let raw = read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
        let values = bytemuck::cast_slice::<u8, [f32; 4]>(&raw);
        let i = (nx / 2 + nx * ((ny / 2) + (ny + 1) * (nz / 2))) as usize;
        let ratio = values[i][0] as f64 / initial[i][0] as f64;
        let expected = (1. / (1. + lambda * state.config.dt_s / (mu_sigma * h * h))).powi(steps);
        assert!((ratio - expected).abs() < 2e-5, "{ratio} != BE {expected}");
        let error = (ratio - continuous).abs();
        assert!(error < previous_error);
        previous_error = error;
    }
}
#[test]
fn failed_step_rolls_back_and_input_changes_are_rejected() {
    let _lock = crate::gpu_test_lock();
    let (gpu, mut volume, model) = setup(16);
    let cfg = EmConfig {
        uniform: vec![UniformField {
            tesla: [0., 0., 0.01],
            scale: Waveform::Pulse {
                peak: 1.,
                delay_s: 0.,
                rise_s: 50e-6,
                hold_s: 50e-6,
                fall_s: 50e-6,
            },
        }],
        ..Default::default()
    };
    let mut state =
        EmState::new(gpu.context.clone(), &volume, &model, cfg, Mode::Induction).unwrap();
    let before = state.all_fields();
    state.config.max_iterations = 1;
    assert!(
        state
            .step(&model, &volume)
            .unwrap_err()
            .contains("did not converge")
    );
    assert_eq!(state.time_s, 0.);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&before),
        bytemuck::cast_slice::<_, u8>(&state.all_fields())
    );
    state.config.max_iterations = 2000;
    state.step(&model, &volume).unwrap();
    assert!(state.diagnostics.maxima[1] > 0.);
    let mut materials = volume.inputs.snapshot().materials.clone();
    materials[6].conductivity_s_m = Some(1e6);
    let snapshot =
        crate::simulation::SimulationSnapshot::from_model(&model, &volume.settings, &materials)
            .unwrap();
    gpu.update_inputs(&mut volume, snapshot).unwrap();
    assert!(
        state
            .step(&model, &volume)
            .unwrap_err()
            .contains("inputs changed")
    );
    assert!(state.probe([100.; 3]).unwrap().is_none());
    let cfg = EmConfig {
        uniform: vec![UniformField {
            tesla: [0., 0., 0.001],
            scale: Waveform::Constant { value: 1. },
        }],
        ..Default::default()
    };
    let mut applied =
        EmState::new(gpu.context.clone(), &volume, &model, cfg, Mode::Applied).unwrap();
    let checkpoint = applied.all_fields();
    applied.config.uniform[0].scale = Waveform::Constant { value: 1e30 };
    assert!(
        applied
            .evaluate_applied(&model, 1e-5, &volume)
            .unwrap_err()
            .contains("Nonfinite EM field")
    );
    assert_eq!(applied.time_s, 0.);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&checkpoint),
        bytemuck::cast_slice::<_, u8>(&applied.all_fields())
    );
}
#[test]
fn drive_waveforms_interpolate_and_validate() {
    let pulse = Waveform::Pulse {
        peak: 100.,
        delay_s: 1.,
        rise_s: 2.,
        hold_s: 3.,
        fall_s: 4.,
    };
    assert_eq!(pulse.value(0.), 0.);
    assert_eq!(pulse.value(2.), 50.);
    assert_eq!(pulse.value(4.), 100.);
    assert_eq!(pulse.value(8.), 50.);
    assert_eq!(pulse.value(10.), 0.);
    let table = Waveform::Table {
        samples: vec![[0., 0.], [2., 100.], [4., 0.]],
    };
    assert_eq!(table.value(1.), 50.);
    assert_eq!(table.value(3.), 50.);
    assert_eq!(table.value(5.), 0.);
    assert!(
        Waveform::Table {
            samples: vec![[1., 0.], [0., 0.]]
        }
        .validate()
        .is_err()
    );
}

#[test]
fn multiple_cad_currents_and_magnets_superpose_with_independent_drives() {
    let _lock = crate::gpu_test_lock();
    let loaded = crate::project::load(std::path::Path::new("assets/hotend-cad.mhd.json")).unwrap();
    let gpu = GpuDomain::new().unwrap();
    let mut settings = loaded.settings.clone();
    settings.cell_mm = 1.2;
    let volume = gpu
        .build(&loaded.model, &settings, &loaded.materials)
        .unwrap();
    let model = loaded.model;
    let mut config = loaded.electromagnetics;
    config.wires[1].reverse = true;
    let points = config.wires[1].points(&model).unwrap();
    assert_eq!(
        points[1],
        model.curves[1].points_mm[model.curves[1].points_mm.len() - 2]
    );

    assert_eq!(model.curves.len(), 2);
    assert_eq!(config.wires.len(), 2);
    assert_eq!(config.magnets.len(), 2);
    assert_eq!(
        config.wires.iter().map(|w| w.cad_curve).collect::<Vec<_>>(),
        vec![0, 1]
    );
    let mut combined = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config.clone(),
        Mode::Applied,
    )
    .unwrap();
    for time in [25e-6, 75e-6] {
        combined.evaluate_applied(&model, time, &volume).unwrap();
        let actual = glam::Vec3::from_slice(&combined.probe([1., 0., 4.]).unwrap().unwrap().b);
        let mut sum = glam::Vec3::ZERO;
        for i in 0..4 {
            let single = EmConfig {
                wires: if i < 2 {
                    vec![config.wires[i].clone()]
                } else {
                    vec![]
                },
                magnets: if i >= 2 {
                    vec![config.magnets[i - 2].clone()]
                } else {
                    vec![]
                },
                ..Default::default()
            };
            let mut state =
                EmState::new(gpu.context.clone(), &volume, &model, single, Mode::Applied).unwrap();
            state.evaluate_applied(&model, time, &volume).unwrap();
            sum += glam::Vec3::from_slice(&state.probe([1., 0., 4.]).unwrap().unwrap().b);
        }
        assert!(
            (actual - sum).length() < sum.length() * 2e-4 + 1e-7,
            "{actual:?} vs {sum:?}"
        );
        let a = config.wires[0].points(&model).unwrap().len() - 1;
        let b = config.wires[1].points(&model).unwrap().len() - 1;
        assert_eq!(
            combined.source_elements[0].c[2],
            config.wires[0].current.value(time)
        );
        assert_eq!(
            combined.source_elements[a + b - 1].c[2],
            config.wires[1].current.value(time)
        );
    }
    let mut changed = model.clone();
    changed.curves[0].points_mm[0][0] += 0.1;
    assert!(
        combined
            .evaluate_applied(&changed, 100e-6, &volume)
            .unwrap_err()
            .contains("centrelines changed")
    );
    let mut invalid = config.clone();
    invalid.wires[0].cad_curve = 99;
    assert!(invalid.validate(&model).is_err());
    let mut induction = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config,
        Mode::Induction,
    )
    .unwrap();
    induction.step(&model, &volume).unwrap();
    assert!(induction.diagnostics.maxima[0] > 0.);
}

#[test]
fn rotating_sphere_matches_analytic_torque_and_motional_energy_balance() {
    // Low-Rm sphere, Ω perpendicular to B: |τ|=2πσΩB²R⁵/15, P=|τ|Ω.
    // See NASA/GE 1974 report, section 4.6.2 (controlled spin), linked in docs/ENERGY.md.
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let radius_mm = 3.;
    let radius = radius_mm * 1e-3f32;
    let omega = 10f32;
    let b = 0.05f32;
    let sigma = 1e6f32;
    let expected_power =
        2. * std::f32::consts::PI / 15. * sigma * omega.powi(2) * b.powi(2) * radius.powi(5);
    let mut errors = Vec::new();
    for n in [4, 8] {
        let mut model = Model::default();
        let mut body = crate::model::Body::new(
            "Rotating conducting sphere".into(),
            sphere(radius_mm, 64, 32),
        )
        .unwrap();
        body.simulation.material_id = 2;
        model.bodies.push(body);
        let mut materials = default_materials();
        materials[2].conductivity_s_m = Some(sigma);
        let volume = gpu
            .build(
                &model,
                &DomainSettings {
                    min_mm: [-6.; 3],
                    max_mm: [6.; 3],
                    cell_mm: radius_mm / n as f32,
                },
                &materials,
            )
            .unwrap();
        let mut cfg = EmConfig {
            dt_s: 5e-6 * (4. / n as f64).powi(2),
            relative_tolerance: 1e-5,
            max_iterations: 3000,
            ..Default::default()
        };
        cfg.uniform.push(UniformField {
            tesla: [0., 0., b],
            scale: Waveform::Constant { value: 1. },
        });
        let mut state =
            EmState::new(gpu.context.clone(), &volume, &model, cfg, Mode::Induction).unwrap();
        let grid = state.grid.clone();
        let count = grid.dims.iter().map(|v| *v as usize).product::<usize>();
        let mut velocities = vec![crate::fluid::FluidCell::default(); count];
        for (i, v) in velocities.iter_mut().enumerate() {
            let p = glam::Vec3::from(grid.origin_m)
                + glam::Vec3::new(
                    (i % grid.dims[0] as usize) as f32 + 0.5,
                    ((i / grid.dims[0] as usize) % grid.dims[1] as usize) as f32 + 0.5,
                    (i / (grid.dims[0] * grid.dims[1]) as usize) as f32 + 0.5,
                ) * grid.cell_m;
            v.velocity = [0., -omega * p.z, omega * p.y, 0.];
            v.reserved[0] = 1.;
        }
        let velocity = gpu
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Analytic rigid rotation"),
                contents: bytemuck::cast_slice(&velocities),
                usage: wgpu::BufferUsages::STORAGE,
            });
        eprintln!("Starting sphere R/h={n}");
        for step in 0..20 * (n / 4) * (n / 4) {
            state.set_velocity(&volume, &velocity, &grid).unwrap();
            state
                .step(&model, &volume)
                .unwrap_or_else(|e| panic!("R/h={n} step={step}: {e}"));
        }
        let e = state.energy_step().unwrap();
        let power = e.joule_j / state.config.dt_s;
        let error = (power / expected_power as f64 - 1.).abs();
        errors.push(error);
        eprintln!(
            "sphere R/h={n}: P={power:.6e} analytic={expected_power:.6e} error={error:.3e} energy={e:?}"
        );
        assert!(e.mechanical_j < 0. && e.joule_j > 0.);
        assert!((e.mechanical_j / e.joule_j + 1.).abs() < 2e-3);
        assert!(e.balance_error_j.abs() / e.joule_j < 0.002, "{e:?}");
        let fields = state.all_fields();
        let torque: f64 = fields
            .iter()
            .zip(&velocities)
            .map(|(f, v)| {
                let z = -v.velocity[1] / omega;
                let y = v.velocity[2] / omega;
                (y * f.force_heat[2] - z * f.force_heat[1]) as f64 * grid.cell_m.powi(3) as f64
            })
            .sum();
        assert!(
            (torque * omega as f64 / power + 1.).abs() < 3e-3,
            "torque={torque} P={power}"
        );
    }
    assert!(errors[1] < 0.025 && errors[1] < errors[0], "{errors:?}");
}

#[test]
fn driven_induction_accounts_external_work_heat_and_backward_euler_loss() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model) = setup(20);
    let mut config = EmConfig::default();
    config.uniform.push(UniformField {
        tesla: [0., 0., 0.01],
        scale: Waveform::Sine {
            amplitude: 1.,
            frequency_hz: 1000.,
            phase_rad: 0.,
        },
    });
    let mut s = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config,
        Mode::Induction,
    )
    .unwrap();
    for _ in 0..5 {
        s.step(&model, &volume).unwrap();
        let e = s.energy_step().unwrap();
        eprintln!("driven EM energy: {e:?}");
        let scale = e.external_j.abs() + e.delta_magnetic_j.abs() + e.joule_j + e.backward_euler_j;
        assert!(e.joule_j > 0. && e.external_j > 0. && e.backward_euler_j > 0.);
        assert!(e.balance_error_j.abs() / scale < 1e-4, "{e:?}");
        assert!((e.joule_j / (s.diagnostics.power_w as f64 * s.config.dt_s) - 1.).abs() < 1e-5);
    }
}

#[test]
fn air_stencil_matches_full_curl_gauge_operator_at_materials_and_boundaries() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
    model.bodies[0].simulation.material_id = 6;
    let settings = DomainSettings {
        min_mm: [-8.; 3],
        max_mm: [8.; 3],
        cell_mm: 1.,
    };
    for (sigma, mu) in [(0., 1.), (1.33e6, 1.), (0., 2.)] {
        let mut materials = default_materials();
        materials[6].conductivity_s_m = Some(sigma);
        materials[6].relative_permeability = Some(mu);
        let volume = gpu.build(&model, &settings, &materials).unwrap();
        let state = EmState::new(
            gpu.context.clone(),
            &volume,
            &model,
            EmConfig::default(),
            Mode::Induction,
        )
        .unwrap();
        let n = state.params.edges[3] as usize;
        let work: Vec<[f32; 4]> = (0..n)
            .map(|i| {
                let v = ((i as f32 * 1.2345).sin() + (i as f32 * 0.345).cos()) * 0.01;
                [0., 0., v, 0.]
            })
            .collect();
        state
            .context
            .queue
            .write_buffer(&state._work, 0, bytemuck::cast_slice(&work));
        state
            .context
            .queue
            .write_buffer(&state.scalars, 0, &[0; 32]);
        state.dispatch(&[("apply_operator", state.edge_groups())]);
        let fast = read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
        let data = read_bytes(&state.context, &state._source, 0, state._source.size()).unwrap();
        let mut coefficients = bytemuck::cast_slice::<u8, [f32; 4]>(&data).to_vec();
        let eligible = coefficients[n..].iter().filter(|v| v[3] > 0.5).count();
        assert!(
            eligible > n / 4,
            "Expected substantial air-stencil coverage"
        );
        for c in &mut coefficients[n..] {
            c[3] = 0.;
        }
        state
            .context
            .queue
            .write_buffer(&state._source, 0, bytemuck::cast_slice(&coefficients));
        state.dispatch(&[("apply_operator", state.edge_groups())]);
        let exact = read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
        let a = bytemuck::cast_slice::<u8, [f32; 4]>(&fast);
        let b = bytemuck::cast_slice::<u8, [f32; 4]>(&exact);
        let error = a
            .iter()
            .zip(b)
            .map(|(a, b)| (a[3] as f64 - b[3] as f64).powi(2))
            .sum::<f64>();
        let norm = b.iter().map(|v| (v[3] as f64).powi(2)).sum::<f64>();
        assert!(
            (error / norm).sqrt() < 2e-7,
            "sigma={sigma} mu={mu} relative operator error {}",
            (error / norm).sqrt()
        );
    }
}

#[test]
#[ignore = "GPU performance microbenchmark; select a project with MHD_BENCH_PROJECT"]
fn benchmark_em_kernels() {
    let _lock = crate::gpu_test_lock();
    let path =
        std::env::var("MHD_BENCH_PROJECT").unwrap_or_else(|_| "assets/hotend-cad.mhd.json".into());
    let p = crate::project::load(std::path::Path::new(&path)).unwrap();
    let gpu = GpuDomain::new().unwrap();
    let volume = gpu.build(&p.model, &p.settings, &p.materials).unwrap();
    let state = EmState::new(
        gpu.context.clone(),
        &volume,
        &p.model,
        p.electromagnetics,
        Mode::Induction,
    )
    .unwrap();
    if !state
        .context
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        eprintln!("GPU timestamps unavailable; kernel throughput measurement skipped");
        return;
    }
    let n = state.params.edges[3] as usize;
    let work = vec![[0f32, 1e-6, 1e-6, 1e-6]; n];
    let partials = vec![[1f32; 4]; n.div_ceil(256)];
    for (step, entry) in [
        "apply_operator",
        "alpha_reduce",
        "update_residual",
        "beta_reduce",
        "update_search",
    ]
    .into_iter()
    .enumerate()
    {
        state
            .context
            .queue
            .write_buffer(&state._work, 0, bytemuck::cast_slice(&work));
        state
            .context
            .queue
            .write_buffer(&state._partials, 0, bytemuck::cast_slice(&partials));
        state.context.queue.write_buffer(
            &state.scalars,
            0,
            bytemuck::cast_slice(&[[1f32, 1., 1., 0.], [0.01, 0.5, 0., 0.]]),
        );
        state.context.device.poll(wgpu::Maintain::Wait);
        state.context.profiler.begin_step(step as u64, 0.);
        {
            let _stage = state.context.profiler.stage(crate::profile::Stage::EmSolve);
            let count = if entry.ends_with("reduce") {
                [1, 1, 1]
            } else {
                state.edge_groups()
            };
            state.dispatch(&vec![(entry, count); 50]);
        }
        state.context.profiler.end_step(true);
        let status = state.read_f32(&state.scalars, 0, 8).unwrap();
        assert_eq!(
            status[7], 0.,
            "Kernel fixture must keep GPU iterations active"
        );
        for report in state.context.profiler.take_completed(true) {
            if let Some(gpu) = report.gpu {
                eprintln!(
                    "{entry}: {:.4} ms per dispatch",
                    gpu.stages_ms.values().sum::<f64>() / 50.
                );
            }
        }
    }
}
