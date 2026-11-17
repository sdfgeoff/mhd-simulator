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
            DropOptions {
                dump: true,
                cad: std::env::var_os("MHD_DROP_CAD").is_some(),
                liquid_nu: std::env::var("MHD_DROP_LIQUID_NU")
                    .unwrap_or("0".into())
                    .parse()
                    .unwrap(),
            },
        );
        if periods >= 1.3 {
            errors.push(check_frequency(n, &report.crossings, report.omega));
        }
    }
    check_frequency_refinement(&errors);
}

fn check_frequency(n: u32, crossings: &[f64], reference: f64) -> f64 {
    assert!(
        crossings.len() >= 3,
        "Drop did not complete three half-period crossings"
    );
    let measured = 2. * PI / (crossings[2] - crossings[0]);
    let error = (measured / reference - 1.).abs();
    eprintln!("drop n={n}: measured omega={measured:.8}, frequency error={error:.6e}");
    assert!(
        error < 0.03,
        "Drop frequency differs from its linear mode reference by more than 3%"
    );
    error
}
fn check_frequency_refinement(errors: &[f64]) {
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
    let report = simulate_drop(48, periods, 0.4, 0.03, 7000., DropOptions::default());
    let reference = (2. * PI * periods).cos();
    assert!(
        (report.shape - reference).abs() < 0.1,
        "Drop shape {} differs from reference {reference}",
        report.shape
    );
    assert!(report.peak <= 1.1);
}

#[derive(Default)]
struct DropOptions {
    dump: bool,
    cad: bool,
    /// Liquid kinematic viscosity in m²/s; ambient remains inviscid.
    liquid_nu: f32,
}

fn simulate_drop(
    n: u32,
    periods: f64,
    factor: f64,
    epsilon: f64,
    ratio: f32,
    options: DropOptions,
) -> DropReport {
    let DropOptions {
        dump,
        cad,
        liquid_nu,
    } = options;
    assert!(liquid_nu.is_finite() && liquid_nu >= 0.);
    let wall_started = std::time::Instant::now();
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(ratio);
    materials[3].dynamic_viscosity_pa_s = Some(ratio * liquid_nu);
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
    let viscous_reference = (liquid_nu > 0.).then(|| {
        super::viscous_reference::eigenvalue(
            (8. / (ratio as f64 * effective_r.powi(3))).sqrt(),
            liquid_nu as f64,
            effective_r,
        )
    });
    let frequency_reference = viscous_reference.map_or(omega, |r| r.im);
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
    fluid.euler_viscosity_reference =
        dump && std::env::var_os("MHD_DROP_EULER_VISCOSITY").is_some();
    fluid
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    std::fs::create_dir_all("artifacts").unwrap();
    let path = if liquid_nu > 0. {
        format!("artifacts/drop-oscillation-viscous-{n}.csv")
    } else if cad {
        format!("artifacts/drop-oscillation-cad-{n}.csv")
    } else if dump {
        format!("artifacts/drop-oscillation-{n}.csv")
    } else {
        format!("artifacts/drop-oscillation-regular-{n}.csv")
    };
    let mut csv = String::from(
        "step,time_s,quadrupole,linear_quadrupole,relative_volume_drift,centroid_drift_m,max_speed_m_s,rms_divergence_s_inv,min_alpha,max_alpha,viscous_substeps,material_volume_m3\n",
    );
    let mut crossings = Vec::new();
    let mut trace = Vec::new();
    let mut previous = a0;
    let mut peak = a0.abs();
    for step in 0..=steps {
        let bytes = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let (a, v, drift) = moments(n, h, r, bytemuck::cast_slice(&bytes));
        let t = step as f64 * dt;
        trace.push((t, a));
        if step > 0 && a * previous < 0. {
            crossings.push(t - dt + dt * previous / (previous - a));
        }
        peak = peak.max(a.abs());
        csv.push_str(&format!(
            "{step},{t},{a},{},{},{drift},{},{},{},{},{},{v}\n",
            a0 * (omega * t).cos(),
            v / v0 - 1.,
            fluid.diagnostics.max_speed_m_s,
            fluid.diagnostics.rms_divergence_s_inv,
            surface.diagnostics.min_fraction,
            surface.diagnostics.max_fraction,
            fluid.diagnostics.viscous_substeps
        ));
        if step % 20 == 0 || step == steps {
            eprintln!(
                "drop n={n} {step}/{steps}: q/q0={:.6}, speed={:.4e}, volume={:.3e}, pressure={} it, viscosity={} stages",
                a / a0,
                fluid.diagnostics.max_speed_m_s,
                v / v0 - 1.,
                fluid.diagnostics.iterations,
                fluid.diagnostics.viscous_substeps
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
    if let Some(reference) = viscous_reference.filter(|_| periods >= 1.) {
        report_viscous_fit(&trace, reference, period, n);
    }
    eprintln!(
        "drop n={n}: complete wall {:.3} s",
        wall_started.elapsed().as_secs_f64()
    );
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
        omega: frequency_reference,
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

fn report_viscous_fit(
    trace: &[(f64, f64)],
    reference: num_complex::Complex64,
    period: f64,
    n: u32,
) {
    for start in [0., 0.25 * period, 0.5 * period] {
        let fit = super::drop_fit::fit(trace, start, reference.re, reference.im);
        eprintln!(
            "viscous drop n={n}, fit t>={start:.5}: beta={:.9} omega={:.9} RMS={:.6e}, reference beta={:.9} omega={:.9}",
            fit.decay, fit.omega, fit.rms, reference.re, reference.im
        );
    }
}
#[test]
#[ignore = "Analyze saved viscous-drop CSVs without rerunning the GPU"]
fn viscous_drop_saved_diagnostics() {
    let grids = std::env::var("MHD_DROP_GRIDS").unwrap_or("48,64".into());
    let nu: f64 = std::env::var("MHD_DROP_LIQUID_NU")
        .unwrap_or("0.0002".into())
        .parse()
        .unwrap();
    let ratio: f64 = std::env::var("MHD_DROP_DENSITY_RATIO")
        .unwrap_or("7000".into())
        .parse()
        .unwrap();
    let mut errors = Vec::new();
    for n in grids.split(',').map(|v| v.parse::<u32>().unwrap()) {
        let path = format!("artifacts/drop-oscillation-viscous-{n}.csv");
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        let header: Vec<_> = lines.next().unwrap().split(',').collect();
        let col = |name: &str| header.iter().position(|v| *v == name).unwrap();
        let time = col("time_s");
        let q = col("quadrupole");
        let mut radius = 0.2f64;
        let rows: Vec<Vec<f64>> = lines
            .map(|l| l.split(',').map(|s| s.parse().unwrap()).collect())
            .collect();
        if let Some(v) = header.iter().position(|v| *v == "material_volume_m3") {
            radius = (3. * rows[0][v] / (4. * PI)).cbrt();
        }
        let trace: Vec<_> = rows.iter().map(|r| (r[time], r[q])).collect();
        let reference = super::viscous_reference::eigenvalue(
            (8. / (ratio * radius.powi(3))).sqrt(),
            nu,
            radius,
        );
        let period = 2. * PI / (24. / (radius.powi(3) * (3. * ratio + 2.))).sqrt();
        let mut crossings = Vec::new();
        for pair in trace.windows(2) {
            let [(t0, q0), (t1, q1)] = pair else {
                unreachable!()
            };
            if q0 * q1 < 0. {
                crossings.push(t0 + (t1 - t0) * q0 / (q0 - q1));
            }
        }
        errors.push(check_frequency(n, &crossings, reference.im));
        report_viscous_fit(&trace, reference, period, n);
    }
    check_frequency_refinement(&errors);
}
