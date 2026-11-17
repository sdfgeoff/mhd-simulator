//! Test-only same-orientation EM principal-block preconditioning experiments.
use super::*;
use crate::{
    domain::GpuDomain,
    simulation::{DomainSettings, default_materials},
};

fn chebyshev_coefficients(stages: u32) -> Vec<[f64; 2]> {
    // Lower target: smallest normalized eigenvalue of a uniform 4³ air block.
    // The upper bound 2 follows diagonal dominance for every component block.
    let lower = 1. - (std::f64::consts::PI / 5.).cos();
    let theta = (2. + lower) / 2.;
    let delta = (2. - lower) / 2.;
    let sigma = theta / delta;
    let mut rho = 1. / sigma;
    let mut coefficients = vec![[1. / theta, 0.]];
    for _ in 1..stages {
        let next = 1. / (2. * sigma - rho);
        coefficients.push([2. * next / delta, next * rho]);
        rho = next;
    }
    coefficients
}

pub(super) fn install(state: &mut EmState) {
    if state.pipelines.contains_key("em_tile_cg_precondition") {
        return;
    }
    let pairs: u32 = std::env::var("MHD_EM_TILE_PAIRS")
        .unwrap_or_else(|_| "4".into())
        .parse()
        .expect("MHD_EM_TILE_PAIRS must be an integer");
    assert!(
        (1..=8).contains(&pairs),
        "MHD_EM_TILE_PAIRS must be between 1 and 8"
    );
    let chebyshev = std::env::var_os("MHD_EM_TILE_CHEBYSHEV").is_some();
    let mut coefficients = format!(
        "const EM_TILE_CHEBYSHEV:bool={chebyshev};\nfn em_tile_coeff(stage:u32)->vec2<f32>{{switch stage{{"
    );
    for (i, [alpha, beta]) in chebyshev_coefficients(2 * pairs).into_iter().enumerate() {
        coefficients.push_str(&format!(
            "case {i}u:{{return vec2<f32>({:?},{:?});}}",
            alpha as f32, beta as f32
        ));
    }
    coefficients.push_str("default:{return vec2<f32>(0.);}}}\n");
    let context = &state.context;
    let shader = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Test-only EM principal tiles"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "const EM_TILE_PAIRS:u32={pairs}u;\n{coefficients}\n{}\n{}\n{}",
                    include_str!("common.wgsl"),
                    include_str!("induction.wgsl"),
                    include_str!("tile.wgsl")
                )
                .into(),
            ),
        });
    let layout = state.pipelines["apply_operator"].get_bind_group_layout(0);
    let pl = context
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
    for name in [
        "em_tile_block_action",
        "em_tile_precondition",
        "em_tile_cg_precondition",
        "em_tile_cg_initial",
        "em_tile_search",
    ] {
        let pipeline = context
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: Some(&pl),
                module: &shader,
                entry_point: Some(name),
                compilation_options: Default::default(),
                cache: None,
            });
        state.pipelines.insert(name, pipeline);
    }
}
fn tile_groups(state: &EmState) -> [u32; 3] {
    let mut count = 0u32;
    for axis in 0..3 {
        let d: [u32; 3] =
            std::array::from_fn(|a| (state.params.dims[a] + u32::from(a != axis)).div_ceil(4));
        count += d.iter().product::<u32>();
    }
    [count.min(65535), count.div_ceil(65535), 1]
}
fn setup(dims: [u32; 3], sigma: f32, mu: f32) -> (EmState, Volume, Model) {
    let gpu = GpuDomain::new().unwrap();
    let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
    model.bodies[0].simulation.material_id = 6;
    let mut materials = default_materials();
    materials[6].conductivity_s_m = Some(sigma);
    materials[6].relative_permeability = Some(mu);
    let volume = gpu
        .build(
            &model,
            &DomainSettings {
                min_mm: dims.map(|v| -0.5 * v as f32),
                max_mm: dims.map(|v| 0.5 * v as f32),
                cell_mm: 1.,
            },
            &materials,
        )
        .unwrap();
    let mut state = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        EmConfig::default(),
        Mode::Induction,
    )
    .unwrap();
    install(&mut state);
    (state, volume, model)
}
fn decode(state: &EmState, i: usize) -> (usize, [u32; 3]) {
    let mut offset = i as u32;
    let mut axis = 0;
    while axis < 2 && offset >= state.params.edges[axis] {
        offset -= state.params.edges[axis];
        axis += 1;
    }
    let d: [u32; 3] = std::array::from_fn(|a| state.params.dims[a] + u32::from(a != axis));
    (
        axis,
        [
            offset % d[0],
            (offset / d[0]) % d[1],
            offset / (d[0] * d[1]),
        ],
    )
}
fn unconstrained(state: &EmState, axis: usize, q: [u32; 3]) -> bool {
    (0..3)
        .filter(|&a| a != axis)
        .all(|a| q[a] != 0 && q[a] != state.params.dims[a])
}
fn apply(state: &EmState, rhs: &[f32]) -> Vec<[f32; 4]> {
    let values: Vec<_> = rhs
        .iter()
        .enumerate()
        .map(|(i, &r)| [1. + i as f32 * 0.001, r, -2. - i as f32 * 0.001, 99.])
        .collect();
    state
        .context
        .queue
        .write_buffer(&state._work, 0, bytemuck::cast_slice(&values));
    state.dispatch(&[("em_tile_precondition", tile_groups(state))]);
    let bytes = read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
    let out = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes).to_vec();
    assert!(out.iter().flatten().all(|v| v.is_finite()));
    for (a, b) in values.iter().zip(&out) {
        assert_eq!(&a[..3], &b[..3]);
    }
    out
}
#[test]
fn em_tile_preconditioner_is_symmetric_positive_and_linear() {
    let _lock = crate::gpu_test_lock();
    for (dims, sigma, mu) in [
        ([17, 19, 21], 0., 1.),
        ([17, 19, 21], 1.33e6, 1.),
        ([17, 19, 21], 1.33e6, 20.),
        ([9, 7, 3], 0., 2.),
    ] {
        let (mut state, _volume, _model) = setup(dims, sigma, mu);
        for initial in [0, 1] {
            state.params.control[1] = initial;
            state
                .context
                .queue
                .write_buffer(&state.uniform, 0, bytemuck::bytes_of(&state.params));
            let n = state.params.edges[3] as usize;
            let u: Vec<_> = (0..n)
                .map(|i| {
                    let (a, q) = decode(&state, i);
                    if unconstrained(&state, a, q) {
                        (i as f32 * 0.713).sin()
                    } else {
                        0.
                    }
                })
                .collect();
            let v: Vec<_> = (0..n)
                .map(|i| {
                    let (a, q) = decode(&state, i);
                    if unconstrained(&state, a, q) {
                        (i as f32 * 0.327).cos()
                    } else {
                        0.
                    }
                })
                .collect();
            let mu = apply(&state, &u);
            let mv = apply(&state, &v);
            let uv: Vec<_> = u.iter().zip(&v).map(|(a, b)| a + b).collect();
            let muv = apply(&state, &uv);
            let dot = |a: &[f32], b: &[[f32; 4]]| {
                a.iter()
                    .zip(b)
                    .map(|(a, b)| *a as f64 * b[3] as f64)
                    .sum::<f64>()
            };
            let umu = dot(&u, &mu);
            let vmv = dot(&v, &mv);
            let umv = dot(&u, &mv);
            let vmu = dot(&v, &mu);
            let symmetry = (umv - vmu).abs() / (umu * vmv).sqrt();
            let error = muv
                .iter()
                .zip(mu.iter().zip(&mv))
                .map(|(a, (b, c))| (a[3] as f64 - b[3] as f64 - c[3] as f64).powi(2))
                .sum::<f64>();
            let norm = muv.iter().map(|a| (a[3] as f64).powi(2)).sum::<f64>();
            eprintln!(
                "EM tile dims={dims:?}, sigma={sigma}, initial={initial}: uMu={umu}, symmetry={symmetry:.5e}, linearity={:.5e}",
                (error / norm).sqrt()
            );
            assert!(umu > 0. && vmv > 0. && symmetry < 3e-6);
            assert!((error / norm).sqrt() < 2e-6);
            assert!(apply(&state, &vec![0.; n]).iter().all(|v| v[3] == 0.));
        }
    }
}

#[test]
fn em_tile_principal_action_matches_full_gpu_curl_gauge_operator() {
    let _lock = crate::gpu_test_lock();
    for (dims, sigma, permeability) in [
        ([17, 19, 21], 0., 1.),
        ([17, 19, 21], 1.33e6, 1.),
        ([17, 19, 21], 1.33e6, 20.),
        ([9, 7, 3], 0., 2.),
    ] {
        let (mut state, _volume, _model) = setup(dims, sigma, permeability);
        let n = state.params.edges[3] as usize;
        let original = read_bytes(&state.context, &state._source, 0, state._source.size()).unwrap();
        let mut full_coefficients = bytemuck::cast_slice::<u8, [f32; 4]>(&original).to_vec();
        // Reference uses the uncached full curl/gauge expression even in air.
        for value in &mut full_coefficients[n..] {
            value[3] = 0.;
        }
        for initial in [0, 1] {
            state.params.control[1] = initial;
            state
                .context
                .queue
                .write_buffer(&state.uniform, 0, bytemuck::bytes_of(&state.params));
            for axis in 0..3 {
                let d: [u32; 3] = std::array::from_fn(|a| dims[a] + u32::from(a != axis));
                for origin in [[0; 3], d.map(|v| 4 * (v / 8)), d.map(|v| 4 * ((v - 1) / 4))] {
                    let selected = |i| {
                        let (a, q) = decode(&state, i);
                        a == axis && (0..3).all(|b| q[b] / 4 == origin[b] / 4)
                    };
                    let work: Vec<_> = (0..n)
                        .map(|i| {
                            let (a, q) = decode(&state, i);
                            let z = if selected(i) && unconstrained(&state, a, q) {
                                (i as f32 * 0.17).sin() + (i as f32 * 0.31).cos()
                            } else {
                                0.
                            };
                            [0., 0., z, 0.]
                        })
                        .collect();
                    state
                        .context
                        .queue
                        .write_buffer(&state._work, 0, bytemuck::cast_slice(&work));
                    state.context.queue.write_buffer(
                        &state._source,
                        0,
                        bytemuck::cast_slice(&full_coefficients),
                    );
                    state
                        .context
                        .queue
                        .write_buffer(&state.scalars, 0, &[0; 32]);
                    state.dispatch(&[("apply_operator", state.edge_groups())]);
                    let reference =
                        read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
                    state
                        .context
                        .queue
                        .write_buffer(&state._source, 0, &original);
                    state.dispatch(&[("em_tile_block_action", tile_groups(&state))]);
                    let actual =
                        read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
                    let a = bytemuck::cast_slice::<u8, [f32; 4]>(&actual);
                    let b = bytemuck::cast_slice::<u8, [f32; 4]>(&reference);
                    let error: f64 = (0..n)
                        .filter(|&i| selected(i))
                        .map(|i| (a[i][3] as f64 - b[i][3] as f64).powi(2))
                        .sum();
                    let norm: f64 = (0..n)
                        .filter(|&i| selected(i))
                        .map(|i| (b[i][3] as f64).powi(2))
                        .sum();
                    let relative = (error / norm.max(1e-30)).sqrt();
                    assert!(
                        relative < 5e-7,
                        "dims={dims:?}, sigma={sigma}, mu={permeability}, initial={initial}, axis={axis}, origin={origin:?}: relative={relative}"
                    );
                }
            }
        }
    }
}

type Command<'a> = (&'a str, [u32; 3]);

pub(super) fn expand_commands<'a>(
    state: &EmState,
    commands: &'a [Command<'a>],
) -> std::borrow::Cow<'a, [Command<'a>]> {
    use std::borrow::Cow;
    if !state.pipelines.contains_key("em_tile_cg_precondition")
        || !commands.iter().any(|(name, _)| {
            matches!(
                *name,
                "initial_reduce" | "restart_reduce" | "beta_reduce" | "update_search"
            )
        })
    {
        return Cow::Borrowed(commands);
    }
    let mut expanded = Vec::new();
    for &(name, groups) in commands {
        if matches!(name, "initial_reduce" | "restart_reduce" | "beta_reduce") {
            expanded.push((
                if name == "beta_reduce" {
                    "em_tile_cg_precondition"
                } else {
                    "em_tile_cg_initial"
                },
                state.edge_groups(),
            ));
        }
        expanded.push((
            if name == "update_search" {
                "em_tile_search"
            } else {
                name
            },
            groups,
        ));
    }
    Cow::Owned(expanded)
}

#[test]
#[ignore = "Combined project EM preconditioner/performance probe; no desktop windows"]
fn combined_project_preconditioner_probe() {
    let _lock = crate::gpu_test_lock();
    let path = std::env::var("MHD_BENCH_PROJECT")
        .unwrap_or_else(|_| "assets/hotend-free-surface.mhd.json".into());
    let steps: u32 = std::env::var("MHD_BENCH_STEPS")
        .unwrap_or_else(|_| "3".into())
        .parse()
        .unwrap();
    let project = crate::project::load(std::path::Path::new(&path)).unwrap();
    let gpu = GpuDomain::new().unwrap();
    gpu.context
        .profiler
        .set_enabled(std::env::var("MHD_BENCH_PROFILE").as_deref() == Ok("1"));
    let volume = gpu
        .build(&project.model, &project.settings, &project.materials)
        .unwrap();
    let mut run = crate::run::RunState::new(
        gpu.context.clone(),
        &volume,
        &project.model,
        project.run,
        project.electromagnetics,
        project.fluid,
    )
    .unwrap();
    let memory = run.working_memory(&volume);
    eprintln!(
        "Combined EM tile={}, working={} bytes",
        std::env::var_os("MHD_EM_TILE").is_some(),
        memory.total()
    );
    let mut csv = String::from(
        "step,time_s,em_iterations,em_residual,max_B_T,max_u_m_s,pressure_iterations,rms_divergence_s_inv,magnetic_j,kinetic_j,thermal_j,material_volume_m3,em_balance_error_j,joule_j\n",
    );
    let mut performance = String::from(
        "step,time_s,accepted,stage,cpu_wall_ms,gpu_pass_ms,recorded_passes,total_passes\n",
    );
    std::fs::create_dir_all("artifacts").unwrap();
    for step in 0..=steps {
        if step > 0 {
            run.step(&project.model, &volume).unwrap();
        }
        let em = run.em.as_ref().expect("Probe requires an EM source");
        let velocity = run
            .fluid
            .as_ref()
            .map_or(0., |f| f.diagnostics.max_speed_m_s);
        let divergence = run
            .fluid
            .as_ref()
            .map_or(0., |f| f.diagnostics.rms_divergence_s_inv);
        let pressure = run.fluid.as_ref().map_or(0, |f| f.diagnostics.iterations);
        let material_volume = run.surface.as_ref().map_or(0., |s| s.diagnostics.volume_m3);
        let e = &run.energy;
        csv.push_str(&format!("{step},{},{},{},{},{velocity},{pressure},{divergence},{},{},{},{material_volume},{},{}\n",run.time_s,em.diagnostics.iterations,em.diagnostics.relative_residual,em.diagnostics.maxima[0],e.magnetic_j,e.kinetic_j,e.thermal_j,e.em_balance_error_j,e.joule_j));
        std::fs::write("artifacts/em-tile-combined.csv", &csv).unwrap();
        for report in gpu.context.profiler.take_completed(false) {
            performance.push_str(&report.csv_rows());
        }
        eprintln!(
            "Combined step {step}: EM {} iterations, residual {:.6e}, B={:.6e} T, U={velocity:.6e} m/s",
            em.diagnostics.iterations, em.diagnostics.relative_residual, em.diagnostics.maxima[0]
        );
    }
    for report in gpu.context.profiler.take_completed(true) {
        performance.push_str(&report.csv_rows());
    }
    std::fs::write("artifacts/em-tile-combined-performance.csv", performance).unwrap();
}

#[test]
fn em_tile_fused_dispatch_matches_independent_tiles_and_reduction() {
    let _lock = crate::gpu_test_lock();
    let (mut state, _volume, _model) = setup([17, 19, 21], 1.33e6, 20.);
    for initial in [0, 1] {
        state.params.control[1] = initial;
        state
            .context
            .queue
            .write_buffer(&state.uniform, 0, bytemuck::bytes_of(&state.params));
        let n = state.params.edges[3] as usize;
        let rhs: Vec<_> = (0..n)
            .map(|i| {
                let (a, q) = decode(&state, i);
                if unconstrained(&state, a, q) {
                    (i as f32 * 0.317).sin()
                } else {
                    0.
                }
            })
            .collect();
        let reference = apply(&state, &rhs);
        let input: Vec<_> = rhs.iter().map(|&r| [0.123, r, -0.456, 99.]).collect();
        state
            .context
            .queue
            .write_buffer(&state._work, 0, bytemuck::cast_slice(&input));
        state.dispatch(&[("em_tile_cg_precondition", state.edge_groups())]);
        let bytes = read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
        let actual = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        let error: f64 = actual
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a[3] as f64 - b[3] as f64).powi(2))
            .sum();
        let norm: f64 = reference.iter().map(|v| (v[3] as f64).powi(2)).sum();
        assert!((error / norm).sqrt() < 2e-7);
        for (a, b) in actual.iter().zip(&input) {
            assert_eq!(&a[..3], &b[..3]);
        }
        let groups = state.params.edges[3].div_ceil(256);
        let partial = read_bytes(&state.context, &state._partials, 0, groups as u64 * 16).unwrap();
        let partial = bytemuck::cast_slice::<u8, [f32; 4]>(&partial);
        let sums: [f64; 2] = std::array::from_fn(|a| partial.iter().map(|p| p[a] as f64).sum());
        let expected = [
            rhs.iter()
                .zip(&reference)
                .map(|(r, z)| *r as f64 * z[3] as f64)
                .sum::<f64>(),
            rhs.iter().map(|r| (*r as f64).powi(2)).sum::<f64>(),
        ];
        for (a, b) in sums.into_iter().zip(expected) {
            assert!((a / b - 1.).abs() < 2e-6, "{a} vs {b}");
        }
        state
            .context
            .queue
            .write_buffer(&state._work, 0, bytemuck::cast_slice(&input));
        state.dispatch(&[("em_tile_cg_initial", state.edge_groups())]);
        let bytes = read_bytes(&state.context, &state._work, 0, state._work.size()).unwrap();
        let actual = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        for ((a, b), r) in actual.iter().zip(&input).zip(&reference) {
            assert_eq!(&a[..2], &b[..2]);
            assert_eq!(a[2], a[3]);
            assert!((a[3] - r[3]).abs() < 2e-7);
        }
    }
}

fn closed_polynomial(lambda: f64, stages: u32, chebyshev: bool) -> f64 {
    if !chebyshev {
        return (1. - (1. - (2. / 3.) * lambda).powi(stages as i32)) / lambda;
    }
    let lower = 1. - (std::f64::consts::PI / 5.).cos();
    let theta = (2. + lower) / 2.;
    let delta = (2. - lower) / 2.;
    let sigma = theta / delta;
    let t = (theta - lambda) / delta;
    let numerator = if t > 1. {
        (stages as f64 * t.acosh()).cosh()
    } else {
        (stages as f64 * t.acos()).cos()
    };
    (1. - numerator / (stages as f64 * sigma.acosh()).cosh()) / lambda
}
#[test]
fn em_tile_polynomial_matches_analytic_cube_modes() {
    let _lock = crate::gpu_test_lock();
    let pairs: u32 = std::env::var("MHD_EM_TILE_PAIRS")
        .unwrap_or_else(|_| "4".into())
        .parse()
        .unwrap();
    let stages = 2 * pairs;
    let chebyshev = std::env::var_os("MHD_EM_TILE_CHEBYSHEV").is_some();
    // Scalar polynomial checks, not a second grid solver. The lower target is
    // not a guaranteed bound for material blocks: positivity must hold to zero.
    if chebyshev {
        let coefficients = chebyshev_coefficients(stages);
        for j in 0..=1000 {
            let lambda = 2. * j as f64 / 1000.;
            let mut previous = 0.;
            let mut current = 0.;
            for [alpha, beta] in &coefficients {
                let next = current + alpha * (1. - lambda * current) + beta * (current - previous);
                previous = current;
                current = next;
            }
            assert!(
                current > 0. && current.is_finite(),
                "lambda={lambda}: {current}"
            );
            if j > 0 {
                let exact = closed_polynomial(lambda, stages, true);
                assert!((current / exact - 1.).abs() < 1e-10);
            }
        }
    }
    let (state, _volume, _model) = setup([32; 3], 0., 1.);
    let n = state.params.edges[3] as usize;
    let mut rhs = vec![0.; n];
    let mut expected = vec![0.; n];
    for i in 0..n {
        let (_axis, q) = decode(&state, i);
        let tile = q.map(|x| x / 4);
        if tile.iter().all(|&k| (1..=4).contains(&k)) {
            let mode = tile.map(|k| std::f64::consts::PI * k as f64 / 5.);
            let value = (0..3)
                .map(|a| (mode[a] * (q[a] % 4 + 1) as f64).sin())
                .product::<f64>();
            let lambda = 1. - mode.iter().map(|k| k.cos()).sum::<f64>() / 3.;
            rhs[i] = value as f32;
            expected[i] = closed_polynomial(lambda, stages, chebyshev) * rhs[i] as f64 / 6.;
        }
    }
    let actual = apply(&state, &rhs);
    let error = actual
        .iter()
        .zip(&expected)
        .map(|(a, b)| (a[3] as f64 - b).powi(2))
        .sum::<f64>();
    let norm = expected.iter().map(|v| v * v).sum::<f64>();
    eprintln!(
        "EM tile 64 analytic cube modes ×3 orientations, stages={stages}, Chebyshev={chebyshev}: relative error {:.6e}",
        (error / norm).sqrt()
    );
    assert!((error / norm).sqrt() < 2e-6);
}
