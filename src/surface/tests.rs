use super::*;
use crate::{
    domain::GpuDomain,
    model::{Body, Model},
    simulation::{DomainSettings, default_materials},
    thermal::ThermalCell,
};
use glam::Vec3;
pub(crate) fn fixture(n: u32) -> (GpuDomain, Volume, SurfaceState, Volume) {
    let hi = 1000.;
    let v = [
        Vec3::ZERO,
        Vec3::new(hi, 0., 0.),
        Vec3::new(hi, hi, 0.),
        Vec3::new(0., hi, 0.),
        Vec3::new(0., 0., hi),
        Vec3::new(hi, 0., hi),
        Vec3::splat(hi),
        Vec3::new(0., hi, hi),
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
    let volume = gpu
        .build(
            &model,
            &DomainSettings {
                min_mm: [0.; 3],
                max_mm: [hi; 3],
                cell_mm: hi / n as f32,
            },
            &default_materials(),
        )
        .unwrap();
    let (surface, evolving) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
    (gpu, volume, surface, evolving)
}
fn sphere(n: u32, center: Vec3, r: f32) -> Vec<[f32; 4]> {
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
                Some((&thermal, &transported)),
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
                advection: false,
                relative_tolerance: 1e-6,
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
                    s.advance(
                        &volume,
                        fluid.face_buffer(),
                        None,
                        0.001,
                        fluid.max_face_speed(),
                    )
                    .unwrap();
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
    // Deliberately under-report speed to exercise the independent GPU occupancy check.
    assert!(s.advance(&volume, &velocity, None, 1., 0.).is_err());
    assert_eq!(
        before,
        read_bytes(&gpu.context, s.field(), 0, s.field().size()).unwrap()
    );
    assert_eq!(s.diagnostics.volume_m3, volume_before);
}
