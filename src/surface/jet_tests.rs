//! Exploratory nozzle experiment, using the production CAD/interface/fluid path.
use super::*;
use crate::fluid::{FluidConfig, FluidState, ForceField};
use crate::model::{Body, Model};
use glam::Vec3;
use std::f32::consts::TAU;

fn revolved(name: &str, profile: &[[f32; 2]], material: u32) -> Body {
    let segments = 96;
    let point = |ring: usize, j: usize| {
        let [r, z] = profile[ring];
        let theta = TAU * j as f32 / segments as f32;
        Vec3::new(r * theta.cos(), r * theta.sin(), z)
    };
    let mut triangles = Vec::new();
    for ring in 0..profile.len() {
        let next = (ring + 1) % profile.len();
        for j in 0..segments {
            let k = (j + 1) % segments;
            for t in [
                [point(ring, j), point(next, j), point(next, k)],
                [point(ring, j), point(next, k), point(ring, k)],
            ] {
                if (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 1e-18 {
                    triangles.push(t);
                }
            }
        }
    }
    let mut body = Body::new(name.into(), triangles).unwrap();
    body.simulation.material_id = material;
    body.simulation.temperature_c = Some(250.);
    body
}
fn model() -> Model {
    // Millimetres: an open-backed reservoir, a finite-thickness nozzle and a
    // flush initial meniscus. The liquid top touches the aperture exit at z=.15.
    Model {
        bodies: vec![
            revolved(
                "Nozzle wall",
                &[
                    [0.75, -1.],
                    [0.75, 0.15],
                    [0.15, 0.15],
                    [0.15, 0.],
                    [0.6, 0.],
                    [0.6, -1.],
                ],
                1,
            ),
            revolved(
                "Liquid pool",
                &[
                    [0., -0.75],
                    [0.6, -0.75],
                    [0.6, 0.],
                    [0.15, 0.],
                    [0.15, 0.15],
                    [0., 0.15],
                ],
                3,
            ),
        ],
        ..Default::default()
    }
}
struct Pulse {
    force: wgpu::Buffer,
    uniform: wgpu::Buffer,
    binds: [wgpu::BindGroup; 2],
    pipeline: wgpu::ComputePipeline,
    dims: [u32; 4],
    origin: [f32; 4],
    h: f32,
}
impl Pulse {
    fn new(surface: &SurfaceState, grid: &GridSi) -> Self {
        let device = &surface.context.device;
        let n = grid.dims.iter().product::<u32>();
        let force = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Prescribed jet force; not an EM solve"),
            size: n as u64 * 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Jet force pulse parameters"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Jet pulse"),
            source: wgpu::ShaderSource::Wgsl(include_str!("jet_pulse.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Jet pulse"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let binds = std::array::from_fn(|current| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: surface.fields[current].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: force.as_entire_binding(),
                    },
                ],
            })
        });
        Self {
            force,
            uniform,
            binds,
            pipeline,
            dims: [grid.dims[0], grid.dims[1], grid.dims[2], n],
            origin: [grid.origin_m[0], grid.origin_m[1], grid.origin_m[2], 0.],
            h: grid.cell_m,
        }
    }
    fn update(&self, surface: &SurfaceState, force: f32) {
        let mut bytes = bytemuck::bytes_of(&self.dims).to_vec();
        bytes.extend_from_slice(bytemuck::bytes_of(&self.origin));
        bytes.extend_from_slice(bytemuck::bytes_of(&[self.h, force, 0., 0.]));
        surface.context.queue.write_buffer(&self.uniform, 0, &bytes);
        let mut encoder = surface
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = surface.context.compute_pass(&mut encoder, None);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.binds[surface.current], &[]);
            let g = groups(self.dims[3]);
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
        surface.context.queue.submit(Some(encoder.finish()));
    }
}
// Connectivity of a thresholded VOF core is a diagnostic, not a subcell
// pinch-off model. Comparing thresholds exposes unresolved bridges/fragments.
fn core_components(fields: &[[f32; 4]], dims: [u32; 3], threshold: f32) -> (usize, f64) {
    let d = dims.map(|n| n as usize);
    assert_eq!(fields.len(), d.iter().product::<usize>());
    let mut seen = vec![false; fields.len()];
    let mut volumes = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    for seed in 0..fields.len() {
        if seen[seed] || fields[seed][0] <= threshold {
            continue;
        }
        queue.push_back(seed);
        seen[seed] = true;
        let mut volume = 0.;
        while let Some(i) = queue.pop_front() {
            volume += f64::from(fields[i][0]);
            let q = [i % d[0], (i / d[0]) % d[1], i / (d[0] * d[1])];
            for (axis, stride) in [1, d[0], d[0] * d[1]].into_iter().enumerate() {
                for forward in [false, true] {
                    if (!forward && q[axis] == 0) || (forward && q[axis] + 1 == d[axis]) {
                        continue;
                    }
                    let j = if forward { i + stride } else { i - stride };
                    if !seen[j] && fields[j][0] > threshold {
                        seen[j] = true;
                        queue.push_back(j);
                    }
                }
            }
        }
        volumes.push(volume);
    }
    let largest = volumes.iter().copied().fold(0., f64::max);
    (volumes.len(), volumes.iter().sum::<f64>() - largest)
}
#[test]
fn jet_core_connectivity_exposes_threshold_dependent_bridges() {
    let field = vec![
        [0.9, 0., 0., 0.],
        [0.2, 0., 0., 0.],
        [0.8, 0., 0., 0.],
        [0., 0., 0., 0.],
    ];
    assert_eq!(core_components(&field, [4, 1, 1], 0.01), (1, 0.));
    let (count, volume) = core_components(&field, [4, 1, 1], 0.5);
    assert_eq!(count, 2);
    assert!((volume - 0.8).abs() < 1e-7);
    // Flattened row neighbours must not wrap across transverse boundaries.
    let field = vec![[0.; 4], [1., 0., 0., 0.], [1., 0., 0., 0.], [0.; 4]];
    assert_eq!(core_components(&field, [2, 2, 1], 0.5), (2, 1.));
}
#[test]
#[ignore = "Exploratory pulsed nozzle; writes sparse snapshots and CSV"]
fn pulsed_nozzle_experiment() {
    let _lock = crate::gpu_test_lock();
    let h_mm: f32 = std::env::var("MHD_JET_CELL_MM")
        .unwrap_or_else(|_| "0.05".into())
        .parse()
        .unwrap();
    let end: f64 = std::env::var("MHD_JET_END_S")
        .unwrap_or_else(|_| "0.001".into())
        .parse()
        .unwrap();
    let peak: f32 = std::env::var("MHD_JET_FORCE_N_M3")
        .unwrap_or_else(|_| "50000000".into())
        .parse()
        .unwrap();
    let pulse_s: f64 = std::env::var("MHD_JET_PULSE_S")
        .unwrap_or_else(|_| "0.0005".into())
        .parse()
        .unwrap();
    assert!((0.00625..=0.05).contains(&h_mm) && end > 0. && pulse_s > 0. && peak >= 0.);
    let z_min_mm: f32 = std::env::var("MHD_JET_Z_MIN_MM")
        .unwrap_or_else(|_| "-2.0".into())
        .parse()
        .unwrap();
    assert!((-4.0..=-1.2).contains(&z_min_mm));
    let mut materials = crate::simulation::default_materials();
    materials[3].density_kg_m3 = Some(7000.);
    materials[3].dynamic_viscosity_pa_s = Some(0.002);
    materials[3].surface_tension_n_m = Some(0.5);
    materials[0].density_kg_m3 = Some(1.2);
    materials[0].dynamic_viscosity_pa_s = Some(1.8e-5);
    let model = model();
    let gpu = crate::domain::GpuDomain::new().unwrap();
    let volume = gpu
        .build(
            &model,
            &crate::simulation::DomainSettings {
                min_mm: [-0.9, -0.9, z_min_mm],
                max_mm: [0.9, 0.9, 1.5],
                cell_mm: h_mm,
            },
            &materials,
        )
        .unwrap();
    let grid = volume.inputs.snapshot().grid.clone();
    let h = grid.cell_m as f64;
    let (mut surface, _) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
    let initialization = std::env::var("MHD_JET_INITIALIZATION").unwrap_or_else(|_| "cad".into());
    match initialization.as_str() {
        "cad" => surface.initialize_cad(&volume, &model, None).unwrap(),
        "raw" => {
            let started = std::time::Instant::now();
            let workspace = crate::domain::fractions::initialize_raw(
                &gpu.context,
                &volume,
                &model,
                3,
                surface.field(),
                None,
            )
            .unwrap();
            surface.refresh(&volume).unwrap();
            surface.diagnostics.initial_workspace_bytes = workspace;
            surface.diagnostics.initial_sampling_ms = started.elapsed().as_secs_f64() * 1000.;
        }
        // Diagnostic control: use the same cell-centre approximation as the
        // fixed voxel walls, instead of integrating the liquid mesh volume.
        "voxel" => (),
        _ => panic!("MHD_JET_INITIALIZATION must be cad, raw or voxel"),
    }
    let pulse = Pulse::new(&surface, &grid);
    pulse.update(&surface, 0.);
    let factor: f64 = std::env::var("MHD_JET_DT_FACTOR")
        .unwrap_or_else(|_| "0.05".into())
        .parse()
        .unwrap();
    assert!(factor > 0. && factor <= 0.5);
    let steps = (end / (factor * (7001.2 * h.powi(3) / (2. * std::f64::consts::PI * 0.5)).sqrt()))
        .ceil() as u32;
    let dt = end / steps as f64;
    let mut fluid = FluidState::new_coupled(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            surface: SurfaceConfig { enabled: true },
            material_id: 3,
            dt_s: dt,
            acceleration_m_s2: [0.; 3],
            use_em_force: true,
            relative_tolerance: 1e-5,
            absolute_divergence_tolerance: 1e-3,
            max_iterations: 10000,
            ..Default::default()
        },
        Some(ForceField {
            buffer: &pulse.force,
            grid: &grid,
        }),
    )
    .unwrap();
    fluid
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    let initial = surface.diagnostics.volume_m3;
    let output =
        std::env::var("MHD_JET_OUTPUT").unwrap_or_else(|_| "artifacts/jet-experiment".into());
    let out = std::path::Path::new(&output);
    std::fs::create_dir_all(out).unwrap();
    std::fs::write(
        out.join("grid.json"),
        serde_json::to_string_pretty(&serde_json::json!({"origin_m":grid.origin_m,"cell_m":grid.cell_m,"dims":grid.dims,"dt_s":dt,"pulse_s":pulse_s,"force_peak_n_m3":peak,"initialization":initialization})).unwrap(),
    )
    .unwrap();
    let mut csv = String::from(
        "step,time_s,force_peak_n_m3,volume_m3,relative_volume_drift,max_speed_m_s,kinetic_j,pressure_iterations,rms_divergence_s_inv,expelled_m3,front_z_m,max_liquid_cell_speed_m_s,max_air_cell_speed_m_s,max_mixed_cell_speed_m_s,core_count_alpha_0_01,core_count_alpha_0_5,nonlargest_core_volume_alpha_0_5_m3,max_outer_boundary_alpha\n",
    );
    eprintln!(
        "Jet grid={:?}, h={h:.8e}, dt={dt:.8e}, steps={steps}, initial volume={initial:.8e}",
        grid.dims
    );
    for step in 0..=steps {
        let time = step as f64 * dt;
        let phase = std::f64::consts::PI * (time - 0.5 * dt) / pulse_s;
        let force = if step > 0 && time - 0.5 * dt < pulse_s {
            peak * phase.sin().powi(2) as f32
        } else {
            0.
        };
        if step > 0 {
            pulse.update(&surface, force);
            fluid
                .update_force(ForceField {
                    buffer: &pulse.force,
                    grid: &grid,
                })
                .unwrap();
            fluid
                .step(&volume)
                .unwrap_or_else(|e| panic!("Jet step {step}, t={time}: {e}"));
            surface
                .advance_coupled(&volume, &mut fluid, None, dt)
                .unwrap();
            fluid
                .update_surface(&volume, surface.field(), None)
                .unwrap();
        }
        let drift = surface.diagnostics.volume_m3 / initial - 1.;
        assert!(drift.abs() < 2e-4, "volume drift={drift}");
        if step % 20 == 0 || step == steps {
            let bytes =
                read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
            let fields = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
            let mut expelled = 0.;
            let mut front = f64::NEG_INFINITY;
            for (i, v) in fields.iter().enumerate() {
                let z = grid.origin_m[2] as f64
                    + (i as u32 / (grid.dims[0] * grid.dims[1])) as f64 * h
                    + 0.5 * h;
                if z > 0.00015 {
                    expelled += v[0] as f64 * h.powi(3);
                }
                if v[0] > 0.5 {
                    front = front.max(z);
                }
            }
            let flow_bytes = read_bytes(
                &gpu.context,
                fluid.result_buffer(),
                0,
                fluid.result_buffer().size(),
            )
            .unwrap();
            let flow = bytemuck::cast_slice::<u8, crate::fluid::FluidCell>(&flow_bytes);
            let mut phase_speed = [0f32; 3];
            let mut peak_i = 0;
            for (i, (cell, fraction)) in flow.iter().zip(fields).enumerate() {
                let speed =
                    Vec3::new(cell.velocity[0], cell.velocity[1], cell.velocity[2]).length();
                let phase = if fraction[0] > 0.99 {
                    0
                } else if fraction[0] < 0.01 {
                    1
                } else {
                    2
                };
                phase_speed[phase] = phase_speed[phase].max(speed);
                if cell.velocity[3] > flow[peak_i].velocity[3] {
                    peak_i = i;
                }
            }
            let velocities: Vec<_> = flow
                .iter()
                .map(|v| [v.velocity[0], v.velocity[1], v.velocity[2], v.pressure[0]])
                .collect();
            std::fs::write(
                out.join(format!("flow-{step:05}.bin")),
                bytemuck::cast_slice(&velocities),
            )
            .unwrap();
            eprintln!("Jet phase cell speeds {phase_speed:?}, max cell index={peak_i}");
            std::fs::write(
                out.join(format!("alpha-{step:05}.bin")),
                bytemuck::cast_slice(&fields.iter().map(|v| v[0]).collect::<Vec<_>>()),
            )
            .unwrap();
            let mut boundary_alpha = 0f32;
            for (i, v) in fields.iter().enumerate() {
                let [nx, ny, nz] = grid.dims;
                let q = [i as u32 % nx, (i as u32 / nx) % ny, i as u32 / (nx * ny)];
                if q[0] == 0
                    || q[0] + 1 == nx
                    || q[1] == 0
                    || q[1] + 1 == ny
                    || q[2] == 0
                    || q[2] + 1 == nz
                {
                    boundary_alpha = boundary_alpha.max(v[0]);
                }
            }
            let low_core = core_components(fields, grid.dims, 0.01);
            let high_core = core_components(fields, grid.dims, 0.5);
            csv.push_str(&format!(
                "{step},{time},{force},{},{drift},{},{},{},{},{expelled},{front},{},{},{},{},{},{},{}\n",
                surface.diagnostics.volume_m3,
                fluid.diagnostics.max_speed_m_s,
                fluid.diagnostics.kinetic_energy_j,
                fluid.diagnostics.iterations,
                fluid.diagnostics.rms_divergence_s_inv,
                phase_speed[0],
                phase_speed[1],
                phase_speed[2], low_core.0, high_core.0, high_core.1*h.powi(3), boundary_alpha
            ));
            std::fs::write(out.join("diagnostics.csv"), &csv).unwrap();
            assert!(
                boundary_alpha <= 0.01,
                "Jet step {step}, t={time}: liquid reached the closed outer boundary (alpha={boundary_alpha}); enlarge the domain"
            );
            eprintln!(
                "Jet {step}/{steps}, t={time:.6e}, expelled={expelled:.5e}, front={front:.5e}, U={:.5e}, pressure={}it",
                fluid.diagnostics.max_speed_m_s, fluid.diagnostics.iterations
            );
        }
    }
}

#[test]
fn jet_geometry_is_closed_and_has_expected_volume() {
    let model = model();
    for body in &model.bodies {
        assert!(body.closed() && body.consistently_oriented());
    }
    let polygon_factor = 96. * (TAU as f64 / 96.).sin() / 2.;
    let expected_liquid = polygon_factor * (0.6f64.powi(2) * 0.75 + 0.15f64.powi(2) * 0.15);
    assert!((model.bodies[1].volume_mm3() / expected_liquid - 1.).abs() < 1e-6);
}
#[test]
fn jet_force_gpu_matches_prescribed_window_and_current_fraction() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut surface, _) = super::tests::box_fixture(
        crate::simulation::DomainSettings {
            min_mm: [-0.6, -0.6, -1.],
            max_mm: [0.6, 0.6, 0.],
            cell_mm: 0.1,
        },
        &crate::simulation::default_materials(),
    );
    let grid = volume.inputs.snapshot().grid.clone();
    let pulse = Pulse::new(&surface, &grid);
    let peak = 12345.;
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0., 1.);
        t * t * (3. - 2. * t)
    };
    for current in 0..2 {
        let field: Vec<_> = (0..surface.params.dims[3])
            .map(|i| {
                [
                    if i % 3 == 0 {
                        0.
                    } else {
                        0.25 + current as f32 * 0.5
                    },
                    0.,
                    0.,
                    0.,
                ]
            })
            .collect();
        gpu.context
            .queue
            .write_buffer(&surface.fields[current], 0, bytemuck::cast_slice(&field));
        surface.current = current;
        pulse.update(&surface, peak);
        let bytes = read_bytes(&gpu.context, &pulse.force, 0, pulse.force.size()).unwrap();
        let data = bytemuck::cast_slice::<u8, crate::em::CellField>(&bytes);
        let mut nonzero = 0;
        for (i, cell) in data.iter().enumerate() {
            let [nx, ny, _] = grid.dims;
            let q = [i as u32 % nx, (i as u32 / nx) % ny, i as u32 / (nx * ny)];
            let x = std::array::from_fn::<_, 3, _>(|a| {
                grid.origin_m[a] + (q[a] as f32 + 0.5) * grid.cell_m
            });
            let window = (1. - smooth(0.00045, 0.00055, (x[0] * x[0] + x[1] * x[1]).sqrt()))
                * smooth(-0.00085, -0.00075, x[2])
                * (1. - smooth(-0.00025, -0.00015, x[2]));
            let expected = peak * field[i][0] * window;
            assert!(
                (cell.force_heat[2] - expected).abs() < peak * 2e-6,
                "{i}: {} vs {expected}",
                cell.force_heat[2]
            );
            assert_eq!(cell.b, [0.; 4]);
            assert_eq!(cell.j, [0.; 4]);
            assert_eq!(cell.applied, [0.; 4]);
            assert_eq!(cell.force_heat[0], 0.);
            assert_eq!(cell.force_heat[1], 0.);
            assert_eq!(cell.force_heat[3], 0.);
            if expected > 0. {
                nonzero += 1;
            }
        }
        assert!(nonzero > 0);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&field),
            read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap()
        );
    }
}
