//! Test-only mean-free pressure research. Production pressure code is unchanged.
use super::*;
impl FluidState {
    pub(crate) fn enable_floating_pressure(&mut self) -> Result<(), String> {
        if self.components != 1 {
            return Err("Floating-pressure research requires one connected region".into());
        }
        let mut solver = include_str!("solver.wgsl").to_owned();
        let replacements = [
            ("if (mask[index(pos,p.dims.xyz)]&3u)==2u{return 1.0;}", ""),
            ("if (mask[index(pos,p.dims.xyz)]&3u)==2u{return 0.0;}", ""),
            (
                "    if (mask[index(pos,p.dims.xyz)]&3u)==2u {return potential(pos,kind);}\n",
                "",
            ),
            (
                "sum=vec4<f32>(r*r/d,true_residual*true_residual,physical*physical,0.0);",
                "sum=vec4<f32>(r*r/d,true_residual*true_residual,physical*physical,r);",
            ),
            (
                "let pressure=work[i].x*p.physical.w/p.physical.y;",
                "let pressure=(work[i].x-work[mask[i]>>2u].x)*p.physical.w/p.physical.y;",
            ),
        ];
        for (old, new) in replacements {
            assert_eq!(solver.matches(old).count(), 1, "source drift: {old}");
            solver = solver.replace(old, new);
        }
        let old = "v.x+a*v.z,r,v.z,diagonal);sum=vec4<f32>(r*r/diagonal,r*r,0.0,0.0);";
        assert_eq!(solver.matches(old).count(), 1);
        solver = solver.replace(
            old,
            "v.x+a*(v.z-work[mask[i]>>2u].z),r,v.z,diagonal);sum=vec4<f32>(r*r/diagonal,r*r,0.0,r);",
        );
        // Search lanes remain read-only while all cells use the reference search.
        // Individual stores avoid reading a lane another invocation writes.
        let old = "work[i]=vec4<f32>(v.x+a*(v.z-work[mask[i]>>2u].z),r,v.z,diagonal);";
        assert_eq!(solver.matches(old).count(), 1);
        solver = solver.replace(
            old,
            "work[i].x=v.x+a*(v.z-work[mask[i]>>2u].z);work[i].y=r;work[i].w=diagonal;",
        );
        let source = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            include_str!("../periodic.wgsl"),
            include_str!("viscosity.wgsl"),
            solver,
            include_str!("stabilized.wgsl"),
            include_str!("stress_probes.wgsl"),
            include_str!("floating.wgsl")
        );
        let device = &self.context.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Floating-pressure research"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let bind_layout = self.pipelines["pressure_apply"].get_bind_group_layout(0);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        for entry in [
            "pressure_coefficients",
            "pressure_refresh",
            "pressure_apply",
            "fields",
            "floating_collect",
            "floating_prepare",
            "update",
            "floating_residual_reduce",
            "floating_initial_project",
            "floating_residual_project",
        ] {
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
            self.pipelines.insert(entry, pipeline);
        }
        if let Some(mg) = self.multigrid.take() {
            self.bytes -= mg.bytes;
        }
        self.force_multigrid = Some(self.force_multigrid.unwrap_or(false));
        self.multigrid_attempted = false;
        self.floating_pressure = true;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn floating_periodic_gradient_recovers_analytic_pressure() {
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
            if floating {
                fluid.enable_floating_pressure().unwrap();
            }
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
                let expected = (phi(i as u32 % n) - phi(0)) * fluid.params.physical[3] as f64
                    / fluid.config.dt_s;
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
}
