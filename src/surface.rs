//! Conservative liquid occupancy and co-transported enthalpy, independent of CAD masks.
use crate::{
    domain::Volume,
    gpu::{GpuContext, read_bytes},
    simulation::GridSi,
};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wgpu::util::DeviceExt;
/// Accepted fraction roundoff, not a clipping or volume-renormalization rule.
pub(crate) const FRACTION_TOLERANCE: f32 = 1e-5;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceConfig {
    pub enabled: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    dims: [u32; 4],
    choice: [u32; 4],
    step: [f32; 4],
}
/// Existing thermal ping-pong storage and its rollback checkpoint. Input may be
/// scratch during split sweeps; output holds the complete result on success.
#[derive(Clone, Copy)]
pub struct EnthalpyTransport<'a> {
    pub input: &'a wgpu::Buffer,
    pub output: &'a wgpu::Buffer,
    pub checkpoint: &'a wgpu::Buffer,
}
/// Fluid-owned storage reused only after the velocity/force step has succeeded.
#[derive(Clone, Copy)]
pub(crate) struct MomentumTransport<'a> {
    pub velocity: &'a wgpu::Buffer,
    pub scratch: &'a wgpu::Buffer,
    pub advector: &'a wgpu::Buffer,
    pub mask: &'a wgpu::Buffer,
    pub density: [f32; 2],
}
#[derive(Clone, Debug, Default)]
pub struct SurfaceDiagnostics {
    pub material_id: u32,
    pub initial_sampling_ms: f64,
    pub initial_workspace_bytes: u64,
    pub volume_m3: f64,
    pub area_m2: f64,
    pub min_fraction: f32,
    pub max_fraction: f32,
    pub transport_error_m3: f64,
    pub transport_divergence_m3: f64,
    pub max_curvature_m_inv: f32,
}
pub struct SurfaceState {
    context: GpuContext,
    pub grid: GridSi,
    pub diagnostics: SurfaceDiagnostics,
    params: Params,
    step_divergence_m3: f64,
    sweep_index: u32,
    uniform: wgpu::Buffer,
    fields: [wgpu::Buffer; 2],
    current: usize,
    normals: wgpu::Buffer,
    checkpoint: wgpu::Buffer,
    stats: wgpu::Buffer,
    zero: wgpu::Buffer,
    zero_output: wgpu::Buffer,
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    layout: wgpu::BindGroupLayout,
    id_pipeline: wgpu::ComputePipeline,
    momentum_pipeline: wgpu::ComputePipeline,
}
impl SurfaceState {
    pub fn new(
        context: GpuContext,
        volume: &Volume,
        material: u32,
    ) -> Result<(Self, Volume), String> {
        Self::with_periodic_boundaries(context, volume, material, [false; 3])
    }
    pub fn with_periodic_boundaries(
        context: GpuContext,
        volume: &Volume,
        material: u32,
        periodic: [bool; 3],
    ) -> Result<(Self, Volume), String> {
        let boundary_flags = periodic
            .iter()
            .enumerate()
            .fold(0u32, |bits, (axis, &enabled)| {
                bits | (u32::from(enabled) << (axis + 2))
            });
        if volume
            .inputs
            .snapshot()
            .materials
            .iter()
            .any(|m| m.id > 255)
        {
            return Err("Moving-interface material IDs must fit in eight bits".into());
        }
        let grid = volume.inputs.snapshot().grid.clone();
        let n = grid.dims.iter().map(|v| *v as u64).product::<u64>();
        let sizes = [
            n * 16,
            n * 16,
            n * 16,
            n * 16,
            128,
            (n.div_ceil(256) + 1) * 32,
            n * 4,
        ];
        let limits = context.device.limits();
        if sizes.iter().any(|s| {
            *s > limits.max_storage_buffer_binding_size as u64 || *s > limits.max_buffer_size
        }) {
            return Err("Moving-interface buffers exceed device buffer limits".into());
        }
        context.ensure_memory(sizes.iter().sum(), "Moving-interface working buffers")?;
        let buffer = |label, size| {
            context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let params = Params {
            dims: [grid.dims[0], grid.dims[1], grid.dims[2], n as u32],
            choice: [material, 0, 0, boundary_flags],
            step: [0., grid.cell_m, 0., 0.],
        };
        let uniform = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Liquid interface configuration"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let fields = [
            buffer("Liquid occupancy A", n * 16),
            buffer("Liquid occupancy B", n * 16),
        ];
        let normals = buffer("Interface normal", n * 16);
        let checkpoint = buffer("Accepted interface checkpoint", n * 16);
        let stats = buffer("Interface reductions", (n.div_ceil(256) + 1) * 32);
        let zero = buffer("Absent interface input", 64);
        let zero_output = buffer("Absent transport output", 64);
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Conservative liquid interface"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}\n{}\n{}",
                        include_str!("periodic.wgsl"),
                        include_str!("vof.wgsl"),
                        include_str!("surface_geometry.wgsl"),
                        include_str!("surface.wgsl")
                    )
                    .into(),
                ),
            });
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Portable liquid interface layout"),
                entries: &(0..9)
                    .map(|i| wgpu::BindGroupLayoutEntry {
                        binding: i,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: if i == 0 {
                                wgpu::BufferBindingType::Uniform
                            } else {
                                wgpu::BufferBindingType::Storage {
                                    read_only: matches!(i, 1 | 2 | 4 | 5),
                                }
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    })
                    .collect::<Vec<_>>(),
            });
        let pl = context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
        let pipelines = [
            "initialize",
            "normal",
            "curvature",
            "reconstruct",
            "sweep",
            "collect",
            "reduce",
        ]
        .into_iter()
        .map(|name| {
            (
                name,
                context
                    .device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(name),
                        layout: Some(&pl),
                        module: &shader,
                        entry_point: Some(name),
                        compilation_options: Default::default(),
                        cache: None,
                    }),
            )
        })
        .collect();
        let momentum_shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Mass-consistent MAC momentum"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}\n{}\n{}",
                        include_str!("periodic.wgsl"),
                        include_str!("vof.wgsl"),
                        include_str!("surface_geometry.wgsl"),
                        include_str!("surface_momentum.wgsl")
                    )
                    .into(),
                ),
            });
        let momentum_pipeline =
            context
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Geometric momentum transport"),
                    layout: None,
                    module: &momentum_shader,
                    entry_point: Some("momentum"),
                    compilation_options: Default::default(),
                    cache: None,
                });
        let idshader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Moving electromagnetic material occupancy"),
                source: wgpu::ShaderSource::Wgsl(include_str!("surface_ids.wgsl").into()),
            });
        let id_pipeline =
            context
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Update moving material grid"),
                    layout: None,
                    module: &idshader,
                    entry_point: Some("update"),
                    compilation_options: Default::default(),
                    cache: None,
                });
        let mut evolving = volume.clone();
        evolving.materials = buffer("Run-private material occupancy", n * 4);
        evolving.persistent_bytes += n * 4;
        let mut s = Self {
            context: context.clone(),
            grid,
            diagnostics: Default::default(),
            params,
            step_divergence_m3: 0.,
            sweep_index: 0,
            uniform,
            fields,
            current: 0,
            normals,
            checkpoint,
            stats,
            zero,
            zero_output,
            pipelines,
            layout,
            id_pipeline,
            momentum_pipeline,
        };
        s.dispatch(volume, None, None, &["initialize"]);
        s.current = 1;
        s.refresh(volume)?;
        s.update_materials(volume, &evolving);
        Ok((s, evolving))
    }
    pub(crate) fn initialize_cad(
        &mut self,
        volume: &Volume,
        model: &crate::model::Model,
        thermal: Option<crate::domain::fractions::InitialThermal<'_>>,
    ) -> Result<(), String> {
        let started = std::time::Instant::now();
        let workspace = crate::domain::fractions::initialize(
            &self.context,
            volume,
            model,
            self.params.choice[0],
            self.field(),
            thermal,
        )?;
        self.refresh(volume)?;
        self.diagnostics.initial_sampling_ms = started.elapsed().as_secs_f64() * 1000.;
        self.diagnostics.initial_workspace_bytes = workspace;
        Ok(())
    }
    pub fn working_bytes(&self) -> u64 {
        self.fields.iter().map(wgpu::Buffer::size).sum::<u64>()
            + self.normals.size()
            + self.checkpoint.size()
            + self.stats.size()
            + self.uniform.size()
            + self.zero.size()
            + self.zero_output.size()
    }
    pub fn field(&self) -> &wgpu::Buffer {
        &self.fields[self.current]
    }
    fn dispatch(
        &self,
        volume: &Volume,
        velocity: Option<&wgpu::Buffer>,
        thermal: Option<(&wgpu::Buffer, &wgpu::Buffer)>,
        entries: &[&str],
    ) {
        let buffers = [
            &self.uniform,
            volume.material_ids(),
            self.field(),
            &self.fields[1 - self.current],
            velocity.unwrap_or(&self.zero),
            thermal.map_or(&self.zero, |(input, _)| input),
            thermal.map_or(&self.zero_output, |(_, output)| output),
            &self.normals,
            &self.stats,
        ];
        let bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Interface fields"),
                layout: &self.layout,
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
            pass.set_bind_group(0, &bind, &[]);
            for name in entries {
                pass.set_pipeline(&self.pipelines[name]);
                let g = if *name == "reduce" {
                    [1, 1, 1]
                } else {
                    groups(self.params.dims[3])
                };
                pass.dispatch_workgroups(g[0], g[1], g[2]);
            }
        }
        self.context.queue.submit(Some(enc.finish()));
    }
    pub fn refresh(&mut self, volume: &Volume) -> Result<(), String> {
        self.dispatch(volume, None, None, &["normal", "curvature"]);
        self.current = 1 - self.current;
        self.dispatch(volume, None, None, &["collect", "reduce"]);
        let data = read_bytes(
            &self.context,
            &self.stats,
            self.params.dims[3].div_ceil(256) as u64 * 32,
            32,
        )?;
        let s = bytemuck::cast_slice::<u8, f32>(&data);
        if s.iter().any(|v| !v.is_finite())
            || s[3] > 0.
            || s[4] < -FRACTION_TOLERANCE
            || s[5] > 1. + FRACTION_TOLERANCE
        {
            return Err(
                "Liquid interface transport violated its face CFL, left [0,1], or became nonfinite; reduce timestep".into(),
            );
        }
        self.step_divergence_m3 = s[2] as f64;
        self.diagnostics.material_id = self.params.choice[0];
        self.diagnostics.volume_m3 = s[0] as f64;
        self.diagnostics.area_m2 = s[1] as f64;
        self.diagnostics.min_fraction = s[4];
        self.diagnostics.max_fraction = s[5];
        self.diagnostics.max_curvature_m_inv = s[6];
        Ok(())
    }
    /// Transport a moving fluid with the same mass fluxes as its interface.
    /// Call FluidState::update_surface afterward to project with the new density.
    pub fn advance_coupled(
        &mut self,
        volume: &Volume,
        fluid: &mut crate::fluid::FluidState,
        thermal: Option<EnthalpyTransport<'_>>,
        dt: f64,
    ) -> Result<(), String> {
        if fluid.grid != self.grid
            || !fluid.config().surface.enabled
            || fluid.config().material_id != self.params.choice[0]
            || fluid
                .config()
                .periodic
                .iter()
                .enumerate()
                .any(|(axis, &enabled)| enabled != ((self.params.choice[3] & (4 << axis)) != 0))
        {
            return Err(
                "Coupled interface and fluid grids, material and boundaries must match".into(),
            );
        }
        let speed = fluid.max_face_speed();
        let result = if let Some(momentum) = fluid.begin_momentum_transport()? {
            self.advance_transport(
                volume,
                momentum.advector,
                thermal,
                dt,
                speed,
                Some(momentum),
            )
        } else {
            self.advance_transport(volume, fluid.face_buffer(), thermal, dt, speed, None)
        };
        if result.is_ok() {
            fluid.finish_momentum_transport();
        }
        result
    }
    pub fn advance(
        &mut self,
        volume: &Volume,
        velocity: &wgpu::Buffer,
        thermal: Option<EnthalpyTransport<'_>>,
        dt: f64,
        max_speed: f32,
    ) -> Result<(), String> {
        self.advance_transport(volume, velocity, thermal, dt, max_speed, None)
    }
    fn advance_transport(
        &mut self,
        volume: &Volume,
        velocity: &wgpu::Buffer,
        thermal: Option<EnthalpyTransport<'_>>,
        dt: f64,
        max_speed: f32,
        momentum: Option<MomentumTransport<'_>>,
    ) -> Result<(), String> {
        if dt <= 0.
            || !dt.is_finite()
            || !max_speed.is_finite()
            || max_speed < 0.
            || 3. * max_speed * dt as f32 / self.grid.cell_m > 0.45
        {
            return Err("Conservative interface CFL exceeds 0.45; reduce common timestep".into());
        }
        if let Some(t) = thermal {
            let size = self.params.dims[3] as u64 * 64;
            if t.input.size() != size
                || t.output.size() != size
                || t.checkpoint.size() != size
                || t.input == t.output
                || t.input == t.checkpoint
                || t.output == t.checkpoint
                || !t.input.usage().contains(
                    wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::STORAGE,
                )
                || !t.output.usage().contains(wgpu::BufferUsages::STORAGE)
                || !t
                    .checkpoint
                    .usage()
                    .contains(wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST)
            {
                return Err("Interface enthalpy transport requires matching distinct ping-pong buffers and a rollback checkpoint".into());
            }
        }
        let old = self.diagnostics.volume_m3;
        let old_diagnostics = self.diagnostics.clone();
        let old_divergence = self.step_divergence_m3;
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(self.field(), 0, &self.checkpoint, 0, self.field().size());
        if let Some(t) = thermal {
            enc.copy_buffer_to_buffer(t.input, 0, t.checkpoint, 0, t.input.size());
        }
        self.context.queue.submit(Some(enc.finish()));
        self.params.step[0] = dt as f32;
        if let Some(m) = momentum {
            self.params.step[2..4].copy_from_slice(&m.density);
        }
        self.params.choice[1] = u32::from(thermal.is_some());
        for sweep in 0..3 {
            self.params.choice[2] = (self.sweep_index + sweep) % 3;
            self.params.choice[3] =
                (self.params.choice[3] & !3) | u32::from(sweep == 0) | (u32::from(sweep == 2) << 1);
            self.context
                .queue
                .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&self.params));
            let heat = thermal.map(|t| {
                if sweep % 2 == 0 {
                    (t.input, t.output)
                } else {
                    (t.output, t.input)
                }
            });
            self.dispatch(volume, Some(velocity), heat, &["reconstruct", "sweep"]);
            if let Some(m) = momentum {
                self.dispatch_momentum(volume, m, sweep);
            }
            self.current = 1 - self.current;
        }
        if let Err(e) = self.refresh(volume) {
            let mut enc = self
                .context
                .device
                .create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(&self.checkpoint, 0, self.field(), 0, self.field().size());
            if let Some(m) = momentum {
                enc.copy_buffer_to_buffer(m.advector, 0, m.velocity, 0, m.velocity.size());
            }
            if let Some(t) = thermal {
                enc.copy_buffer_to_buffer(t.checkpoint, 0, t.input, 0, t.input.size());
            }
            self.context.queue.submit(Some(enc.finish()));
            self.diagnostics = old_diagnostics;
            self.step_divergence_m3 = old_divergence;
            return Err(e);
        }
        if let Some(m) = momentum {
            let mut enc = self
                .context
                .device
                .create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(m.scratch, 0, m.velocity, 0, m.velocity.size());
            self.context.queue.submit(Some(enc.finish()));
        }
        self.sweep_index = (self.sweep_index + 1) % 3;
        self.diagnostics.transport_error_m3 += self.diagnostics.volume_m3 - old;
        self.diagnostics.transport_divergence_m3 += self.step_divergence_m3;
        Ok(())
    }
    fn dispatch_momentum(&self, volume: &Volume, m: MomentumTransport<'_>, sweep: u32) {
        let (input, output) = if sweep % 2 == 0 {
            (m.velocity, m.scratch)
        } else {
            (m.scratch, m.velocity)
        };
        let buffers = [
            &self.uniform,
            volume.material_ids(),
            self.field(),
            &self.fields[1 - self.current],
            m.advector,
            input,
            output,
            &self.normals,
            m.mask,
        ];
        let bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shared liquid/momentum flux"),
                layout: &self.momentum_pipeline.get_bind_group_layout(0),
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
            pass.set_pipeline(&self.momentum_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            let count = self.grid.dims.map(|n| n + 1).iter().product();
            let g = groups(count);
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
        self.context.queue.submit(Some(enc.finish()));
    }
    pub fn update_materials(&self, original: &Volume, evolving: &Volume) {
        let buffers = [
            &self.uniform,
            original.material_ids(),
            self.field(),
            evolving.material_ids(),
        ];
        let bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Private evolving material IDs"),
                layout: &self.id_pipeline.get_bind_group_layout(0),
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
            pass.set_pipeline(&self.id_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            let g = groups(self.params.dims[3]);
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
        self.context.queue.submit(Some(enc.finish()));
    }
}
fn groups(n: u32) -> [u32; 3] {
    let n = n.div_ceil(256);
    [n.min(65535), n.div_ceil(65535), 1]
}
#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod capillary_tests;

#[cfg(test)]
mod periodic_tests;

#[cfg(test)]
mod drop_tests;
