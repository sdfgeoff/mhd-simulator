//! Neutral-contact geometry at voxel CAD walls. Direct ID/fraction fixtures
//! isolate the interface kernels from CAD tessellation and raster integration.
use super::*;
#[test]
fn neutral_wall_geometry_uses_the_liquid_side_across_a_periodic_seam() {
    let _lock = crate::gpu_test_lock();
    let n = 24u32;
    let (gpu, volume, _, _) = super::tests::fixture(n);
    let (mut surface, _) = SurfaceState::with_periodic_boundaries(
        gpu.context.clone(),
        &volume,
        3,
        [true, false, false],
    )
    .unwrap();
    let ids: Vec<_> = (0..n.pow(3))
        .map(|i| if i % n == n - 1 { 1u32 } else { 3u32 })
        .collect();
    let field: Vec<_> = (0..n.pow(3))
        .map(|i| {
            let x = i % n;
            let z = i / (n * n);
            [
                if x < n / 2 {
                    (12.5 - z as f32).clamp(0., 1.)
                } else {
                    0.
                },
                0.,
                0.,
                0.,
            ]
        })
        .collect();
    gpu.context
        .queue
        .write_buffer(volume.material_ids(), 0, bytemuck::cast_slice(&ids));
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&field));
    surface.refresh(&volume).unwrap();
    let bytes = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
    let values = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
    for y in 3..n - 3 {
        for x in 0..3 {
            let i = (x + n * (y + n * 12)) as usize;
            assert!(values[i][1].abs() < 1e-5, "{x},{y}: {:?}", values[i]);
        }
    }
}
#[test]
fn planar_neutral_contact_has_zero_curvature_on_both_sides_of_thin_walls() {
    let _lock = crate::gpu_test_lock();
    let n = 24u32;
    let (gpu, volume, mut surface, _) = super::tests::fixture(n);
    for wall_axis in 0..3 {
        let normal_axis = (wall_axis + 1) % 3;
        let coordinates = |i: u32| [i % n, (i / n) % n, i / (n * n)];
        for thickness in [1, 2, 6] {
            for liquid_right in [false, true] {
                let ids: Vec<_> = (0..n.pow(3))
                    .map(|i| {
                        if (8..8 + thickness).contains(&coordinates(i)[wall_axis]) {
                            1u32
                        } else {
                            3u32
                        }
                    })
                    .collect();
                let field: Vec<_> = (0..n.pow(3))
                    .map(|i| {
                        let q = coordinates(i);
                        let x = q[wall_axis];
                        let z = q[normal_axis];
                        let liquid = if liquid_right {
                            x >= 8 + thickness
                        } else {
                            x < 8
                        };
                        [
                            if liquid {
                                (12.5 - z as f32).clamp(0., 1.)
                            } else {
                                0.
                            },
                            0.,
                            0.,
                            0.,
                        ]
                    })
                    .collect();
                gpu.context.queue.write_buffer(
                    volume.material_ids(),
                    0,
                    bytemuck::cast_slice(&ids),
                );
                gpu.context
                    .queue
                    .write_buffer(surface.field(), 0, bytemuck::cast_slice(&field));
                surface.refresh(&volume).unwrap();
                let bytes =
                    read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
                let values = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
                let mut maximum = 0f32;
                for (initial, actual) in field.iter().zip(values) {
                    if initial[0] > 0. && initial[0] < 1. {
                        maximum = maximum.max(actual[1].abs());
                    }
                }
                eprintln!(
                    "Neutral wall axis={wall_axis}, thickness={thickness}, liquid_right={liquid_right}: max curvature={maximum:.8e} m^-1 (reference zero)"
                );
                assert!(maximum < 1e-5);
                if wall_axis == 0 && thickness == 1 && liquid_right {
                    let mut fluid = crate::fluid::FluidState::new(
                        gpu.context.clone(),
                        &volume,
                        crate::fluid::FluidConfig {
                            surface: SurfaceConfig { enabled: true },
                            acceleration_m_s2: [0.; 3],
                            ..Default::default()
                        },
                        None,
                    )
                    .unwrap();
                    fluid
                        .update_surface(&volume, surface.field(), None)
                        .unwrap();
                    let dt = fluid.config().dt_s;
                    for _ in 0..8 {
                        fluid.step(&volume).unwrap();
                        surface
                            .advance_coupled(&volume, &mut fluid, None, dt)
                            .unwrap();
                        fluid
                            .update_surface(&volume, surface.field(), None)
                            .unwrap();
                        assert!(
                            fluid.diagnostics.max_speed_m_s < 1e-8,
                            "{:?}",
                            fluid.diagnostics
                        );
                    }
                    let after =
                        read_bytes(&gpu.context, surface.field(), 0, surface.field().size())
                            .unwrap();
                    let values = bytemuck::cast_slice::<u8, [f32; 4]>(&after);
                    for (initial, after) in field.iter().zip(values) {
                        assert_eq!(initial[0], after[0]);
                    }
                }
            }
        }
    }
}

#[test]
fn neutral_wall_hemisphere_matches_mirrored_sphere_and_laplace_curvature() {
    let _lock = crate::gpu_test_lock();
    let mut errors = Vec::new();
    for n in [32u32, 48] {
        let (gpu, volume, mut surface, _) = super::tests::fixture(n);
        let wall_x = n / 4;
        let radius = 0.2;
        let field = super::tests::sphere(n, glam::Vec3::new(0.25, 0.5, 0.5), radius);
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&field));
        surface.refresh(&volume).unwrap();
        let full_bytes =
            read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let full = bytemuck::cast_slice::<u8, [f32; 4]>(&full_bytes);
        let ids: Vec<_> = (0..n.pow(3))
            .map(|i| if i % n < wall_x { 1u32 } else { 3u32 })
            .collect();
        let half: Vec<_> = field
            .iter()
            .enumerate()
            .map(|(i, &v)| if i as u32 % n < wall_x { [0.; 4] } else { v })
            .collect();
        gpu.context
            .queue
            .write_buffer(volume.material_ids(), 0, bytemuck::cast_slice(&ids));
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&half));
        surface.refresh(&volume).unwrap();
        let bytes = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let actual = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        let target = 2. / radius as f64;
        let mut sums = [0.; 4];
        for (i, (a, b)) in actual.iter().zip(full).enumerate() {
            let alpha = half[i][0] as f64;
            if alpha <= 1e-6 || alpha >= 1. - 1e-6 {
                continue;
            }
            let w = alpha * (1. - alpha);
            sums[0] += w;
            sums[1] += w * (a[1] as f64 - target).powi(2);
            sums[2] += w * (a[1] as f64 - b[1] as f64).powi(2);
            if i as u32 % n == wall_x {
                sums[3] += w * (a[1] as f64 - b[1] as f64).powi(2);
            }
        }
        let error = (sums[1] / sums[0]).sqrt() / target;
        let mirror_error = (sums[2] / sums[0]).sqrt() / target;
        eprintln!(
            "Neutral hemisphere n={n}: analytic curvature relative RMS={error:.8e}, mirrored-sphere difference={mirror_error:.8e}, contact error sum={:.8e}",
            sums[3]
        );
        assert!(mirror_error < 1e-4, "mirror error={mirror_error}");
        assert!(error < 0.05, "analytic error={error}");
        errors.push(error);
    }
    assert!(errors[1] < errors[0], "{errors:?}");
}
