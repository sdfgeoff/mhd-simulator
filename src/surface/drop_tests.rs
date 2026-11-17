//! Full 3D Rayleigh-Lamb l=2 drop-oscillation diagnostic.
use super::*;
use crate::{
    fluid::{FluidConfig, FluidState},
    simulation::{DomainSettings, default_materials},
};
use std::f64::consts::PI;

/// Integrate vertical ellipsoid chords over each cell's x/y cross-section.
fn ellipsoid(n: u32, h: f64, r: f64, epsilon: f64) -> Vec<[f32; 4]> {
    let radii = [
        r / (1. + epsilon).sqrt(),
        r / (1. + epsilon).sqrt(),
        r * (1. + epsilon),
    ];
    let mut values = Vec::new();
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let lo = [x as f64 * h - 0.5, y as f64 * h - 0.5, z as f64 * h - 0.5];
                let center = std::array::from_fn::<_, 3, _>(|a| (lo[a] + 0.5 * h).abs() / radii[a]);
                let half = std::array::from_fn::<_, 3, _>(|a| 0.5 * h / radii[a]);
                let nearest = (0..3)
                    .map(|a| (center[a] - half[a]).max(0.).powi(2))
                    .sum::<f64>();
                let farthest = (0..3).map(|a| (center[a] + half[a]).powi(2)).sum::<f64>();
                let mut alpha = 0.;
                if farthest <= 1. {
                    alpha = 1.;
                } else if nearest < 1. {
                    let samples = 64;
                    for j in 0..samples {
                        for i in 0..samples {
                            let xx = (lo[0] + (i as f64 + 0.5) * h / samples as f64) / radii[0];
                            let yy = (lo[1] + (j as f64 + 0.5) * h / samples as f64) / radii[1];
                            let remaining = 1. - xx * xx - yy * yy;
                            if remaining > 0. {
                                let dz = radii[2] * remaining.sqrt();
                                alpha += (dz.min(lo[2] + h) - (-dz).max(lo[2])).max(0.) / h;
                            }
                        }
                    }
                    alpha /= (samples * samples) as f64;
                }
                values.push([alpha as f32, 0., 0., 0.]);
            }
        }
    }
    values
}

/// Central second moments isolate the quadrupole from centre-of-mass drift.
fn moments(n: u32, h: f64, r: f64, field: &[[f32; 4]]) -> (f64, f64, f64) {
    let mut volume = 0.;
    let mut first = [0.; 3];
    let mut second = [0.; 3];
    for (i, a) in field.iter().enumerate() {
        let i = i as u32;
        let q = [i % n, (i / n) % n, i / (n * n)];
        let dv = a[0] as f64 * h.powi(3);
        volume += dv;
        for axis in 0..3 {
            let x = (q[axis] as f64 + 0.5) * h - 0.5;
            first[axis] += dv * x;
            second[axis] += dv * x * x;
        }
    }
    let central = std::array::from_fn::<_, 3, _>(|a| second[a] - first[a] * first[a] / volume);
    let quadrupole = 5. * (2. * central[2] - central[0] - central[1]) / (6. * r * r * volume);
    let drift = first
        .iter()
        .map(|x| (x / volume).powi(2))
        .sum::<f64>()
        .sqrt();
    (quadrupole, volume, drift)
}

#[test]
#[ignore = "Full 3D drop oscillation refinement; writes artifacts/drop-oscillation-*.csv"]
fn rayleigh_lamb_drop_oscillation() {
    let _lock = crate::gpu_test_lock();
    let grids: Vec<u32> = std::env::var("MHD_DROP_GRIDS")
        .unwrap_or("48,64".into())
        .split(',')
        .map(|x| x.trim().parse().unwrap())
        .collect();
    let periods: f64 = std::env::var("MHD_DROP_PERIODS")
        .unwrap_or("1.5".into())
        .parse()
        .unwrap();
    let factor: f64 = std::env::var("MHD_DROP_DT_FACTOR")
        .unwrap_or("0.4".into())
        .parse()
        .unwrap();
    let epsilon: f64 = std::env::var("MHD_DROP_DEFORMATION")
        .unwrap_or("0.03".into())
        .parse()
        .unwrap();
    let ratio: f32 = std::env::var("MHD_DROP_DENSITY_RATIO")
        .unwrap_or("7000".into())
        .parse()
        .unwrap();
    assert!(periods > 0. && factor > 0. && factor <= 1. && epsilon > 0. && epsilon <= 0.1);
    std::fs::create_dir_all("artifacts").unwrap();
    let mut errors = Vec::new();
    for n in grids {
        let report = simulate_drop(
            n,
            periods,
            factor,
            epsilon,
            ratio,
            true,
            std::env::var_os("MHD_DROP_CAD").is_some(),
        );
        if periods >= 1.3 {
            assert!(
                report.crossings.len() >= 3,
                "Drop did not complete three half-period crossings"
            );
            let measured = 2. * PI / (report.crossings[2] - report.crossings[0]);
            let error = (measured / report.omega - 1.).abs();
            eprintln!("drop n={n}: measured omega={measured:.8}, frequency error={error:.6e}");
            assert!(
                error < 0.03,
                "Drop frequency differs from Rayleigh-Lamb reference by more than 3%"
            );
            errors.push(error);
        }
    }
    if errors.len() > 1 {
        assert!(
            errors.last().unwrap() < &errors[0],
            "Drop frequency did not improve with refinement"
        );
    }
}

struct DropReport {
    omega: f64,
    shape: f64,
    peak: f64,
    crossings: Vec<f64>,
}

#[test]
fn three_dimensional_drop_restores_on_the_rayleigh_lamb_timescale() {
    let _lock = crate::gpu_test_lock();
    let periods = 0.35;
    let report = simulate_drop(48, periods, 0.4, 0.03, 7000., false, false);
    let reference = (2. * PI * periods).cos();
    assert!(
        (report.shape - reference).abs() < 0.1,
        "Drop shape {} differs from reference {reference}",
        report.shape
    );
    assert!(report.peak <= 1.1);
}

fn simulate_drop(
    n: u32,
    periods: f64,
    factor: f64,
    epsilon: f64,
    ratio: f32,
    dump: bool,
    cad: bool,
) -> DropReport {
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(ratio);
    materials[3].dynamic_viscosity_pa_s = Some(0.);
    materials[3].surface_tension_n_m = Some(1.);
    materials[0].density_kg_m3 = Some(1.);
    materials[0].dynamic_viscosity_pa_s = Some(0.);
    let settings = DomainSettings {
        min_mm: [0.; 3],
        max_mm: [1000.; 3],
        cell_mm: 1000. / n as f32,
    };
    let model = cad_ellipsoid(epsilon);
    let (gpu, volume) = if cad {
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let volume = gpu.build(&model, &settings, &materials).unwrap();
        (gpu, volume)
    } else {
        let (gpu, volume, _, _) = super::tests::box_fixture(settings, &materials);
        (gpu, volume)
    };
    assert_eq!(volume.dims, [n; 3]);
    let h = volume.inputs.snapshot().grid.cell_m as f64;
    let r = 0.2;
    let (mut surface, _) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
    let initial = if cad {
        surface.initialize_cad(&volume, &model, None).unwrap();
        let bytes = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        bytemuck::cast_slice(&bytes).to_vec()
    } else {
        let initial = ellipsoid(n, h, r, epsilon);
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
        surface.refresh(&volume).unwrap();
        initial
    };
    let (a0, v0, _) = moments(n, h, r, &initial);
    // Potential flow inside r^l and outside r^(-l-1), with Laplace jump:
    // omega_l^2=l(l-1)(l+1)(l+2)sigma/[R^3((l+1)rho_l+l rho_g)].
    let effective_r = (3. * v0 / (4. * PI)).cbrt();
    let omega = (24. / (effective_r.powi(3) * (3. * ratio as f64 + 2.))).sqrt();
    let period = 2. * PI / omega;
    let capillary = ((ratio as f64 + 1.) * h.powi(3) / (2. * PI)).sqrt();
    let steps = (periods * period / (factor * capillary)).ceil() as u32;
    let dt = periods * period / steps as f64;
    let mut fluid = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            surface: SurfaceConfig { enabled: true },
            dt_s: dt,
            acceleration_m_s2: [0.; 3],
            relative_tolerance: 1e-6,
            absolute_divergence_tolerance: 1e-6,
            max_iterations: 4000,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    fluid
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    std::fs::create_dir_all("artifacts").unwrap();
    let path = if cad {
        format!("artifacts/drop-oscillation-cad-{n}.csv")
    } else if dump {
        format!("artifacts/drop-oscillation-{n}.csv")
    } else {
        format!("artifacts/drop-oscillation-regular-{n}.csv")
    };
    let mut csv = String::from(
        "step,time_s,quadrupole,linear_quadrupole,relative_volume_drift,centroid_drift_m,max_speed_m_s,rms_divergence_s_inv,min_alpha,max_alpha\n",
    );
    let mut crossings = Vec::new();
    let mut previous = a0;
    let mut peak = a0.abs();
    for step in 0..=steps {
        let bytes = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let (a, v, drift) = moments(n, h, r, bytemuck::cast_slice(&bytes));
        let t = step as f64 * dt;
        if step > 0 && a * previous < 0. {
            crossings.push(t - dt + dt * previous / (previous - a));
        }
        peak = peak.max(a.abs());
        csv.push_str(&format!(
            "{step},{t},{a},{},{},{drift},{},{},{},{}\n",
            a0 * (omega * t).cos(),
            v / v0 - 1.,
            fluid.diagnostics.max_speed_m_s,
            fluid.diagnostics.rms_divergence_s_inv,
            surface.diagnostics.min_fraction,
            surface.diagnostics.max_fraction
        ));
        if step % 20 == 0 || step == steps {
            eprintln!(
                "drop n={n} {step}/{steps}: q/q0={:.6}, speed={:.4e}, volume={:.3e}, pressure={} it",
                a / a0,
                fluid.diagnostics.max_speed_m_s,
                v / v0 - 1.,
                fluid.diagnostics.iterations
            );
        }
        previous = a;
        if step == steps {
            break;
        }
        let result = (|| -> Result<(), String> {
            fluid.step(&volume)?;
            surface.advance_coupled(&volume, &mut fluid, None, dt)?;
            fluid.update_surface(&volume, surface.field(), None)?;
            Ok(())
        })();
        if let Err(error) = result {
            std::fs::write(&path, &csv).unwrap();
            panic!("Drop n={n} step {} dt={dt}: {error}", step + 1);
        }
    }
    if dump {
        std::fs::write(&path, &csv).unwrap();
    }
    let drift = (surface.diagnostics.volume_m3 / v0 - 1.).abs();
    eprintln!(
        "drop n={n}: omega exact={omega:.8}, peak/q0={:.5}, volume drift={drift:.4e}, crossings={crossings:?}",
        peak / a0.abs()
    );
    assert!(drift < 2e-6);
    assert!(peak / a0.abs() < 1.1, "Unforced drop-mode amplitude grew");
    DropReport {
        omega,
        shape: previous / a0,
        peak: peak / a0.abs(),
        crossings,
    }
}

fn cad_ellipsoid(epsilon: f64) -> crate::model::Model {
    use crate::model::{Body, Model};
    use glam::Vec3;
    let latitude: u32 = std::env::var("MHD_DROP_CAD_LATITUDES")
        .unwrap_or("64".into())
        .parse()
        .unwrap();
    assert!((8..=512).contains(&latitude));
    let longitude = 2 * latitude;
    let axes = Vec3::new(
        (200. / (1. + epsilon).sqrt()) as f32,
        (200. / (1. + epsilon).sqrt()) as f32,
        (200. * (1. + epsilon)) as f32,
    );
    let center = Vec3::splat(500.);
    let point = |j: u32, k: u32| {
        if j == 0 {
            return center + Vec3::Z * axes.z;
        }
        if j == latitude {
            return center - Vec3::Z * axes.z;
        }
        let theta = PI * j as f64 / latitude as f64;
        let phi = 2. * PI * (k % longitude) as f64 / longitude as f64;
        center
            + Vec3::new(
                (theta.sin() * phi.cos()) as f32,
                (theta.sin() * phi.sin()) as f32,
                theta.cos() as f32,
            ) * axes
    };
    let mut triangles = Vec::new();
    for k in 0..longitude {
        triangles.push([point(0, 0), point(1, k), point(1, k + 1)]);
        for j in 1..latitude - 1 {
            triangles.push([point(j, k), point(j + 1, k), point(j + 1, k + 1)]);
            triangles.push([point(j, k), point(j + 1, k + 1), point(j, k + 1)]);
        }
        triangles.push([
            point(latitude - 1, k),
            point(latitude, 0),
            point(latitude - 1, k + 1),
        ]);
    }
    let mut body = Body::new("Liquid ellipsoid".into(), triangles).unwrap();
    body.simulation.material_id = 3;
    assert!(body.closed());
    Model {
        bodies: vec![body],
        ..Default::default()
    }
}

#[test]
#[ignore = "CAD fractional initialization diagnostic"]
fn imported_drop_initialization_diagnostic() {
    let _lock = crate::gpu_test_lock();
    let model = cad_ellipsoid(0.03);
    let mesh_volume = model.bodies[0].volume_mm3() * 1e-9;
    let mut csv = String::from("grid,relative_volume_error,relative_fraction_l1,quadrupole\n");
    for n in [32u32, 48, 64] {
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let volume = gpu
            .build(
                &model,
                &DomainSettings {
                    min_mm: [0.; 3],
                    max_mm: [1000.; 3],
                    cell_mm: 1000. / n as f32,
                },
                &default_materials(),
            )
            .unwrap();
        let (mut surface, _) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
        surface.initialize_cad(&volume, &model, None).unwrap();
        let h = volume.inputs.snapshot().grid.cell_m as f64;
        let exact = ellipsoid(n, h, 0.2, 0.03);
        let bytes = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let values = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        let (q, v, _) = moments(n, h, 0.2, values);
        let l1 = values
            .iter()
            .zip(&exact)
            .map(|(a, b)| (a[0] as f64 - b[0] as f64).abs() * h.powi(3))
            .sum::<f64>()
            / mesh_volume;
        eprintln!(
            "CAD initial n={n}: volume error={:.7e}, fraction L1={l1:.7e}, q={q:.7e}",
            v / mesh_volume - 1.
        );
        csv.push_str(&format!("{n},{},{l1},{q}\n", v / mesh_volume - 1.));
    }
    std::fs::create_dir_all("artifacts").unwrap();
    std::fs::write("artifacts/cad-initialization.csv", csv).unwrap();
}
