//! Research finite-volume DC conduction, with no CPU PDE implementation.
use bytemuck::{Pod, Zeroable};
use mhd_workbench::{
    em::CellField,
    gpu::{GpuContext, read_bytes},
};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

pub const SOLVER_VERSION: &str = "conservative-split-f32-pair-interface-v3";

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    dims: [u32; 4],
    physical: [f32; 4],
    control: [u32; 4],
    extra: [f32; 4],
    interface: [f32; 4],
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
    pub basis_wall_ms: [f64; 2],
    pub relative_residual: f32,
    pub boundary_current_defect: f32,
    boundary_current_imbalance_a: f32,
    pub unit_boundary_current_defect: f32,
    pub motional_boundary_current_imbalance_a: f32,
    motion_boundary_absolute_tolerance_a: f32,
    max_iterations: u32,
    has_material_groups: bool,
    dispatch_row_width: u32,
    compact: bool,
}
impl Conduction {
    pub fn new(
        context: GpuContext,
        dims: [u32; 3],
        h: f32,
        cells: &[[f32; 2]],
        bias: &wgpu::Buffer,
    ) -> Result<Self, String> {
        Self::new_with_groups(context, dims, h, cells, bias, None)
    }
    /// Material groups are packed in the unused low16 mantissa bits of the
    /// discrete flag word. No extra GPU storage/binding is required. Legacy
    /// callers retain byte-identical cell data and use group zero.
    pub fn new_with_groups(
        context: GpuContext,
        dims: [u32; 3],
        h: f32,
        cells: &[[f32; 2]],
        bias: &wgpu::Buffer,
        groups: Option<&[u32]>,
    ) -> Result<Self, String> {
        Self::new_with_dispatch_width(context, dims, h, cells, bias, groups, 65535)
    }
    // An explicit narrow row exercises the same multi-row indexing on a small
    // analytic test grid, without allocating millions of test cells.
    pub(crate) fn new_with_dispatch_width(
        context: GpuContext,
        dims: [u32; 3],
        h: f32,
        cells: &[[f32; 2]],
        bias: &wgpu::Buffer,
        groups: Option<&[u32]>,
        dispatch_row_width: u32,
    ) -> Result<Self, String> {
        Self::new_layout(
            context,
            dims,
            h,
            cells,
            bias,
            groups,
            (dispatch_row_width, false),
        )
    }
    /// Coupled-flow output: vec4(B) and vec4(force.xyz, Joule density), both f32.
    /// Full diagnostic constructors retain the original CellField contract.
    pub fn new_compact_with_groups(
        context: GpuContext,
        dims: [u32; 3],
        h: f32,
        cells: &[[f32; 2]],
        bias: &wgpu::Buffer,
        groups: Option<&[u32]>,
    ) -> Result<Self, String> {
        Self::new_layout(context, dims, h, cells, bias, groups, (65535, true))
    }
    fn new_layout(
        context: GpuContext,
        dims: [u32; 3],
        h: f32,
        cells: &[[f32; 2]],
        bias: &wgpu::Buffer,
        groups: Option<&[u32]>,
        layout: (u32, bool),
    ) -> Result<Self, String> {
        let (dispatch_row_width, compact) = layout;
        if !(1..=65535).contains(&dispatch_row_width) {
            return Err("Invalid dispatch row width".into());
        }
        let n = dims
            .into_iter()
            .try_fold(1u32, |n, d| n.checked_mul(d))
            .ok_or("Conduction grid exceeds 32-bit indexing")?;
        if n == 0 {
            return Err("Empty conduction grid".into());
        }
        let max_groups = n.div_ceil(256);
        let dispatch_row_width = if max_groups > dispatch_row_width {
            max_groups.div_ceil(max_groups.div_ceil(dispatch_row_width))
        } else {
            dispatch_row_width
        };
        if cells.len() != n as usize || !h.is_finite() || h <= 0. {
            return Err("Invalid conduction grid".into());
        }
        if !cells.iter().any(|v| v[1] == 0.5) || !cells.iter().any(|v| v[1] == -0.5) {
            return Err("Both terminal regions are required".into());
        }
        let packed;
        let properties_cells = if let Some(groups) = groups {
            if groups.len() != cells.len()
                || groups.iter().any(|&g| g > 65535)
                || cells.iter().any(|c| c[1].to_bits() & 0xffff != 0)
            {
                return Err("Invalid material groups or non-discrete cell flags".into());
            }
            packed = cells
                .iter()
                .zip(groups)
                .map(|(c, g)| [c[0], f32::from_bits(c[1].to_bits() | g)])
                .collect::<Vec<_>>();
            &packed[..]
        } else {
            cells
        };
        if bias.size() < u64::from(n) * 64 {
            return Err("Magnetic input requires a full CellField per cell".into());
        }
        let packed_bias = if compact {
            Some(pack_bias(&context, bias, n, dispatch_row_width))
        } else {
            None
        };
        let bias = packed_bias.as_ref().unwrap_or(bias);
        // No-flow mode never reads this binding. Do not retain a wide B packet
        // just to satisfy the unrelated legacy fluid packet layout.
        let empty_flow = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("DC unattached flow placeholder"),
            size: 64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let params = Params {
            dims: [dims[0], dims[1], dims[2], n],
            physical: [h, 1., 1e-5, 0.],
            control: [0, 0, 1, 0],
            interface: [0.; 4],
            extra: [
                cells
                    .iter()
                    .filter(|c| c[1] == 3.)
                    .map(|c| c[0])
                    .fold(0., f32::max),
                1e-5,
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
                contents: bytemuck::cast_slice(properties_cells),
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
        let group_count = n.div_ceil(256);
        let scratch_bytes = 16
            * (2 * u64::from(group_count)
                + u64::from(group_count.div_ceil(256))
                + u64::from(n.div_ceil(4)));
        let limits = context.device.limits();
        if scratch_bytes > limits.max_storage_buffer_binding_size as u64
            || scratch_bytes > limits.max_buffer_size
        {
            return Err("DC diagonal and reduction scratch exceed the device buffer limit".into());
        }
        context.ensure_memory(scratch_bytes, "DC diagonal and reduction scratch")?;
        let partials = storage(
            "DC partial sums, diagonal and conducting workgroups",
            scratch_bytes,
        );
        let scalars = storage("DC scalars", 64);
        let result = storage(
            "DC current force and heating",
            u64::from(n) * if compact { 16 } else { 64 },
        );
        let unit_potential = storage(
            "DC compensated unit and motional potential predictors",
            u64::from(n) * 16,
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
            &empty_flow,
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
                source: wgpu::ShaderSource::Wgsl(
                    shader_source(compact)
                        .replace(
                            "const DISPATCH_ROW_WIDTH:u32=65535u;",
                            &format!("const DISPATCH_ROW_WIDTH:u32={dispatch_row_width}u;"),
                        )
                        .into(),
                ),
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
            "prepare_diagonal",
            "reduce_blocks",
            "clear_partials",
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
            "terminal_flux",
            "terminal_flux_reduce",
            "collect_ports",
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
        let list_shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Conducting workgroup list"),
                source: wgpu::ShaderSource::Wgsl(include_str!("active_groups.wgsl").into()),
            });
        for name in ["reset_count", "list_groups"] {
            pipes.insert(
                name,
                context
                    .device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(name),
                        layout: Some(&pipeline_layout),
                        module: &list_shader,
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
            basis_wall_ms: [0.; 2],
            relative_residual: f32::INFINITY,
            boundary_current_defect: f32::INFINITY,
            boundary_current_imbalance_a: f32::INFINITY,
            unit_boundary_current_defect: f32::INFINITY,
            motional_boundary_current_imbalance_a: f32::INFINITY,
            motion_boundary_absolute_tolerance_a: 0.,
            max_iterations: 4000,
            has_material_groups: groups.is_some(),
            dispatch_row_width,
            compact,
        })
    }
    fn dispatch(&self, commands: &[(&str, u32)]) {
        // A 32-iteration batch is a long unpreemptible submission on an iGPU
        // at >16M cells. Preserve the command order and residual-check cadence,
        // keep each iteration-sized chunk separate when dispatch spans rows.
        // GpuContext controls how many short submissions precede a host wait.
        let multi_row = self.params.dims[3].div_ceil(256) > self.dispatch_row_width;
        let chunk_size = if multi_row { 5 } else { commands.len().max(1) };
        for chunk in commands.chunks(chunk_size) {
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            {
                let mut pass = self
                    .context
                    .compute_pass(&mut encoder, Some("DC finite-volume solve"));
                pass.set_bind_group(0, &self.bind, &[]);
                for (name, count) in chunk {
                    let groups = self.params.dims[3].div_ceil(256);
                    if groups >= 4096 && name.ends_with("_reduce") {
                        pass.set_pipeline(&self.pipes["reduce_blocks"]);
                        let blocks = groups.div_ceil(256);
                        pass.dispatch_workgroups(
                            blocks.min(self.dispatch_row_width),
                            blocks.div_ceil(self.dispatch_row_width),
                            1,
                        );
                    }
                    pass.set_pipeline(&self.pipes[name]);
                    pass.dispatch_workgroups(
                        (*count).min(self.dispatch_row_width),
                        (*count).div_ceil(self.dispatch_row_width),
                        1,
                    );
                }
            }
            if multi_row {
                self.context.submit_bounded(encoder.finish());
            } else {
                self.context.queue.submit(Some(encoder.finish()));
            }
        }
    }
    pub fn scalar_values(&self) -> Result<[[f32; 4]; 4], String> {
        let bytes = read_bytes(&self.context, &self.scalars, 0, 64)?;
        let values = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
        Ok(values.try_into().expect("four scalar records"))
    }
    pub fn solve(&mut self) -> Result<(), String> {
        self.solve_prepared(true, true)
    }
    fn clear_cg_partials(&self) {
        if self.params.control[3] != 0 {
            self.dispatch(&[(
                "clear_partials",
                self.params.dims[3].div_ceil(256).div_ceil(256),
            )]);
        }
    }
    fn solve_prepared(&mut self, update_phase: bool, compact_work: bool) -> Result<(), String> {
        let groups = self.params.dims[3].div_ceil(256);
        self.params.extra[1] = self.params.physical[2];
        self.write_params();
        let mut preparation = Vec::with_capacity(4);
        if update_phase {
            preparation.push(("update_phase", groups));
            preparation.push(("prepare_diagonal", groups.div_ceil(4)));
            if compact_work {
                preparation.extend([("reset_count", 1), ("list_groups", groups)]);
            }
        }
        preparation.extend([
            ("initial_guess", groups),
            ("initialize", groups),
            ("initial_reduce", 1),
        ]);
        self.dispatch(&preparation);
        self.iterations = 0;
        let initial_scalars = self.scalar_values()?;
        if update_phase {
            let active = initial_scalars[3][3].to_bits();
            // Dense domains avoid the extra list lookup. Both paths keep the
            // same per-cell stencil and the same dense reduction destinations.
            self.params.control[3] = if compact_work && active < groups * 3 / 4 {
                active
            } else {
                0
            };
            self.write_params();
        }
        let active_groups = match self.params.control[3] {
            0 => groups,
            n => n,
        };
        let initial = initial_scalars[0];
        if initial[2] == 0. {
            self.dispatch(&[("clear_solution", groups)]);
            self.relative_residual = 0.;
            self.boundary_current_defect = 0.;
            self.boundary_current_imbalance_a = 0.;
            return Ok(());
        }
        self.relative_residual = (initial[1] / initial[2]).sqrt();
        if self.relative_residual <= self.params.physical[2] {
            if self.boundary_current_conserved()? {
                return Ok(());
            }
            self.clear_cg_partials();
            self.tighten_iteration_target();
        }
        for batch in 0..self.max_iterations / 32 {
            let mut commands = Vec::new();
            for _ in 0..32 {
                commands.extend([
                    ("apply_operator", active_groups),
                    ("alpha_reduce", 1),
                    ("update_residual", active_groups),
                    ("beta_reduce", 1),
                    ("update_search", active_groups),
                ]);
            }
            self.dispatch(&commands);
            self.iterations = (batch + 1) * 32;
            let s = self.scalar_values()?;
            let ratio = (s[0][1] / s[0][2].max(1e-38)).sqrt();
            if !ratio.is_finite() {
                return Err("Nonfinite DC residual".into());
            }
            if ratio <= self.params.extra[1] {
                self.dispatch(&[("true_residual", groups), ("initial_reduce", 1)]);
                let s = self.scalar_values()?;
                self.relative_residual = (s[0][1] / s[0][2].max(1e-38)).sqrt();
                if self.relative_residual <= self.params.physical[2]
                    && self.boundary_current_conserved()?
                {
                    return Ok(());
                }
                self.clear_cg_partials();
                self.tighten_iteration_target();
            }
        }
        self.dispatch(&[("true_residual", groups), ("initial_reduce", 1)]);
        let s = self.scalar_values()?;
        self.relative_residual = (s[0][1] / s[0][2].max(1e-38)).sqrt();
        if self.boundary_current_conserved()? && self.relative_residual <= self.params.physical[2] {
            return Ok(());
        }
        let terminal = self.scalar_values()?[3];
        let amperes = self.params.physical[0] * 1e6;
        Err(format!(
            "DC solve did not converge after {} iterations, true residual {}, boundary current defect {}, mode {}, net boundary flux {} A / absolute {} A",
            self.iterations,
            self.relative_residual,
            self.boundary_current_defect,
            self.params.control[0],
            terminal[0] * amperes,
            terminal[1] * amperes
        ))
    }
    fn tighten_iteration_target(&mut self) {
        // The recursive CG stop must not freeze the iteration while the
        // independent true-residual/current-conservation checks still fail.
        self.params.extra[1] = (self.params.extra[1].min(self.relative_residual) * 0.1).max(1e-30);
        self.write_params();
    }
    fn boundary_current_conserved(&mut self) -> Result<bool, String> {
        self.dispatch(&[
            ("terminal_flux", self.params.dims[3].div_ceil(256)),
            ("terminal_flux_reduce", 1),
        ]);
        let values = self.scalar_values()?[3];
        self.boundary_current_defect = values[0].abs() / values[1].max(1e-30);
        self.boundary_current_imbalance_a = values[0].abs() * self.params.physical[0] * 1e6;
        // A local L2 residual alone can hide a coherent current leak in a large
        // conductor. Normalize by absolute terminal-face flux, including when
        // the motional solve has opposing currents at a zero-net-current port.
        let mixed = self.params.control[0] == 1
            && self.motion_boundary_absolute_tolerance_a > 0.
            && values[0].abs()
                <= 1e-5 * values[1]
                    + self.motion_boundary_absolute_tolerance_a / (self.params.physical[0] * 1e6);
        Ok(self.boundary_current_defect.is_finite()
            && (self.boundary_current_defect <= 1e-5 || mixed))
    }
    /// Optional dimensional floor for the zero-terminal-voltage motional basis.
    /// Unit-voltage current conservation and local true-residual limits stay strict.
    pub fn set_motional_boundary_absolute_tolerance_a(&mut self, value: f32) -> Result<(), String> {
        if !value.is_finite() || value < 0. {
            return Err("Motional boundary tolerance must be finite and nonnegative".into());
        }
        self.motion_boundary_absolute_tolerance_a = value;
        Ok(())
    }
    pub fn motional_boundary_absolute_tolerance_a(&self) -> f32 {
        self.motion_boundary_absolute_tolerance_a
    }
    pub fn set_relative_tolerance(&mut self, tolerance: f32) -> Result<(), String> {
        if !tolerance.is_finite() || tolerance <= 0. || tolerance >= 1. {
            return Err("Relative tolerance must lie between zero and one".into());
        }
        self.params.physical[2] = tolerance;
        self.write_params();
        Ok(())
    }
    pub fn relative_tolerance(&self) -> f32 {
        self.params.physical[2]
    }
    /// Change only the work budget, retaining true-residual and flux gates.
    pub fn set_max_iterations(&mut self, iterations: u32) -> Result<(), String> {
        if iterations < 32 || iterations % 32 != 0 {
            return Err("DC iteration limit must be a positive multiple of32".into());
        }
        self.max_iterations = iterations;
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
    /// Add an area-specific resistance on faces between just these groups.
    /// Units are ohm*m² electrically, or m²*K/W in the thermal analogy.
    /// Liquid/metal resistance remains a separate, additive legacy control.
    pub fn set_pair_resistivity(&mut self, a: u32, b: u32, value: f32) -> Result<(), String> {
        if !self.has_material_groups
            || a == b
            || a == 0
            || b == 0
            || a > 65535
            || b > 65535
            || !value.is_finite()
            || value < 0.
        {
            return Err("Invalid pair resistance or missing material groups".into());
        }
        self.params.interface = [a as f32, b as f32, value, 1.];
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
    /// Rebuild conductivity once from accepted liquid fractions, shared by both bases.
    pub fn solve_current(&mut self, current_a: f32) -> Result<(f32, [f32; 4]), String> {
        self.solve_current_impl(current_a, true)
    }
    fn solve_current_impl(
        &mut self,
        current_a: f32,
        optimized: bool,
    ) -> Result<(f32, [f32; 4]), String> {
        if !current_a.is_finite() {
            return Err("Nonfinite commanded current".into());
        }
        let unit_start = std::time::Instant::now();
        self.params.control[0] = 0;
        self.params.physical[1] = 1.;
        self.write_params();
        self.solve_prepared(true, optimized)?;
        let unit_iterations = self.iterations;
        let unit_residual = self.relative_residual;
        let unit_boundary_defect = self.boundary_current_defect;
        self.unit_boundary_current_defect = unit_boundary_defect;
        let unit = if optimized {
            self.port_currents(1.)?
        } else {
            self.fields(1.)?
        };
        if !unit[0].is_finite() || unit[0] <= 0. {
            return Err("No conducting path between terminals".into());
        }
        self.dispatch(&[("save_unit", self.params.dims[3].div_ceil(256))]);
        self.basis_wall_ms[0] = unit_start.elapsed().as_secs_f64() * 1000.;
        let motion_start = std::time::Instant::now();
        self.params.control[0] = 1;
        self.params.physical[1] = 0.;
        self.write_params();
        self.solve_prepared(!optimized, optimized)?;
        self.motional_boundary_current_imbalance_a = self.boundary_current_imbalance_a;
        let motion = if optimized {
            self.port_currents(0.)?
        } else {
            self.fields(0.)?
        };
        self.dispatch(&[("save_motion", self.params.dims[3].div_ceil(256))]);
        self.basis_wall_ms[1] = motion_start.elapsed().as_secs_f64() * 1000.;
        let voltage = (current_a - motion[0]) / unit[0];
        let result = self.fields(voltage)?;
        self.iterations += unit_iterations;
        self.relative_residual = self.relative_residual.max(unit_residual);
        self.boundary_current_defect = self.boundary_current_defect.max(unit_boundary_defect);
        Ok((voltage, result))
    }
    fn port_currents(&mut self, voltage: f32) -> Result<[f32; 4], String> {
        self.params.physical[1] = voltage;
        self.write_params();
        self.dispatch(&[
            ("collect_ports", self.params.dims[3].div_ceil(256)),
            ("fields_reduce", 1),
        ]);
        Ok(self.scalar_values()?[2])
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
    /// Read the unscaled scalar solution for static research diagnostics.
    /// This does not modify PCG state or the exported current/force fields.
    pub fn read_scalar_solution(&self) -> Result<Vec<f32>, String> {
        let work = &self.buffers[2];
        let bytes = read_bytes(&self.context, work, 0, work.size())?;
        let predictors = &self.buffers[8];
        let low_bytes = read_bytes(&self.context, predictors, 0, predictors.size())?;
        let low = bytemuck::cast_slice::<u8, [f32; 4]>(&low_bytes);
        Ok(bytemuck::cast_slice::<u8, [f32; 4]>(&bytes)
            .iter()
            .zip(low)
            .map(|(v, correction)| v[0] + correction[self.params.control[0] as usize + 2])
            .collect())
    }
    pub fn read_fields(&self) -> Result<Vec<CellField>, String> {
        if self.compact {
            return Err("Compact output contains force/heat only; use full diagnostic constructor for J/B fields".into());
        }
        let bytes = read_bytes(&self.context, &self.result, 0, self.result.size())?;
        Ok(bytemuck::cast_slice(&bytes).to_vec())
    }
}

fn shader_source(compact: bool) -> String {
    let s = include_str!("solver.wgsl").to_owned();
    if !compact {
        return s;
    }
    s.replace("bias:array<Field>", "bias:array<vec4<f32>>")
        .replace("output:array<Field>", "output:array<vec4<f32>>")
        .replace("bias[i].b", "bias[i]")
        .replace("output[i]=Field(b,vec4<f32>(jvec,divergence),vec4<f32>(cross(jvec,b.xyz),power/(h*h*h)),bias[i].applied);",
                 "output[i]=vec4<f32>(cross(jvec,b.xyz),power/(h*h*h));")
}
fn pack_bias(context: &GpuContext, wide: &wgpu::Buffer, n: u32, row: u32) -> wgpu::Buffer {
    let packed = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Compact fixed magnetic field"),
        size: u64::from(n) * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let source = format!(
        r#"
        struct Field {{ b:vec4<f32>, j:vec4<f32>, f:vec4<f32>, a:vec4<f32> }}
        @group(0) @binding(0) var<storage,read> wide:array<Field>;
        @group(0) @binding(1) var<storage,read_write> packed:array<vec4<f32>>;
        @compute @workgroup_size(256) fn pack(@builtin(global_invocation_id) g:vec3<u32>) {{
            let i=g.x+g.y*{row}u*256u; if i<{n}u {{packed[i]=wide[i].b;}}
        }}"#
    );
    let module = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Pack fixed B without a host field roundtrip"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let pipeline = context
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Pack fixed B"),
            layout: None,
            module: &module,
            entry_point: Some("pack"),
            compilation_options: Default::default(),
            cache: None,
        });
    let bind = context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Pack fixed B"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wide.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: packed.as_entire_binding(),
                },
            ],
        });
    let mut encoder = context.device.create_command_encoder(&Default::default());
    {
        let mut pass = context.compute_pass(&mut encoder, Some("Pack fixed B"));
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        let groups = n.div_ceil(256);
        pass.dispatch_workgroups(groups.min(row), groups.div_ceil(row), 1);
    }
    context.queue.submit(Some(encoder.finish()));
    packed
}

#[cfg(test)]
mod optimization_tests {
    use super::*;
    /// Isolate work-list scheduling on a known one-volt conducting bar.
    #[test]
    #[ignore = "GPU work-list benchmark; run explicitly with --nocapture"]
    fn sparse_and_dense_conductor_timing() {
        let context = GpuContext::headless().unwrap();
        let dims = [129u32, 128, 128];
        let count = dims.iter().product::<u32>();
        let field = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Zero applied field"),
            size: 64 * u64::from(count),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        for side in [12, 48, 128] {
            let cells: Vec<_> = (0..count)
                .map(|i| {
                    let x = i % dims[0];
                    if i / dims[0] % dims[1] >= side || i / (dims[0] * dims[1]) >= side {
                        return [0., 2.];
                    }
                    let flag = if x == 0 {
                        0.5
                    } else if x + 1 == dims[0] {
                        -0.5
                    } else {
                        2.
                    };
                    [1., flag]
                })
                .collect();
            let mut solver = Conduction::new(context.clone(), dims, 1e-4, &cells, &field).unwrap();
            solver.params.control[2] = 0; // identical cold guess on every repeat
            let mut reference = None;
            for compact in [false, true, true, false] {
                let start = std::time::Instant::now();
                solver.solve_prepared(true, compact).unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                let current = solver.port_currents(1.).unwrap()[0];
                let analytic = (side * side) as f32 * 100. / (dims[0] - 1) as f32;
                assert!((current - analytic).abs() < analytic * 1e-4);
                let check = (solver.iterations, current, solver.relative_residual);
                assert_eq!(*reference.get_or_insert(check), check);
                eprintln!(
                    "bar_side={side} compact={compact} dispatched={}/{} iterations={} solve_s={elapsed:.6}",
                    if solver.params.control[3] == 0 {
                        count.div_ceil(256)
                    } else {
                        solver.params.control[3]
                    },
                    count.div_ceil(256),
                    solver.iterations
                );
            }
        }
    }
    #[test]
    fn optimized_ports_compact_packets_and_submission_window_match_reference() {
        let context = GpuContext::headless().unwrap();
        let dims = [17, 17, 12];
        let n = dims.iter().product::<u32>() as usize;
        let cells: Vec<_> = (0..n)
            .map(|i| {
                if i >= 17 * 17 * 3 {
                    return [0., 2.];
                }
                [
                    if i % 17 < 5 { 50. } else { 2.2 },
                    if i % 17 == 0 {
                        0.5
                    } else if i % 17 == 16 {
                        -0.5
                    } else if i % 17 >= 5 {
                        3.
                    } else {
                        2.
                    },
                ]
            })
            .collect();
        let groups: Vec<_> = (0..n).map(|i| if i % 17 < 5 { 14 } else { 20 }).collect();
        let b: Vec<_> = (0..n)
            .map(|i| CellField {
                b: [0.01 * (i % 17) as f32, 0.5, 0., 0.],
                ..Default::default()
            })
            .collect();
        let bias = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Optimization magnetic fixture"),
                contents: bytemuck::cast_slice(&b),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let flow = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Moving alpha and shear"),
            size: n as u64 * 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut reference = Conduction::new_layout(
            context.clone(),
            dims,
            1e-4,
            &cells,
            &bias,
            Some(&groups),
            (3, false),
        )
        .unwrap();
        let mut optimized = Conduction::new_layout(
            context.clone(),
            dims,
            1e-4,
            &cells,
            &bias,
            Some(&groups),
            (3, false),
        )
        .unwrap();
        let mut compact = Conduction::new_layout(
            context.clone(),
            dims,
            1e-4,
            &cells,
            &bias,
            Some(&groups),
            (3, true),
        )
        .unwrap();
        assert_eq!(compact.buffers[5].size(), n as u64 * 16);
        assert_eq!(compact.result.size(), n as u64 * 16);
        for solver in [&mut reference, &mut optimized, &mut compact] {
            solver.attach_flow(&flow).unwrap();
            solver.set_pair_resistivity(14, 20, 1e-10).unwrap();
            solver.set_contact_resistivity(1e-10).unwrap();
            solver
                .set_motional_boundary_absolute_tolerance_a(1e-6)
                .unwrap();
        }
        for (step, current) in [100., 0., -25.].into_iter().enumerate() {
            let phase: Vec<_> = (0..n)
                .map(|i| CellField {
                    b: [
                        0.,
                        0.,
                        0.1 * (std::f32::consts::TAU * ((i / 17) % 17) as f32 / 17.).sin(),
                        0.,
                    ],
                    applied: [
                        0.,
                        0.,
                        if step == 0 {
                            1.
                        } else {
                            0.3 + 0.7 * (i % 3) as f32 / 2.
                        },
                        0.,
                    ],
                    ..Default::default()
                })
                .collect();
            context
                .queue
                .write_buffer(&flow, 0, bytemuck::cast_slice(&phase));
            context.set_bounded_submission_window(1).unwrap();
            let start = std::time::Instant::now();
            let expected = reference.solve_current_impl(current, false).unwrap();
            let reference_ms = start.elapsed().as_secs_f64() * 1000.;
            let reference_counts = context.bounded_submission_counts();
            let fields = reference.read_fields().unwrap();
            context.set_bounded_submission_window(4).unwrap();
            let start = std::time::Instant::now();
            let actual = optimized.solve_current(current).unwrap();
            assert!(
                optimized.params.control[3] > 0,
                "exercise the sparse work list"
            );
            let optimized_ms = start.elapsed().as_secs_f64() * 1000.;
            let optimized_counts = context.bounded_submission_counts();
            assert_eq!(expected, actual, "full output stats at command {current}");
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&fields),
                bytemuck::cast_slice::<_, u8>(&optimized.read_fields().unwrap())
            );
            let small = compact.solve_current(current).unwrap();
            assert_eq!(expected, small, "compact stats at command {current}");
            let bytes = read_bytes(&context, &compact.result, 0, compact.result.size()).unwrap();
            let packed = bytemuck::cast_slice::<u8, [f32; 4]>(&bytes);
            for (full, small) in fields.iter().zip(packed) {
                assert_eq!(&full.force_heat, small);
            }
            assert_eq!(reference.iterations, optimized.iterations);
            assert_eq!(reference.iterations, compact.iterations);
            eprintln!(
                "step={step} command={current} reference_ms={reference_ms:.3} optimized_ms={optimized_ms:.3} bounded_submissions/waits={reference_counts:?}->{optimized_counts:?}"
            );
        }
        assert!(compact.read_fields().is_err());
    }
}
