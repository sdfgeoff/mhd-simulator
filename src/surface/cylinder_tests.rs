//! Full-3D periodic-cylinder capillary diagnostics. CPU code supplies scalar
//! analytic references and initial geometry, not another fluid solver.
use super::*;
use std::f64::consts::PI;

fn modified_bessels(x: f64) -> [f64; 4] {
    assert!(x > 0. && x < 8.);
    let mut i0 = 1.;
    let mut i1 = x / 2.;
    let mut t0 = 1.;
    let mut t1 = x / 2.;
    for n in 1..128 {
        let n = n as f64;
        t0 *= x * x / (4. * n * n);
        t1 *= x * x / (4. * n * (n + 1.));
        i0 += t0;
        i1 += t1;
        if t0.abs() < 1e-16 * i0 && t1.abs() < 1e-16 * i1 {
            break;
        }
    }
    // Integral K_n(x)=integral_0^infinity exp(-x cosh(t))*cosh(n*t) dt.
    // The endpoint x*cosh(T)=50 leaves an exponentially small omitted tail.
    let end = (50. / x).acosh();
    let steps = 2048;
    let dt = end / steps as f64;
    let mut k = [0.; 2];
    for j in 0..=steps {
        let c = (j as f64 * dt).cosh();
        let value = (-x * c).exp();
        let weight = if j == 0 || j == steps {
            1.
        } else if j % 2 == 0 {
            2.
        } else {
            4.
        };
        k[0] += weight * value;
        k[1] += weight * value * c;
    }
    [i0, i1, k[0] * dt / 3., k[1] * dt / 3.]
}
fn growth_squared(radius: f64, wavenumber: f64, rho_l: f64, rho_g: f64, tension: f64) -> f64 {
    let x = radius * wavenumber;
    let [i0, i1, k0, k1] = modified_bessels(x);
    tension / radius.powi(3) * x * (1. - x * x) / (rho_l * i0 / i1 + rho_g * k0 / k1)
}
fn circle_primitive(x: f64, y: f64, r: f64) -> f64 {
    let sign = x.signum() * y.signum();
    let x = x.abs().min(r);
    let y = y.abs().min(r);
    if x * x + y * y <= r * r {
        return sign * x * y;
    }
    let t = (r * r - y * y).max(0.).sqrt();
    let integral =
        |u: f64| 0.5 * (u * (r * r - u * u).max(0.).sqrt() + r * r * (u / r).clamp(-1., 1.).asin());
    sign * (y * t + integral(x) - integral(t)).clamp(0., x * y)
}
fn circle_rectangle(x: f64, y: f64, h: f64, r: f64) -> f64 {
    (circle_primitive(x + h, y + h, r)
        - circle_primitive(x, y + h, r)
        - circle_primitive(x + h, y, r)
        + circle_primitive(x, y, r))
    .clamp(0., h * h)
}
fn cylinder_fractions(
    dims: [u32; 3],
    h: f64,
    volume_radius: f64,
    epsilon: f64,
    samples: u32,
) -> Vec<[f32; 4]> {
    assert!(samples > 0 && epsilon.abs() < 0.2);
    let mean_radius = volume_radius / (1. + 0.5 * epsilon * epsilon).sqrt();
    let length = dims[2] as f64 * h;
    let mut field = Vec::with_capacity(dims.iter().map(|&v| v as usize).product());
    for z in 0..dims[2] {
        let radii: Vec<_> = (0..samples)
            .map(|s| {
                mean_radius
                    * (1.
                        + epsilon
                            * (2. * PI * (z as f64 + (s as f64 + 0.5) / samples as f64) * h
                                / length)
                                .cos())
            })
            .collect();
        let low = radii.iter().copied().fold(f64::INFINITY, f64::min);
        let high = radii.iter().copied().fold(0., f64::max);
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let x = (x as f64 - dims[0] as f64 / 2.) * h;
                let y = (y as f64 - dims[1] as f64 / 2.) * h;
                let nearest = ((x + 0.5 * h).abs() - 0.5 * h).max(0.).powi(2)
                    + ((y + 0.5 * h).abs() - 0.5 * h).max(0.).powi(2);
                let farthest = ((x + 0.5 * h).abs() + 0.5 * h).powi(2)
                    + ((y + 0.5 * h).abs() + 0.5 * h).powi(2);
                let alpha = if nearest >= high * high {
                    0.
                } else if farthest <= low * low {
                    1.
                } else {
                    radii
                        .iter()
                        .map(|&r| circle_rectangle(x, y, h, r))
                        .sum::<f64>()
                        / (samples as f64 * h * h)
                };
                field.push([alpha as f32, 0., 0., 0.]);
            }
        }
    }
    field
}
#[test]
fn cylinder_scalar_reference_matches_bessels_and_density_limits() {
    // Independent 50-digit mpmath values, rounded once to f64.
    for [x, i0, i1, k0, k1] in [
        [
            0.2,
            1.010025027795146,
            0.10050083402812512,
            1.7527038555281458,
            4.775972543220472,
        ],
        [
            0.7,
            1.1263030183068092,
            0.37187967777700864,
            0.6605198599151016,
            1.050283535312918,
        ],
        [
            1.,
            1.2660658777520084,
            0.565159103992485,
            0.42102443824070834,
            0.6019072301972346,
        ],
        [
            1.4,
            1.5533950997312165,
            0.8860919814143273,
            0.24365506118154193,
            0.3208359022298758,
        ],
        [
            2.,
            2.2795853023360673,
            1.590636854637329,
            0.11389387274953344,
            0.13986588181652243,
        ],
    ] {
        let actual = modified_bessels(x);
        for (a, b) in actual.into_iter().zip([i0, i1, k0, k1]) {
            assert!((a / b - 1.).abs() < 2e-11, "x={x}: {a} vs {b}");
        }
        assert!((x * (actual[0] * actual[3] + actual[1] * actual[2]) - 1.).abs() < 2e-11);
        let scale = 1. / (5. * 0.1f64.powi(3));
        let vacuum = scale * x * (1. - x * x) * i1 / i0;
        let equal = scale * x * x * (1. - x * x) * i1 * k1;
        assert!((growth_squared(0.1, x / 0.1, 5., 0., 1.) - vacuum).abs() < 1e-10 * scale);
        assert!((growth_squared(0.1, x / 0.1, 5., 5., 1.) - equal).abs() < 1e-10 * scale);
    }
    assert!(growth_squared(0.1, 7., 7000., 1., 1.) > 0.);
    assert!(growth_squared(0.1, 14., 7000., 1., 1.) < 0.);
    assert!(growth_squared(0.1, 10., 7000., 1., 1.).abs() < 1e-12);
}
#[test]
fn cylinder_geometry_preserves_volume_and_integrated_axial_mode() {
    for shift in [0., 0.37] {
        let h = 0.125;
        let n = 24;
        let mut area = 0.;
        for y in 0..n {
            for x in 0..n {
                area += circle_rectangle(
                    (x as f64 - n as f64 / 2. - shift) * h,
                    (y as f64 - n as f64 / 2. - shift) * h,
                    h,
                    1.,
                );
            }
        }
        assert!((area / PI - 1.).abs() < 1e-12);
    }
    let dims = [32, 32, 36];
    let h = 0.025;
    let r: f64 = 0.1;
    let epsilon: f64 = 0.01;
    let samples = 16;
    let field = cylinder_fractions(dims, h, r, epsilon, samples);
    let mut sums = [0.; 3];
    for (i, v) in field.iter().enumerate() {
        assert!((0. ..=1.).contains(&v[0]));
        let z = i as u32 / (dims[0] * dims[1]);
        let theta = 2. * PI * (z as f64 + 0.5) / dims[2] as f64;
        sums[0] += v[0] as f64;
        sums[1] += v[0] as f64 * theta.cos();
        sums[2] += v[0] as f64 * theta.sin();
    }
    let expected_volume = PI * r * r * dims[2] as f64 * h;
    let a = PI / dims[2] as f64;
    let filter = a.sin() / (samples as f64 * (a / samples as f64).sin());
    let expected_mode = 2. * epsilon / (1. + 0.5 * epsilon * epsilon) * filter;
    assert!((sums[0] * h.powi(3) / expected_volume - 1.).abs() < 2e-8);
    assert!((2. * sums[1] / sums[0] / expected_mode - 1.).abs() < 2e-7);
    assert!((2. * sums[2] / sums[0]).abs() < 1e-9);
}

struct ModeProbe {
    stats: wgpu::Buffer,
    binds: [wgpu::BindGroup; 2],
    count: u32,
}
#[derive(Clone, Copy, Debug)]
struct Moments {
    volume: f64,
    cosine: f64,
    sine: f64,
    second: f64,
    center_cells: [f64; 2],
}
impl ModeProbe {
    fn new(surface: &mut SurfaceState, volume: &Volume) -> Self {
        Self::with_source(surface, volume, include_str!("cylinder_probes.wgsl"))
    }
    fn with_source(surface: &mut SurfaceState, volume: &Volume, source: &str) -> Self {
        for entry in ["cylinder_collect", "cylinder_reduce"] {
            install_test_entry(surface, entry, source);
        }
        let count = surface.params.dims[3].div_ceil(256);
        let stats = surface
            .context
            .device
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("Cylinder Fourier moments"),
                size: (count as u64 + 1) * 32,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
        let binds = std::array::from_fn(|current| {
            let buffers = [
                &surface.uniform,
                volume.material_ids(),
                &surface.fields[current],
                &surface.fields[1 - current],
                &surface.zero,
                &surface.zero,
                &surface.zero_output,
                &surface.normals,
                &stats,
            ];
            let entries: Vec<_> = buffers
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect();
            surface
                .context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Cylinder read-only field probe"),
                    layout: &surface.layout,
                    entries: &entries,
                })
        });
        Self {
            stats,
            binds,
            count,
        }
    }
    fn raw_sample(&self, surface: &SurfaceState) -> [f32; 8] {
        let context = &surface.context;
        let mut encoder = context.device.create_command_encoder(&Default::default());
        {
            let mut pass = context.compute_pass(&mut encoder, None);
            pass.set_bind_group(0, &self.binds[surface.current], &[]);
            pass.set_pipeline(&surface.pipelines["cylinder_collect"]);
            let g = groups(surface.params.dims[3]);
            pass.dispatch_workgroups(g[0], g[1], g[2]);
            pass.set_pipeline(&surface.pipelines["cylinder_reduce"]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        context.queue.submit(Some(encoder.finish()));
        let bytes = read_bytes(context, &self.stats, self.count as u64 * 32, 32).unwrap();
        let v = bytemuck::cast_slice::<u8, f32>(&bytes);
        assert!(v.iter().all(|v| v.is_finite()) && v[0] > 0., "{v:?}");
        v.try_into().unwrap()
    }
    fn sample(&self, surface: &SurfaceState) -> Moments {
        let v = self.raw_sample(surface);
        let sum = v[0] as f64;
        Moments {
            volume: sum * (surface.params.step[1] as f64).powi(3),
            cosine: 2. * v[1] as f64 / sum,
            sine: 2. * v[2] as f64 / sum,
            second: 2. * v[3] as f64 / sum,
            center_cells: [v[4] as f64 / sum, v[5] as f64 / sum],
        }
    }
}
struct Geometry {
    h: f64,
    volume_radius: f64,
    mean_radius: f64,
    length: f64,
    wavenumber: f64,
    epsilon: f64,
    dims: [u32; 3],
}
fn setup(
    radius_cells: u32,
    target_kr: f64,
    epsilon: f64,
) -> (crate::domain::GpuDomain, Volume, SurfaceState, Geometry) {
    let n = 8 * radius_cells;
    let nz = (2. * PI * radius_cells as f64 / target_kr).round() as u32;
    let cell_mm = 100. / radius_cells as f32;
    let min_mm = [-0.5 * n as f32 * cell_mm, -0.5 * n as f32 * cell_mm, 0.];
    // The domain API rounds requested bounds outward to complete cells. Place
    // each maximum inside its final cell, beyond its centre, so f32 rounding
    // cannot accidentally add a cell. Geometry/reference use the actual grid.
    let requested_dims = [n, n, nz];
    let settings = crate::simulation::DomainSettings {
        min_mm,
        max_mm: std::array::from_fn(|a| min_mm[a] + (requested_dims[a] as f32 - 0.25) * cell_mm),
        cell_mm,
    };
    let mut materials = crate::simulation::default_materials();
    materials[3].density_kg_m3 = Some(7000.);
    materials[3].dynamic_viscosity_pa_s = Some(0.);
    materials[3].surface_tension_n_m = Some(1.);
    materials[0].density_kg_m3 = Some(1.);
    materials[0].dynamic_viscosity_pa_s = Some(0.);
    let (gpu, volume, _, _) = super::tests::box_fixture(settings, &materials);
    assert_eq!(volume.dims, [n, n, nz]);
    let h = volume.inputs.snapshot().grid.cell_m as f64;
    let volume_radius = radius_cells as f64 * h;
    let mean_radius = volume_radius / (1. + 0.5 * epsilon * epsilon).sqrt();
    let length = nz as f64 * h;
    let (mut surface, _) = SurfaceState::with_periodic_boundaries(
        gpu.context.clone(),
        &volume,
        3,
        [false, false, true],
    )
    .unwrap();
    let field = cylinder_fractions(volume.dims, h, volume_radius, epsilon, 16);
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&field));
    surface.refresh(&volume).unwrap();
    let geometry = Geometry {
        h,
        volume_radius,
        mean_radius,
        length,
        wavenumber: 2. * PI / length,
        epsilon,
        dims: volume.dims,
    };
    (gpu, volume, surface, geometry)
}
#[test]
fn cylinder_gpu_moments_match_initial_geometry_on_both_field_buffers() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut surface, g) = setup(4, 0.7, 0.01);
    let probe = ModeProbe::new(&mut surface, &volume);
    let capture = |s: &SurfaceState| {
        [
            &s.fields[0],
            &s.fields[1],
            &s.normals,
            &s.stats,
            &s.checkpoint,
        ]
        .iter()
        .map(|b| read_bytes(&s.context, b, 0, b.size()).unwrap())
        .collect::<Vec<_>>()
    };
    let a = PI / g.dims[2] as f64;
    let filter = |m: f64| (m * a).sin() / (16. * (m * a / 16.).sin());
    let expected = 2. * g.epsilon / (1. + 0.5 * g.epsilon * g.epsilon) * filter(1.);
    let second = 0.5 * g.epsilon * g.epsilon / (1. + 0.5 * g.epsilon * g.epsilon) * filter(2.);
    let volume_ref = PI * g.volume_radius.powi(2) * g.length;
    for _ in 0..2 {
        let before = capture(&surface);
        let m = probe.sample(&surface);
        assert_eq!(before, capture(&surface));
        eprintln!("Cylinder initial GPU moments {m:?}, expected mode={expected:.8e}");
        assert!((m.volume / volume_ref - 1.).abs() < 5e-7);
        assert!((m.cosine / expected - 1.).abs() < 1e-5);
        assert!(m.sine.abs() < 2e-7 && (m.second - second).abs() < 2e-7);
        assert!(m.center_cells.iter().all(|x| x.abs() < 5e-5));
        let mut encoder = gpu
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(
            surface.field(),
            0,
            &surface.fields[1 - surface.current],
            0,
            surface.field().size(),
        );
        gpu.context.queue.submit(Some(encoder.finish()));
        surface.current = 1 - surface.current;
    }
    assert!(g.h > 0. && g.wavenumber * g.mean_radius < 1.);
}

#[test]
#[ignore = "Full-3D periodic cylinder growth/oscillation diagnostic; writes cylinder CSVs"]
fn rayleigh_plateau_cylinder_diagnostic() {
    let _lock = crate::gpu_test_lock();
    let grids = std::env::var("MHD_CYLINDER_RADIUS_CELLS").unwrap_or_else(|_| "8".into());
    let kr: f64 = std::env::var("MHD_CYLINDER_KR")
        .unwrap_or_else(|_| "0.7".into())
        .parse()
        .unwrap();
    let duration: f64 = std::env::var("MHD_CYLINDER_DURATION")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let epsilon: f64 = std::env::var("MHD_CYLINDER_DEFORMATION")
        .unwrap_or_else(|_| "0.01".into())
        .parse()
        .unwrap();
    let factor: f64 = std::env::var("MHD_CYLINDER_DT_FACTOR")
        .unwrap_or_else(|_| "0.4".into())
        .parse()
        .unwrap();
    assert!(
        (0.1..=3.).contains(&kr)
            && duration > 0.
            && (0. ..=0.1).contains(&epsilon)
            && factor > 0.
            && factor <= 0.5
    );
    std::fs::create_dir_all("artifacts").unwrap();
    for cells in grids.split(',').map(|s| s.trim().parse::<u32>().unwrap()) {
        assert!((4..=64).contains(&cells));
        let (gpu, volume, mut surface, g) = setup(cells, kr, epsilon);
        gpu.context.profiler.set_enabled(false);
        let s2 = growth_squared(g.mean_radius, g.wavenumber, 7000., 1., 1.);
        let omega = s2.abs().sqrt();
        assert!(omega > 1e-8);
        let end = duration / omega;
        let bound = (7001. * g.h.powi(3) / (2. * PI)).sqrt();
        let steps = (end / (factor * bound)).ceil() as u32;
        let dt = end / steps as f64;
        let mut fluid = crate::fluid::FluidState::new(
            gpu.context.clone(),
            &volume,
            crate::fluid::FluidConfig {
                material_id: 3,
                dt_s: dt,
                acceleration_m_s2: [0.; 3],
                advection: true,
                periodic: [false, false, true],
                relative_tolerance: 1e-6,
                absolute_divergence_tolerance: 1e-6,
                max_iterations: 4000,
                surface: SurfaceConfig { enabled: true },
                ..Default::default()
            },
            None,
        )
        .unwrap();
        fluid.force_multigrid =
            std::env::var("MHD_CYLINDER_PRESSURE_MG")
                .ok()
                .map(|v| match v.as_str() {
                    "0" => false,
                    "1" => true,
                    _ => panic!("MHD_CYLINDER_PRESSURE_MG must be 0 or 1"),
                });
        fluid
            .update_surface(&volume, surface.field(), None)
            .unwrap();
        let probe = ModeProbe::new(&mut surface, &volume);
        let initial = probe.sample(&surface);
        let normalization = if epsilon > 0. { initial.cosine } else { 1. };
        assert!(epsilon == 0. || normalization > 0.);
        let volume_ref = PI * g.volume_radius.powi(2) * g.length;
        let path = format!("artifacts/cylinder-{cells}-kr{kr:.3}-e{epsilon:.3}.csv");
        let mut csv = String::from(
            "radius_cells,kr,step,time_s,omega_t,mode_ratio,linear_reference,cosine_mode,sine_mode,second_mode,relative_volume_drift,center_x_cells,center_y_cells,max_speed_m_s,kinetic_j,rms_divergence_s_inv,pressure_iterations,min_alpha,max_alpha,mean_radius_m,axial_length_m,dt_s,deformation,linear_kinetic_j\n",
        );
        eprintln!(
            "Cylinder R/h={cells}, dims={:?}, kR={:.8}, s²={s2:.8e}, dt={dt:.8e}, steps={steps}; initial volume error={:.6e}",
            g.dims,
            g.wavenumber * g.mean_radius,
            initial.volume / volume_ref - 1.
        );
        let started = std::time::Instant::now();
        let mut peak_k = 0f64;
        let mut max_drift = 0f64;
        let mut trace = Vec::new();
        let mode_energy = PI * g.length / (2. * g.mean_radius)
            * (1. - (g.wavenumber * g.mean_radius).powi(2))
            * (g.mean_radius * epsilon).powi(2);
        for step in 0..=steps {
            if step > 0 {
                let result = (|| -> Result<(), String> {
                    fluid.step(&volume)?;
                    surface.advance_coupled(&volume, &mut fluid, None, dt)?;
                    fluid.update_surface(&volume, surface.field(), None)?;
                    Ok(())
                })();
                if let Err(error) = result {
                    std::fs::write(&path, &csv).unwrap();
                    panic!("Cylinder failed at step {step}: {error}");
                }
            }
            let m = probe.sample(&surface);
            let t = step as f64 * dt;
            let u = omega * t;
            // At rest, the unstable linear solution is cosh(s*t); exp(s*t)
            // would require seeding its nonzero growing-mode velocity as well.
            let reference = if epsilon == 0. {
                0.
            } else if s2 > 0. {
                u.cosh()
            } else {
                u.cos()
            };
            let ratio = m.cosine / normalization;
            trace.push((u, ratio));
            let linear_kinetic = (mode_energy * (reference * reference - 1.)).max(0.);
            let drift = m.volume / initial.volume - 1.;
            max_drift = max_drift.max(drift.abs());
            peak_k = peak_k.max(fluid.diagnostics.kinetic_energy_j as f64);
            assert!((m.volume / surface.diagnostics.volume_m3 - 1.).abs() < 5e-7);
            assert!(drift.abs() < 2e-6, "Cylinder volume drift {drift}");
            csv.push_str(&format!("{cells},{},{step},{t},{u},{ratio},{reference},{},{},{},{drift},{},{},{},{},{},{},{},{},{},{},{dt},{epsilon},{linear_kinetic}\n",g.wavenumber*g.mean_radius,m.cosine,m.sine,m.second,m.center_cells[0],m.center_cells[1],fluid.diagnostics.max_speed_m_s,fluid.diagnostics.kinetic_energy_j,fluid.diagnostics.rms_divergence_s_inv,fluid.diagnostics.iterations,surface.diagnostics.min_fraction,surface.diagnostics.max_fraction,g.mean_radius,g.length));
            if step % 20 == 0 || step == steps {
                std::fs::write(&path, &csv).unwrap();
                eprintln!(
                    "Cylinder R/h={cells} {step}/{steps}, |s|t={u:.4}: mode={ratio:.7}, reference={reference:.7}, U={:.5e}, volume={drift:.4e}, pressure={}it",
                    fluid.diagnostics.max_speed_m_s, fluid.diagnostics.iterations
                );
            }
        }
        if epsilon > 0. && duration > 0.2 {
            let (rate, rms) = fit_rate(&trace, s2 > 0.);
            eprintln!(
                "Cylinder fitted rate/reference={rate:.9}, signed error={:.6}%, fixed-amplitude RMS mode residual={rms:.8e}",
                100. * (rate - 1.)
            );
        }
        eprintln!(
            "Cylinder complete R/h={cells}: wall={:.3}s, max volume drift={max_drift:.5e}, peak K={peak_k:.8e}J",
            started.elapsed().as_secs_f64()
        );
    }
}

fn fit_rate(trace: &[(f64, f64)], unstable: bool) -> (f64, f64) {
    // Fit a scalar analytic mode with its amplitude and zero-time derivative
    // fixed by initialization. No arbitrary phase/amplitude correction.
    let reference: fn(f64) -> f64 = if unstable { f64::cosh } else { f64::cos };
    let samples: Vec<_> = trace.iter().copied().filter(|(u, _)| *u >= 0.15).collect();
    assert!(!samples.is_empty());
    let error = |rate: f64| {
        samples
            .iter()
            .map(|(u, v)| (v - reference(rate * u)).powi(2))
            .sum::<f64>()
            / samples.len() as f64
    };
    let best = (0..=200)
        .map(|j| 0.5 + j as f64 / 200.)
        .min_by(|a, b| error(*a).total_cmp(&error(*b)))
        .unwrap();
    let mut lo = (best - 0.01).max(0.5);
    let mut hi = (best + 0.01).min(1.5);
    let golden = (5f64.sqrt() - 1.) / 2.;
    for _ in 0..70 {
        let a = hi - golden * (hi - lo);
        let b = lo + golden * (hi - lo);
        if error(a) < error(b) {
            hi = b;
        } else {
            lo = a;
        }
    }
    let rate = 0.5 * (lo + hi);
    (rate, error(rate).sqrt())
}
#[test]
fn cylinder_rate_fit_recovers_known_scalar_modes() {
    for (unstable, rate) in [(true, 1.02), (false, 0.98)] {
        let reference: fn(f64) -> f64 = if unstable { f64::cosh } else { f64::cos };
        let trace: Vec<_> = (0..=300)
            .map(|j| {
                let u = j as f64 / 100.;
                (u, reference(rate * u))
            })
            .collect();
        let (actual, rms) = fit_rate(&trace, unstable);
        assert!((actual - rate).abs() < 1e-8 && rms < 1e-8);
    }
}

// Weighted regression removes the correlation between mixed-cell weights and
// axial position. A raw Fourier sum of curvature would alias its mean into
// the small physical harmonic when the number of mixed cells varies with z.
fn curvature_fit(v: [f32; 8]) -> [f64; 4] {
    let v = v.map(f64::from);
    let denominator = v[2] - v[1] * v[1] / v[0];
    assert!(denominator > 0.);
    let slope = |sum: f64, cosine_sum: f64| (cosine_sum - v[1] * sum / v[0]) / denominator;
    [
        v[3] / v[0],
        slope(v[3], v[4]),
        slope(v[5], v[6]),
        (v[7] / v[0]).sqrt(),
    ]
}
#[test]
fn cylinder_curvature_regression_removes_weight_bias() {
    // Unequal samples and weights: y=10+2*cos(theta), reference=10+3*cos(theta).
    let mut sums = [0.; 8];
    for (c, w) in [(-1., 1.), (-0.2, 3.), (0.3, 2.), (1., 4.)] {
        let y = 10. + 2. * c;
        let r = 10. + 3. * c;
        let values = [
            w,
            w * c,
            w * c * c,
            w * y,
            w * y * c,
            w * r,
            w * r * c,
            w * (y - r) * (y - r),
        ];
        for (sum, value) in sums.iter_mut().zip(values) {
            *sum += value;
        }
    }
    let fit = curvature_fit(sums);
    assert!(
        (fit[1] - 2.).abs() < 2e-6 && (fit[2] - 3.).abs() < 2e-6,
        "{fit:?}"
    );
}
fn curvature_probe(surface: &mut SurfaceState, volume: &Volume, g: &Geometry) -> ModeProbe {
    let source = format!(
        "const CYLINDER_RADIUS:f32={};const CYLINDER_EPSILON:f32={};const CYLINDER_K:f32={};\n{}",
        g.mean_radius,
        g.epsilon,
        g.wavenumber,
        include_str!("cylinder_curvature_probes.wgsl")
    );
    ModeProbe::with_source(surface, volume, &source)
}
#[test]
fn cylinder_curvature_gpu_reduction_matches_readback() {
    let _lock = crate::gpu_test_lock();
    let (_gpu, volume, mut surface, g) = setup(4, 0.7, 0.01);
    let probe = curvature_probe(&mut surface, &volume, &g);
    let capture = |s: &SurfaceState| {
        [
            &s.fields[0],
            &s.fields[1],
            &s.normals,
            &s.stats,
            &s.checkpoint,
        ]
        .iter()
        .map(|b| read_bytes(&s.context, b, 0, b.size()).unwrap())
        .collect::<Vec<_>>()
    };
    let before = capture(&surface);
    let raw = probe.raw_sample(&surface);
    assert_eq!(before, capture(&surface));
    let fields = bytemuck::cast_slice::<u8, [f32; 4]>(&before[surface.current]);
    let mut expected = [0.; 8];
    for (i, field) in fields.iter().enumerate() {
        let alpha = f64::from(field[0]);
        if alpha <= 1e-6 || alpha >= 1. - 1e-6 {
            continue;
        }
        let z = i as u32 / (g.dims[0] * g.dims[1]);
        let phase = 2. * PI * (z as f64 + 0.5) / g.dims[2] as f64;
        let c = phase.cos();
        let r = g.mean_radius * (1. + g.epsilon * c);
        let rz = -g.mean_radius * g.epsilon * g.wavenumber * phase.sin();
        let rzz = -g.mean_radius * g.epsilon * g.wavenumber.powi(2) * c;
        let reference = 1. / (r * (1. + rz * rz).sqrt()) - rzz / (1. + rz * rz).powf(1.5);
        let kappa = f64::from(field[1]);
        let w = alpha * (1. - alpha);
        let values = [
            1.,
            c,
            c * c,
            kappa,
            kappa * c,
            reference,
            reference * c,
            (kappa - reference).powi(2),
        ];
        for (sum, value) in expected.iter_mut().zip(values) {
            *sum += w * value;
        }
    }
    for (i, (&actual, &expected)) in raw.iter().zip(expected.iter()).enumerate() {
        assert!(
            (f64::from(actual) - expected).abs() < 2e-5 * expected.abs().max(1.),
            "moment {i}: {actual} vs {expected}"
        );
    }
}
#[test]
#[ignore = "Initial cylinder curvature harmonic diagnostic; writes CSV"]
fn cylinder_initial_curvature_diagnostic() {
    let _lock = crate::gpu_test_lock();
    let mut csv = String::from(
        "radius_cells,kr,mean_curvature,curvature_harmonic,reference_harmonic,harmonic_relative_error,curvature_rms_error\n",
    );
    for cells in [8, 10, 12, 16] {
        let (_gpu, volume, mut surface, g) = setup(cells, 0.7, 0.01);
        let probe = curvature_probe(&mut surface, &volume, &g);
        let before =
            read_bytes(&surface.context, surface.field(), 0, surface.field().size()).unwrap();
        let fit = curvature_fit(probe.raw_sample(&surface));
        assert_eq!(
            before,
            read_bytes(&surface.context, surface.field(), 0, surface.field().size()).unwrap()
        );
        eprintln!(
            "Cylinder initial curvature R/h={cells}: {fit:?}, harmonic error={:.5}%",
            100. * (fit[1] / fit[2] - 1.)
        );
        csv.push_str(&format!(
            "{cells},{},{},{},{},{},{}\n",
            g.mean_radius * g.wavenumber,
            fit[0],
            fit[1],
            fit[2],
            fit[1] / fit[2] - 1.,
            fit[3]
        ));
    }
    std::fs::create_dir_all("artifacts").unwrap();
    std::fs::write("artifacts/cylinder-initial-curvature.csv", csv).unwrap();
}
