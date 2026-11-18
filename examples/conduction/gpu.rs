//! Research finite-volume DC conduction, with no CPU PDE implementation.
use bytemuck::{Pod, Zeroable};
use mhd_workbench::{
    em::CellField,
    gpu::{GpuContext, read_bytes},
};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    dims: [u32; 4],
    physical: [f32; 4],
    control: [u32; 4],
    extra: [f32; 4],
}

pub struct Conduction {
    context: GpuContext,
    params: Params,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    buffers: Vec<wgpu::Buffer>,
    pipes: HashMap<&'static str, wgpu::ComputePipeline>,
    scalars: wgpu::Buffer,
    pub result: wgpu::Buffer,
    pub iterations: u32,
    pub relative_residual: f32,
}
impl Conduction {
    pub fn new(
        context: GpuContext,
        dims: [u32; 3],
        h: f32,
        cells: &[[f32; 2]],
        bias: &wgpu::Buffer,
    ) -> Result<Self, String> {
        let n = dims.iter().product::<u32>();
        if cells.len() != n as usize || !h.is_finite() || h <= 0. {
            return Err("Invalid conduction grid".into());
        }
        if !cells.iter().any(|v| v[1] == 0.5) || !cells.iter().any(|v| v[1] == -0.5) {
            return Err("Both terminal regions are required".into());
        }
        let params = Params {
            dims: [dims[0], dims[1], dims[2], n],
            physical: [h, 1., 1e-5, 0.],
            control: [0, 0, 1, 0],
            extra: [
                cells
                    .iter()
                    .filter(|c| c[1] == 3.)
                    .map(|c| c[0])
                    .fold(0., f32::max),
                0.,
                0.,
                0.,
            ],
        };
        let uniform = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("DC parameters"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let properties = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("DC conductivity and terminals"),
                contents: bytemuck::cast_slice(cells),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let storage = |label, size| {
            context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        let work = storage("DC PCG vectors", u64::from(n) * 16);
        let partials = storage("DC reduction scratch", u64::from(n.div_ceil(256)) * 16);
        let scalars = storage("DC scalars", 64);
        let result = storage("DC current force and heating", u64::from(n) * 64);
        let unit_potential = storage(
            "DC unit and motional potential predictors",
            u64::from(n) * 8,
        );
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("DC bindings"),
                entries: &(0..9)
                    .map(|binding| wgpu::BindGroupLayoutEntry {
                        binding,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: if binding == 0 {
                                wgpu::BufferBindingType::Uniform
                            } else {
                                wgpu::BufferBindingType::Storage {
                                    read_only: binding == 5 || binding == 7,
                                }
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    })
                    .collect::<Vec<_>>(),
            });
        let buffers = [
            &uniform,
            &properties,
            &work,
            &partials,
            &scalars,
            bias,
            &result,
            bias,
            &unit_potential,
        ];
        let bind = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("DC bindings"),
                layout: &layout,
                entries: &buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect::<Vec<_>>(),
            });
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("DC finite volumes"),
                source: wgpu::ShaderSource::Wgsl(include_str!("solver.wgsl").into()),
            });
        let pipeline_layout =
            context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("DC pipeline layout"),
                    bind_group_layouts: &[&layout],
                    push_constant_ranges: &[],
                });
        let mut pipes = HashMap::new();
        for name in [
            "update_phase",
            "save_unit",
            "save_motion",
            "initial_guess",
            "clear_solution",
            "initialize",
            "initial_reduce",
            "apply_operator",
            "alpha_reduce",
            "update_residual",
            "beta_reduce",
            "update_search",
            "true_residual",
            "collect_fields",
            "fields_reduce",
        ] {
            pipes.insert(
                name,
                context
                    .device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(name),
                        layout: Some(&pipeline_layout),
                        module: &shader,
                        entry_point: Some(name),
                        compilation_options: Default::default(),
                        cache: None,
                    }),
            );
        }
        let owned_buffers = buffers.iter().map(|b| (*b).clone()).collect();
        Ok(Self {
            context,
            params,
            uniform,
            bind,
            layout,
            buffers: owned_buffers,
            pipes,
            scalars,
            result,
            iterations: 0,
            relative_residual: f32::INFINITY,
        })
    }
    fn dispatch(&self, commands: &[(&str, u32)]) {
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = self
                .context
                .compute_pass(&mut encoder, Some("DC finite-volume solve"));
            pass.set_bind_group(0, &self.bind, &[]);
            for (name, count) in commands {
                pass.set_pipeline(&self.pipes[name]);
                pass.dispatch_workgroups(*count, 1, 1);
            }
        }
        self.context.queue.submit(Some(encoder.finish()));
    }
    pub fn scalar_values(&self) -> Result<[[f32; 4]; 4], String> {
        let bytes = read_bytes(&self.context, &self.scalars, 0, 64)?;
        let values = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        Ok(values.try_into().expect("four scalar records"))
    }
    pub fn solve(&mut self) -> Result<(), String> {
        let groups = self.params.dims[3].div_ceil(256);
        self.dispatch(&[
            ("update_phase", groups),
            ("initial_guess", groups),
            ("initialize", groups),
            ("initial_reduce", 1),
        ]);
        self.iterations = 0;
        let initial = self.scalar_values()?[0];
        if initial[2] == 0. {
            self.dispatch(&[("clear_solution", groups)]);
            self.relative_residual = 0.;
            return Ok(());
        }
        self.relative_residual = (initial[1] / initial[2]).sqrt();
        if self.relative_residual <= self.params.physical[2] {
            return Ok(());
        }
        for batch in 0..125 {
            let mut commands = Vec::new();
            for _ in 0..32 {
                commands.extend([
                    ("apply_operator", groups),
                    ("alpha_reduce", 1),
                    ("update_residual", groups),
                    ("beta_reduce", 1),
                    ("update_search", groups),
                ]);
            }
            self.dispatch(&commands);
            self.iterations = (batch + 1) * 32;
            let s = self.scalar_values()?;
            let ratio = (s[0][1] / s[0][2].max(1e-38)).sqrt();
            if !ratio.is_finite() {
                return Err("Nonfinite DC residual".into());
            }
            if ratio <= self.params.physical[2] {
                self.dispatch(&[("true_residual", groups), ("initial_reduce", 1)]);
                let s = self.scalar_values()?;
                self.relative_residual = (s[0][1] / s[0][2].max(1e-38)).sqrt();
                if self.relative_residual <= self.params.physical[2] {
                    return Ok(());
                }
            }
        }
        Err(format!(
            "DC solve did not converge after {} iterations, true residual {}",
            self.iterations, self.relative_residual
        ))
    }
    pub fn set_relative_tolerance(&mut self, tolerance: f32) -> Result<(), String> {
        if !tolerance.is_finite() || tolerance <= 0. || tolerance >= 1. {
            return Err("Relative tolerance must lie between zero and one".into());
        }
        self.params.physical[2] = tolerance;
        self.write_params();
        Ok(())
    }
    pub fn set_contact_resistivity(&mut self, resistance_ohm_m2: f32) -> Result<(), String> {
        if !resistance_ohm_m2.is_finite() || resistance_ohm_m2 < 0. {
            return Err("Invalid contact resistivity".into());
        }
        self.params.physical[3] = resistance_ohm_m2;
        self.write_params();
        Ok(())
    }
    fn write_params(&self) {
        self.context
            .queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&self.params));
    }
    pub fn attach_flow(&mut self, flow: &wgpu::Buffer) -> Result<(), String> {
        if flow.size() < u64::from(self.params.dims[3]) * 64 {
            return Err("Flow packet is too small".into());
        }
        self.buffers[7] = flow.clone();
        self.bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("DC moving-phase bindings"),
                layout: &self.layout,
                entries: &self
                    .buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect::<Vec<_>>(),
            });
        self.params.control[1] = 1;
        self.write_params();
        Ok(())
    }
    /// Inductionless, charge-conserving current drive, including u cross B.
    /// Rebuild conductivity from accepted liquid fractions before both solves.
    pub fn solve_current(&mut self, current_a: f32) -> Result<(f32, [f32; 4]), String> {
        if !current_a.is_finite() {
            return Err("Nonfinite commanded current".into());
        }
        self.params.control[0] = 0;
        self.params.physical[1] = 1.;
        self.write_params();
        self.solve()?;
        let unit_iterations = self.iterations;
        let unit_residual = self.relative_residual;
        let unit = self.fields(1.)?;
        if !unit[0].is_finite() || unit[0] <= 0. {
            return Err("No conducting path between terminals".into());
        }
        self.dispatch(&[("save_unit", self.params.dims[3].div_ceil(256))]);
        self.params.control[0] = 1;
        self.params.physical[1] = 0.;
        self.write_params();
        self.solve()?;
        let motion = self.fields(0.)?;
        self.dispatch(&[("save_motion", self.params.dims[3].div_ceil(256))]);
        let voltage = (current_a - motion[0]) / unit[0];
        let result = self.fields(voltage)?;
        self.iterations += unit_iterations;
        self.relative_residual = self.relative_residual.max(unit_residual);
        Ok((voltage, result))
    }
    pub fn fields(&mut self, voltage: f32) -> Result<[f32; 4], String> {
        self.params.physical[1] = voltage;
        self.context
            .queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&self.params));
        self.dispatch(&[
            ("collect_fields", self.params.dims[3].div_ceil(256)),
            ("fields_reduce", 1),
        ]);
        Ok(self.scalar_values()?[2])
    }
    pub fn read_fields(&self) -> Result<Vec<CellField>, String> {
        let bytes = read_bytes(&self.context, &self.result, 0, self.result.size())?;
        Ok(bytemuck::cast_slice(&bytes).to_vec())
    }
}
