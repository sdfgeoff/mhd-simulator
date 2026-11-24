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
    pub max_viscous_substeps: u32,
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
            max_viscous_substeps: 4096,
        }
    }
}
impl FluidConfig {
    pub(crate) fn stress_workspace_bytes(&self, alignment: u32) -> u64 {
        if self.surface.enabled {
            stabilized::bytes(alignment)
        } else {
            0
        }
    }
    pub fn validate(&self) -> Result<(), String> {
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
            || !(1..=stabilized::MAX_EULER_STEPS).contains(&self.max_viscous_substeps)
        {
            return Err(
                "Invalid fluid timestep, initial condition or solver limits/tolerances".into(),
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct FluidDiagnostics {
    pub iterations: u32,
    pub viscous_substeps: u32,
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
/// A force packet on the matching grid. Exactly16 bytes/cell selects compact
/// f32 force.xyz + heat; at least64 bytes/cell selects the legacy EM CellField.
/// Layout is fixed when FluidState is constructed, including snapshot updates.
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
    // Initial velocity xyz; inverse fluid-cell count w during mean-free pressure.
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
    /// Host wall times for the velocity and post-transport pressure projections.
    pub projection_wall_ms: [f64; 2],
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
    shared_force: bool,
    bind: wgpu::BindGroup,
    energy_bind: wgpu::BindGroup,
    stabilized: Option<stabilized::StabilizedGpu>,
    phase_gpu: Option<PhaseGpu>,
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    surface_tension_n_m: f32,
    air_density_kg_m3: f32,
    force_max: f32,
    max_face_speed: f32,
    momentum_projection_pending: bool,
    #[cfg(test)]
    pub(crate) euler_viscosity_reference: bool,
    #[cfg(test)]
    pub(crate) force_multigrid: Option<bool>,
    #[cfg(test)]
    pub(crate) force_meanfree_pressure: Option<bool>,
    #[cfg(test)]
    force_bounded_submissions: Option<bool>,
    multigrid: Option<multigrid::Multigrid>,
    multigrid_attempted: bool,
}
impl FluidState {
    pub fn new(
        context: GpuContext,
        volume: &Volume,
        config: FluidConfig,
        force: Option<ForceField<'_>>,
    ) -> Result<Self, String> {
        Self::new_with_force(context, volume, config, force, false)
    }
    /// Coupled runs consume the live EM output; standalone frozen-force runs keep a private copy.
    pub(crate) fn new_coupled(
        context: GpuContext,
        volume: &Volume,
        config: FluidConfig,
        force: Option<ForceField<'_>>,
    ) -> Result<Self, String> {
        Self::new_with_force(context, volume, config, force, true)
    }
    fn new_with_force(
        context: GpuContext,
        volume: &Volume,
        config: FluidConfig,
        force: Option<ForceField<'_>>,
        shared_force: bool,
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
            .filter(|v| v.is_finite() && *v >= 0.)
            .ok_or("Fluid material needs a nonnegative dynamic viscosity")?;
        let (surface_tension_n_m, air_density_kg_m3, air_viscosity_pa_s) = if config.surface.enabled
        {
            let tension = material
                .surface_tension_n_m
                .ok_or("Liquid material needs surface tension against the ambient material")?;
            let ambient = volume
                .inputs
                .snapshot()
                .materials
                .iter()
                .find(|m| m.id == 0)
                .ok_or("Ambient material (ID 0) is missing")?;
            let air = ambient
                .density_kg_m3
                .ok_or("Ambient material needs a density")?;
            (tension, air, ambient.dynamic_viscosity_pa_s.unwrap_or(0.))
        } else {
            (0., rho, 0.)
        };
        let nu = mu / rho;
        if !nu.is_finite() || nu < 0. {
            return Err("Fluid viscosity/density ratio is outside the numerical range".into());
        }
        let grid = volume.inputs.snapshot().grid.clone();
        if !config.surface.enabled
            && config.dt_s as f32 > grid.cell_m * grid.cell_m / (6. * nu) * 0.95
        {
            return Err(
                "Fluid timestep exceeds the explicit viscosity limit h²/(6ν); reduce Δt".into(),
            );
        }
        let n = grid.dims.iter().map(|&v| v as u64).product::<u64>();
        let f = grid.dims.map(|v| v + 1);
        let nf = f.iter().map(|&v| v as u64).product::<u64>();
        // Compact force packets are exactly vec4<f32> per cell; legacy packets
        // remain 64 bytes/cell. Reject other/truncated layouts before allocation.
        let force_bytes = if config.use_em_force {
            let field = force.as_ref().ok_or("Missing EM force")?;
            if field.grid != &grid {
                return Err("EM force grid mismatch".into());
            }
            if field.buffer.size() == n * 16 {
                n * 16
            } else if field.buffer.size() >= n * 64 {
                n * 64
            } else {
                return Err("EM force packet must be compact16 or full64 bytes/cell".into());
            }
        } else {
            64
        };
        let compact_force = config.use_em_force && force_bytes == n * 16;
        let sizes = [
            n * 4,
            nf * 16,
            nf * 16,
            n * 16,
            nf.max(n).div_ceil(256) * 48,
            80,
            n * 64,
            nf * 16,
            (n * 64).max(nf * 16),
            if config.use_em_force {
                if shared_force { 0 } else { force_bytes }
            } else {
                64
            },
        ];
        let stage_bytes = config
            .stress_workspace_bytes(context.device.limits().min_uniform_buffer_offset_alignment);
        let bytes = sizes.iter().sum::<u64>() + stage_bytes;
        let limits = context.device.limits();
        if sizes.iter().any(|&s| {
            s > limits.max_buffer_size || s > limits.max_storage_buffer_binding_size as u64
        }) || n > u32::MAX as u64
            || nf > u32::MAX as u64
        {
            return Err(format!(
                "Fluid allocation exceeds device buffer limits ({:.1} MiB)",
                bytes as f64 / 1048576.
            ));
        }
        context.ensure_memory(bytes, "Fluid working buffers")?;
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
            if field.grid != &grid || field.buffer.size() < force_bytes {
                return Err("Frozen EM force must use the same fluid grid".into());
            }
            if !field.buffer.usage().contains(wgpu::BufferUsages::COPY_SRC) {
                return Err("EM force must support a GPU snapshot copy".into());
            }
            if shared_force {
                if !field.buffer.usage().contains(wgpu::BufferUsages::STORAGE) {
                    return Err("Coupled EM force must support storage binding".into());
                }
                field.buffer.clone()
            } else {
                let frozen = context.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Frozen EM force snapshot"),
                    size: force_bytes,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let mut encoder = context.device.create_command_encoder(&Default::default());
                encoder.copy_buffer_to_buffer(field.buffer, 0, &frozen, 0, force_bytes);
                context.queue.submit(Some(encoder.finish()));
                frozen
            }
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
                air_density_kg_m3,
                surface_tension_n_m,
                air_viscosity_pa_s,
            ],
            dims: [grid.dims[0], grid.dims[1], grid.dims[2], n as u32],
            faces: [f[0], f[1], f[2], nf as u32],
            flags: [
                config
                    .periodic
                    .iter()
                    .enumerate()
                    .fold(0, |bits, (i, &v)| bits | ((v as u32) << i)),
                u32::from(config.advection && !config.surface.enabled),
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
        let partials = buffer("Fluid reductions", nf.max(n).div_ceil(256) * 48);
        let scalars = buffer("Fluid diagnostics", 80);
        let result = buffer("Fluid display fields", n * 64);
        let previous = buffer("Velocity checkpoint", nf * 16);
        let checkpoint = buffer("Fluid field / advector checkpoint", (n * 64).max(nf * 16));
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
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}\n{}\n{}\n{}\n{}",
                        include_str!("../periodic.wgsl"),
                        include_str!("viscosity.wgsl"),
                        if compact_force {
                            include_str!("solver.wgsl")
                                .replace("em:array<Result>", "em:array<vec4<f32>>")
                                .replace(
                                    "em[index(q,p.dims.xyz)].force.xyz",
                                    "em[index(q,p.dims.xyz)].xyz",
                                )
                        } else {
                            include_str!("solver.wgsl").to_owned()
                        },
                        include_str!("stabilized.wgsl"),
                        include_str!("pressure_subspace.wgsl"),
                        if cfg!(test) {
                            include_str!("stress_probes.wgsl")
                        } else {
                            ""
                        }
                    )
                    .into(),
                ),
            });
        let entries = [
            "initialize",
            "predict",
            "viscous",
            "viscosity_bound",
            "viscosity_bound_reduce",
            "stress_diffuse",
            "pressure_coefficients",
            "pressure_residual_mean",
            "pressure_reduce_blocks",
            "pressure_initial_subspace",
            "pressure_residual_subspace",
            "pressure_guess",
            "pressure_warm",
            "pressure_commit",
            "phase_faces",
            "pressure_refresh",
            "pressure_check",
            "check_reduce",
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
            .chain(
                ["stress_original_probe", "stress_cached_probe"]
                    .into_iter()
                    .filter(|_| cfg!(test)),
            )
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
        let stabilized = config
            .surface
            .enabled
            .then(|| stabilized::StabilizedGpu::new(&context, &shader, buffers));
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
            shared_force,
            bind,
            energy_bind,
            stabilized,
            phase_gpu: None,
            pipelines,
            surface_tension_n_m,
            air_density_kg_m3,
            force_max: 0.,
            projection_wall_ms: [0.; 2],
            momentum_projection_pending: false,
            #[cfg(test)]
            euler_viscosity_reference: false,
            #[cfg(test)]
            force_multigrid: None,
            #[cfg(test)]
            force_meanfree_pressure: None,
            #[cfg(test)]
            force_bounded_submissions: None,
            multigrid: None,
            multigrid_attempted: false,
            max_face_speed: 0.,
        };
        state.initialize_velocity()?;
        Ok(state)
    }
    fn initialize_velocity(&mut self) -> Result<(), String> {
        self.dispatch(&[("initialize", groups(self.params.faces[3]))]);
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.velocity, 0, &self.scratch, 0, self.velocity.size());
        self.context.queue.submit(Some(encoder.finish()));
        let (iterations, residual) = self.project(false)?;
        self.dispatch(&[
            ("fields", groups(self.params.dims[3])),
            ("diagnostics", [1, 1, 1]),
        ]);
        self.read_diagnostics(iterations, residual, 0.)
    }
    /// Apply the requested initial field after the actual density/phase is known.
    pub(crate) fn reset_initial_velocity(&mut self) -> Result<(), String> {
        if self.time_s != 0. {
            return Err("Initial velocity can only be reset at time zero".into());
        }
        if self.config.initial_velocity_m_s.iter().any(|u| *u != 0.) {
            self.initialize_velocity()?;
        }
        Ok(())
    }
    /// Replace the driving Lorentz force before a coupled step (GPU to GPU).
    pub fn update_force(&mut self, field: ForceField<'_>) -> Result<(), String> {
        if !self.config.use_em_force
            || field.grid != &self.grid
            || field.buffer.size() < self._force.size()
            || !field.buffer.usage().contains(wgpu::BufferUsages::COPY_SRC)
        {
            return Err("Coupled force requires enabled EM forcing and matching GPU grids".into());
        }
        if self.shared_force {
            if field.buffer != &self._force {
                return Err("Coupled EM output allocation changed".into());
            }
        } else {
            let compact = self._force.size() == u64::from(self.params.dims[3]) * 16;
            if compact && field.buffer.size() != self._force.size() {
                return Err("Coupled force packet layout changed".into());
            }
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(field.buffer, 0, &self._force, 0, self._force.size());
            self.context.queue.submit(Some(encoder.finish()));
        }
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
                            [
                                self.config.material_id,
                                u32::from(thermal.is_some()),
                                crate::surface::FRACTION_TOLERANCE.to_bits(),
                                0,
                            ],
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
                [
                    self.config.material_id,
                    u32::from(thermal.is_some()),
                    crate::surface::FRACTION_TOLERANCE.to_bits(),
                    0,
                ],
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
            let mut pass = self.context.compute_pass(&mut enc, None);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            let g = groups(self.params.dims[3]);
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
        self.context.queue.submit(Some(enc.finish()));
        let changed = read_bytes(&self.context, &phase.changed, 0, 4)?;
        let flags = bytemuck::pod_read_unaligned::<u32>(&changed);
        if flags & 2 != 0 {
            return Err("Fluid interface fractions must be finite and within the accepted [0,1] roundoff tolerance".into());
        }
        let changed = flags & 1 != 0;
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
        let mut iterations = self.diagnostics.iterations;
        let mut residual = self.diagnostics.relative_residual;
        let mut elapsed = self.diagnostics.elapsed_ms;
        if changed || self.momentum_projection_pending {
            let started = std::time::Instant::now();
            self.dispatch(&[("phase_faces", groups(self.params.faces[3]))]);
            let mut enc = self
                .context
                .device
                .create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(&self.velocity, 0, &self.scratch, 0, self.velocity.size());
            self.context.queue.submit(Some(enc.finish()));
            let (extra, r) = self.project(!self.momentum_projection_pending)?;
            if self.momentum_projection_pending {
                // This solve is the pressure increment caused by momentum
                // transport. Retain the preceding force-balance pressure too.
                self.dispatch(&[("pressure_commit", groups(self.params.dims[3]))]);
            }
            iterations += extra;
            residual = r;
            self.projection_wall_ms[1] = started.elapsed().as_secs_f64() * 1000.;
            elapsed += self.projection_wall_ms[1];
        }
        self.dispatch(&[
            ("fields", groups(self.params.dims[3])),
            ("diagnostics", [1, 1, 1]),
        ]);
        self.read_diagnostics(iterations, residual, elapsed)?;
        self.momentum_projection_pending = false;
        Ok(())
    }
    pub(crate) fn begin_momentum_transport(
        &mut self,
    ) -> Result<Option<crate::surface::MomentumTransport<'_>>, String> {
        if self.momentum_projection_pending {
            return Err("Updated momentum still requires interface projection".into());
        }
        if !self.config.advection {
            return Ok(None);
        }
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(&self.velocity, 0, &self.checkpoint, 0, self.velocity.size());
        self.context.queue.submit(Some(enc.finish()));
        Ok(Some(crate::surface::MomentumTransport {
            velocity: &self.velocity,
            scratch: &self.scratch,
            advector: &self.checkpoint,
            mask: &self._mask,
            density: [self.params.physical[3], self.air_density_kg_m3],
        }))
    }
    pub(crate) fn finish_momentum_transport(&mut self) {
        self.momentum_projection_pending = self.config.advection;
    }
    pub fn surface_tension_n_m(&self) -> f32 {
        self.surface_tension_n_m
    }
    pub fn config(&self) -> &FluidConfig {
        &self.config
    }
    pub(crate) fn set_timestep(&mut self, dt_s: f64) -> Result<(), String> {
        if self.momentum_projection_pending {
            return Err("Finish the interface momentum projection before changing timestep".into());
        }
        let mut config = self.config.clone();
        config.dt_s = dt_s;
        config.validate()?;
        if !config.surface.enabled
            && dt_s as f32 > self.grid.cell_m.powi(2) / (6. * self.params.physical[2]) * 0.95
        {
            return Err("Fluid timestep exceeds the explicit viscosity limit".into());
        }
        self.config = config;
        self.params.physical[1] = dt_s as f32;
        self.context
            .queue
            .write_buffer(&self._uniform, 0, bytemuck::bytes_of(&self.params));
        Ok(())
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
    fn bounded_submissions(&self) -> bool {
        let bounded = self.params.dims[3].div_ceil(256) > 65535;
        #[cfg(test)]
        let bounded = self.force_bounded_submissions.unwrap_or(bounded);
        bounded
    }
    fn dispatch(&self, entries: &[(&str, [u32; 3])]) {
        let bounded = self.bounded_submissions();
        let chunk_size = if bounded { 1 } else { entries.len().max(1) };
        for chunk in entries.chunks(chunk_size) {
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            {
                let mut pass = self.context.compute_pass(&mut encoder, None);
                self.encode_stages(&mut pass, chunk);
            }
            if bounded {
                self.context.submit_bounded(encoder.finish());
            } else {
                self.context.queue.submit(Some(encoder.finish()));
            }
        }
    }
    fn encode_stages(&self, pass: &mut wgpu::ComputePass<'_>, stages: &[(&str, [u32; 3])]) {
        pass.set_bind_group(0, &self.bind, &[]);
        let group_count = self.params.dims[3].div_ceil(256);
        for &(stage, g) in stages {
            if group_count >= 4096
                && matches!(stage, "initial_reduce" | "alpha_reduce" | "beta_reduce" | "pressure_residual_mean")
            {
                pass.set_pipeline(&self.pipelines["pressure_reduce_blocks"]);
                pass.dispatch_workgroups(group_count.div_ceil(256), 1, 1);
            }
            pass.set_pipeline(&self.pipelines[stage]);
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
    }
    fn convergence(&self) -> Result<(f32, bool, u32), String> {
        let data = read_bytes(&self.context, &self.scalars, 0, 32)?;
        let s = bytemuck::cast_slice::<u8, f32>(&data);
        let residual = (s[1] / s[2].max(1e-38)).sqrt();
        if !residual.is_finite() || s.iter().any(|v| !v.is_finite()) || s[1] < 0. || s[2] < 0. {
            return Err("Nonfinite fluid pressure residual".into());
        }
        Ok((residual, s[6] > 0.5, s[7] as u32))
    }
    fn configure_pressure_projection(&mut self) -> bool {
        // A global projection is valid for exactly one null vector. Disconnected
        // graphs retain the existing independent point references and MG graph.
        let meanfree = self.components == 1;
        #[cfg(test)]
        let meanfree = self.force_meanfree_pressure.unwrap_or(meanfree) && self.components == 1;
        let inverse = if meanfree {
            1. / self.fluid_cells.max(1) as f32
        } else {
            0.
        };
        let bound = if meanfree {
            1.
        } else {
            self.fluid_cells.max(1) as f32
        };
        if self.params.initial[3] != inverse || self.params.solve[2] != bound {
            self.params.initial[3] = inverse;
            self.params.solve[2] = bound;
            self.context
                .queue
                .write_buffer(&self._uniform, 0, bytemuck::bytes_of(&self.params));
        }
        meanfree
    }
    fn pressure_refresh_stages(&self) -> Vec<(&'static str, [u32; 3])> {
        let cells = groups(self.params.dims[3]);
        let mut stages = vec![("pressure_refresh", cells)];
        if self.params.initial[3] > 0. {
            stages.extend([
                ("pressure_residual_mean", [1, 1, 1]),
                ("pressure_initial_subspace", cells),
            ]);
        }
        stages.push(("initial_reduce", [1, 1, 1]));
        stages
    }
    fn project(&mut self, warm_start: bool) -> Result<(u32, f32), String> {
        let meanfree = self.configure_pressure_projection();
        let _stage = self.context.profiler.stage(crate::profile::Stage::Pressure);
        let cells = groups(self.params.dims[3]);
        // Solve a zero-based correction, retaining the preceding force balance.
        let mut preparation = Vec::with_capacity(7);
        if warm_start {
            preparation.push(("pressure_warm", groups(self.params.faces[3])));
        }
        preparation.extend([
            ("pressure_coefficients", groups(self.params.faces[3])),
            ("pressure_guess", cells),
        ]);
        preparation.extend(self.pressure_refresh_stages());
        self.dispatch(&preparation);
        let mut iterations = 0;
        let mut residual = self.convergence()?;
        // A small but difficult, high-contrast or elongated domain can need
        // more acceleration than a larger easy one. Keep an allocated hierarchy
        // and also try it after a preceding projection needed many iterations.
        let preferred = self.params.dims[3] >= multigrid::MIN_CELLS
            || self.multigrid.is_some()
            || self.diagnostics.iterations >= 256;
        #[cfg(test)]
        let preferred = self.force_multigrid.unwrap_or(preferred);
        if preferred && !residual.1 && !self.multigrid_attempted {
            // Optional acceleration must never make a fitting Jacobi solve fail.
            // Attempt once per state; a new run rechecks the live memory budget.
            self.multigrid_attempted = true;
            if let Ok(mg) = multigrid::Multigrid::new(self) {
                self.bytes += mg.bytes;
                self.multigrid = Some(mg);
            }
        }
        let mg = self.multigrid.as_ref().filter(|_| preferred);
        if !residual.1
            && let Some(mg) = mg
        {
            mg.build(&self.context);
            mg.initialize(self);
        }
        while !residual.1 && iterations < self.config.max_iterations {
            // The GPU guards converged iterations. Poll less often while still
            // stopping at each independent physical-residual check and the cap.
            let batch = (if mg.is_some() { 8u32 } else { 32u32 })
                .min(self.config.max_iterations - iterations)
                .min(64 - iterations % 64);
            if let Some(mg) = mg {
                mg.iterate(self, batch);
            } else {
                let mut stages = Vec::new();
                for _ in 0..batch {
                    stages.extend([
                        ("pressure_apply", cells),
                        ("alpha_reduce", [1, 1, 1]),
                        ("update", cells),
                    ]);
                    if meanfree {
                        stages.extend([
                            ("pressure_residual_mean", [1, 1, 1]),
                            ("pressure_residual_subspace", cells),
                        ]);
                    }
                    stages.extend([("beta_reduce", [1, 1, 1]), ("search", cells)]);
                }
                self.dispatch(&stages);
            }
            residual = self.convergence()?;
            if residual.2 <= iterations && !residual.1 {
                return Err("Fluid pressure iteration made no progress".into());
            }
            iterations = residual.2;
            if residual.1 {
                self.dispatch(&self.pressure_refresh_stages());
                residual = self.convergence()?;
                if !residual.1
                    && let Some(mg) = mg
                {
                    mg.initialize(self);
                }
            } else if iterations % 64 == 0 || iterations == self.config.max_iterations {
                // Inspect the full physical equation without resetting CG conjugacy.
                // The conservative inner gauge bound may be below f32 precision.
                self.dispatch(&[("pressure_check", cells), ("check_reduce", [1, 1, 1])]);
                residual = self.convergence()?;
            }
        }
        if !residual.1 {
            return Err(format!(
                "Fluid pressure solve did not converge after {iterations} iterations (true relative residual {:.3e}, requested {:.3e}, RMS divergence floor {:.3e} s⁻¹)",
                residual.0,
                self.config.relative_tolerance,
                self.config.absolute_divergence_tolerance
            ));
        }
        let completion = [
            ("correct", groups(self.params.faces[3])),
            ("pressure_commit", cells),
        ];
        self.dispatch(if warm_start {
            &completion
        } else {
            &completion[..1]
        });
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
            viscous_substeps: self.diagnostics.viscous_substeps,
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
    /// Upper bound valid for every accepted fraction distribution, including
    /// roundoff outside [0,1]. Keep density consistent with raw transported alpha.
    fn global_viscous_bound(&self) -> Option<f64> {
        let liquid = self.params.physical[3] as f64;
        let ambient = self.params.surface[1] as f64;
        let slack = crate::surface::FRACTION_TOLERANCE as f64 + 4. * f32::EPSILON as f64;
        let density_floor = liquid.min(ambient) - (liquid - ambient).abs() * slack;
        if density_floor <= 0. {
            return None;
        }
        let mu =
            (self.params.physical[2] * self.params.physical[3]).max(self.params.surface[3]) as f64;
        let h = self.grid.cell_m as f64;
        // Round upward to cover f32 mixture/edge/stencil arithmetic.
        Some(24. * mu / (density_floor * h * h) * (1. + 32. * f32::EPSILON as f64))
    }
    fn diffuse_surface_stress(&mut self) -> Result<(), String> {
        let _stage = self
            .context
            .profiler
            .stage(crate::profile::Stage::Viscosity);
        self.diagnostics.viscous_substeps = 0;
        let faces = groups(self.params.faces[3]);
        let count = if let Some(bound) = self
            .global_viscous_bound()
            .filter(|b| self.config.dt_s * b <= 1.8)
        {
            // One step is proved safe without GPU reductions or a scalar readback.
            if bound == 0. {
                return Ok(());
            }
            1.
        } else {
            self.dispatch(&[
                ("viscosity_bound", faces),
                ("viscosity_bound_reduce", [1, 1, 1]),
            ]);
            let bytes = read_bytes(&self.context, &self.scalars, 0, 4)?;
            let bound = bytemuck::pod_read_unaligned::<f32>(&bytes);
            if !bound.is_finite() || bound < 0. {
                return Err("Nonfinite viscous stability bound".into());
            }
            if bound == 0. {
                return Ok(());
            }
            (self.config.dt_s * bound as f64 / 1.8).ceil().max(1.)
        };
        if count > self.config.max_viscous_substeps as f64 {
            return Err(format!(
                "Viscosity requires {count:.0} equivalent Euler substeps, above the {} work limit; reduce the common timestep or increase Viscosity work limit",
                self.config.max_viscous_substeps
            ));
        }
        let count = count as u32;
        let accelerated = count > 8;
        #[cfg(test)]
        let accelerated = accelerated && !self.euler_viscosity_reference;
        if accelerated {
            let rows = stabilized::schedule(self.config.dt_s, count);
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            self.stabilized.as_ref().unwrap().encode(
                &self.context,
                &mut encoder,
                &rows,
                faces,
                &self.velocity,
                &self.scratch,
            );
            self.context.queue.submit(Some(encoder.finish()));
            self.diagnostics.viscous_substeps = rows.len() as u32;
            return Ok(());
        }
        self.diagnostics.viscous_substeps = count;
        self.params.physical[1] = (self.config.dt_s / count as f64) as f32;
        self.context
            .queue
            .write_buffer(&self._uniform, 0, bytemuck::bytes_of(&self.params));
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        for _ in 0..count {
            {
                let mut pass = self
                    .context
                    .compute_pass(&mut enc, Some("Viscous stress substep"));
                pass.set_bind_group(0, &self.bind, &[]);
                pass.set_pipeline(&self.pipelines["stress_diffuse"]);
                pass.dispatch_workgroups(faces[0], faces[1], faces[2]);
            }
            enc.copy_buffer_to_buffer(&self.velocity, 0, &self.scratch, 0, self.velocity.size());
        }
        self.context.queue.submit(Some(enc.finish()));
        self.params.physical[1] = self.config.dt_s as f32;
        self.context
            .queue
            .write_buffer(&self._uniform, 0, bytemuck::bytes_of(&self.params));
        Ok(())
    }
    pub fn step(&mut self, volume: &Volume) -> Result<&FluidDiagnostics, String> {
        if self.momentum_projection_pending {
            return Err(
                "Refresh the interface and project updated momentum before another fluid step"
                    .into(),
            );
        }
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
            let capillary = if self.surface_tension_n_m > 0. {
                ((self.params.physical[3] + self.air_density_kg_m3) * self.grid.cell_m.powi(3)
                    / (2. * std::f32::consts::PI * self.surface_tension_n_m))
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
            self.dispatch(&[("predict", faces)]);
            if self.time_s == 0. && self.bounded_submissions() {
                eprintln!("First fluid step: predictor submitted; starting viscosity");
            }
            if self.config.surface.enabled {
                self.diffuse_surface_stress()?;
            }
            self.dispatch(&[("viscous", faces)]);
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
            if self.time_s == 0. && self.bounded_submissions() {
                eprintln!("First fluid step: viscosity submitted; starting pressure");
            }
            let started = std::time::Instant::now();
            let projected = self.project(true);
            self.projection_wall_ms = [started.elapsed().as_secs_f64() * 1000., 0.];
            projected
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
            let mut pass = self.context.compute_pass(&mut encoder, None);
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
        let mut frame = self.field_view();
        frame.result = crate::recording::FieldStorage::gpu(result);
        frame
    }
    pub(crate) fn field_view(&self) -> FluidFrame {
        FluidFrame {
            grid: self.grid.clone(),
            time_s: self.time_s,
            diagnostics: self.diagnostics.clone(),
            result: crate::recording::FieldStorage::gpu(self.result.clone()),
            context: self.context.clone(),
            input_versions: self.input_versions,
            bytes: self.result.size(),
        }
    }
}
#[derive(Clone)]
pub struct FluidFrame {
    pub grid: GridSi,
    pub time_s: f64,
    pub diagnostics: FluidDiagnostics,
    pub bytes: u64,
    pub(crate) result: crate::recording::FieldStorage,
    context: GpuContext,
    input_versions: InputVersions,
}
impl FluidFrame {
    pub fn result_buffer(&self) -> &wgpu::Buffer {
        self.result.buffer()
    }
    pub fn is_resident(&self) -> bool {
        self.result.is_resident()
    }
    pub fn is_current(&self, volume: &Volume) -> bool {
        self.input_versions == volume.inputs.versions()
    }
    pub fn probe(&self, point_mm: [f32; 3]) -> Result<Option<FluidCell>, String> {
        crate::recording::probe(&self.context, &self.grid, &self.result, point_mm)
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
    crate::gpu::volume_groups(n)
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

#[cfg(test)]
mod viscosity_tests;

mod stabilized;

mod multigrid;

#[cfg(test)]
mod pressure_tests;
