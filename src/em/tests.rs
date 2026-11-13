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
    let (gpu, volume, model) = setup(24);
    let radius = 5.;
    let n = 128;
    let points = (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [radius * a.cos(), radius * a.sin(), 0.]
        })
        .collect();
    let wire = Wire {
        body_current: None,
        name: "Loop".into(),
        points_mm: points,
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
    let mut rotated = cfg.clone();
    let axis = glam::Vec3::new(1., 2., 3.).normalize();
    let rotation = glam::Quat::from_rotation_arc(glam::Vec3::Z, axis);
    for point in &mut rotated.wires[0].points_mm {
        *point = (rotation * glam::Vec3::from(*point)).to_array();
    }
    let rotated_state =
        EmState::new(gpu.context.clone(), &volume, &model, rotated, Mode::Applied).unwrap();
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
