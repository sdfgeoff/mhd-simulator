//! TEAM 6 reference: Emson (1988), hollow sphere in a uniform harmonic field.
//! This is a closed-form radial reference, not a second numerical EM solver.
use super::*;
use num_complex::Complex64 as C;
struct Reference {
    k: C,
    c: [C; 4],
    inner: f64,
}
impl Reference {
    fn new() -> Self {
        let inner = 50. / 55.;
        let k = C::new(
            0.,
            -std::f64::consts::TAU * 50. * (4e-7 * std::f64::consts::PI) * 5e8,
        )
        .sqrt()
            * 0.055;
        let mut s = Self {
            k,
            c: [C::new(0., 0.); 4],
            inner,
        };
        let p = s.basis(inner, true);
        let q = s.basis(inner, false);
        let po = s.basis(1., true);
        let qo = s.basis(1., false);
        let z = C::new(0., 0.);
        let o = C::new(1., 0.);
        let mut matrix = [
            [-o * inner, p, q, z, z],
            [
                -o * 2.,
                p * s.log_derivative(inner, true),
                q * s.log_derivative(inner, false),
                z,
                z,
            ],
            [z, po, qo, -o, o],
            [
                z,
                po * s.log_derivative(1., true),
                qo * s.log_derivative(1., false),
                o,
                o * 2.,
            ],
        ];
        for i in 0..4 {
            let row = (i..4)
                .max_by(|&a, &b| matrix[a][i].norm().total_cmp(&matrix[b][i].norm()))
                .unwrap();
            matrix.swap(i, row);
            let pivot = matrix[i][i];
            for j in i..5 {
                matrix[i][j] /= pivot;
            }
            let normalized = matrix[i];
            for r in 0..4 {
                if r == i {
                    continue;
                }
                let scale = matrix[r][i];
                for j in i..5 {
                    matrix[r][j] -= scale * normalized[j];
                }
            }
        }
        s.c = std::array::from_fn(|i| matrix[i][4]);
        s
    }
    // Scaled spherical Hankel functions avoid cancellation of J/Y at high skin depth ratios.
    fn basis(&self, r: f64, plus: bool) -> C {
        let i = C::new(0., 1.);
        if plus {
            (i * self.k * (r - 1.)).exp() * (self.k * r + i) / (r * r * (self.k + i))
        } else {
            (-i * self.k * (r - self.inner)).exp() * (self.k * r - i) * self.inner.powi(2)
                / (r * r * (self.k * self.inner - i))
        }
    }
    fn log_derivative(&self, r: f64, plus: bool) -> C {
        let i = C::new(0., 1.);
        if plus {
            i * self.k + self.k / (self.k * r + i) - 1. / r
        } else {
            -i * self.k + self.k / (self.k * r - i) - 1. / r
        }
    }
    fn factor(&self, r: f64) -> C {
        let r = r / 0.055;
        if r < self.inner {
            self.c[0]
        } else if r <= 1. {
            (self.c[1] * self.basis(r, true) + self.c[2] * self.basis(r, false)) / r
        } else {
            C::new(1., 0.) + self.c[3] / r.powi(3)
        }
    }
    fn reaction_a(&self, p: [f64; 3], axis: usize) -> C {
        let r = p.iter().map(|x| x * x).sum::<f64>().sqrt();
        let value = match axis {
            0 => -p[1] * 0.5,
            1 => p[0] * 0.5,
            _ => 0.,
        };
        (self.factor(r) - 1.) * value
    }
    fn b(&self, p: [f64; 3]) -> [C; 3] {
        let r = p.iter().map(|x| x * x).sum::<f64>().sqrt();
        if r < 1e-12 {
            return [C::new(0., 0.), C::new(0., 0.), self.c[0]];
        }
        let rho = r / 0.055;
        let f = self.factor(r);
        let hlog = if rho < self.inner {
            self.c[0] * 2.
        } else if rho <= 1. {
            self.c[1] * self.basis(rho, true) * self.log_derivative(rho, true)
                + self.c[2] * self.basis(rho, false) * self.log_derivative(rho, false)
        } else {
            C::new(2., 0.) - self.c[3] / rho.powi(3)
        };
        std::array::from_fn(|i| {
            0.5 * (if i == 2 { hlog } else { C::new(0., 0.) }
                - (hlog - 2. * f) * (p[2] * p[i] / (r * r)))
        })
    }
    fn j(&self, p: [f64; 3]) -> [C; 3] {
        let r = p.iter().map(|x| x * x).sum::<f64>().sqrt();
        let f = self.factor(r);
        let a = [f * (-p[1] * 0.5), f * (p[0] * 0.5), C::new(0., 0.)];
        a.map(|a| -C::new(0., std::f64::consts::TAU * 50. * 5e8) * a)
    }
}
fn edge_data(state: &EmState, i: u32) -> ([f64; 3], usize, bool) {
    let mut q = i;
    let mut axis = 0usize;
    if q >= state.params.edges[0] {
        q -= state.params.edges[0];
        axis = 1;
        if q >= state.params.edges[1] {
            q -= state.params.edges[1];
            axis = 2;
        }
    }
    let mut dims = state.grid.dims.map(|v| v + 1);
    dims[axis] -= 1;
    let coordinate = [
        q % dims[0],
        (q / dims[0]) % dims[1],
        q / (dims[0] * dims[1]),
    ];
    let position = std::array::from_fn(|a| {
        state.grid.origin_m[a] as f64
            + (coordinate[a] as f64 + if a == axis { 0.5 } else { 0. }) * state.grid.cell_m as f64
    });
    let boundary =
        (0..3).any(|a| a != axis && (coordinate[a] == 0 || coordinate[a] == state.grid.dims[a]));
    (position, axis, boundary)
}
#[test]
#[ignore = "Published 3D TEAM6 refinement benchmark; run explicitly, writes artifacts/team6.csv"]
fn published_team6_hollow_sphere() {
    let _lock = crate::gpu_test_lock();
    let reference = Reference::new();
    eprintln!(
        "TEAM6 analytic centre phasor {:?}, amplitude {} phase {}",
        reference.c[0],
        reference.c[0].norm(),
        reference.c[0].arg()
    );
    std::fs::create_dir_all("artifacts").unwrap();
    let mut csv = String::from("n,time_s,centre_bz_t,shell_jy_a_m2,iterations,residual\n");
    let grids = std::env::var("MHD_TEAM6_GRIDS")
        .unwrap_or("64,96".into())
        .split(',')
        .map(|s| s.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    for n in grids {
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let mut model = Model::default();
        let mut triangles = super::tests::sphere(55., 96, 48);
        triangles.extend(
            super::tests::sphere(50., 96, 48)
                .into_iter()
                .map(|[a, b, c]| [a, c, b]),
        );
        let mut body = crate::model::Body::new("TEAM6 shell".into(), triangles).unwrap();
        body.simulation.material_id = 6;
        model.bodies.push(body);
        let mut materials = crate::simulation::default_materials();
        materials[6].conductivity_s_m = Some(5e8);
        materials[6].relative_permeability = Some(1.);
        let settings = crate::simulation::DomainSettings {
            min_mm: [-72.; 3],
            max_mm: [72.; 3],
            cell_mm: 144. / n as f32,
        };
        let volume = gpu.build(&model, &settings, &materials).unwrap();
        let steps = 96u32;
        let period = 1. / 50.;
        let cfg = EmConfig {
            uniform: vec![UniformField {
                tesla: [0., 0., 1.],
                scale: Waveform::Sine {
                    amplitude: 1.,
                    frequency_hz: 50.,
                    phase_rad: std::f64::consts::FRAC_PI_2,
                },
            }],
            dt_s: period / steps as f64,
            max_iterations: 2500,
            relative_tolerance: 3e-5,
            ..Default::default()
        };
        let mut state =
            EmState::new(gpu.context.clone(), &volume, &model, cfg, Mode::Induction).unwrap();
        let mut initial = vec![[0f32; 4]; state.params.edges[3] as usize];
        let mut boundary = Vec::new();
        for i in 0..state.params.edges[3] {
            let (p, a, on_boundary) = edge_data(&state, i);
            let phasor = reference.reaction_a(p, a);
            initial[i as usize][0] = phasor.re as f32;
            if on_boundary {
                boundary.push((i as usize, phasor));
            }
        }
        state
            .context
            .queue
            .write_buffer(&state._work, 0, bytemuck::cast_slice(&initial));
        drop(initial);
        let mut boundary_values = vec![0f32; state.params.edges[3] as usize];
        let mut centre = C::new(0., 0.);
        let mut jy = C::new(0., 0.);
        let mut heat_mean = 0.;
        let mut force_mean = 0.;
        for step in 1..=steps * 2 {
            let t = step as f64 * state.config.dt_s;
            let phase = C::from_polar(1., std::f64::consts::TAU * 50. * t);
            for &(i, a) in &boundary {
                boundary_values[i] = (a * phase).re as f32;
            }
            state.context.queue.write_buffer(
                &state._previous,
                0,
                bytemuck::cast_slice(&boundary_values),
            );
            state.dispatch(&[("set_boundary", state.edge_groups())]);
            state.step(&model, &volume).unwrap();
            let b = state.probe([0.; 3]).unwrap().unwrap().b[2];
            let sample = state.probe([52.5, 0., 0.]).unwrap().unwrap();
            let j = sample.j[1];
            csv.push_str(&format!(
                "{n},{t},{b},{j},{},{}\n",
                state.diagnostics.iterations, state.diagnostics.relative_residual
            ));
            if step > steps {
                centre += phase.conj() * b as f64 * (2. / steps as f64);
                jy += phase.conj() * j as f64 * (2. / steps as f64);
                heat_mean += sample.force_heat[3] as f64 / steps as f64;
                force_mean += sample.force_heat[0] as f64 / steps as f64;
            }
            if step % 24 == 0 {
                eprintln!(
                    "TEAM6 n={n} step {step}/{}: {} iterations, {:.1}ms",
                    steps * 2,
                    state.diagnostics.iterations,
                    state.diagnostics.elapsed_ms
                );
            }
        }
        let point_mm = [52.5, 0., 0.];
        let point = std::array::from_fn(|i| {
            let c = ((point_mm[i] * 1e-3 - state.grid.origin_m[i] as f64)
                / state.grid.cell_m as f64)
                .floor();
            state.grid.origin_m[i] as f64 + (c + 0.5) * state.grid.cell_m as f64
        });
        let j_reference = reference.j(point);
        let b_reference = reference.b(point);
        let expected_j = j_reference[1];
        let expected_heat = j_reference.iter().map(|v| v.norm_sqr()).sum::<f64>() / (2. * 5e8);
        let expected_force = 0.5
            * (j_reference[1] * b_reference[2].conj() - j_reference[2] * b_reference[1].conj()).re;
        let heat_error = (heat_mean / expected_heat - 1.).abs();
        let force_error = (force_mean / expected_force - 1.).abs();
        eprintln!(
            "TEAM6 n={n} mean heat {heat_mean} expected {expected_heat}: {:.2}% error; mean Fx {force_mean} expected {expected_force}: {:.2}% error",
            heat_error * 100.,
            force_error * 100.
        );
        let b_error = (centre / reference.c[0] - 1.).norm();
        let j_error = (jy / expected_j - 1.).norm();
        eprintln!(
            "TEAM6 n={n} centre {:?} error {:.2}%; shell Jy {:?} expected {:?} error {:.2}%",
            centre,
            b_error * 100.,
            jy,
            expected_j,
            j_error * 100.
        );
        std::fs::write("artifacts/team6.csv", &csv).unwrap();
        if n >= 96 {
            assert!(heat_error < 0.4);
            assert!(force_error < 0.35);
        }
        assert!(b_error < 0.06, "TEAM6 centre complex-field error {b_error}");
        assert!(
            j_error < if n == 64 { 0.45 } else { 0.18 },
            "TEAM6 shell complex-current error {j_error}"
        );
    }
}
