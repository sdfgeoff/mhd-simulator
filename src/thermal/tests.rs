use super::*;
use crate::{
    domain::GpuDomain,
    model::{Body, Model},
    simulation::{DomainSettings, ThermalProperties, default_materials},
};
use glam::Vec3;
fn fixture(
    n: u32,
    length: f32,
    rho: f32,
    cp: f32,
    k: f32,
    phase: Option<(f32, f32, f32)>,
    initial: f32,
    source: f32,
    config: ThermalConfig,
) -> (GpuDomain, Volume, ThermalState) {
    let h = length / n as f32;
    let hi = Vec3::new(length, h, h) * 1000.;
    let v = [
        Vec3::ZERO,
        Vec3::new(hi.x, 0., 0.),
        Vec3::new(hi.x, hi.y, 0.),
        Vec3::new(0., hi.y, 0.),
        Vec3::new(0., 0., hi.z),
        Vec3::new(hi.x, 0., hi.z),
        hi,
        Vec3::new(0., hi.y, hi.z),
    ];
    let f = [
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
    let mut b = Body::new("Thermal slab".into(), f.map(|t| t.map(|i| v[i])).to_vec()).unwrap();
    b.simulation.material_id = 2;
    b.simulation.temperature_c = Some(initial);
    b.simulation.heat_source_w_m3 = source;
    let model = Model {
        bodies: vec![b],
        ..Default::default()
    };
    let mut materials = default_materials();
    let m = &mut materials[2];
    m.density_kg_m3 = Some(rho);
    m.thermal = ThermalProperties {
        heat_capacity_j_kg_k: Some(cp),
        conductivity_w_m_k: Some(k),
        ..Default::default()
    };
    if let Some((s, l, latent)) = phase {
        m.thermal.solidus_c = Some(s);
        m.thermal.liquidus_c = Some(l);
        m.thermal.latent_heat_j_kg = (latent > 0.).then_some(latent);
    }
    let gpu = GpuDomain::new().unwrap();
    let volume = gpu
        .build(
            &model,
            &DomainSettings {
                min_mm: [0.; 3],
                max_mm: hi.to_array(),
                cell_mm: h * 1000.,
            },
            &materials,
        )
        .unwrap();
    let state = ThermalState::new(gpu.context.clone(), &volume, config).unwrap();
    (gpu, volume, state)
}
#[test]
fn insulated_conduction_matches_discrete_decay_and_conserves_energy() {
    let _lock = crate::gpu_test_lock();
    let mut errors = Vec::new();
    for n in [16, 32] {
        let (gpu, v, mut s) = fixture(n, 1., 1., 1., 0.1, None, 20., 0., ThermalConfig::default());
        let mut cells = Vec::new();
        for i in 0..n {
            let t = 20. + 10. * (std::f32::consts::PI * (i as f32 + 0.5) / n as f32).cos();
            cells.push(ThermalCell {
                state: [t + 273.15, 0., t, 1.],
                valid: [1., 0., 0., 0.],
                ..Default::default()
            });
        }
        gpu.context
            .queue
            .write_buffer(s.result_buffer(), 0, bytemuck::cast_slice(&cells));
        s.refresh(&v, None, None, 0).unwrap();
        let initial = s.diagnostics.enthalpy_j;
        for _ in 0..40 {
            s.step(&v, 0.005, None, None).unwrap();
        }
        let data =
            read_bytes(&gpu.context, s.result_buffer(), 0, s.result_buffer().size()).unwrap();
        let output = bytemuck::cast_slice::<u8, ThermalCell>(&data);
        let h = s.grid.cell_m;
        let lambda = 4. * 0.1 / h.powi(2) * (std::f32::consts::PI / (2. * n as f32)).sin().powi(2);
        let sub = s.diagnostics.substeps;
        let decay = (1. - lambda * 0.005 / sub as f32).powi((40 * sub) as i32);
        let mut discrete = 0f64;
        let mut continuous = 0f64;
        for (i, c) in output.iter().enumerate() {
            let cos = (std::f64::consts::PI * (i as f64 + 0.5) / n as f64).cos();
            let measured = c.state[0] as f64 - 273.15;
            discrete += (measured - (20. + 10. * decay as f64 * cos)).powi(2);
            continuous += (measured
                - (20. + 10. * (-0.1 * std::f64::consts::PI.powi(2) * 0.2).exp() * cos))
                .powi(2);
        }
        let error = (continuous / n as f64).sqrt();
        errors.push(error);
        eprintln!(
            "thermal cosine n={n}: PDE L2={error:.5e}, discrete L2={:.5e}",
            (discrete / n as f64).sqrt()
        );
        assert!((discrete / n as f64).sqrt() < 1e-3);
        assert!((s.diagnostics.enthalpy_j - initial).abs() / initial.abs() < 2e-5);
    }
    assert!(errors[1] < errors[0] && errors[1] < 0.002, "{errors:?}");
}
#[test]
fn latent_heat_plateau_and_missing_data_failure_are_conservative() {
    let _lock = crate::gpu_test_lock();
    let (_, v, mut s) = fixture(
        1,
        1.,
        1.,
        1.,
        0.001,
        Some((0., 0., 10.)),
        -1.,
        1.,
        ThermalConfig::default(),
    );
    let p = [500.; 3];
    s.step(&v, 1., None, None).unwrap();
    let a = s.probe(p).unwrap().unwrap();
    assert!((a.state[0] - 273.15).abs() < 1e-4);
    assert!(a.state[1] < 1e-4);
    s.step(&v, 5., None, None).unwrap();
    let a = s.probe(p).unwrap().unwrap();
    assert!((a.state[0] - 273.15).abs() < 1e-4);
    assert!((a.state[1] - 0.5).abs() < 1e-4);
    assert!(s.diagnostics.energy_error_j.abs() < 1e-5);
    s.step(&v, 7., None, None).unwrap();
    let a = s.probe(p).unwrap().unwrap();
    assert!((a.state[0] - 275.15).abs() < 1e-4);
    assert!((a.state[1] - 1.).abs() < 1e-4);
    let (_, v, mut invalid) = fixture(
        1,
        1.,
        1.,
        1.,
        0.001,
        Some((0., 0., 0.)),
        -1.,
        1.,
        ThermalConfig::default(),
    );
    let original = invalid.probe(p).unwrap().unwrap();
    assert!(invalid.step(&v, 2., None, None).is_err());
    assert_eq!(invalid.time_s, 0.);
    assert_eq!(invalid.probe(p).unwrap().unwrap().state, original.state);
}
fn erf(x: f64) -> f64 {
    let mut sum = 0.;
    let mut term = x;
    let mut factorial = 1.;
    for n in 0..40 {
        if n > 0 {
            factorial *= n as f64;
            term *= -x * x;
        }
        sum += term / (factorial * (2 * n + 1) as f64);
    }
    2. / std::f64::consts::PI.sqrt() * sum
}
#[test]
fn stefan_melting_front_refines_and_closes_boundary_heat_balance() {
    let _lock = crate::gpu_test_lock();
    let mut a = 0.;
    let mut b = 1.;
    for _ in 0..60 {
        let x = (a + b) * 0.5;
        if std::f64::consts::PI.sqrt() * x * (x * x).exp() * erf(x) < 1. {
            a = x;
        } else {
            b = x;
        }
    }
    let lambda = (a + b) * 0.5;
    let exact = 2. * lambda * (0.001f64 * 10.).sqrt();
    let mut errors = Vec::new();
    for n in [64, 128] {
        let mut config = ThermalConfig::default();
        config.boundaries[0] = Boundary::Temperature { celsius: 1. };
        let (_, v, mut s) = fixture(n, 0.5, 1000., 1., 1., Some((0., 0., 1.)), 0., 0., config);
        let mut heat = 0.;
        for _ in 0..50 {
            s.step(&v, 0.2, None, None).unwrap();
            heat += s.diagnostics.external_j;
        }
        let front = s.diagnostics.liquid_volume_m3 / (s.grid.cell_m as f64).powi(2);
        let error = (front / exact - 1.).abs();
        errors.push(error);
        eprintln!(
            "Stefan n={n}: front={front:.6e}, exact={exact:.6e}, error={error:.3e}, heat_balance={:.3e}",
            (s.diagnostics.enthalpy_j - heat) / heat
        );
        assert!((s.diagnostics.enthalpy_j - heat).abs() / heat < 5e-4);
    }
    assert!(errors[1] < 0.05 && errors[1] < errors[0], "{errors:?}");
}

#[test]
fn sub_ulp_heating_accumulates_in_compensated_enthalpy() {
    let _lock = crate::gpu_test_lock();
    let (_, v, mut s) = fixture(
        1,
        1.,
        1.,
        1.,
        0.001,
        None,
        20.,
        1e-8,
        ThermalConfig::default(),
    );
    let before = s.probe([500.; 3]).unwrap().unwrap();
    let mut delta = 0.;
    for _ in 0..1000 {
        s.step(&v, 1., None, None).unwrap();
        delta += s.diagnostics.delta_enthalpy_j;
    }
    let after = s.probe([500.; 3]).unwrap().unwrap();
    let measured = (after.state[2] as f64 - after.valid[1] as f64)
        - (before.state[2] as f64 - before.valid[1] as f64);
    assert!((measured / 1e-5 - 1.).abs() < 2e-4, "{measured}");
    assert!((delta / 1e-5 - 1.).abs() < 2e-4, "{delta}");
}

#[test]
fn prescribed_joule_input_heats_without_electrical_sources_or_boundary_work() {
    let _lock = crate::gpu_test_lock();
    let (gpu, v, mut s) = fixture(
        1,
        1.,
        1.,
        1.,
        0.001,
        None,
        20.,
        0.,
        ThermalConfig::default(),
    );
    let mut source = [[0f32; 4]; 4];
    source[2][3] = 2.;
    let heat = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Known Joule heating packet"),
            contents: bytemuck::cast_slice(&source),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let grid = s.grid.clone();
    s.step(
        &v,
        0.25,
        Some(Source {
            buffer: &heat,
            grid: &grid,
        }),
        None,
    )
    .unwrap();
    let c = s.probe([500.; 3]).unwrap().unwrap();
    assert!((c.state[0] - 293.65).abs() < 1e-4);
    assert!((s.diagnostics.joule_j - 0.5).abs() < 1e-6);
    assert_eq!(s.diagnostics.external_j, 0.);
    assert!(s.diagnostics.energy_error_j.abs() < 1e-5);
}

#[test]
fn face_enthalpy_flux_is_upwind_and_conservative() {
    let _lock = crate::gpu_test_lock();
    let (gpu, v, mut s) = fixture(
        2,
        1.,
        1.,
        1.,
        0.001,
        None,
        20.,
        0.,
        ThermalConfig::default(),
    );
    let cells: Vec<_> = [30., 10.]
        .into_iter()
        .map(|t| ThermalCell {
            state: [t + 273.15, 0., t, 1.],
            valid: [1., 0., 0., 0.],
            ..Default::default()
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(s.result_buffer(), 0, bytemuck::cast_slice(&cells));
    s.refresh(&v, None, None, 0).unwrap();
    let mut faces = vec![[0f32; 4]; 3 * 2 * 2];
    faces[1][0] = 0.1;
    let flow = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Known interior enthalpy flux"),
            contents: bytemuck::cast_slice(&faces),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let grid = s.grid.clone();
    s.step(
        &v,
        0.1,
        None,
        Some(Flow {
            buffer: &flow,
            grid: &grid,
            max_face_speed: 0.1,
        }),
    )
    .unwrap();
    for (i, expected) in [29.392, 10.608].into_iter().enumerate() {
        let c = s
            .probe([(i as f32 + 0.5) * 500., 250., 250.])
            .unwrap()
            .unwrap();
        assert!(
            (c.state[0] - 273.15 - expected).abs() < 1e-4,
            "{:?}",
            c.state
        );
    }
    assert!(s.diagnostics.energy_error_j.abs() < 1e-5);
}
