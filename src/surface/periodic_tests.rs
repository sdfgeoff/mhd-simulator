use super::*;
use crate::{
    fluid::{FluidConfig, FluidState},
    simulation::{DomainSettings, default_materials},
};
use glam::Vec3;

fn seed_heat(
    context: &GpuContext,
    fractions: &[[f32; 4]],
) -> (wgpu::Buffer, wgpu::Buffer, wgpu::Buffer) {
    let cells: Vec<crate::thermal::ThermalCell> = fractions
        .iter()
        .map(|a| crate::thermal::ThermalCell {
            state: [300., 1., 20. * a[0], a[0]],
            valid: [f32::from(a[0] > 0.), 0., 0., 0.],
            ..Default::default()
        })
        .collect();
    let input = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Periodic thermal tracer"),
            contents: bytemuck::cast_slice(&cells),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        });
    let output = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: input.size(),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let checkpoint = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: input.size(),
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (input, output, checkpoint)
}

#[test]
fn periodic_sphere_and_heat_cross_each_domain_seam_conservatively() {
    let _lock = crate::gpu_test_lock();
    let n = 24;
    let h = 1. / n as f32;
    let (gpu, volume, _, _) = super::tests::fixture(n);
    assert_eq!(volume.dims, [n; 3]);
    for axis in 0..3 {
        let mut periodic = [false; 3];
        periodic[axis] = true;
        let (mut surface, _) =
            SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, periodic)
                .unwrap();
        let mut center = Vec3::splat(0.5);
        center[axis] = 0.75;
        let radius = 0.14;
        let initial = super::tests::sphere(n, center, radius);
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
        surface.refresh(&volume).unwrap();
        let v0 = surface.diagnostics.volume_m3;
        let (input, output, checkpoint) = seed_heat(&gpu.context, &initial);
        let fd = n + 1;
        let mut uniform = [0f32; 4];
        uniform[axis] = 0.25;
        let velocity = gpu
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Periodic uniform MAC velocity"),
                contents: bytemuck::cast_slice(&vec![uniform; fd.pow(3) as usize]),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let steps = 2 * n;
        let dt = 1. / steps as f64;
        for _ in 0..steps {
            surface
                .advance(
                    &volume,
                    &velocity,
                    Some(EnthalpyTransport {
                        input: &input,
                        output: &output,
                        checkpoint: &checkpoint,
                    }),
                    dt,
                    0.25,
                )
                .unwrap();
            let mut enc = gpu
                .context
                .device
                .create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(&output, 0, &input, 0, input.size());
            gpu.context.queue.submit(Some(enc.finish()));
        }
        center[axis] += 0.25;
        let a = super::tests::sphere(n, center, radius);
        center[axis] -= 1.;
        let b = super::tests::sphere(n, center, radius);
        let data = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        let cells = bytemuck::cast_slice::<u8, [f32; 4]>(&data);
        let heat = read_bytes(&gpu.context, &input, 0, input.size()).unwrap();
        let heat = bytemuck::cast_slice::<u8, crate::thermal::ThermalCell>(&heat);
        let l1 = cells
            .iter()
            .zip(a.iter().zip(&b))
            .map(|(actual, (a, b))| (actual[0] - a[0] - b[0]).abs() as f64 * h.powi(3) as f64)
            .sum::<f64>()
            / v0;
        let energy = heat
            .iter()
            .map(|c| c.state[2] as f64 * h.powi(3) as f64)
            .sum::<f64>();
        for (cell, c) in cells.iter().zip(heat) {
            if cell[0] > 1e-3 {
                assert!((c.state[2] / (20. * cell[0]) - 1.).abs() < 2e-5);
            }
        }
        eprintln!(
            "periodic axis {axis}: fraction L1={l1:.4e}, volume drift={:.3e}, heat drift={:.3e}",
            surface.diagnostics.volume_m3 / v0 - 1.,
            energy / (20. * v0) - 1.
        );
        assert!(l1 < 0.06);
        assert!((surface.diagnostics.volume_m3 / v0 - 1.).abs() < 2e-6);
        assert!((energy / (20. * v0) - 1.).abs() < 3e-6);
    }
}

#[test]
fn uniform_periodic_metal_drop_preserves_velocity_and_integrated_momentum() {
    let _lock = crate::gpu_test_lock();
    let n = 16u32;
    let h = 1. / n as f32;
    let mut materials = default_materials();
    materials[3].density_kg_m3 = Some(7000.);
    materials[3].dynamic_viscosity_pa_s = Some(1e-6);
    materials[3].surface_tension_n_m = Some(0.);
    materials[0].density_kg_m3 = Some(1.);
    let (gpu, volume, _, _) = super::tests::box_fixture(
        DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &materials,
    );
    let (mut surface, _) =
        SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, [true; 3]).unwrap();
    let initial = super::tests::sphere(n, Vec3::new(0.75, 0.5, 0.5), 0.14);
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
    surface.refresh(&volume).unwrap();
    let u = [0.25, 0.125, -0.0625];
    let mut fluid = FluidState::new(
        gpu.context.clone(),
        &volume,
        FluidConfig {
            periodic: [true; 3],
            surface: SurfaceConfig { enabled: true },
            initial_velocity_m_s: u,
            acceleration_m_s2: [0.; 3],
            dt_s: 1. / 64.,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    fluid
        .update_surface(&volume, surface.field(), None)
        .unwrap();
    let mass = 1. + 6999. * surface.diagnostics.volume_m3;
    for _ in 0..64 {
        fluid.step(&volume).unwrap();
        surface
            .advance(
                &volume,
                fluid.face_buffer(),
                None,
                1. / 64.,
                fluid.max_face_speed(),
            )
            .unwrap();
        fluid
            .update_surface(&volume, surface.field(), None)
            .unwrap();
    }
    let data = read_bytes(
        &gpu.context,
        fluid.result_buffer(),
        0,
        fluid.result_buffer().size(),
    )
    .unwrap();
    let cells = bytemuck::cast_slice::<u8, crate::fluid::FluidCell>(&data);
    let mut momentum = [0f64; 3];
    for c in cells {
        for axis in 0..3 {
            assert!((c.velocity[axis] - u[axis]).abs() < 2e-6);
            momentum[axis] +=
                (1. + 6999. * c.reserved[2] as f64) * c.velocity[axis] as f64 * h.powi(3) as f64;
        }
    }
    for axis in 0..3 {
        assert!(
            (momentum[axis] / (mass * u[axis] as f64) - 1.).abs() < 1e-5,
            "momentum {momentum:?}, mass {mass}"
        );
    }
}
