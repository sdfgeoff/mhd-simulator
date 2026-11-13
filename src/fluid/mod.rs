//! GPU MAC velocity/pressure with fixed-region and variable-density liquid/air modes.
//! Static CAD walls are impermeable/no-slip; phase can suppress material motion.
use crate::{
    domain::Volume,
    gpu::{GpuContext, read_bytes},
    inputs::InputVersions,
    simulation::GridSi,
};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use wgpu::util::DeviceExt;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FluidConfig {
    pub surface: crate::surface::SurfaceConfig,
    pub material_id: u32,
    pub dt_s: f64,
    pub acceleration_m_s2: [f32; 3],
    pub initial_velocity_m_s: [f32; 3],
    pub periodic: [bool; 3],
    pub advection: bool,
    pub use_em_force: bool,
    pub mushy_drag_rate_s_inv: f32,
    pub relative_tolerance: f32,
    pub absolute_divergence_tolerance: f32,
    pub max_iterations: u32,
}
impl Default for FluidConfig {
    fn default() -> Self {
        Self {
            surface: Default::default(),
            material_id: 3,
            dt_s: 1e-5,
            acceleration_m_s2: [0., 0., -9.81],
            initial_velocity_m_s: [0.; 3],
            periodic: [false; 3],
            advection: true,
            use_em_force: false,
            mushy_drag_rate_s_inv: 1e6,
            relative_tolerance: 1e-5,
            absolute_divergence_tolerance: 1e-5,
            max_iterations: 1000,
        }
    }
}
impl FluidConfig {
    pub fn validate(&self) -> Result<(), String> {
        self.surface.validate()?;
        if self.surface.enabled && self.periodic.iter().any(|p| *p) {
            return Err("Moving-interface runs currently require nonperiodic domain faces".into());
        }
        if !self.mushy_drag_rate_s_inv.is_finite()
            || self.mushy_drag_rate_s_inv <= 0.
            || !self.dt_s.is_finite()
            || self.dt_s <= 0.
            || !(self.dt_s as f32).is_finite()
            || self.dt_s as f32 <= 0.
            || self
                .acceleration_m_s2
                .iter()
                .chain(&self.initial_velocity_m_s)
                .any(|v| !v.is_finite())
            || !self.absolute_divergence_tolerance.is_finite()
            || self.absolute_divergence_tolerance < 0.
            || !(1e-7..=1e-2).contains(&self.relative_tolerance)
            || !(1..=20000).contains(&self.max_iterations)
        {
            return Err("Invalid fluid timestep, initial condition or pressure tolerance".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct FluidDiagnostics {
    pub iterations: u32,
    pub relative_residual: f32,
    pub elapsed_ms: f64,
    pub max_speed_m_s: f32,
    pub max_divergence_s_inv: f32,
    pub rms_divergence_s_inv: f32,
    pub max_pressure_pa: f32,
    pub kinetic_energy_j: f32,
    pub fluid_mass_kg: f32,
    pub net_flux_m3_s: f32,
}
/// Display packet: velocity/speed, pressure/divergence, applied force, reserved.
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct FluidCell {
    pub velocity: [f32; 4],
    pub pressure: [f32; 4],
    pub force: [f32; 4],
    pub reserved: [f32; 4],
}
#[derive(Clone, Debug, Default)]
pub struct EnergyStep {
    pub kinetic_j: f64,
    pub delta_kinetic_j: f64,
    pub em_work_j: f64,
    pub acceleration_work_j: f64,
    pub surface_work_j: f64,
}
pub struct ForceField<'a> {
    pub buffer: &'a wgpu::Buffer,
    pub grid: &'a GridSi,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    dims: [u32; 4],
    faces: [u32; 4],
    flags: [u32; 4],
    physical: [f32; 4],
    acceleration: [f32; 4],
    initial: [f32; 4],
    solve: [f32; 4],
    surface: [f32; 4],
}
struct PhaseGpu {
    pipeline: wgpu::ComputePipeline,
    surface_pipeline: wgpu::ComputePipeline,
    zero: wgpu::Buffer,
    uniform: wgpu::Buffer,
    changed: wgpu::Buffer,
}
pub struct FluidState {
    context: GpuContext,
    pub grid: GridSi,
    pub time_s: f64,
    pub diagnostics: FluidDiagnostics,
    pub fluid_cells: u64,
    pub reference_probe_mm: [f32; 3],
    pub components: usize,
    pub bytes: u64,
    config: FluidConfig,
    input_versions: InputVersions,
    params: Params,
    _uniform: wgpu::Buffer,
    _mask: wgpu::Buffer,
    velocity: wgpu::Buffer,
    scratch: wgpu::Buffer,
    _work: wgpu::Buffer,
    _partials: wgpu::Buffer,
    scalars: wgpu::Buffer,
    result: wgpu::Buffer,
    previous: wgpu::Buffer,
    checkpoint: wgpu::Buffer,
    _force: wgpu::Buffer,
    bind: wgpu::BindGroup,
    energy_bind: wgpu::BindGroup,
    phase_gpu: Option<PhaseGpu>,
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    force_max: f32,
    max_face_speed: f32,
}
impl FluidState {
    pub fn new(
        context: GpuContext,
        volume: &Volume,
        config: FluidConfig,
        force: Option<ForceField<'_>>,
    ) -> Result<Self, String> {
        if volume.overlaps > 0 {
            return Err("Resolve overlapping components before fluids".into());
        }
        config.validate()?;
        let material = volume
            .inputs
            .snapshot()
            .materials
            .iter()
            .find(|m| m.id == config.material_id)
            .ok_or("Select a fluid material")?;
        let rho = material
            .density_kg_m3
            .filter(|v| v.is_finite() && *v > 0.)
            .ok_or("Fluid material needs a positive density")?;
        let mu = material
            .dynamic_viscosity_pa_s
            .filter(|v| v.is_finite() && *v > 0.)
            .ok_or("Fluid material needs a positive liquid viscosity")?;
        let nu = mu / rho;
        if !nu.is_finite() || nu <= 0. {
            return Err("Fluid viscosity/density ratio is outside the numerical range".into());
        }
        let grid = volume.inputs.snapshot().grid.clone();
        if config.dt_s as f32 > grid.cell_m * grid.cell_m / (6. * nu) * 0.95 {
            return Err(
                "Fluid timestep exceeds the explicit viscosity limit h²/(6ν); reduce Δt".into(),
            );
        }
        let n = grid.dims.iter().map(|&v| v as u64).product::<u64>();
        let f = grid.dims.map(|v| v + 1);
        let nf = f.iter().map(|&v| v as u64).product::<u64>();
        let sizes = [
            n * 4,
            nf * 16,
            nf * 16,
            n * 16,
            n.div_ceil(256) * 48,
            80,
            n * 64,
            nf * 16,
            n * 64,
            if config.use_em_force { n * 64 } else { 64 },
        ];
        let bytes = sizes.iter().sum::<u64>();
        let limits = context.device.limits();
        if sizes.iter().any(|&s| {
            s > limits.max_buffer_size || s > limits.max_storage_buffer_binding_size as u64
        }) || n > u32::MAX as u64
            || nf > u32::MAX as u64
            || bytes + volume.persistent_bytes > 3 * 1024 * 1024 * 1024
        {
            return Err(format!(
                "Fluid allocation exceeds device limits or the 3 GiB working budget ({:.1} MiB)",
                bytes as f64 / 1048576.
            ));
        }
        let ids = read_bytes(&context, volume.material_ids(), 0, n * 4)?;
        let mut mask: Vec<u32> = bytemuck::cast_slice::<u8, u32>(&ids)
            .iter()
            .map(|&id| u32::from(id == config.material_id || (config.surface.enabled && id == 0)))
            .collect();
        let fluid_cells = mask.iter().filter(|&&v| v != 0).count() as u64;
        if fluid_cells == 0 {
            return Err("The selected fluid material occupies no cells in this domain".into());
        }
        let mut centroid = [0f64; 3];
        for (i, _) in mask.iter().enumerate().filter(|(_, v)| **v != 0) {
            let p = [
                i % grid.dims[0] as usize,
                (i / grid.dims[0] as usize) % grid.dims[1] as usize,
                i / (grid.dims[0] * grid.dims[1]) as usize,
            ];
            for axis in 0..3 {
                centroid[axis] += p[axis] as f64 + 0.5;
            }
        }
        for c in &mut centroid {
            *c /= fluid_cells as f64;
        }
        let mut best = (f64::INFINITY, [0.; 3]);
        for (i, _) in mask.iter().enumerate().filter(|(_, v)| **v != 0) {
            let q = [
                i % grid.dims[0] as usize,
                (i / grid.dims[0] as usize) % grid.dims[1] as usize,
                i / (grid.dims[0] * grid.dims[1]) as usize,
            ]
            .map(|v| v as f64 + 0.5);
            let distance = (0..3).map(|a| (q[a] - centroid[a]).powi(2)).sum();
            if distance < best.0 {
                best = (
                    distance,
                    std::array::from_fn(|a| (grid.origin_m[a] + q[a] as f32 * grid.cell_m) * 1000.),
                );
            }
        }
        let reference_probe_mm = best.1;
        let components = anchors(&mut mask, grid.dims, config.periodic);
        let force = if config.use_em_force {
            let field = force
                .ok_or("Compute electromagnetics first, then select a recorded force frame")?;
            if field.grid != &grid || field.buffer.size() < n * 64 {
                return Err("Frozen EM force must use the same fluid grid".into());
            }
            if !field.buffer.usage().contains(wgpu::BufferUsages::COPY_SRC) {
                return Err("EM force must support a GPU snapshot copy".into());
            }
            let frozen = context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Frozen EM force snapshot"),
                size: n * 64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = context.device.create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(field.buffer, 0, &frozen, 0, n * 64);
            context.queue.submit(Some(encoder.finish()));
            frozen
        } else {
            context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Zero optional force"),
                    contents: &[0; 64],
                    usage: wgpu::BufferUsages::STORAGE,
                })
        };
        let params = Params {
            surface: [
                f32::from(config.surface.enabled),
                config.surface.air_density_kg_m3,
                config.surface.tension_n_m,
                0.,
            ],
            dims: [grid.dims[0], grid.dims[1], grid.dims[2], n as u32],
            faces: [f[0], f[1], f[2], nf as u32],
            flags: [
                config
                    .periodic
                    .iter()
                    .enumerate()
                    .fold(0, |bits, (i, &v)| bits | ((v as u32) << i)),
                u32::from(config.advection),
                u32::from(config.use_em_force),
                0,
            ],
            physical: [grid.cell_m, config.dt_s as f32, nu, rho],
            acceleration: [
                config.acceleration_m_s2[0],
                config.acceleration_m_s2[1],
                config.acceleration_m_s2[2],
                0.,
            ],
            initial: [
                config.initial_velocity_m_s[0],
                config.initial_velocity_m_s[1],
                config.initial_velocity_m_s[2],
                0.,
            ],
            solve: [
                config.relative_tolerance.powi(2),
                config.absolute_divergence_tolerance.powi(2)
                    * grid.cell_m.powi(4)
                    * fluid_cells as f32,
                fluid_cells as f32,
                0.,
            ],
        };
        let uniform = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Fluid solver parameters"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let mask = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Fluid cells and pressure gauges"),
                contents: bytemuck::cast_slice(&mask),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            });
        let buffer = |label: &str, size: u64| {
            context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let velocity = buffer("MAC face velocities", nf * 16);
        let scratch = buffer("Predicted face velocities", nf * 16);
        let work = buffer("Pressure PCG state", n * 16);
        let partials = buffer("Fluid reductions", n.div_ceil(256) * 48);
        let scalars = buffer("Fluid diagnostics", 80);
        let result = buffer("Fluid display fields", n * 64);
        let previous = buffer("Velocity checkpoint", nf * 16);
        let checkpoint = buffer("Fluid field checkpoint", n * 64);
        if config.surface.enabled {
            let packets: Vec<_> = bytemuck::cast_slice::<u8, u32>(&ids)
                .iter()
                .map(|id| FluidCell {
                    reserved: [1., 1., f32::from(*id == config.material_id), 0.],
                    ..Default::default()
                })
                .collect();
            context
                .queue
                .write_buffer(&result, 0, bytemuck::cast_slice(&packets));
        }

        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("MAC fluid solver"),
                source: wgpu::ShaderSource::Wgsl(include_str!("solver.wgsl").into()),
            });
        let entries = [
            "initialize",
            "predict",
            "viscous",
            "pressure_guess",
            "pressure_warm",
            "pressure_commit",
            "phase_faces",
            "pressure_refresh",
            "initial_reduce",
            "pressure_apply",
            "alpha_reduce",
            "update",
            "beta_reduce",
            "search",
            "correct",
            "fields",
            "diagnostics",
            "energy_collect",
            "energy_reduce",
        ];
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &(0..9)
                    .map(|binding| wgpu::BindGroupLayoutEntry {
                        binding,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: if binding == 0 {
                                wgpu::BufferBindingType::Uniform
                            } else {
                                wgpu::BufferBindingType::Storage {
                                    read_only: binding == 1 || binding == 8,
                                }
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    })
                    .collect::<Vec<_>>(),
            });
        let pipeline_layout =
            context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[&layout],
                    push_constant_ranges: &[],
                });
        let pipelines = entries
            .into_iter()
            .map(|entry| {
                (
                    entry,
                    context
                        .device
                        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                            label: Some(entry),
                            layout: Some(&pipeline_layout),
                            module: &shader,
                            entry_point: Some(entry),
                            compilation_options: Default::default(),
                            cache: None,
                        }),
                )
            })
            .collect();
        let buffers = [
            &uniform, &mask, &velocity, &scratch, &work, &partials, &scalars, &result, &force,
        ];
        let bind = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Fluid inputs and state"),
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
        let energy_buffers = [
            &uniform, &mask, &velocity, &previous, &work, &partials, &scalars, &result, &force,
        ];
        let energy_bind = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Fluid work integrals"),
                layout: &layout,
                entries: &energy_buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect::<Vec<_>>(),
            });
        let mut state = Self {
            context,
            grid,
            time_s: 0.,
            diagnostics: Default::default(),
            fluid_cells,
            reference_probe_mm,
            components,
            bytes,
            config,
            input_versions: volume.inputs.versions(),
            params,
            _uniform: uniform,
            _mask: mask,
            velocity,
            scratch,
            _work: work,
            _partials: partials,
            scalars,
            result,
            previous,
            checkpoint,
            _force: force,
            bind,
            energy_bind,
            phase_gpu: None,
            pipelines,
            force_max: 0.,
            max_face_speed: 0.,
        };
        state.dispatch(&[("initialize", groups(nf as u32))]);
        let mut encoder = state
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&state.velocity, 0, &state.scratch, 0, state.velocity.size());
        state.context.queue.submit(Some(encoder.finish()));
        let (iterations, residual) = state.project()?;
        state.dispatch(&[("fields", groups(n as u32)), ("diagnostics", [1, 1, 1])]);
        state.read_diagnostics(iterations, residual, 0.)?;
        Ok(state)
    }
    /// Replace the driving Lorentz force before a coupled step (GPU to GPU).
    pub fn update_force(&mut self, field: ForceField<'_>) -> Result<(), String> {
        if !self.config.use_em_force
            || field.grid != &self.grid
            || field.buffer.size() < self.result.size()
            || !field.buffer.usage().contains(wgpu::BufferUsages::COPY_SRC)
        {
            return Err("Coupled force requires enabled EM forcing and matching GPU grids".into());
        }
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(field.buffer, 0, &self._force, 0, self.result.size());
        self.context.queue.submit(Some(encoder.finish()));
        // Refresh the force bound before the CFL check, not after taking the step.
        self.dispatch(&[
            ("fields", groups(self.params.dims[3])),
            ("diagnostics", [1, 1, 1]),
        ]);
        self.read_diagnostics(
            self.diagnostics.iterations,
            self.diagnostics.relative_residual,
            0.,
        )?;
        Ok(())
    }
    /// GPU phase classification; CPU work only prepares component gauges when topology changes.
    pub fn update_phase(&mut self, volume: &Volume, thermal: &wgpu::Buffer) -> Result<(), String> {
        self.update_cells(volume, Some(thermal), None)
    }
    pub fn update_surface(
        &mut self,
        volume: &Volume,
        surface: &wgpu::Buffer,
        thermal: Option<&wgpu::Buffer>,
    ) -> Result<(), String> {
        self.update_cells(volume, thermal, Some(surface))
    }
    fn update_cells(
        &mut self,
        volume: &Volume,
        thermal: Option<&wgpu::Buffer>,
        surface: Option<&wgpu::Buffer>,
    ) -> Result<(), String> {
        if thermal.is_some_and(|t| t.size() < self.result.size()) {
            return Err("Phase field does not match fluid grid".into());
        }
        if self.phase_gpu.is_none() {
            let shader = self
                .context
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Thermal phase to fluid mobility"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("phase.wgsl").into()),
                });
            let pipeline =
                self.context
                    .device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some("Phase classification"),
                        layout: None,
                        module: &shader,
                        entry_point: Some("phase"),
                        compilation_options: Default::default(),
                        cache: None,
                    });
            let surface_pipeline =
                self.context
                    .device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some("Moving liquid/air and thermal phase classification"),
                        layout: None,
                        module: &shader,
                        entry_point: Some("surface"),
                        compilation_options: Default::default(),
                        cache: None,
                    });
            let zero = self.context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Absent thermal phase"),
                size: 64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            let uniform =
                self.context
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Fluid phase material"),
                        contents: bytemuck::cast_slice(&[
                            self.params.dims,
                            [self.config.material_id, u32::from(thermal.is_some()), 0, 0],
                        ]),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });
            let changed = self.context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Phase topology flag"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.phase_gpu = Some(PhaseGpu {
                surface_pipeline,
                zero,
                pipeline,
                uniform,
                changed,
            });
        }
        let phase = self.phase_gpu.as_ref().unwrap();
        self.context.queue.write_buffer(
            &phase.uniform,
            0,
            bytemuck::cast_slice(&[
                self.params.dims,
                [self.config.material_id, u32::from(thermal.is_some()), 0, 0],
            ]),
        );
        self.context.queue.write_buffer(&phase.changed, 0, &[0; 4]);
        let mut buffers = vec![
            &phase.uniform,
            thermal.unwrap_or(&phase.zero),
            volume.material_ids(),
            &self._mask,
            &self.result,
            &phase.changed,
        ];
        if let Some(field) = surface {
            buffers.push(field);
        }
        let pipeline = if surface.is_some() {
            &phase.surface_pipeline
        } else {
            &phase.pipeline
        };
        let bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Phase and occupancy"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect::<Vec<_>>(),
            });
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            let g = groups(self.params.dims[3]);
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
        self.context.queue.submit(Some(enc.finish()));
        let changed = read_bytes(&self.context, &phase.changed, 0, 4)?;
        let changed = bytemuck::pod_read_unaligned::<u32>(&changed) != 0;
        if changed {
            let data = read_bytes(
                &self.context,
                &self._mask,
                0,
                self.params.dims[3] as u64 * 4,
            )?;
            let mut mask: Vec<u32> = bytemuck::cast_slice::<u8, u32>(&data)
                .iter()
                .map(|v| u32::from(*v != 0))
                .collect();
            self.fluid_cells = mask.iter().filter(|v| **v != 0).count() as u64;
            self.components = anchors(&mut mask, self.grid.dims, self.config.periodic);
            self.context
                .queue
                .write_buffer(&self._mask, 0, bytemuck::cast_slice(&mask));
        }
        self.params.flags[3] = u32::from(thermal.is_some());
        self.params.solve[3] = self.config.mushy_drag_rate_s_inv;
        self.params.solve[1] = self.config.absolute_divergence_tolerance.powi(2)
            * self.grid.cell_m.powi(4)
            * self.fluid_cells as f32;
        self.params.solve[2] = self.fluid_cells.max(1) as f32;
        self.context
            .queue
            .write_buffer(&self._uniform, 0, bytemuck::bytes_of(&self.params));
        let mut iterations = 0;
        let mut residual = 0.;
        if changed {
            self.dispatch(&[("phase_faces", groups(self.params.faces[3]))]);
            let mut enc = self
                .context
                .device
                .create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(&self.velocity, 0, &self.scratch, 0, self.velocity.size());
            self.context.queue.submit(Some(enc.finish()));
            (iterations, residual) = self.project()?;
        }
        self.dispatch(&[
            ("fields", groups(self.params.dims[3])),
            ("diagnostics", [1, 1, 1]),
        ]);
        self.read_diagnostics(iterations, residual, 0.)?;
        Ok(())
    }
    pub fn config(&self) -> &FluidConfig {
        &self.config
    }
    pub fn is_current(&self, volume: &Volume) -> bool {
        self.input_versions == volume.inputs.versions()
    }
    pub(crate) fn face_buffer(&self) -> &wgpu::Buffer {
        &self.velocity
    }
    pub(crate) fn max_face_speed(&self) -> f32 {
        self.max_face_speed
    }
    pub fn result_buffer(&self) -> &wgpu::Buffer {
        &self.result
    }
    fn dispatch(&self, entries: &[(&str, [u32; 3])]) {
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_bind_group(0, &self.bind, &[]);
            for (entry, g) in entries {
                pass.set_pipeline(&self.pipelines[entry]);
                pass.dispatch_workgroups(g[0], g[1], g[2]);
            }
        }
        self.context.queue.submit(Some(encoder.finish()));
    }
    fn convergence(&self) -> Result<(f32, bool), String> {
        let data = read_bytes(&self.context, &self.scalars, 0, 32)?;
        let s = bytemuck::cast_slice::<u8, f32>(&data);
        let residual = (s[1] / s[2].max(1e-38)).sqrt();
        if !residual.is_finite() || s[1] < 0. || s[2] < 0. {
            return Err("Nonfinite fluid pressure residual".into());
        }
        Ok((residual, s[6] > 0.5))
    }
    fn project(&mut self) -> Result<(u32, f32), String> {
        let cells = groups(self.params.dims[3]);
        self.dispatch(&[
            ("pressure_warm", groups(self.params.faces[3])),
            ("pressure_guess", cells),
            ("pressure_refresh", cells),
            ("initial_reduce", [1, 1, 1]),
        ]);
        let mut iterations = 0;
        let mut residual = self.convergence()?;
        while !residual.1 && iterations < self.config.max_iterations {
            let batch = 8.min(self.config.max_iterations - iterations);
            let mut stages = Vec::new();
            for _ in 0..batch {
                stages.extend([
                    ("pressure_apply", cells),
                    ("alpha_reduce", [1, 1, 1]),
                    ("update", cells),
                    ("beta_reduce", [1, 1, 1]),
                    ("search", cells),
                ]);
            }
            self.dispatch(&stages);
            iterations += batch;
            residual = self.convergence()?;
            if residual.1 {
                self.dispatch(&[("pressure_refresh", cells), ("initial_reduce", [1, 1, 1])]);
                residual = self.convergence()?;
            }
        }
        if !residual.1 {
            return Err(format!(
                "Fluid pressure solve did not converge after {iterations} iterations (relative residual {:.3e})",
                residual.0
            ));
        }
        self.dispatch(&[
            ("correct", groups(self.params.faces[3])),
            ("pressure_commit", cells),
        ]);
        Ok((iterations, residual.0))
    }
    fn read_diagnostics(
        &mut self,
        iterations: u32,
        residual: f32,
        elapsed: f64,
    ) -> Result<(), String> {
        let bytes = read_bytes(&self.context, &self.scalars, 32, 48)?;
        let s = bytemuck::cast_slice::<u8, f32>(&bytes);
        if s.iter().any(|v| !v.is_finite()) || s[7] != 0. {
            return Err("Nonfinite fluid fields; state restored".into());
        }
        self.diagnostics = FluidDiagnostics {
            iterations,
            relative_residual: residual,
            elapsed_ms: elapsed,
            max_speed_m_s: s[0],
            max_divergence_s_inv: s[1],
            rms_divergence_s_inv: (s[2] / self.fluid_cells.max(1) as f32).sqrt(),
            kinetic_energy_j: s[3],
            fluid_mass_kg: self.params.physical[3] * s[9] * self.grid.cell_m.powi(3),
            max_pressure_pa: s[4],
            net_flux_m3_s: s[5] * self.grid.cell_m.powi(3),
        };
        self.force_max = s[6];
        self.max_face_speed = s[8];
        Ok(())
    }
    pub fn step(&mut self, volume: &Volume) -> Result<&FluidDiagnostics, String> {
        if !self.is_current(volume) {
            return Err("Fluid inputs changed; initialize fluids again".into());
        }
        let dt = self.config.dt_s as f32;
        let acceleration = glam::Vec3::from(self.config.acceleration_m_s2).length()
            + self.force_max / self.params.physical[3];
        if (self.max_face_speed * 3f32.sqrt() + acceleration * dt) * dt / self.grid.cell_m > 0.9 {
            return Err("Fluid timestep exceeds the velocity CFL limit; reduce Δt".into());
        }
        if self.config.surface.enabled {
            let capillary = if self.config.surface.tension_n_m > 0. {
                ((self.params.physical[3] + self.config.surface.air_density_kg_m3)
                    * self.grid.cell_m.powi(3)
                    / (2. * std::f32::consts::PI * self.config.surface.tension_n_m))
                    .sqrt()
            } else {
                f32::INFINITY
            };
            if dt > capillary
                || 3. * (self.max_face_speed + acceleration * dt) * dt / self.grid.cell_m > 0.45
            {
                return Err("Moving-interface capillary or conservative CFL limit exceeded; reduce common timestep".into());
            }
        }
        let start = std::time::Instant::now();
        let old_diagnostics = self.diagnostics.clone();
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.velocity, 0, &self.previous, 0, self.velocity.size());
        encoder.copy_buffer_to_buffer(&self.result, 0, &self.checkpoint, 0, self.result.size());
        self.context.queue.submit(Some(encoder.finish()));
        let mut run = || -> Result<(u32, f32), String> {
            let faces = groups(self.params.faces[3]);
            self.dispatch(&[("predict", faces), ("viscous", faces)]);
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(
                &self.velocity,
                0,
                &self.scratch,
                0,
                self.velocity.size(),
            );
            self.context.queue.submit(Some(encoder.finish()));
            self.project()
        };
        let result = run().and_then(|(iterations, residual)| {
            self.dispatch(&[
                ("fields", groups(self.params.dims[3])),
                ("diagnostics", [1, 1, 1]),
            ]);
            self.read_diagnostics(iterations, residual, 0.)
        });
        if let Err(error) = result {
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(
                &self.previous,
                0,
                &self.velocity,
                0,
                self.velocity.size(),
            );
            encoder.copy_buffer_to_buffer(&self.checkpoint, 0, &self.result, 0, self.result.size());
            self.context.queue.submit(Some(encoder.finish()));
            self.diagnostics = old_diagnostics;
            return Err(error);
        }
        self.diagnostics.elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
        self.time_s += self.config.dt_s;
        Ok(&self.diagnostics)
    }
    pub fn probe(&self, point_mm: [f32; 3]) -> Result<Option<FluidCell>, String> {
        probe_cell(&self.context, &self.grid, &self.result, point_mm)
    }
    pub fn energy_step(&self) -> Result<EnergyStep, String> {
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_bind_group(0, &self.energy_bind, &[]);
            pass.set_pipeline(&self.pipelines["energy_collect"]);
            let g = groups(self.params.dims[3]);
            pass.dispatch_workgroups(g[0], g[1], g[2]);
            pass.set_pipeline(&self.pipelines["energy_reduce"]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        self.context.queue.submit(Some(encoder.finish()));
        let data = read_bytes(&self.context, &self.scalars, 0, 32)?;
        let v = bytemuck::cast_slice::<u8, f32>(&data);
        if v.iter().any(|v| !v.is_finite()) {
            return Err("Nonfinite fluid work integral".into());
        }
        Ok(EnergyStep {
            kinetic_j: v[0] as f64,
            delta_kinetic_j: v[1] as f64,
            em_work_j: v[2] as f64,
            acceleration_work_j: v[3] as f64,
            surface_work_j: v[4] as f64,
        })
    }
    pub fn snapshot(&self) -> FluidFrame {
        let result = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Recorded fluid fields"),
            size: self.result.size(),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.result, 0, &result, 0, result.size());
        self.context.queue.submit(Some(encoder.finish()));
        FluidFrame {
            grid: self.grid.clone(),
            time_s: self.time_s,
            diagnostics: self.diagnostics.clone(),
            result,
            context: self.context.clone(),
            input_versions: self.input_versions,
            bytes: self.result.size(),
        }
    }
}
pub struct FluidFrame {
    pub grid: GridSi,
    pub time_s: f64,
    pub diagnostics: FluidDiagnostics,
    pub bytes: u64,
    result: wgpu::Buffer,
    context: GpuContext,
    input_versions: InputVersions,
}
impl FluidFrame {
    pub fn result_buffer(&self) -> &wgpu::Buffer {
        &self.result
    }
    pub fn is_current(&self, volume: &Volume) -> bool {
        self.input_versions == volume.inputs.versions()
    }
    pub fn probe(&self, point_mm: [f32; 3]) -> Result<Option<FluidCell>, String> {
        probe_cell(&self.context, &self.grid, &self.result, point_mm)
    }
}
fn probe_cell(
    context: &GpuContext,
    grid: &GridSi,
    result: &wgpu::Buffer,
    point_mm: [f32; 3],
) -> Result<Option<FluidCell>, String> {
    let mut p = [0u32; 3];
    for i in 0..3 {
        let q = ((point_mm[i] * 1e-3 - grid.origin_m[i]) / grid.cell_m).floor();
        if !q.is_finite() || q < 0. || q >= grid.dims[i] as f32 {
            return Ok(None);
        }
        p[i] = q as u32;
    }
    let index =
        p[0] as u64 + grid.dims[0] as u64 * (p[1] as u64 + grid.dims[1] as u64 * p[2] as u64);
    Ok(Some(bytemuck::pod_read_unaligned(&read_bytes(
        context,
        result,
        index * 64,
        64,
    )?)))
}
fn groups(n: u32) -> [u32; 3] {
    let n = n.div_ceil(256);
    [n.min(65535), n.div_ceil(65535), 1]
}
fn anchors(mask: &mut [u32], dims: [u32; 3], periodic: [bool; 3]) -> usize {
    let mut components = 0;
    let mut roots = vec![0u32; mask.len()];
    let mut queue = VecDeque::new();
    for root in 0..mask.len() {
        if mask[root] != 1 {
            continue;
        }
        components += 1;
        mask[root] = 4;
        roots[root] = root as u32;
        queue.push_back(root);
        while let Some(i) = queue.pop_front() {
            let p = [
                i % dims[0] as usize,
                (i / dims[0] as usize) % dims[1] as usize,
                i / (dims[0] * dims[1]) as usize,
            ];
            for axis in 0..3 {
                for direction in [-1i32, 1] {
                    let mut q = p;
                    let mut v = p[axis] as i32 + direction;
                    if periodic[axis] {
                        v = v.rem_euclid(dims[axis] as i32);
                    } else if v < 0 || v >= dims[axis] as i32 {
                        continue;
                    }
                    q[axis] = v as usize;
                    let j = q[0] + dims[0] as usize * (q[1] + dims[1] as usize * q[2]);
                    if mask[j] == 1 {
                        mask[j] = 3;
                        roots[j] = root as u32;
                        queue.push_back(j);
                    }
                }
            }
        }
    }
    for (v, root) in mask.iter_mut().zip(roots) {
        if *v == 3 {
            *v = 1 | (root << 2);
        } else if *v == 4 {
            *v = 2 | (root << 2);
        }
    }
    components
}
#[cfg(test)]
mod tests;
