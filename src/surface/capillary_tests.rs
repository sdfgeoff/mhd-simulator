//! Dynamic capillarity checks against linear potential-flow dispersion.
use super::*;
use crate::{
    fluid::{FluidConfig, FluidState},
    simulation::{DomainSettings, default_materials},
};
use std::f64::consts::PI;

/// Integrate the sinusoid over each cell instead of sampling its centre.
fn wave_fraction(n: u32, nz: u32, amplitude: f64) -> Vec<[f32; 4]> {
    let h = 1. / n as f64;
    let mut fractions = Vec::new();
    for _z in 0..nz {
        for y in 0..n {
            for x in 0..n {
                let low = y as f64 * h;
                let high = low + h;
                let mut value = 0.;
                for s in 0..128 {
                    let px = (x as f64 + (s as f64 + 0.5) / 128.) * h;
                    let interface = 0.5 + amplitude * (2. * PI * px).cos();
                    value += (interface.min(high) - low).clamp(0., h) / h;
                }
                fractions.push([(value / 128.) as f32, 0., 0., 0.]);
            }
        }
    }
    fractions
}
fn fourier_amplitude(n: u32, nz: u32, values: &[[f32; 4]]) -> f64 {
    let h = 1. / n as f64;
    let mut result = 0.;
    for z in 0..nz {
        for x in 0..n {
            let height = (0..n)
                .map(|y| values[(x + n * (y + n * z)) as usize][0] as f64 * h)
                .sum::<f64>();
            result += (height - 0.5) * (2. * PI * (x as f64 + 0.5) * h).cos();
        }
    }
    result * 2. / (n * nz) as f64
}
struct Wave {
    n: u32,
    nz: u32,
    steps: u32,
    dt: f64,
    omega: f64,
    a0: f64,
    volume0: f64,
    context: GpuContext,
    volume: Volume,
    surface: SurfaceState,
    fluid: FluidState,
}
impl Wave {
    fn new(n: u32, periods: f64, factor: f64, density_ratio: f32) -> Self {
        let nz = 4;
        let h = 1. / n as f32;
        let amplitude = 0.005;
        let mut materials = default_materials();
        materials[3].density_kg_m3 = Some(density_ratio);
        materials[3].dynamic_viscosity_pa_s = Some(1e-6);
        materials[3].surface_tension_n_m = Some(1.);
        materials[0].density_kg_m3 = Some(1.);
        materials[0].dynamic_viscosity_pa_s = Some(0.);
        let (gpu, volume, mut surface, _) = super::tests::box_fixture(
            DomainSettings {
                min_mm: [0.; 3],
                max_mm: [1000., 1000., 4000. / n as f32],
                cell_mm: 1000. / n as f32,
            },
            &materials,
        );
        assert_eq!(volume.dims, [n, n, nz]);
        let initial = wave_fraction(n, nz, amplitude);
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
        surface.refresh(&volume).unwrap();
        // Rigid top/bottom at depths 1/2, zero gravity, sigma=1:
        // eta_tt + sigma*k^3/[rho_l*coth(k*h_l)+rho_g*coth(k*h_g)] eta = 0.
        let k = 2. * PI;
        let omega = (k.powi(3) / ((density_ratio as f64 + 1.) / (0.5 * k).tanh())).sqrt();
        let period = 2. * PI / omega;
        let dt_cap =
            (((density_ratio + 1.) * h.powi(3)) / (2. * std::f32::consts::PI)).sqrt() as f64;
        let steps = (periods * period / (factor * dt_cap)).ceil() as u32;
        let dt = periods * period / steps as f64;
        let config = FluidConfig {
            dt_s: dt,
            acceleration_m_s2: [0.; 3],
            advection: true,
            relative_tolerance: 1e-6,
            absolute_divergence_tolerance: 1e-6,
            max_iterations: 4000,
            surface: SurfaceConfig { enabled: true },
            ..Default::default()
        };
        let mut fluid = FluidState::new(gpu.context.clone(), &volume, config, None).unwrap();
        fluid
            .update_surface(&volume, surface.field(), None)
            .unwrap();
        let a0 = fourier_amplitude(n, nz, &initial);
        let volume0 = surface.diagnostics.volume_m3;
        Self {
            n,
            nz,
            steps,
            dt,
            omega,
            a0,
            volume0,
            context: gpu.context,
            volume,
            surface,
            fluid,
        }
    }
    fn step(&mut self) -> (f64, Vec<u8>) {
        self.fluid.step(&self.volume).unwrap();
        self.surface
            .advance_coupled(&self.volume, &mut self.fluid, None, self.dt)
            .unwrap();
        self.fluid
            .update_surface(&self.volume, self.surface.field(), None)
            .unwrap();
        let data = read_bytes(
            &self.context,
            self.surface.field(),
            0,
            self.surface.field().size(),
        )
        .unwrap();
        let a = fourier_amplitude(self.n, self.nz, bytemuck::cast_slice(&data));
        (a, data)
    }
}

#[test]
fn capillary_wave_restores_without_spurious_amplitude_growth() {
    let _lock = crate::gpu_test_lock();
    for density_ratio in [1., 7000.] {
        let mut wave = Wave::new(16, 0.5, 0.8, density_ratio);
        let mut last = wave.a0;
        let mut peak = last.abs();
        for _ in 0..wave.steps {
            let (a, _) = wave.step();
            last = a;
            peak = peak.max(a.abs());
        }
        eprintln!(
            "Half-period capillary wave density ratio {density_ratio}: amplitude ratio {}, peak ratio {}",
            last / wave.a0,
            peak / wave.a0
        );
        assert!((last / wave.a0 + 1.).abs() < 0.25);
        assert!(peak / wave.a0 < 1.1);
        assert!((wave.surface.diagnostics.volume_m3 / wave.volume0 - 1.).abs() < 2e-6);
    }
}

#[test]
#[ignore = "Dynamic capillary-wave grid refinement; writes artifacts/capillary-wave-*.csv"]
fn standing_capillary_wave_matches_finite_depth_dispersion() {
    let _lock = crate::gpu_test_lock();
    std::fs::create_dir_all("artifacts").unwrap();
    let grids = std::env::var("MHD_CAPILLARY_GRIDS")
        .unwrap_or_else(|_| "32,64".into())
        .split(',')
        .map(|n| n.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    let factor = std::env::var("MHD_CAPILLARY_DT_FACTOR")
        .map(|v| v.parse::<f64>().unwrap())
        .unwrap_or(0.8);
    let density_ratio = std::env::var("MHD_CAPILLARY_DENSITY_RATIO")
        .map(|v| v.parse::<f32>().unwrap())
        .unwrap_or(1.);
    let mut errors = Vec::new();
    for n in grids {
        let mut wave = Wave::new(n, 1.5, factor, density_ratio);
        let mut samples = vec![(0., wave.a0)];
        let mut csv = String::from(
            "time_s,amplitude_m,linear_amplitude_m,max_speed_m_s,relative_volume_drift,rms_divergence_s_inv\n",
        );
        for step in 1..=wave.steps {
            let (a, data) = wave.step();
            let time = step as f64 * wave.dt;
            samples.push((time, a));
            csv += &format!(
                "{time},{a},{},{},{},{}\n",
                wave.a0 * (wave.omega * time).cos(),
                wave.fluid.diagnostics.max_speed_m_s,
                wave.surface.diagnostics.volume_m3 / wave.volume0 - 1.,
                wave.fluid.diagnostics.rms_divergence_s_inv
            );
            if step % 100 == 0 {
                if std::env::var_os("MHD_CAPILLARY_DUMP").is_some() {
                    std::fs::write(format!("artifacts/capillary-field-{n}-{step}.bin"), data)
                        .unwrap();
                    let flow = read_bytes(
                        &wave.context,
                        wave.fluid.result_buffer(),
                        0,
                        wave.fluid.result_buffer().size(),
                    )
                    .unwrap();
                    std::fs::write(format!("artifacts/capillary-flow-{n}-{step}.bin"), flow)
                        .unwrap();
                }
                eprintln!(
                    "capillary n={n} {step}/{}: amplitude {a:.5e}, speed {:.4e}, pressure {} it",
                    wave.steps,
                    wave.fluid.diagnostics.max_speed_m_s,
                    wave.fluid.diagnostics.iterations
                );
            }
        }
        std::fs::write(format!("artifacts/capillary-wave-{n}.csv"), csv).unwrap();
        let crossings: Vec<f64> = samples
            .windows(2)
            .filter_map(|w| {
                if w[0].1 * w[1].1 < 0. {
                    Some(w[0].0 + (w[1].0 - w[0].0) * w[0].1 / (w[0].1 - w[1].1))
                } else {
                    None
                }
            })
            .collect();
        assert!(
            crossings.len() >= 3,
            "n={n} missed oscillations: {crossings:?}"
        );
        let measured = 2. * PI / (crossings[2] - crossings[0]);
        let error = (measured / wave.omega - 1.).abs();
        let drift = (wave.surface.diagnostics.volume_m3 / wave.volume0 - 1.).abs();
        eprintln!(
            "capillary n={n} rho ratio {density_ratio}: omega={measured:.7}, exact={:.7}, error={error:.4e}, drift={drift:.3e}, crossings={crossings:?}",
            wave.omega
        );
        errors.push(error);
        assert!(drift < 2e-5);
        assert!(error < 0.12, "capillary frequency error {error}");
    }
    if errors.len() > 1 {
        assert!(
            errors.last().unwrap() < errors.first().unwrap(),
            "{errors:?}"
        );
    }
}

#[test]
fn reconstruction_normal_is_consistent_when_a_shallow_plane_crosses_cell_rows() {
    let _lock = crate::gpu_test_lock();
    let n = 32;
    let nz = 4;
    let h = 1. / n as f64;
    let slope = 0.03;
    let (gpu, volume, mut surface, _) = super::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000., 1000., 4000. / n as f32],
            cell_mm: 1000. / n as f32,
        },
        &default_materials(),
    );
    for inverse in [false, true] {
        let mut values = Vec::new();
        for _z in 0..nz {
            for y in 0..n {
                for x in 0..n {
                    let xl = x as f64 * h;
                    let yl = y as f64 * h;
                    let z0 = 0.5 + slope * (xl - 0.5) - yl;
                    let z1 = z0 + slope * h;
                    let integral = |z: f64| 0.5 * z.max(0.).powi(2) - 0.5 * (z - h).max(0.).powi(2);
                    let mut fraction =
                        ((integral(z1) - integral(z0)) / (slope * h * h)).clamp(0., 1.);
                    if inverse {
                        fraction = 1. - fraction;
                    }
                    values.push([fraction as f32, 0., 0., 0.]);
                }
            }
        }
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
        surface.refresh(&volume).unwrap();
        let data = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let cells = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
        surface.dispatch(&volume, None, None, &["reconstruct"]);
        let data = read_bytes(&gpu.context, &surface.normals, 0, surface.normals.size()).unwrap();
        let planes = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
        let sign = if inverse { -1. } else { 1. };
        let expected = [-sign * slope / (1. + slope), sign / (1. + slope), 0.];
        let mut checked = 0;
        for (i, cell) in cells.iter().enumerate() {
            let x = i as u32 % n;
            let y = (i as u32 / n) % n;
            if x < 3 || x >= n - 3 || cell[0] < 1e-4 || cell[0] > 1. - 1e-4 {
                continue;
            }
            for axis in 0..3 {
                assert!(
                    (planes[i][axis] as f64 - expected[axis]).abs() < 2e-5,
                    "inverse {inverse}, cell {i}: {:?}",
                    planes[i]
                );
            }
            let center_x = (x as f64 + 0.5) * h;
            let center_y = (y as f64 + 0.5) * h;
            let intercept = sign * (0.5 + slope * (center_x - 0.5) - center_y) / (h * (1. + slope));
            assert!((planes[i][3] as f64 - intercept).abs() < 2e-5);
            assert!(
                cell[1].abs() < 5e-4,
                "Planar interface curvature at {i}: {}",
                cell[1]
            );
            checked += 1;
        }
        assert!(checked > 50);
    }
}

#[test]
fn height_columns_reject_multiple_crossings_and_preserve_orientation() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut surface, _) = super::tests::fixture(8);
    super::install_test_entry(
        &mut surface,
        "probe",
        r#"
        @compute @workgroup_size(256)
        fn probe(@builtin(global_invocation_id) g:vec3<u32>){
            if flat(g)!=0u{return;}
            stats[0]=vec4<f32>(height(vec3<i32>(3,3,3),1u),0.0);
        }"#,
    );

    for (profile, valid, orientation) in [
        ([1., 1., 0.8, 0.4, 0.6, 0.2, 0., 0.], false, 1.),
        ([1., 1., 0.8, 0.6, 0.4, 0.2, 0., 0.], true, 1.),
        ([0., 0., 0.2, 0.4, 0.6, 0.8, 1., 1.], true, -1.),
    ] {
        let values: Vec<[f32; 4]> = (0..8u32.pow(3))
            .map(|i| [profile[((i / 8) % 8) as usize], 0., 0., 0.])
            .collect();
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
        surface.dispatch(&volume, None, None, &["probe"]);
        let data = read_bytes(&gpu.context, &surface.stats, 0, 16).unwrap();
        let result = bytemuck::cast_slice::<u8, f32>(&data);
        assert_eq!(result[1] > 0.5, valid);
        assert_eq!(result[2], orientation);
    }
}
