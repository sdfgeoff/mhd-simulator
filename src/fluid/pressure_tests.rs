//! GPU validation of the pressure constant-nullspace treatment.
use super::*;
#[test]
fn meanfree_periodic_gradient_recovers_analytic_pressure() {
    let _lock = crate::gpu_test_lock();
    let n = 16u32;
    let (gpu, volume, _, _) = crate::surface::tests::fixture(n);
    for (floating, multigrid) in [(false, false), (true, false), (true, true)] {
        let mut fluid = FluidState::new(
            gpu.context.clone(),
            &volume,
            FluidConfig {
                periodic: [true; 3],
                acceleration_m_s2: [0.; 3],
                advection: false,
                relative_tolerance: 1e-5,
                absolute_divergence_tolerance: 1e-7,
                max_iterations: 4000,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        fluid.force_multigrid = Some(multigrid);
        fluid.force_meanfree_pressure = Some(floating);
        let phi = |x: u32| {
            0.001
                * (std::f64::consts::TAU * (x % n) as f64 / n as f64
                    + std::f64::consts::PI / n as f64)
                    .cos()
        };
        let fd = n + 1;
        let h = fluid.grid.cell_m as f64;
        let mut faces = vec![[0f32; 4]; fd.pow(3) as usize];
        for z in 0..=n {
            for y in 0..=n {
                for x in 0..=n {
                    faces[(x + fd * (y + fd * z)) as usize][0] =
                        ((phi(x) - phi((x + n - 1) % n)) / h) as f32;
                }
            }
        }
        gpu.context
            .queue
            .write_buffer(&fluid.velocity, 0, bytemuck::cast_slice(&faces));
        fluid.step(&volume).unwrap();
        let bytes = read_bytes(
            &gpu.context,
            fluid.result_buffer(),
            0,
            fluid.result_buffer().size(),
        )
        .unwrap();
        let field = bytemuck::cast_slice::<u8, FluidCell>(&bytes);
        let mut error = 0.;
        let mut norm = 0.;
        for (i, cell) in field.iter().enumerate() {
            let expected =
                (phi(i as u32 % n) - phi(0)) * fluid.params.physical[3] as f64 / fluid.config.dt_s;
            error += (cell.pressure[0] as f64 - expected).powi(2);
            norm += expected * expected;
        }
        let relative = (error / norm).sqrt();
        eprintln!(
            "Floating={floating}, MG={multigrid}: pressure relative L2={relative:.8e}, U={:.8e}, iterations={}",
            fluid.diagnostics.max_speed_m_s, fluid.diagnostics.iterations
        );
        assert!(relative < 1e-4);
        assert!(fluid.diagnostics.max_speed_m_s < 2e-6);
    }
}

#[test]
fn bounded_pressure_submissions_preserve_jacobi_and_multigrid_fields() {
    let _lock = crate::gpu_test_lock();
    let n = 16u32;
    let (gpu, volume, _, _) = crate::surface::tests::fixture(n);
    let mut prior = Vec::new();
    for (multigrid, bounded, window) in [
        (false, false, 1),
        (false, true, 1),
        (false, true, 4),
        (true, false, 1),
        (true, true, 1),
        (true, true, 4),
    ] {
        gpu.context.set_bounded_submission_window(window).unwrap();
        let floating = true;
        let mut fluid = FluidState::new(
            gpu.context.clone(),
            &volume,
            FluidConfig {
                periodic: [true; 3],
                acceleration_m_s2: [0.; 3],
                advection: false,
                relative_tolerance: 1e-5,
                absolute_divergence_tolerance: 1e-7,
                max_iterations: 4000,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        fluid.force_multigrid = Some(multigrid);
        fluid.force_bounded_submissions = Some(bounded);
        fluid.force_meanfree_pressure = Some(floating);
        let phi = |x: u32| {
            0.001
                * (std::f64::consts::TAU * (x % n) as f64 / n as f64
                    + std::f64::consts::PI / n as f64)
                    .cos()
        };
        let fd = n + 1;
        let h = fluid.grid.cell_m as f64;
        let mut faces = vec![[0f32; 4]; fd.pow(3) as usize];
        for z in 0..=n {
            for y in 0..=n {
                for x in 0..=n {
                    faces[(x + fd * (y + fd * z)) as usize][0] =
                        ((phi(x) - phi((x + n - 1) % n)) / h) as f32;
                }
            }
        }
        gpu.context
            .queue
            .write_buffer(&fluid.velocity, 0, bytemuck::cast_slice(&faces));
        fluid.step(&volume).unwrap();
        let bytes = read_bytes(
            &gpu.context,
            fluid.result_buffer(),
            0,
            fluid.result_buffer().size(),
        )
        .unwrap();
        if bounded {
            assert_eq!(
                prior, bytes,
                "Submission boundaries changed pressure/velocity fields; MG={multigrid}"
            );
        } else {
            prior = bytes.clone();
        }
        let field = bytemuck::cast_slice::<u8, FluidCell>(&bytes);
        let mut error = 0.;
        let mut norm = 0.;
        for (i, cell) in field.iter().enumerate() {
            let expected =
                (phi(i as u32 % n) - phi(0)) * fluid.params.physical[3] as f64 / fluid.config.dt_s;
            error += (cell.pressure[0] as f64 - expected).powi(2);
            norm += expected * expected;
        }
        let relative = (error / norm).sqrt();
        eprintln!(
            "Floating={floating}, MG={multigrid}: pressure relative L2={relative:.8e}, U={:.8e}, iterations={}",
            fluid.diagnostics.max_speed_m_s, fluid.diagnostics.iterations
        );
        assert!(relative < 1e-4);
        assert!(fluid.diagnostics.max_speed_m_s < 2e-6);
    }
}
