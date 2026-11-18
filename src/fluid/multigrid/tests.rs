use super::*;
use crate::simulation::{DomainSettings, default_materials};

fn packets(context: &GpuContext, buffer: &wgpu::Buffer) -> Vec<[f32; 4]> {
    let bytes = read_bytes(context, buffer, 0, buffer.size()).unwrap();
    bytemuck::cast_slice(&bytes).to_vec()
}
fn fixture(d: [u32; 3], periodic: [bool; 3], walls: bool, reverse: bool) -> (FluidState, Vec<u32>) {
    let materials = default_materials();
    let (gpu, volume, _, _) = crate::surface::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: d.map(|n| 1000. * n as f32),
            cell_mm: 1000.,
        },
        &materials,
    );
    let mut state = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            periodic,
            acceleration_m_s2: [0.; 3],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    assert_eq!(state.grid.dims, d);
    let count = d.iter().product::<u32>();
    let mut mask: Vec<_> = (0..count)
        .map(|i| {
            let x = i % d[0];
            let y = (i / d[0]) % d[1];
            u32::from(!walls || (x != d[0] / 2 && !(x == 1 && y == 1)))
        })
        .collect();
    anchors(&mut mask, d, periodic);
    let mut fields = vec![FluidCell::default(); count as usize];
    for (i, field) in fields.iter_mut().enumerate() {
        let x = i as u32 % d[0];
        field.reserved[2] = f32::from((x < d[0] / 2) ^ reverse);
    }
    state.params.surface[0] = 1.;
    state.params.surface[1] = 1.;
    state.params.physical[3] = 7000.;
    let queue = &state.context.queue;
    queue.write_buffer(&state._mask, 0, bytemuck::cast_slice(&mask));
    queue.write_buffer(&state.result, 0, bytemuck::cast_slice(&fields));
    queue.write_buffer(&state._uniform, 0, bytemuck::bytes_of(&state.params));
    state.dispatch(&[("pressure_coefficients", groups(state.params.faces[3]))]);
    state
        .context
        .queue
        .write_buffer(&state.scalars, 0, bytemuck::cast_slice(&[[0f32; 4]; 2]));
    (state, mask)
}
fn parent_index(i: usize, child: [u32; 4], parent: [u32; 4]) -> usize {
    let x = i as u32 % child[0];
    let y = (i as u32 / child[0]) % child[1];
    let z = i as u32 / (child[0] * child[1]);
    (x / 2 + parent[0] * (y / 2 + parent[1] * (z / 2))) as usize
}
#[test]
fn aggregation_is_gpu_galerkin_on_odd_periodic_and_disconnected_domains() {
    let _lock = crate::gpu_test_lock();
    for (dims, periodic, walls) in [
        ([17, 13, 9], [true; 3], false),
        ([16, 12, 8], [false; 3], true),
        ([13, 1, 3], [true; 3], true),
        ([1, 1, 1], [false; 3], false),
    ] {
        for reverse in [false, true] {
            let (state, mask) = fixture(dims, periodic, walls, reverse);
            let mg = Multigrid::new(&state).unwrap();
            mg.build(&state.context);
            assert!(
                packets(&state.context, &mg.coeff)
                    .iter()
                    .flatten()
                    .all(|c| c.is_finite() && *c >= 0.)
            );
            let mut values = packets(&state.context, &mg.value);
            for level in 1..mg.levels.len() {
                let p = mg.levels[level];
                let child = mg.levels[level - 1];
                let vector: Vec<_> = (0..p.dims[3]).map(|i| (i as f32 * 0.71).sin()).collect();
                for (i, &v) in vector.iter().enumerate() {
                    values[p.meta[0] as usize + i][1] = v;
                }
                let mut work = vec![[0.; 4]; state.params.dims[3] as usize];
                for i in 0..p.child[3] as usize {
                    let v = vector[parent_index(i, p.child, p.dims)];
                    if level == 1 {
                        if mask[i] & 3 == 1 {
                            work[i][3] = v;
                            work[i][2] = v;
                        }
                    } else {
                        values[p.meta[1] as usize + i][1] = v;
                    }
                }
                state
                    .context
                    .queue
                    .write_buffer(&state._work, 0, bytemuck::cast_slice(&work));
                state
                    .context
                    .queue
                    .write_buffer(&mg.value, 0, bytemuck::cast_slice(&values));
                mg.dispatch(
                    &state.context,
                    &[(level - 1, "probe_action"), (level, "probe_action")],
                );
                let current = packets(&state.context, &mg.value);
                let fine_action = if level == 1 {
                    packets(&state.context, &state.scratch)
                } else {
                    Vec::new()
                };
                let mut restricted = vec![0f64; p.dims[3] as usize];
                for i in 0..p.child[3] as usize {
                    if level == 1 && mask[i] & 3 != 1 {
                        continue;
                    }
                    restricted[parent_index(i, p.child, p.dims)] += if level == 1 {
                        fine_action[i][3]
                    } else {
                        current[p.meta[1] as usize + i][2]
                    } as f64;
                }
                let mut error = 0f64;
                let mut norm = 0f64;
                for (i, r) in restricted.iter().enumerate() {
                    error += (current[p.meta[0] as usize + i][2] as f64 - r).powi(2);
                    norm += r * r;
                }
                let relative = (error / norm.max(1e-30)).sqrt();
                eprintln!(
                    "Galerkin {dims:?} periodic={periodic:?} reverse={reverse} level={level}: {relative:.3e}"
                );
                if relative >= 2e-6 {
                    let coefficients = packets(&state.context, &mg.coeff);
                    for (i, r) in restricted.iter().enumerate().take(30) {
                        eprintln!(
                            "cell {i}: actual={} expected={r} coeff={:?} vector={}",
                            current[p.meta[0] as usize + i][2],
                            coefficients[p.meta[0] as usize + i],
                            vector[i]
                        );
                    }
                }
                assert!(relative < 2e-6);
                if level == 1 {
                    // Compare the prototype fine action to the existing production GPU stencil.
                    state.context.queue.write_buffer(
                        &state.scalars,
                        0,
                        bytemuck::cast_slice(&[[0f32; 4]; 2]),
                    );
                    state.dispatch(&[("pressure_apply", groups(state.params.dims[3]))]);
                    let original = packets(&state.context, &state._work);
                    for (i, (&m, v)) in mask.iter().zip(&original).enumerate() {
                        if m & 3 == 1 {
                            assert!((v[3] - fine_action[i][3]).abs() < 1e-6 * (1. + v[3].abs()));
                        }
                    }
                }
                // Read diagonals remain fixed; only test vectors changed above.
                assert_eq!(child.dims, p.child);
            }
        }
    }
}
#[test]
fn v_cycle_is_positive_symmetric_and_preserves_live_lanes() {
    let _lock = crate::gpu_test_lock();
    for (dims, periodic, walls) in [
        ([17, 13, 9], [true; 3], false),
        ([16, 12, 8], [false; 3], true),
        ([13, 1, 3], [true; 3], true),
        ([3, 2, 1], [false; 3], false),
    ] {
        for reverse in [false, true] {
            let (state, mask) = fixture(dims, periodic, walls, reverse);
            let mg = Multigrid::new(&state).unwrap();
            mg.build(&state.context);
            assert!(
                packets(&state.context, &mg.coeff)
                    .iter()
                    .flatten()
                    .all(|c| c.is_finite() && *c >= 0.)
            );
            let original_fields =
                read_bytes(&state.context, &state.result, 0, state.result.size()).unwrap();
            let faces = packets(&state.context, &state.scratch);
            let count = mask.len();
            let r: Vec<_> = (0..count)
                .map(|i| {
                    if mask[i] & 3 == 1 {
                        (i as f64 * 0.17).cos() as f32
                    } else {
                        0.
                    }
                })
                .collect();
            let s: Vec<_> = (0..count)
                .map(|i| {
                    if mask[i] & 3 == 1 {
                        (i as f64 * 0.37).sin() as f32
                    } else {
                        0.
                    }
                })
                .collect();
            let apply = |rhs: &[f32]| {
                let work: Vec<_> = rhs.iter().map(|&r| [0.125, r, -0.25, 0.]).collect();
                state
                    .context
                    .queue
                    .write_buffer(&state._work, 0, bytemuck::cast_slice(&work));
                mg.apply(&state.context);
                let out = packets(&state.context, &state._work);
                for (a, b) in out.iter().zip(&work) {
                    assert_eq!(a[..3], b[..3]);
                }
                out.into_iter().map(|v| v[3]).collect::<Vec<_>>()
            };
            let mr = apply(&r);
            let ms = apply(&s);
            let dot = |a: &[f32], b: &[f32]| {
                a.iter()
                    .zip(b)
                    .map(|(&a, &b)| a as f64 * b as f64)
                    .sum::<f64>()
            };
            let rr = dot(&r, &mr);
            let ss = dot(&s, &ms);
            let symmetry = (dot(&r, &ms) - dot(&s, &mr)).abs() / (rr * ss).sqrt();
            eprintln!(
                "V-cycle {dims:?} reverse={reverse}: rMr={rr:.6e} sMs={ss:.6e} symmetry={symmetry:.3e}"
            );
            assert!(rr > 0. && ss > 0. && symmetry < 3e-6);
            let after = packets(&state.context, &state.scratch);
            for (a, b) in faces.iter().zip(after) {
                assert_eq!(a[..3], b[..3]);
            }
            assert_eq!(
                original_fields,
                read_bytes(&state.context, &state.result, 0, state.result.size()).unwrap()
            );
        }
    }
}

fn manufactured_gradient(state: &FluidState, mask: &[u32], binary: bool) -> Vec<[f32; 4]> {
    let d = state.params.dims;
    let f = state.params.faces;
    let coeff = packets(&state.context, &state.velocity);
    let potential = |q: [i32; 3]| {
        if binary {
            let x = q[0] as u32;
            return x.min(d[0] - x) as f32;
        }
        let tau = std::f64::consts::TAU;
        ((tau * (q[0] as f64 + 0.5) / d[0] as f64).cos()
            + 0.3 * (tau * (q[1] as f64 + 0.5) / d[1] as f64).sin()
            + 0.2 * (tau * (q[2] as f64 + 0.5) / d[2] as f64).cos()) as f32
    };
    let canonical = |mut q: [i32; 3]| {
        for a in 0..3 {
            if state.config.periodic[a] {
                q[a] = q[a].rem_euclid(d[a] as i32);
            }
        }
        q
    };
    let fluid = |q: [i32; 3]| {
        (0..3).all(|a| q[a] >= 0 && q[a] < d[a] as i32)
            && mask[(q[0] + d[0] as i32 * (q[1] + d[1] as i32 * q[2])) as usize] != 0
    };
    (0..f[3])
        .map(|i| {
            let q = canonical([
                (i % f[0]) as i32,
                ((i / f[0]) % f[1]) as i32,
                (i / (f[0] * f[1])) as i32,
            ]);
            let mut out = [0.; 4];
            for a in 0..3 {
                let mut l = q;
                l[a] -= 1;
                l = canonical(l);
                if fluid(l) && fluid(q) {
                    let fi = (q[0] + f[0] as i32 * (q[1] + f[1] as i32 * q[2])) as usize;
                    out[a] =
                        coeff[fi][a] * (potential(q) - potential(l)) / state.params.physical[0];
                }
            }
            out
        })
        .collect()
}
fn compare_projection(
    dims: [u32; 3],
    periodic: [bool; 3],
    walls: bool,
    reverse: bool,
    binary: bool,
) {
    let (mut state, mask) = fixture(dims, periodic, walls, reverse);
    state.config.max_iterations = 20000;
    let candidate = manufactured_gradient(&state, &mask, binary);
    let write = || {
        state
            .context
            .queue
            .write_buffer(&state.scratch, 0, bytemuck::cast_slice(&candidate))
    };
    write();
    let start = std::time::Instant::now();
    state.force_multigrid = Some(false);
    let baseline = state.project(false);
    let baseline_time = start.elapsed().as_secs_f64();
    let baseline_velocity = packets(&state.context, &state.velocity);
    // Candidate xyz is identical; the coefficient cache is rebuilt by both paths.
    state
        .context
        .queue
        .write_buffer(&state.scratch, 0, bytemuck::cast_slice(&candidate));
    state.force_multigrid = Some(true);
    let start = std::time::Instant::now();
    let prototype = state.project(false);
    let prototype_time = start.elapsed().as_secs_f64();
    let output = packets(&state.context, &state.velocity);
    let norm = candidate
        .iter()
        .flat_map(|v| &v[..3])
        .map(|v| (*v as f64).powi(2))
        .sum::<f64>();
    let error = |v: &[[f32; 4]]| {
        (v.iter()
            .flat_map(|v| &v[..3])
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>()
            / norm)
            .sqrt()
    };
    let baseline_error = if baseline.is_ok() {
        error(&baseline_velocity)
    } else {
        f64::NAN
    };
    let prototype_error = if prototype.is_ok() {
        error(&output)
    } else {
        f64::NAN
    };
    eprintln!(
        "Projection {dims:?} periodic={periodic:?} walls={walls} reverse={reverse}: Jacobi={baseline:?} {baseline_time:.6}s error={baseline_error:.3e}; MG={prototype:?} {prototype_time:.6}s error={prototype_error:.3e}"
    );
    assert!(
        baseline.is_ok() && prototype.is_ok() && baseline_error < 2e-4 && prototype_error < 2e-4
    );
}
#[test]
fn manufactured_pressure_projection_keeps_physical_acceptance() {
    let _lock = crate::gpu_test_lock();
    for (dims, periodic, walls) in [
        ([17, 13, 9], [true; 3], false),
        ([16, 12, 8], [false; 3], true),
        ([13, 1, 3], [true; 3], true),
    ] {
        for reverse in [false, true] {
            compare_projection(dims, periodic, walls, reverse, false);
        }
    }
}
#[test]
fn floating_pressure_contrast_and_disconnected_controls() {
    let _lock = crate::gpu_test_lock();
    for (dims, periodic, walls) in [
        ([17, 13, 9], [true; 3], false),
        ([16, 12, 8], [false; 3], false),
        ([13, 1, 3], [true; 3], true),
        ([16, 12, 8], [false; 3], true),
    ] {
        for reverse in [false, true] {
            for multigrid in [false, true] {
                let (mut state, mask) = fixture(dims, periodic, walls, reverse);
                let mut topology: Vec<_> = mask.iter().map(|&m| u32::from(m != 0)).collect();
                state.components = anchors(&mut topology, dims, periodic);
                state.force_multigrid = Some(multigrid);
                if state.components != 1 {
                    assert!(state.enable_floating_pressure().is_err());
                    assert!(!state.floating_pressure);
                    continue;
                }
                state.enable_floating_pressure().unwrap();
                state.config.max_iterations = 20000;
                let candidate = manufactured_gradient(&state, &mask, false);
                state.context.queue.write_buffer(
                    &state.scratch,
                    0,
                    bytemuck::cast_slice(&candidate),
                );
                let result = state.project(false);
                eprintln!(
                    "Floating contrast {dims:?} periodic={periodic:?} walls={walls} reverse={reverse} MG={multigrid}: {result:?}"
                );
                assert!(result.is_ok());
                let output = packets(&state.context, &state.velocity);
                let norm: f64 = candidate
                    .iter()
                    .flat_map(|v| &v[..3])
                    .map(|&v| (v as f64).powi(2))
                    .sum();
                let error: f64 = output
                    .iter()
                    .flat_map(|v| &v[..3])
                    .map(|&v| (v as f64).powi(2))
                    .sum();
                assert!((error / norm).sqrt() < 2e-4);
            }
        }
    }
}
#[test]
#[ignore = "GPU pressure preconditioner timing study"]
fn pressure_projection_timing() {
    let _lock = crate::gpu_test_lock();
    let ns = std::env::var("MHD_MG_GRIDS").unwrap_or_else(|_| "32,64".into());
    for n in ns.split(',').map(|s| s.parse::<u32>().unwrap()) {
        compare_projection([n; 3], [true; 3], false, false, true);
    }
}

#[test]
fn fused_coarse_solve_matches_fixed_jacobi_sequence() {
    let _lock = crate::gpu_test_lock();
    for dims in [[17, 13, 9], [3, 2, 1]] {
        let (state, mask) = fixture(dims, [true; 3], false, false);
        let mg = Multigrid::new(&state).unwrap();
        mg.build(&state.context);
        let coarsest = mg.levels.len() - 1;
        let p = mg.levels[coarsest];
        let mut values = packets(&state.context, &mg.value);
        let mut work = packets(&state.context, &state._work);
        for i in 0..p.dims[3] as usize {
            let rhs = (i as f32 * 0.17).sin();
            if coarsest == 0 {
                work[i] = [0., if mask[i] & 3 == 1 { rhs } else { 0. }, 0., 0.];
            } else {
                values[p.meta[0] as usize + i][0] = rhs;
            }
        }
        state
            .context
            .queue
            .write_buffer(&mg.value, 0, bytemuck::cast_slice(&values));
        state
            .context
            .queue
            .write_buffer(&state._work, 0, bytemuck::cast_slice(&work));
        let mut stages = vec![(coarsest, "clear")];
        for _ in 0..COARSE_SWEEPS / 2 {
            stages.extend([(coarsest, "smooth_to_temp"), (coarsest, "smooth_from_temp")]);
        }
        mg.dispatch(&state.context, &stages);
        let read = || {
            if coarsest == 0 {
                packets(&state.context, &state._work)
                    .into_iter()
                    .map(|v| v[3])
                    .collect::<Vec<_>>()
            } else {
                packets(&state.context, &mg.value)
                    [p.meta[0] as usize..(p.meta[0] + p.dims[3]) as usize]
                    .iter()
                    .map(|v| v[1])
                    .collect()
            }
        };
        let legacy = read();
        mg.dispatch(&state.context, &[(coarsest, "coarse_solve")]);
        let fused = read();
        let relative = (legacy
            .iter()
            .zip(&fused)
            .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
            .sum::<f64>()
            / legacy.iter().map(|v| (*v as f64).powi(2)).sum::<f64>())
        .sqrt();
        eprintln!("Fused coarse {dims:?}: relative difference {relative:.3e}");
        assert!(relative < 2e-6);
    }
}

#[test]
fn hierarchy_uses_exact_ceil_half_workspace_and_checked_indices() {
    for (dims, expected) in [
        ([256, 256, 256, 256u32.pow(3)], 2396736u64),
        ([33, 1, 1, 33], 34),
        ([1, 1, 1, 1], 0),
    ] {
        let (levels, cells) = hierarchy(dims, [1; 4], 0).unwrap();
        assert_eq!(cells, expected);
        assert!(levels.last().unwrap().dims[..3].iter().all(|&d| d <= 4));
        let sum = levels.iter().skip(1).map(|l| l.dims[3] as u64).sum::<u64>();
        assert_eq!(sum, cells);
    }
    assert!(hierarchy([0, 1, 1, 0], [1; 4], 0).is_err());
    assert!(hierarchy([u32::MAX, 1, 1, u32::MAX], [1; 4], 0).is_err());
    assert!(hierarchy([u32::MAX; 4], [1; 4], 0).is_err());
}

#[test]
fn optional_workspace_is_lazy_accounted_reused_and_can_be_unavailable() {
    let _lock = crate::gpu_test_lock();
    let (mut state, mask) = fixture([17, 13, 9], [true; 3], false, false);
    state.force_multigrid = Some(true);
    let base_bytes = state.bytes;
    // Zero divergence never needs the optional allocation.
    state.project(false).unwrap();
    assert!(state.multigrid.is_none() && !state.multigrid_attempted);
    assert_eq!(state.bytes, base_bytes);
    state.dispatch(&[("pressure_coefficients", groups(state.params.faces[3]))]);
    let candidate = manufactured_gradient(&state, &mask, false);
    state
        .context
        .queue
        .write_buffer(&state.scratch, 0, bytemuck::cast_slice(&candidate));
    state.project(false).unwrap();
    let mg = state.multigrid.as_ref().unwrap();
    let actual_bytes = mg.coeff.size()
        + mg.value.size()
        + mg.levels.len() as u64 * std::mem::size_of::<Level>() as u64;
    assert_eq!(state.bytes, base_bytes + actual_bytes);
    state
        .context
        .queue
        .write_buffer(&state.scratch, 0, bytemuck::cast_slice(&candidate));
    state.project(false).unwrap();
    assert_eq!(state.bytes, base_bytes + actual_bytes);
    // Exercise the cached allocation-refusal state without provoking GPU OOM.
    // The normal projection must still work with the original fine equation.
    state.multigrid = None;
    state.bytes = base_bytes;
    assert!(state.multigrid_attempted);
    state
        .context
        .queue
        .write_buffer(&state.scratch, 0, bytemuck::cast_slice(&candidate));
    state.project(false).unwrap();
    assert!(state.multigrid.is_none());
    assert_eq!(state.bytes, base_bytes);
}
