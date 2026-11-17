use super::*;
use crate::{
    domain::GpuDomain,
    model::{Body, Model},
    simulation::{DomainSettings, default_materials},
    thermal::ThermalCell,
};
use glam::Vec3;
pub(crate) fn fixture(n: u32) -> (GpuDomain, Volume, SurfaceState, Volume) {
    box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &default_materials(),
    )
}
pub(crate) fn box_fixture(
    settings: DomainSettings,
    materials: &[crate::simulation::Material],
) -> (GpuDomain, Volume, SurfaceState, Volume) {
    let lo = Vec3::from(settings.min_mm);
    let hi = Vec3::from(settings.max_mm);
    let v = [
        lo,
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(hi.x, hi.y, lo.z),
        Vec3::new(lo.x, hi.y, lo.z),
        Vec3::new(lo.x, lo.y, hi.z),
        Vec3::new(hi.x, lo.y, hi.z),
        hi,
        Vec3::new(lo.x, hi.y, hi.z),
    ];
    let f = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [3, 7, 6],
        [3, 6, 2],
        [0, 4, 7],
        [0, 7, 3],
        [1, 2, 6],
        [1, 6, 5],
    ];
    let mut body = Body::new("Initial liquid".into(), f.map(|t| t.map(|i| v[i])).to_vec()).unwrap();
    body.simulation.material_id = 3;
    body.simulation.temperature_c = Some(250.);
    let model = Model {
        bodies: vec![body],
        ..Default::default()
    };
    let gpu = GpuDomain::new().unwrap();
    let volume = gpu.build(&model, &settings, materials).unwrap();
    let (surface, evolving) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
    (gpu, volume, surface, evolving)
}
pub(crate) fn sphere(n: u32, center: Vec3, r: f32) -> Vec<[f32; 4]> {
    let h = 1. / n as f32;
    let half_diagonal = 3f32.sqrt() * 0.5 * h;
    let mut values = Vec::new();
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let cell = Vec3::new(x as f32, y as f32, z as f32) * h;
                let d = (cell + Vec3::splat(0.5 * h)).distance(center);
                let mut fraction = 0.;
                if d + half_diagonal < r {
                    fraction = 1.;
                } else if d - half_diagonal < r {
                    let samples = 64;
                    for j in 0..samples {
                        for i in 0..samples {
                            let px = cell.x + (i as f32 + 0.5) * h / samples as f32 - center.x;
                            let py = cell.y + (j as f32 + 0.5) * h / samples as f32 - center.y;
                            let radius_squared = r * r - px * px - py * py;
                            if radius_squared > 0. {
                                let dz = radius_squared.sqrt();
                                let low = (center.z - dz).max(cell.z);
                                let high = (center.z + dz).min(cell.z + h);
                                fraction += (high - low).max(0.) / h;
                            }
                        }
                    }
                    fraction /= (samples * samples) as f32;
                }
                values.push([fraction, 0., 0., 0.]);
            }
        }
    }
    values
}
#[test]
fn conservative_surface_transport_moves_volume_and_enthalpy_without_changing_cad() {
    let _lock = crate::gpu_test_lock();
    let n = 24;
    let (gpu, volume, mut surface, evolving) = fixture(n);
    let values = sphere(n, Vec3::new(0.35, 0.4, 0.5), 0.14);
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
    surface.refresh(&volume).unwrap();
    let initial = surface.diagnostics.volume_m3;
    let before = read_bytes(&gpu.context, volume.material_ids(), 0, n.pow(3) as u64 * 4).unwrap();
    let fd = n + 1;
    let h = 1. / n as f32;
    let psi = |x: u32, y: u32, z: f32| {
        (std::f32::consts::PI * x as f32 * h).sin()
            * (std::f32::consts::PI * y as f32 * h).sin()
            * (std::f32::consts::PI * z * h).sin()
            * 0.03
    };
    let mut faces = vec![[0f32; 4]; fd.pow(3) as usize];
    let mut max_speed = 0f32;
    for z in 0..=n {
        for y in 0..=n {
            for x in 0..=n {
                let i = (x + fd * (y + fd * z)) as usize;
                if y < n && z < n {
                    faces[i][0] = (psi(x, y + 1, z as f32 + 0.5) - psi(x, y, z as f32 + 0.5)) / h;
                }
                if x < n && z < n {
                    faces[i][1] = -(psi(x + 1, y, z as f32 + 0.5) - psi(x, y, z as f32 + 0.5)) / h;
                }
                max_speed = max_speed.max(faces[i][0].abs()).max(faces[i][1].abs());
            }
        }
    }
    let velocity = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Divergence-free closed-box circulation"),
            contents: bytemuck::cast_slice(&faces),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let cells: Vec<_> = values
        .iter()
        .map(|a| ThermalCell {
            state: [300., 1., 20. * a[0], a[0]],
            valid: [f32::from(a[0] > 0.), 0., 0., 0.],
            ..Default::default()
        })
        .collect();
    let thermal = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Known liquid enthalpy"),
            contents: bytemuck::cast_slice(&cells),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        });
    let transported = gpu.context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Test transport destination"),
        size: thermal.size(),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let checkpoint = gpu.context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Test thermal rollback"),
        size: thermal.size(),
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let centroid = |v: &[[f32; 4]]| {
        v.iter()
            .enumerate()
            .map(|(i, a)| a[0] as f64 * (i % n as usize) as f64)
            .sum::<f64>()
            / v.iter().map(|a| a[0] as f64).sum::<f64>()
            * h as f64
    };
    let initial_x = centroid(&values);
    for _ in 0..40 {
        surface
            .advance(
                &volume,
                &velocity,
                Some(EnthalpyTransport {
                    input: &thermal,
                    output: &transported,
                    checkpoint: &checkpoint,
                }),
                0.01,
                max_speed,
            )
            .unwrap();
        let mut enc = gpu
            .context
            .device
            .create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(&transported, 0, &thermal, 0, thermal.size());
        gpu.context.queue.submit(Some(enc.finish()));
    }
    let data = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
    let final_values = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
    let heat = read_bytes(&gpu.context, &thermal, 0, thermal.size()).unwrap();
    let heat = bytemuck::cast_slice::<u8, ThermalCell>(&heat);
    let mut energy = 0f64;
    for (a, c) in final_values.iter().zip(heat) {
        energy += c.state[2] as f64 * h.powi(3) as f64;
        if a[0] > 1e-4 {
            assert!((c.state[2] / a[0] - 20.).abs() < 1e-4);
        }
    }
    eprintln!(
        "VOF drift={:.3e}, dx={:.3e}, energy error={:.3e}",
        surface.diagnostics.volume_m3 / initial - 1.,
        centroid(final_values) - initial_x,
        energy / (20. * initial) - 1.
    );
    assert!((surface.diagnostics.volume_m3 / initial - 1.).abs() < 2e-6);
    assert!((energy / (20. * initial) - 1.).abs() < 2e-6);
    assert!((centroid(final_values) - initial_x).abs() > 0.005);
    surface.update_materials(&volume, &evolving);
    assert_eq!(
        before,
        read_bytes(&gpu.context, volume.material_ids(), 0, before.len() as u64).unwrap()
    );
    let data = read_bytes(
        &gpu.context,
        evolving.material_ids(),
        0,
        before.len() as u64,
    )
    .unwrap();
    assert!(
        bytemuck::cast_slice::<u8, u32>(&data)
            .iter()
            .any(|id| id & 0x80000000 != 0)
    );
}
#[test]
fn sphere_height_curvature_refines_towards_laplace_jump() {
    let _lock = crate::gpu_test_lock();
    let mut errors = Vec::new();
    for n in [24, 48] {
        let (gpu, volume, mut s, _) = fixture(n);
        let v = sphere(n, Vec3::splat(0.5), 0.25);
        gpu.context
            .queue
            .write_buffer(s.field(), 0, bytemuck::cast_slice(&v));
        s.refresh(&volume).unwrap();
        let data = read_bytes(&gpu.context, s.field(), 0, s.field().size()).unwrap();
        let output = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
        let mut sum = 0.;
        let mut count = 0.;
        let mut squared = 0.;
        for a in output {
            if a[0] > 0.05 && a[0] < 0.95 {
                sum += a[1];
                squared += (a[1] - 8.).powi(2);
                count += 1.;
            }
        }
        let mean = sum / count;
        let error = (squared / count).sqrt() / 8.;
        errors.push(error);
        eprintln!("sphere n={n}: mean curvature={mean}, rms relative={error}");
        assert!((mean / 8. - 1.).abs() < 0.15);
    }
    assert!(errors[1] < errors[0], "{errors:?}");
}

#[test]
fn balanced_surface_force_and_reconstructed_static_drop_refine() {
    let _lock = crate::gpu_test_lock();
    let mut speeds = Vec::new();
    let mut errors = Vec::new();
    for n in [24, 48] {
        let (gpu, volume, mut s, _) = fixture(n);
        let values = sphere(n, Vec3::splat(0.5), 0.25);
        gpu.context
            .queue
            .write_buffer(s.field(), 0, bytemuck::cast_slice(&values));
        s.refresh(&volume).unwrap();
        for exact in [true, false] {
            if exact {
                let known: Vec<_> = values.iter().map(|a| [a[0], 8., 0., 0.]).collect();
                gpu.context
                    .queue
                    .write_buffer(s.field(), 0, bytemuck::cast_slice(&known));
            } else {
                s.refresh(&volume).unwrap();
            }
            let config = crate::fluid::FluidConfig {
                acceleration_m_s2: [0.; 3],
                dt_s: 0.001,
                advection: !exact,
                relative_tolerance: 1e-7,
                absolute_divergence_tolerance: 1e-7,
                max_iterations: 4000,
                surface: SurfaceConfig {
                    enabled: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut fluid =
                crate::fluid::FluidState::new(gpu.context.clone(), &volume, config, None).unwrap();
            fluid.update_surface(&volume, s.field(), None).unwrap();
            for _ in 0..5 {
                fluid.step(&volume).unwrap();
                if !exact {
                    s.advance_coupled(&volume, &mut fluid, None, 0.001).unwrap();
                    fluid.update_surface(&volume, s.field(), None).unwrap();
                }
            }
            let center = fluid.probe([500.; 3]).unwrap().unwrap().pressure[0];
            let air = fluid.probe([100.; 3]).unwrap().unwrap().pressure[0];
            let error = ((center - air) / 4. - 1.).abs();
            let speed = fluid.diagnostics.max_speed_m_s;
            eprintln!(
                "static drop n={n} exact={exact}: dp={}, error={error:.4e}, umax={speed:.4e}, iterations={}",
                center - air,
                fluid.diagnostics.iterations
            );
            if exact {
                assert!(error < 2e-3);
                assert!(speed < 1e-5);
            } else {
                errors.push(error);
                speeds.push(speed);
                assert!(error < 0.15);
            }
        }
    }
    assert!(errors[1] < errors[0], "{errors:?}");
    assert!(speeds[1] < speeds[0], "{speeds:?}");
}

#[test]
fn frozen_material_remains_in_recorded_occupancy_while_air_can_move() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut surface, _) = fixture(16);
    let values = sphere(16, Vec3::splat(0.5), 0.25);
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
    surface.refresh(&volume).unwrap();
    let thermal = vec![
        ThermalCell {
            state: [293.15, 0., 0., 1.],
            ..Default::default()
        };
        16usize.pow(3)
    ];
    let phase = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Frozen material phase"),
            contents: bytemuck::cast_slice(&thermal),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let config = crate::fluid::FluidConfig {
        surface: SurfaceConfig {
            enabled: true,
            ..Default::default()
        },
        acceleration_m_s2: [0.; 3],
        ..Default::default()
    };
    let mut fluid =
        crate::fluid::FluidState::new(gpu.context.clone(), &volume, config, None).unwrap();
    fluid
        .update_surface(&volume, surface.field(), Some(&phase))
        .unwrap();
    let data = read_bytes(
        &gpu.context,
        fluid.result_buffer(),
        0,
        fluid.result_buffer().size(),
    )
    .unwrap();
    let cells = bytemuck::cast_slice::<u8, crate::fluid::FluidCell>(&data);
    for (a, c) in values.iter().zip(cells) {
        assert_eq!(a[0], c.reserved[2]);
        if a[0] > 0.0 {
            assert_eq!(c.reserved[0], 0.);
            assert_eq!(c.velocity, [0.; 4]);
        }
    }
    assert_eq!(fluid.diagnostics.fluid_mass_kg, 0.);
    assert!(fluid.fluid_cells > 0);
}

#[test]
fn invalid_transport_restores_the_accepted_occupancy() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut s, _) = fixture(8);
    let before = read_bytes(&gpu.context, s.field(), 0, s.field().size()).unwrap();
    let volume_before = s.diagnostics.volume_m3;
    let mut faces = vec![[0f32; 4]; 9usize.pow(3)];
    faces[4 + 9 * (4 + 9 * 4)][0] = 1.;
    let velocity = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Deliberately invalid transport field"),
            contents: bytemuck::cast_slice(&faces),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let cells: Vec<ThermalCell> = (0..8usize.pow(3))
        .map(|i| ThermalCell {
            state: [300., 0.5, 50. + i as f32 * 0.1, 1.],
            valid: [1., 0.125, -7., 3.],
            budget: [9., 8., 7., 6.],
            flux: [1., 2., 3., 4.],
        })
        .collect();
    let thermal = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Accepted thermal state before rejected sweep"),
            contents: bytemuck::cast_slice(&cells),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        });
    let output = gpu.context.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: thermal.size(),
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let checkpoint = gpu.context.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: thermal.size(),
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    // Under-report speed: the independent GPU face-CFL check must reject the tick
    // even when Weymouth-Yue compression keeps a completely full cell bounded.
    assert!(
        s.advance(
            &volume,
            &velocity,
            Some(EnthalpyTransport {
                input: &thermal,
                output: &output,
                checkpoint: &checkpoint
            }),
            1.,
            0.
        )
        .is_err()
    );
    assert_eq!(
        read_bytes(&gpu.context, &thermal, 0, thermal.size()).unwrap(),
        bytemuck::cast_slice::<ThermalCell, u8>(&cells)
    );
    assert_eq!(
        before,
        read_bytes(&gpu.context, s.field(), 0, s.field().size()).unwrap()
    );
    assert_eq!(s.diagnostics.volume_m3, volume_before);
}

#[test]
fn geometric_plane_cuts_inverse_and_flux_partitions_match_analytic_volumes() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let analytic = [
        ([1., 0., 0., -0.25], 0.25),
        ([-1., 0., 0., 0.25], 0.75),
        ([1., 1., 1., -0.5], 1. / 6.),
        ([1., 1., 1., 0.], 0.5),
        ([1., 1., 1., 0.5], 5. / 6.),
        ([1., 1., 0., -0.5], 0.125),
        ([1., 1., 0., 0.], 0.5),
        ([1e-12, 1., 0., 0.125], 0.625),
        ([1e-12, 2e-12, 1., -0.1], 0.4),
        ([1e-8, 1e-8, 1e-8, -0.5e-8], 1. / 6.),
    ];
    let mut cases: Vec<[[f32; 4]; 2]> = analytic
        .iter()
        .enumerate()
        .map(|(i, (plane, _))| [*plane, [0.173, 0.17, (i % 3) as f32, 0.]])
        .collect();
    for i in 0..300 {
        let n = [
            (i as f32 * 0.2345).sin(),
            (i as f32 * 1.345 + 0.7).cos(),
            (i as f32 * 0.712 + 0.3).sin(),
        ];
        cases.push([
            [n[0], n[1], n[2], (i as f32 * 0.15).sin() * 0.4],
            [
                i as f32 / 299.,
                0.13 + 0.003 * (i % 50) as f32,
                (i % 3) as f32,
                0.,
            ],
        ]);
    }
    let input = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Analytic plane cuts"),
            contents: bytemuck::cast_slice(&cases),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let output = gpu.context.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: cases.len() as u64 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let source = format!(
        "{}\nstruct Case{{plane:vec4<f32>,info:vec4<f32>}} @group(0) @binding(0) var<storage,read> cases:array<Case>; @group(0) @binding(1) var<storage,read_write> result:array<vec4<f32>>; @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) g:vec3<u32>){{let i=g.x;if i>=arrayLength(&cases){{return;}}let c=cases[i];let n=c.plane.xyz;let d=c.plane.w;let width=c.info.y;let axis=u32(c.info.z);let fraction=cube_halfspace(n,d);let partitioned=width*slab_fraction(n,d,axis,width,1.0)+(1.0-width)*slab_fraction(n,d,axis,1.0-width,-1.0);result[i]=vec4<f32>(fraction,cube_halfspace(n,cube_intercept(n,c.info.x)),cube_halfspace(n,-d),partitioned);}}",
        include_str!("../vof.wgsl")
    );
    let shader = gpu
        .context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Geometric integration checks"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let pipeline = gpu
        .context
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
    let bind = gpu
        .context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
    let mut enc = gpu
        .context
        .device
        .create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups((cases.len() as u32).div_ceil(64), 1, 1);
    }
    gpu.context.queue.submit(Some(enc.finish()));
    let data = read_bytes(&gpu.context, &output, 0, output.size()).unwrap();
    let results = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
    for (i, (case, result)) in cases.iter().zip(results).enumerate() {
        assert!(result.iter().all(|x| x.is_finite()), "case {i}: {result:?}");
        if i < analytic.len() {
            assert!(
                (result[0] - analytic[i].1).abs() < 2e-6,
                "analytic case {i}: {result:?}"
            );
        }
        assert!(
            (result[1] - case[1][0]).abs() < 2e-6,
            "inverse case {i}: {result:?}"
        );
        assert!(
            (result[0] + result[2] - 1.).abs() < 2e-6,
            "complement case {i}"
        );
        assert!(
            (result[0] - result[3]).abs() < 3e-6,
            "flux partition case {i}: {result:?}"
        );
    }
}

#[test]
fn geometric_transport_keeps_a_translated_slab_sharp_in_both_directions() {
    let _lock = crate::gpu_test_lock();
    let n = 24;
    let (gpu, volume, mut surface, _) = fixture(n);
    let h = 1. / n as f32;
    for direction in [-1., 1.] {
        let left = 0.35;
        let right = 0.65;
        let values: Vec<[f32; 4]> = (0..n.pow(3))
            .map(|i| {
                let x = (i % n) as f32 * h;
                [((x + h).min(right) - x.max(left)).max(0.) / h, 0., 0., 0.]
            })
            .collect();
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
        surface.refresh(&volume).unwrap();
        let initial = surface.diagnostics.volume_m3;
        let fd = n + 1;
        let speed = 0.1;
        let steps = 40;
        let dt = 0.025;
        let faces: Vec<[f32; 4]> = (0..fd.pow(3))
            .map(|i| {
                let x = i % fd;
                [
                    if x > 0 && x < n {
                        direction * speed
                    } else {
                        0.
                    },
                    0.,
                    0.,
                    0.,
                ]
            })
            .collect();
        let velocity = gpu
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Constant slab translation"),
                contents: bytemuck::cast_slice(&faces),
                usage: wgpu::BufferUsages::STORAGE,
            });
        for _ in 0..steps {
            surface
                .advance(&volume, &velocity, None, dt, speed)
                .unwrap();
        }
        let data = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let values = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
        let shift = direction * speed * steps as f32 * dt as f32;
        let mut l1 = 0f64;
        let mut mixed = 0;
        for (i, v) in values.iter().enumerate() {
            let x = (i as u32 % n) as f32 * h;
            let exact = ((x + h).min(right + shift) - x.max(left + shift)).max(0.) / h;
            l1 += (v[0] - exact).abs() as f64 * h.powi(3) as f64;
            if v[0] > 1e-4 && v[0] < 1. - 1e-4 {
                mixed += 1;
            }
            assert!(v[0] >= -2e-6 && v[0] <= 1. + 2e-6);
        }
        eprintln!(
            "PLIC slab direction {direction}: L1={l1:.3e}, mixed={mixed}, relative volume drift {:.3e}",
            (surface.diagnostics.volume_m3 / initial - 1.).abs()
        );
        assert!(l1 < 1e-5);
        assert!(mixed <= 2 * n.pow(2));
        assert!((surface.diagnostics.volume_m3 / initial - 1.).abs() < 2e-6);
    }
}

#[test]
fn geometric_sphere_translation_converges_without_mass_renormalization() {
    let _lock = crate::gpu_test_lock();
    let mut errors = Vec::new();
    for n in [24u32, 48] {
        let (gpu, volume, mut surface, _) = fixture(n);
        let h = 1. / n as f32;
        let center = Vec3::new(0.35, 0.4, 0.55);
        let radius = 0.14;
        let initial = sphere(n, center, radius);
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
        surface.refresh(&volume).unwrap();
        let volume_before = surface.diagnostics.volume_m3;
        let speed = Vec3::new(0.12, 0.07, -0.05);
        let fd = n + 1;
        let faces: Vec<[f32; 4]> = (0..fd.pow(3))
            .map(|i| {
                let q = [i % fd, (i / fd) % fd, i / (fd * fd)];
                let mut v = [0.; 4];
                for axis in 0..3 {
                    if q[axis] > 0 && q[axis] < n {
                        v[axis] = speed[axis];
                    }
                }
                v
            })
            .collect();
        let velocity = gpu
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Three-dimensional sphere translation"),
                contents: bytemuck::cast_slice(&faces),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let steps = 2 * n;
        let dt = 1. / steps as f64;
        for _ in 0..steps {
            surface.advance(&volume, &velocity, None, dt, 0.12).unwrap();
        }
        let reference = sphere(n, center + speed, radius);
        let data = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let values = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
        let l1 = values
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a[0] - b[0]).abs() as f64 * h.powi(3) as f64)
            .sum::<f64>();
        let drift = (surface.diagnostics.volume_m3 / volume_before - 1.).abs();
        eprintln!(
            "PLIC sphere n={n}: relative L1 {:.3e}, mass drift {drift:.3e}, bounds {}..{}",
            l1 / volume_before,
            surface.diagnostics.min_fraction,
            surface.diagnostics.max_fraction
        );
        assert!(values.iter().all(|a| a[0] >= -2e-6 && a[0] <= 1. + 2e-6));
        assert!(drift < 2e-6);
        errors.push(l1 / volume_before);
    }
    assert!(errors[1] < 0.6 * errors[0], "{errors:?}");
    assert!(errors[1] < 0.05, "{errors:?}");
}
