use super::conduction::Conduction;
use mhd_workbench::{em::CellField, gpu::GpuContext};
use wgpu::util::DeviceExt;

#[test]
fn dimensional_motion_gate_preserves_small_current_scaling() {
    let context = GpuContext::headless().unwrap();
    let dims = [16, 8, 3];
    let n = dims.iter().product::<u32>() as usize;
    let h = 1e-4;
    let cells: Vec<_> = (0..n)
        .map(|i| {
            [
                2.2,
                if i % 16 == 0 {
                    0.5
                } else if i % 16 == 15 {
                    -0.5
                } else {
                    3.
                },
            ]
        })
        .collect();
    let fields = vec![
        CellField {
            b: [0., 0.5, 0., 0.],
            ..Default::default()
        };
        n
    ];
    let bias = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shear-flow magnetic bias"),
            contents: bytemuck::cast_slice(&fields),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let mut norms = Vec::new();
    for scale in [1_f32, 1e-6] {
        let flow: Vec<_> = (0..n)
            .map(|i| CellField {
                b: [
                    0.,
                    0.,
                    scale * (std::f32::consts::TAU * ((i / 16) % 8) as f32 / 8.).sin(),
                    0.,
                ],
                applied: [0., 0., 1., 0.],
                ..Default::default()
            })
            .collect();
        let velocity = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Scaled shear velocity and unchanged liquid fraction"),
                contents: bytemuck::cast_slice(&flow),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let mut solver = Conduction::new(context.clone(), dims, h, &cells, &bias).unwrap();
        solver.set_relative_tolerance(1e-6).unwrap();
        solver
            .set_motional_boundary_absolute_tolerance_a(1e-6)
            .unwrap();
        solver.attach_flow(&velocity).unwrap();
        let (_, stats) = solver.solve_current(0.).unwrap();
        let output = solver.read_fields().unwrap();
        let j2: f64 = output
            .iter()
            .map(|f| f.j[..3].iter().map(|v| f64::from(*v).powi(2)).sum::<f64>())
            .sum();
        assert!(j2 > 0. && stats[2] > 0.);
        assert!(solver.unit_boundary_current_defect <= 1e-5);
        assert!(solver.relative_residual <= 1e-6);
        norms.push((
            j2.sqrt() / f64::from(scale),
            f64::from(stats[2]) / f64::from(scale).powi(2),
        ));
    }
    assert!((norms[1].0 / norms[0].0 - 1.).abs() < 5e-4, "{norms:?}");
    assert!((norms[1].1 / norms[0].1 - 1.).abs() < 5e-4, "{norms:?}");
}

#[test]
fn near_insulating_bridge_resolves_tiny_copper_terminal_gradients() {
    let context = GpuContext::headless().unwrap();
    let dims = [128, 1, 1];
    let h = 1e-4;
    let cells: Vec<_> = (0..128)
        .map(|x| {
            [
                if x == 63 { 1e-6 } else { 25. },
                if x == 0 {
                    0.5
                } else if x == 127 {
                    -0.5
                } else {
                    2.
                },
            ]
        })
        .collect();
    let bias = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Zero field for near-insulating bridge identity"),
            contents: bytemuck::cast_slice(&vec![CellField::default(); 128]),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let mut solver = Conduction::new(context, dims, h, &cells, &bias).unwrap();
    solver.set_relative_tolerance(1e-6).unwrap();
    solver.solve().unwrap();
    let stats = solver.fields(1.).unwrap();
    // Exact series face resistances, including half-cells at both terminals.
    let resistance: f64 = cells
        .windows(2)
        .map(|p| {
            0.5 / (f64::from(p[0][0]) * 1e6 * f64::from(h))
                + 0.5 / (f64::from(p[1][0]) * 1e6 * f64::from(h))
        })
        .sum();
    let expected = 1. / resistance;
    assert!(
        (f64::from(stats[0]) / expected - 1.).abs() < 1e-5,
        "{stats:?}, expected{expected}"
    );
    assert!(solver.boundary_current_defect <= 1e-5);
    assert!((stats[2] / stats[0] - 1.).abs() < 2e-5);
}

#[test]
fn long_bottlenecked_parallel_paths_conserve_current_and_match_series_resistance() {
    let context = GpuContext::headless().unwrap();
    let dims = [256, 32, 4];
    let h = 1e-4;
    let n = dims.iter().product::<u32>() as usize;
    let bias = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Bottleneck zero field"),
            contents: bytemuck::cast_slice(&vec![CellField::default(); n]),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let mut cells = vec![[0., 2.]; n];
    for (i, cell) in cells.iter_mut().enumerate() {
        let x = i as u32 % dims[0];
        let y = (i as u32 / dims[0]) % dims[1];
        if y >= 4 && y < 8 {
            continue;
        }
        if x < 8 || x >= 248 {
            *cell = [25., if x < 8 { 0.5 } else { -0.5 }];
        } else if y < 4 {
            *cell = [2.2, 3.];
        } else {
            *cell = [if x <= 10 || x == 247 { 1.1 } else { 25. }, 2.];
        }
    }
    let mut solver = Conduction::new(context, dims, h, &cells, &bias).unwrap();
    solver.set_relative_tolerance(2e-6).unwrap();
    solver.set_max_iterations(32).unwrap();
    assert!(solver.solve().is_err(), "A short work budget must not waive convergence");
    assert_eq!(solver.iterations, 32);
    solver.set_max_iterations(12000).unwrap();
    solver.solve().unwrap();
    let stats = solver.fields(1.).unwrap();
    // Harmonic faces give half-cell endpoint resistances and full unknown
    // cells in series. The two paths are separated by insulating rows.
    let liquid_area = 4. * 4. * h * h;
    let metal_area = 24. * 4. * h * h;
    let liquid_r = h * (240. / 2.2e6 + 1. / 25e6) / liquid_area;
    let metal_r = h * (4. / 1.1e6 + 237. / 25e6) / metal_area;
    let exact_current = 1. / liquid_r + 1. / metal_r;
    assert!(
        (stats[0] / exact_current - 1.).abs() < 1e-4,
        "Current {} versus analytical {}",
        stats[0],
        exact_current
    );
    assert!((stats[1] / stats[0] + 1.).abs() < 3e-5);
    assert!((stats[2] / stats[0] - 1.).abs() < 5e-5);
    assert!(solver.boundary_current_defect <= 1e-5);
    let phi = solver.read_scalar_solution().unwrap();
    let expected_phi = 0.5 - (4.5 / 2.2e6 + 0.5 / 25e6) / (240. / 2.2e6 + 1. / 25e6);
    assert!(
        (phi[12 + 256 + 256 * 32] - expected_phi).abs() < 1e-6,
        "Scalar diagnostics must reconstruct both stored potential words"
    );
}

#[test]
fn potential_predictors_reuse_only_solutions_that_pass_the_current_equation() {
    let context = GpuContext::headless().unwrap();
    let dims = [24, 6, 4];
    let n = dims.iter().product::<u32>() as usize;
    let bias = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Predictor bias"),
            contents: bytemuck::cast_slice(&vec![
                CellField {
                    b: [0., -0.5, 0., 0.],
                    ..Default::default()
                };
                n
            ]),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let mut phase = vec![
        CellField {
            applied: [0., 0., 1., 0.],
            ..Default::default()
        };
        n
    ];
    let flow = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Changing phase and velocity"),
            contents: bytemuck::cast_slice(&phase),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
    let mut cells = vec![[1., 3.]; n];
    for (i, c) in cells.iter_mut().enumerate() {
        let x = i as u32 % dims[0];
        if x == 0 {
            c[1] = 0.5;
        }
        if x == dims[0] - 1 {
            c[1] = -0.5;
        }
    }
    let mut warm = Conduction::new(context.clone(), dims, 1e-4, &cells, &bias).unwrap();
    warm.attach_flow(&flow).unwrap();
    warm.solve_current(40.).unwrap();
    warm.solve_current(40.).unwrap();
    assert_eq!(
        warm.iterations, 0,
        "Unchanged accepted potentials should need no new PCG iterations"
    );
    for (i, f) in phase.iter_mut().enumerate() {
        let x = (i as u32 % dims[0]) as f32;
        let y = (i as u32 / dims[0] % dims[1]) as f32;
        f.b[2] = 0.7 * (x * 0.27).sin() * (y * 0.6).cos();
        f.applied[2] = 0.6 + 0.3 * (x * 0.19 + y * 0.23).cos();
    }
    context
        .queue
        .write_buffer(&flow, 0, bytemuck::cast_slice(&phase));
    let (warm_v, _) = warm.solve_current(0.).unwrap();
    let warm_fields = warm.read_fields().unwrap();
    let mut cold = Conduction::new(context.clone(), dims, 1e-4, &cells, &bias).unwrap();
    cold.attach_flow(&flow).unwrap();
    let (cold_v, _) = cold.solve_current(0.).unwrap();
    let cold_fields = cold.read_fields().unwrap();
    let norm: f64 = cold_fields
        .iter()
        .flat_map(|f| f.j[..3].iter())
        .map(|&v| (v as f64).powi(2))
        .sum();
    let delta: f64 = warm_fields
        .iter()
        .zip(cold_fields)
        .flat_map(|(a, b)| (0..3).map(move |k| (a.j[k] as f64 - b.j[k] as f64).powi(2)))
        .sum();
    assert!(
        (delta / norm).sqrt() < 1e-3,
        "Predictor altered the physical current: {}",
        (delta / norm).sqrt()
    );
    assert!((warm_v - cold_v).abs() < 1e-7);
    assert!(warm.relative_residual <= 1e-5 && cold.relative_residual <= 1e-5);
}
#[test]
fn uniform_and_parallel_conductors_obey_ohms_law_and_power_balance() {
    let context = GpuContext::headless().unwrap();
    let dims = [32, 4, 4];
    let h = 1e-4;
    let n = dims.iter().product::<u32>() as usize;
    let bias = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Test static field"),
            contents: bytemuck::cast_slice(&vec![
                CellField {
                    b: [0., -0.5, 0., 0.],
                    ..Default::default()
                };
                n
            ]),
            usage: wgpu::BufferUsages::STORAGE,
        });
    for parallel in [false, true] {
        let mut cells = vec![[1., 2.]; n];
        for (i, c) in cells.iter_mut().enumerate() {
            let x = i as u32 % dims[0];
            let y = (i as u32 / dims[0]) % dims[1];
            if parallel && y >= 2 {
                c[0] = 2.;
            }
            if x == 0 {
                c[1] = 0.5;
            }
            if x == 31 {
                c[1] = -0.5;
            }
        }
        let mut solver = Conduction::new(context.clone(), dims, h, &cells, &bias).unwrap();
        solver.solve().unwrap();
        let s = solver.fields(1.).unwrap();
        let mean_sigma = if parallel { 1.5e6 } else { 1e6 };
        let exact = mean_sigma * (dims[1] * dims[2]) as f32 * h / (dims[0] - 1) as f32;
        assert!(
            (s[0] / exact - 1.).abs() < 5e-5,
            "current {} exact {}",
            s[0],
            exact
        );
        assert!((s[1] / s[0] + 1.).abs() < 5e-5);
        assert!(
            (s[2] / s[0] - 1.).abs() < 5e-5,
            "power {} current {}",
            s[2],
            s[0]
        );
        let fields = solver.read_fields().unwrap();
        if parallel {
            let low = fields[16].j[0];
            let high = fields[16 + 32 * 2].j[0];
            assert!((high / low - 2.).abs() < 1e-4);
        }
        assert!(fields[16].force_heat[2] < 0.);
    }
}
#[test]
fn contact_resistance_and_motional_emf_preserve_power_balance() {
    let context = GpuContext::headless().unwrap();
    let dims = [32, 4, 4];
    let h = 1e-4;
    let n = dims.iter().product::<u32>() as usize;
    let make_buffer = |label, values: &[CellField]| {
        context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(values),
                usage: wgpu::BufferUsages::STORAGE,
            })
    };
    let bias = make_buffer(
        "Contact/motion test bias",
        &vec![
            CellField {
                b: [0., -0.5, 0., 0.],
                ..Default::default()
            };
            n
        ],
    );
    let mut cells = vec![[1., 3.]; n];
    for (i, c) in cells.iter_mut().enumerate() {
        let x = i as u32 % dims[0];
        if x == 0 {
            c[1] = 0.5;
        }
        if x == 31 {
            c[1] = -0.5;
        }
    }
    let mut solver = Conduction::new(context.clone(), dims, h, &cells, &bias).unwrap();
    let contact = 1e-9;
    solver.set_contact_resistivity(contact).unwrap();
    solver.solve().unwrap();
    let area = (dims[1] * dims[2]) as f32 * h * h;
    let length = (dims[0] - 1) as f32 * h;
    let resistance = length / (1e6 * area) + 2. * contact / area;
    let s = solver.fields(1.).unwrap();
    assert!((s[0] * resistance - 1.).abs() < 5e-5);
    assert!((s[2] / s[0] - 1.).abs() < 5e-5);
    let flow = make_buffer(
        "Uniform translating conductor",
        &vec![
            CellField {
                b: [0., 0., -1., 0.],
                applied: [0., 0., 1., 0.],
                ..Default::default()
            };
            n
        ],
    );
    solver.attach_flow(&flow).unwrap();
    let (voltage, s) = solver.solve_current(0.).unwrap();
    assert!(
        (voltage / (0.5 * length) - 1.).abs() < 2e-4,
        "open-circuit motional voltage {voltage}"
    );
    assert!(
        s[2] < 1e-8,
        "Rigid translation at open circuit must not generate Ohmic heat: {}",
        s[2]
    );
    let (voltage, s) = solver.solve_current(100.).unwrap();
    let expected = 100. * resistance + 0.5 * length;
    assert!((voltage / expected - 1.).abs() < 1e-4);
    assert!((s[0] / 100. - 1.).abs() < 5e-5);
    assert!(
        ((voltage * s[0] - s[2] - s[3]) / (voltage * s[0])).abs() < 5e-5,
        "DC source/Joule/mechanical balance {s:?}"
    );
    // Contact edits must invalidate coefficients even with accepted potentials cached.
    let changed_contact = 4e-9;
    solver.set_contact_resistivity(changed_contact).unwrap();
    let (voltage, s) = solver.solve_current(100.).unwrap();
    let changed_resistance = length / (1e6 * area) + 2. * changed_contact / area;
    assert!((voltage / (100. * changed_resistance + 0.5 * length) - 1.).abs() < 1e-4);
    assert!((s[0] / 100. - 1.).abs() < 5e-5);
}

#[test]
fn detached_translating_conductor_does_not_keep_current_from_its_old_bridge() {
    let context = GpuContext::headless().unwrap();
    let dims = [32u32, 12, 8];
    let n = dims.iter().product::<u32>() as usize;
    let h = 1e-4_f32;
    let mut cells = vec![[0., 3.]; n];
    let mut phase = vec![CellField::default(); n];
    let mut island = Vec::new();
    let mut bridge = Vec::new();
    for i in 0..n {
        let x = i as u32 % dims[0];
        let y = (i as u32 / dims[0]) % dims[1];
        let z = i as u32 / (dims[0] * dims[1]);
        if y < 3 {
            cells[i] = [
                1.,
                if x == 0 {
                    0.5
                } else if x == 31 {
                    -0.5
                } else {
                    2.
                },
            ];
        } else if (2..6).contains(&z) {
            if (8..24).contains(&x) && (6..10).contains(&y) {
                cells[i][0] = 1.;
                phase[i].applied[2] = 1.;
                island.push(i);
            } else if ((8..10).contains(&x) || (22..24).contains(&x)) && y < 6 {
                cells[i][0] = 1.;
                phase[i].applied[2] = 1.;
                bridge.push(i);
            }
        }
    }
    let make = |label, data: &[CellField]| {
        context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(data),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            })
    };
    let bias = make(
        "Uniform field for detached conductor",
        &vec![
            CellField {
                b: [0., -0.5, 0., 0.],
                ..Default::default()
            };
            n
        ],
    );
    let flow = make("Bridge removal and rigid translation", &phase);
    let mut solver = Conduction::new(context.clone(), dims, h, &cells, &bias).unwrap();
    solver.attach_flow(&flow).unwrap();
    solver.solve_current(40.).unwrap();
    for i in bridge {
        phase[i].applied[2] = 0.;
    }
    for &i in &island {
        phase[i].b[2] = -1.;
    }
    context
        .queue
        .write_buffer(&flow, 0, bytemuck::cast_slice(&phase));
    solver.solve_current(40.).unwrap();
    let fields = solver.read_fields().unwrap();
    let mut j2 = 0_f64;
    let mut force = [0_f64; 3];
    for &i in &island {
        for k in 0..3 {
            j2 += f64::from(fields[i].j[k]).powi(2);
            force[k] += f64::from(fields[i].force_heat[k]) * f64::from(h).powi(3);
        }
    }
    let j_rms = (j2 / island.len() as f64).sqrt();
    let main_j = 40. / (3. * 8. * f64::from(h).powi(2));
    let net_force = force.iter().map(|v| v * v).sum::<f64>().sqrt();
    eprintln!("Disconnected translating island: J_rms={j_rms:.6e} A/m2, force={net_force:.6e} N");
    assert!(
        j_rms < main_j * 1e-4,
        "Spurious current survived bridge removal: {j_rms}"
    );
    assert!(
        net_force < 1e-6,
        "Spurious magnetic acceleration of detached conductor: {force:?}"
    );
}

#[test]
fn tagged_pair_contact_matches_series_law_and_leaves_other_interface_ideal() {
    let context = GpuContext::headless().unwrap();
    let n = 32;
    let bias = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tagged-interface analytic zero field"),
            contents: bytemuck::cast_slice(&vec![CellField::default(); n]),
            usage: wgpu::BufferUsages::STORAGE,
        });
    // All phases deliberately have identical conductivity. Selection must use
    // group tags, not conductivity values or liquid/metal flag differences.
    for (conductivity, h, resistance) in [(1e6_f32, 1e-4_f32, 5e-9_f32), (15., 1e-3, 1e-3)] {
        let cells: Vec<_> = (0..n)
            .map(|i| {
                [
                    conductivity / 1e6,
                    if i == 0 {
                        0.5
                    } else if i == n - 1 {
                        -0.5
                    } else {
                        2.
                    },
                ]
            })
            .collect();
        let groups: Vec<_> = (0..n)
            .map(|i| {
                if i < 8 {
                    14
                } else if i < 16 {
                    20
                } else {
                    4
                }
            })
            .collect();
        for (a, b, r) in [(14, 20, resistance), (20, 14, resistance), (14, 20, 0.)] {
            let mut solver = Conduction::new_with_groups(
                context.clone(),
                [n as u32, 1, 1],
                h,
                &cells,
                &bias,
                Some(&groups),
            )
            .unwrap();
            solver.set_pair_resistivity(a, b, r).unwrap();
            solver.set_relative_tolerance(1e-7).unwrap();
            solver.solve().unwrap();
            let stats = solver.fields(1.).unwrap();
            let area = f64::from(h).powi(2);
            let expected_r = (n - 1) as f64 * f64::from(h) / (f64::from(conductivity) * area)
                + f64::from(r) / area;
            assert!(
                (f64::from(stats[0]) * expected_r - 1.).abs() < 2e-5,
                "{stats:?}, expectedR{expected_r}"
            );
            assert!((stats[2] / stats[0] - 1.).abs() < 2e-5, "{stats:?}");
            assert!(solver.boundary_current_defect <= 1e-5);
            let phi = solver.read_scalar_solution().unwrap();
            let face_bulk = f64::from(h) / (f64::from(conductivity) * area);
            let expected_jump = (face_bulk + f64::from(r) / area) / expected_r;
            assert!((f64::from(phi[7] - phi[8]) - expected_jump).abs() < 2e-5);
            assert!((f64::from(phi[15] - phi[16]) - face_bulk / expected_r).abs() < 2e-5);
        }
    }
    let cells = vec![[1., 2.]; n];
    let mut cells = cells;
    cells[0][1] = 0.5;
    cells[n - 1][1] = -0.5;
    let mut untagged = Conduction::new(context, [n as u32, 1, 1], 1e-4, &cells, &bias).unwrap();
    assert!(untagged.set_pair_resistivity(14, 20, 1e-9).is_err());
}
