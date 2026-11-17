//! Exact periodic translating shear isolates momentum transport from capillarity.
use super::*;
use crate::{
    fluid::{FluidConfig, FluidState},
    simulation::{DomainSettings, default_materials},
};
use std::f64::consts::TAU;
#[test]
#[ignore = "Momentum-advection refinement diagnostic; writes artifacts/momentum-shear.csv"]
fn translating_shear_momentum_refinement() {
    let _lock = crate::gpu_test_lock();
    let grids: Vec<u32> = std::env::var("MHD_MOMENTUM_GRIDS")
        .unwrap_or("16,32,64".into())
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    assert!(grids.iter().all(|n| *n >= 8));
    assert!(grids.windows(2).all(|g| g[1] > g[0]));
    let mut csv = String::from(
        "grid,density,translation,steps,relative_velocity_l2,amplitude_ratio,phase_error_rad,relative_momentum_drift,shear_energy_ratio\n",
    );
    for density in [1., 7000.] {
        for translation in [-0.2, 0.2] {
            let mut errors = Vec::new();
            for &n in &grids {
                let report = shear(n, density, translation, false);
                errors.push(report.error);
                csv.push_str(&format!(
                    "{n},{density},{translation},{},{},{},{},{},{}\n",
                    report.steps,
                    report.error,
                    report.amplitude,
                    report.phase,
                    report.momentum,
                    report.energy
                ));
                eprintln!(
                    "Translating shear n={n} rho={density} U={translation}: L2={:.7e}, amplitude={:.8}, phase={:.6e}, dP={:.3e}",
                    report.error, report.amplitude, report.phase, report.momentum
                );
                assert!(report.momentum < 1e-5);
                assert!(report.amplitude <= 1.00001);
                assert!(report.energy <= 1.00002);
            }
            // Require at least order 1.5 for the requested grid ratios.
            assert!(
                errors
                    .windows(2)
                    .zip(grids.windows(2))
                    .all(|(e, g)| e[1] < e[0] * (g[0] as f64 / g[1] as f64).powf(1.5)),
                "{errors:?}"
            );
        }
    }
    std::fs::create_dir_all("artifacts").unwrap();
    std::fs::write("artifacts/momentum-shear.csv", csv).unwrap();
}
struct ShearReport {
    steps: u32,
    error: f64,
    amplitude: f64,
    phase: f64,
    momentum: f64,
    energy: f64,
    variation: f64,
}
fn shear(n: u32, density: f32, translation: f32, sharp: bool) -> ShearReport {
    let amplitude = 0.02;
    let wave = |phase: f64| {
        amplitude
            * if sharp {
                if phase.sin() >= 0. { 1. } else { -1. }
            } else {
                phase.sin()
            }
    };
    let duration = 1.25;
    let steps = (duration / (0.12 / n as f64 / (translation as f64).abs())).ceil() as u32;
    let dt = duration / steps as f64;
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(density);
    materials[3].dynamic_viscosity_pa_s = Some(0.);
    materials[3].surface_tension_n_m = Some(0.);
    materials[0].density_kg_m3 = Some(1.);
    materials[0].dynamic_viscosity_pa_s = Some(0.);
    let (gpu, volume, _, _) = super::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000., 4000. / n as f32, 4000. / n as f32],
            cell_mm: 1000. / n as f32,
        },
        &materials,
    );
    let (mut surface, _) =
        SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, [true; 3]).unwrap();
    let mut fluid = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            surface: SurfaceConfig { enabled: true },
            periodic: [true; 3],
            acceleration_m_s2: [0.; 3],
            dt_s: dt,
            relative_tolerance: 1e-6,
            absolute_divergence_tolerance: 1e-6,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let dims = volume.dims.map(|x| x + 1);
    let count = dims.iter().product::<u32>();
    let initial: Vec<[f32; 4]> = (0..count)
        .map(|i| {
            let x = (i % dims[0]) % n;
            [
                translation,
                wave(TAU * (x as f64 + 0.5) / n as f64) as f32,
                0.,
                0.,
            ]
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(fluid.face_buffer(), 0, bytemuck::cast_slice(&initial));
    fluid
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    for _ in 0..steps {
        fluid.step(&volume).unwrap();
        surface
            .advance_coupled(&volume, &mut fluid, None, dt)
            .unwrap();
        fluid
            .update_surface(&volume, surface.field(), None)
            .unwrap();
        if sharp {
            let bytes = read_bytes(
                &gpu.context,
                fluid.face_buffer(),
                0,
                fluid.face_buffer().size(),
            )
            .unwrap();
            for u in bytemuck::cast_slice::<u8, [f32; 4]>(&bytes) {
                assert!(
                    (u[1] as f64).abs() <= amplitude * (1. + 1e-5),
                    "Overshoot {:?}",
                    u
                );
            }
        }
    }
    let bytes = read_bytes(
        &gpu.context,
        fluid.face_buffer(),
        0,
        fluid.face_buffer().size(),
    )
    .unwrap();
    let output = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
    let mut error = 0.;
    let mut norm = 0.;
    let mut sine = 0.;
    let mut cosine = 0.;
    let mut mass_velocity = 0.;
    let mut transverse_momentum = 0.;
    let mut initial_transverse_momentum = 0.;
    let mut energy = 0.;
    let mut samples = 0.;
    let mut expected_sine = 0.;
    let mut expected_cosine = 0.;
    for z in 0..4 {
        for y in 0..4 {
            for x in 0..n {
                let i = (x + dims[0] * (y + dims[1] * z)) as usize;
                let phase = TAU * ((x as f64 + 0.5) / n as f64 - translation as f64 * duration);
                let expected = wave(phase);
                let actual = output[i][1] as f64;
                error += (actual - expected).powi(2);
                norm += expected.powi(2);
                sine += actual * phase.sin();
                expected_sine += expected * phase.sin();
                expected_cosine += expected * phase.cos();
                cosine += actual * phase.cos();
                mass_velocity += output[i][0] as f64;
                transverse_momentum += actual;
                initial_transverse_momentum += initial[i][1] as f64;
                energy += actual * actual;
                samples += 1.;
                assert!(output[i][2].abs() < 1e-6);
                assert!(actual.abs() <= amplitude * (1. + 1e-5));
            }
        }
    }
    let mut variation = 0.;
    let mut initial_variation = 0.;
    for x in 0..n {
        let previous = (x + n - 1) % n;
        variation += (output[x as usize][1] - output[previous as usize][1]).abs() as f64;
        initial_variation += (initial[x as usize][1] - initial[previous as usize][1]).abs() as f64;
    }
    ShearReport {
        steps,
        error: (error / norm).sqrt(),
        amplitude: sine.hypot(cosine) / expected_sine.hypot(expected_cosine),
        phase: cosine.atan2(sine) - expected_cosine.atan2(expected_sine),
        momentum: (mass_velocity / (samples * translation as f64) - 1.).hypot(
            (transverse_momentum - initial_transverse_momentum) / (samples * translation as f64),
        ),
        energy: energy / norm,
        variation: variation / initial_variation,
    }
}

#[test]
fn smooth_momentum_transport_is_low_diffusion_in_both_directions() {
    let _lock = crate::gpu_test_lock();
    for speed in [-0.2, 0.2] {
        let report = shear(32, 7000., speed, false);
        assert!(report.error < 0.015, "L2={}", report.error);
        assert!(report.amplitude > 0.995 && report.amplitude <= 1.00001);
        assert!(report.phase.abs() < 0.004);
        assert!(report.momentum < 1e-6);
        assert!(report.energy <= 1.00002);
    }
}
#[test]
fn sharp_momentum_profile_remains_bounded_at_every_step() {
    let _lock = crate::gpu_test_lock();
    for speed in [-0.2, 0.2] {
        let report = shear(32, 7000., speed, true);
        eprintln!(
            "Sharp shear U={speed}: L2={}, TV={}, energy={}",
            report.error, report.variation, report.energy
        );
        assert!(report.variation <= 1.00002);
        assert!(report.momentum < 1e-6);
        assert!(report.energy <= 1.00002);
    }
}

fn integrals(
    surface: &SurfaceState,
    fluid: &FluidState,
    context: &GpuContext,
    density: f64,
) -> ([f64; 3], f64, f64) {
    let bytes = read_bytes(context, surface.field(), 0, surface.field().size()).unwrap();
    let a = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
    let bytes = read_bytes(context, fluid.face_buffer(), 0, fluid.face_buffer().size()).unwrap();
    let u = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
    let n = fluid.grid.dims;
    let f = n.map(|d| d + 1);
    let cell = |q: [u32; 3]| (q[0] + n[0] * (q[1] + n[1] * q[2])) as usize;
    let face = |q: [u32; 3]| (q[0] + f[0] * (q[1] + f[1] * q[2])) as usize;
    let rho = |q: [u32; 3]| 1. + (density - 1.) * a[cell(q)][0] as f64;
    let dv = (fluid.grid.cell_m as f64).powi(3);
    let mut momentum = [0.; 3];
    let mut energy = 0.;
    let mut volume = 0.;
    for z in 0..n[2] {
        for y in 0..n[1] {
            for x in 0..n[0] {
                let q = [x, y, z];
                volume += a[cell(q)][0] as f64 * dv;
                for j in 0..3 {
                    let mut left = q;
                    left[j] = (left[j] + n[j] - 1) % n[j];
                    let mass = 0.5 * (rho(q) + rho(left)) * dv;
                    let velocity = u[face(q)][j] as f64;
                    momentum[j] += mass * velocity;
                    energy += 0.5 * mass * velocity * velocity;
                }
            }
        }
    }
    (momentum, energy, volume)
}

#[test]
fn density_jump_shear_layer_remains_bounded_and_conservative() {
    let _lock = crate::gpu_test_lock();
    let n = 16u32;
    let density = 7000.;
    let h = 1. / n as f64;
    let steps = (2. / (0.03 * h / 0.11)).ceil() as u32;
    let dt = 2. / steps as f64;
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(density);
    materials[3].dynamic_viscosity_pa_s = Some(0.);
    materials[3].surface_tension_n_m = Some(0.);
    materials[0].density_kg_m3 = Some(1.);
    materials[0].dynamic_viscosity_pa_s = Some(0.);
    let (gpu, volume, _, _) = super::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000., 1000., 4000. / n as f32],
            cell_mm: 1000. / n as f32,
        },
        &materials,
    );
    let (mut surface, _) =
        SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, [true; 3]).unwrap();
    let fractions: Vec<[f32; 4]> = (0..n * n * 4)
        .map(|i| {
            let y = (i / n) % n;
            [f32::from(y >= n / 4 && y < 3 * n / 4), 0., 0., 0.]
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&fractions));
    surface.refresh(&volume).unwrap();
    let mut fluid = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            surface: SurfaceConfig { enabled: true },
            periodic: [true; 3],
            acceleration_m_s2: [0.; 3],
            dt_s: dt,
            relative_tolerance: 1e-6,
            absolute_divergence_tolerance: 1e-6,
            max_iterations: 4000,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let dims = volume.dims.map(|d| d + 1);
    let psi = |x: u32, y: u32| {
        let x = (x % n) as f64 * h;
        let y = (y % n) as f64 * h;
        let gaussian = [0.25, 0.75]
            .into_iter()
            .map(|center| {
                let d = (y - center + 0.5).rem_euclid(1.) - 0.5;
                (-(d / 0.05).powi(2)).exp()
            })
            .sum::<f64>();
        0.001 / TAU * (TAU * x).cos() * gaussian
    };
    let initial: Vec<[f32; 4]> = (0..dims.iter().product::<u32>())
        .map(|i| {
            let x = i % dims[0];
            let y = (i / dims[0]) % dims[1];
            let base = if y % n >= n / 4 && y % n < 3 * n / 4 {
                0.1
            } else {
                -0.1
            };
            [
                (base + (psi(x, y + 1) - psi(x, y)) / h) as f32,
                (-(psi(x + 1, y) - psi(x, y)) / h) as f32,
                0.,
                0.,
            ]
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(fluid.face_buffer(), 0, bytemuck::cast_slice(&initial));
    fluid
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    let initial = integrals(&surface, &fluid, &gpu.context, density as f64);
    let pnorm = initial.0.iter().map(|p| p * p).sum::<f64>().sqrt();
    let mut max_energy = 1f64;
    for step in 1..=steps {
        fluid.step(&volume).unwrap();
        surface
            .advance_coupled(&volume, &mut fluid, None, dt)
            .unwrap();
        fluid
            .update_surface(&volume, surface.field(), None)
            .unwrap();
        assert!(
            fluid.diagnostics.max_speed_m_s < 0.2,
            "Unforced shear velocity grew: {}",
            fluid.diagnostics.max_speed_m_s
        );
        if step % 8 == 0 || step == steps {
            let now = integrals(&surface, &fluid, &gpu.context, density as f64);
            let drift = now
                .0
                .iter()
                .zip(initial.0)
                .map(|(p, p0)| (p - p0).powi(2))
                .sum::<f64>()
                .sqrt()
                / pnorm;
            max_energy = max_energy.max(now.1 / initial.1);
            eprintln!(
                "Density-jump shear {step}/{steps}: K/K0={:.9}, dP={drift:.3e}, volume={:.3e}",
                now.1 / initial.1,
                now.2 / initial.2 - 1.
            );
            assert!(drift < 1e-5);
            assert!(
                max_energy <= 1.0001,
                "Unforced kinetic energy grew: {max_energy}"
            );
            assert!((now.2 / initial.2 - 1.).abs() < 2e-6);
        }
    }
}
