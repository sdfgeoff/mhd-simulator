//! Conservative cell enthalpy transport with latent heat, explicit subcycling and GPU reductions.
use crate::{
    domain::Volume,
    gpu::{GpuContext, read_bytes},
    inputs::InputVersions,
    simulation::GridSi,
};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wgpu::util::DeviceExt;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Boundary {
    #[default]
    Insulated,
    Temperature {
        celsius: f32,
    },
    HeatFlux {
        w_m2: f32,
    },
    Convection {
        h_w_m2_k: f32,
        ambient_c: f32,
    },
}
impl Boundary {
    fn gpu(&self) -> Result<[f32; 4], String> {
        let v = match *self {
            Self::Insulated => [0.; 4],
            Self::Temperature { celsius } => [1., celsius + 273.15, 0., 0.],
            Self::HeatFlux { w_m2 } => [2., w_m2, 0., 0.],
            Self::Convection {
                h_w_m2_k,
                ambient_c,
            } => [3., h_w_m2_k, ambient_c + 273.15, 0.],
        };
        if v.iter().any(|v| !v.is_finite())
            || (v[0] == 1. && v[1] < 0.)
            || (v[0] == 3. && (v[1] < 0. || v[2] < 0.))
        {
            return Err("Invalid thermal boundary condition".into());
        }
        Ok(v)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThermalConfig {
    pub initial_temperature_c: f32,
    pub boundaries: [Boundary; 6],
    pub exposed_boundary: Boundary,
    pub max_substeps: u32,
}
impl Default for ThermalConfig {
    fn default() -> Self {
        Self {
            initial_temperature_c: 20.,
            boundaries: std::array::from_fn(|_| Boundary::Insulated),
            exposed_boundary: Boundary::Insulated,
            max_substeps: 4096,
        }
    }
}
impl ThermalConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.initial_temperature_c.is_finite()
            || self.initial_temperature_c < -273.15
            || !(1..=100000).contains(&self.max_substeps)
        {
            return Err("Invalid thermal initial temperature or substep limit".into());
        }
        for b in self.boundaries.iter().chain([&self.exposed_boundary]) {
            b.gpu()?;
        }
        Ok(())
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct ThermalCell {
    pub state: [f32; 4],
    pub flux: [f32; 4],
    pub budget: [f32; 4],
    pub valid: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    dims: [u32; 4],
    meta: [u32; 4],
    step: [f32; 4],
    boundary: [[f32; 4]; 6],
    exposed: [f32; 4],
}
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub min_temperature_k: f32,
    pub max_temperature_k: f32,
    pub enthalpy_j: f64,
    pub joule_j: f64,
    pub external_j: f64,
    pub liquid_volume_m3: f64,
    pub cells: u64,
    pub substeps: u32,
    pub energy_error_j: f64,
    pub delta_enthalpy_j: f64,
    pub max_flux_w_m2: f32,
}
pub struct Source<'a> {
    pub buffer: &'a wgpu::Buffer,
    pub grid: &'a GridSi,
}
pub struct Flow<'a> {
    pub buffer: &'a wgpu::Buffer,
    pub grid: &'a GridSi,
    pub max_face_speed: f32,
}
pub struct ThermalState {
    pub grid: GridSi,
    pub time_s: f64,
    pub diagnostics: Diagnostics,
    pub config: ThermalConfig,
    context: GpuContext,
    versions: InputVersions,
    params: Params,
    uniform: wgpu::Buffer,
    states: [wgpu::Buffer; 2],
    checkpoint: wgpu::Buffer,
    current: usize,
    catalog: wgpu::Buffer,
    stats: wgpu::Buffer,
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    layout: wgpu::BindGroupLayout,
    zero: wgpu::Buffer,
    alpha_max: f32,
    capacity_min: f32,
    transport_delta_j: f64,
}
fn groups(n: u32) -> [u32; 3] {
    let n = n.div_ceil(256);
    [n.min(65535), n.div_ceil(65535), 1]
}
impl ThermalState {
    pub fn new(
        context: GpuContext,
        volume: &Volume,
        config: ThermalConfig,
    ) -> Result<Self, String> {
        config.validate()?;
        if volume.overlaps > 0 {
            return Err("Resolve overlapping components before thermal transport".into());
        }
        let snapshot = volume.inputs.snapshot();
        let grid = snapshot.grid.clone();
        let n = grid.dims.iter().map(|v| *v as u64).product::<u64>();
        let bytes = n * 64;
        let total = bytes * 3 + (n.div_ceil(256) + 1) * 48;
        let limits = context.device.limits();
        if bytes > limits.max_storage_buffer_binding_size as u64 || bytes > limits.max_buffer_size {
            return Err("Thermal state exceeds GPU buffer limits; increase cell spacing".into());
        }
        context.ensure_memory(total, "Thermal working buffers")?;
        let max_id = snapshot.materials.iter().map(|m| m.id).max().unwrap_or(0) as usize;
        let offset = (max_id + 1) * 2;
        let mut rows = vec![[0f32; 4]; offset + 2 + snapshot.components.len() * 2];
        let mut alpha_max = 0f32;
        let mut capacity_min = f32::INFINITY;
        for m in &snapshot.materials {
            let used = snapshot
                .components
                .iter()
                .any(|c| c.included && c.material_id == m.id);
            if !used || m.id == 0 {
                continue;
            }
            let rho = m
                .density_kg_m3
                .filter(|v| *v > 0.)
                .ok_or_else(|| format!("{} needs density for thermal transport", m.name))?;
            let cp = m
                .thermal
                .heat_capacity_j_kg_k
                .filter(|v| *v > 0.)
                .ok_or_else(|| format!("{} needs heat capacity", m.name))?;
            let k = m
                .thermal
                .conductivity_w_m_k
                .filter(|v| *v > 0.)
                .ok_or_else(|| format!("{} needs thermal conductivity", m.name))?;
            alpha_max = alpha_max.max(k / (rho * cp));
            capacity_min = capacity_min.min(rho * cp);
            let t = &m.thermal;
            rows[m.id as usize * 2] = [rho, cp, k, t.latent_heat_j_kg.unwrap_or(0.)];
            rows[m.id as usize * 2 + 1] = match (t.solidus_c, t.liquidus_c) {
                (Some(s), Some(l)) => [
                    s + 273.15,
                    l + 273.15,
                    if t.latent_heat_j_kg.is_some() { 1. } else { 2. },
                    0.,
                ],
                (None, None) => [0.; 4],
                _ => {
                    return Err(format!(
                        "{} needs both solidus and liquidus temperatures",
                        m.name
                    ));
                }
            };
        }
        if !capacity_min.is_finite() {
            return Err("No thermal material occupies this model".into());
        }
        rows[offset] = [config.initial_temperature_c + 273.15, 0., 0., 0.];
        for (i, c) in snapshot.components.iter().enumerate() {
            rows[offset + 2 + i * 2] = [
                c.initial_temperature_k
                    .unwrap_or(config.initial_temperature_c + 273.15),
                c.fixed_temperature_k.unwrap_or(0.),
                c.heat_source_w_m3,
                0.,
            ];
            rows[offset + 2 + i * 2 + 1] = [f32::from(c.fixed_temperature_k.is_some()), 0., 0., 0.];
        }
        let params = Params {
            dims: [grid.dims[0], grid.dims[1], grid.dims[2], n as u32],
            meta: [0, offset as u32, 0, 0],
            step: [0., grid.cell_m, config.initial_temperature_c + 273.15, 0.],
            boundary: config
                .boundaries
                .iter()
                .map(Boundary::gpu)
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .unwrap(),
            exposed: config.exposed_boundary.gpu()?,
        };
        let uniform = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Thermal grid, time and boundaries"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let catalog = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Thermal materials and body conditions"),
                contents: bytemuck::cast_slice(&rows),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let buffer = |name, size| {
            context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(name),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let states = [
            buffer("Thermal enthalpy A", bytes),
            buffer("Thermal enthalpy B", bytes),
        ];
        let checkpoint = buffer("Accepted thermal checkpoint", bytes);
        let stats = buffer("Thermal energy and extrema", (n.div_ceil(256) + 1) * 48);
        let zero = buffer("Absent heat/flow source", 64);
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Portable eight-storage thermal layout"),
                entries: &(0..9)
                    .map(|binding| wgpu::BindGroupLayoutEntry {
                        binding,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: if binding == 0 {
                                wgpu::BufferBindingType::Uniform
                            } else {
                                wgpu::BufferBindingType::Storage {
                                    read_only: matches!(binding, 1 | 2 | 3 | 6 | 7),
                                }
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    })
                    .collect::<Vec<_>>(),
            });
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Conservative enthalpy and phase"),
                source: wgpu::ShaderSource::Wgsl(include_str!("solver.wgsl").into()),
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
            "surface_recover",
            "reset_budget",
            "integrate",
            "enthalpy_difference",
            "compensate",
            "flux",
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
        let mut state = Self {
            grid,
            time_s: 0.,
            diagnostics: Default::default(),
            config,
            context,
            versions: volume.inputs.versions(),
            params,
            uniform,
            states,
            checkpoint,
            current: 0,
            catalog,
            stats,
            pipelines,
            layout,
            zero,
            alpha_max,
            capacity_min,
            transport_delta_j: 0.,
        };
        let bind = state.bind(volume, 1, None, None)?;
        let mut enc = state
            .context
            .device
            .create_command_encoder(&Default::default());
        state.dispatch(&mut enc, &bind, "initialize", groups(n as u32));
        state.context.queue.submit(Some(enc.finish()));
        state.refresh(volume, None, None, 0)?;
        Ok(state)
    }
    fn bind(
        &self,
        volume: &Volume,
        read: usize,
        heat: Option<&Source<'_>>,
        flow: Option<&Flow<'_>>,
    ) -> Result<wgpu::BindGroup, String> {
        if heat.is_some_and(|s| s.grid != &self.grid || s.buffer.size() < self.states[0].size())
            || flow.is_some_and(|f| {
                f.grid != &self.grid
                    || f.buffer.size()
                        < self
                            .grid
                            .dims
                            .iter()
                            .map(|n| (*n as u64) + 1)
                            .product::<u64>()
                            * 16
            })
        {
            return Err("Thermal source/velocity grid or buffer size does not match".into());
        }
        let buffers = [
            &self.uniform,
            volume.material_ids(),
            volume.body_masks(),
            &self.catalog,
            &self.states[read],
            &self.states[1 - read],
            heat.map_or(&self.zero, |s| s.buffer),
            flow.map_or(&self.zero, |f| f.buffer),
            &self.stats,
        ];
        Ok(self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Thermal state and driving fields"),
                layout: &self.layout,
                entries: &buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect::<Vec<_>>(),
            }))
    }
    fn dispatch(
        &self,
        enc: &mut wgpu::CommandEncoder,
        bind: &wgpu::BindGroup,
        name: &str,
        g: [u32; 3],
    ) {
        let mut pass = self.context.compute_pass(enc, Some(name));
        pass.set_bind_group(0, bind, &[]);
        pass.set_pipeline(&self.pipelines[name]);
        pass.dispatch_workgroups(g[0], g[1], g[2]);
    }
    fn refresh(
        &mut self,
        volume: &Volume,
        heat: Option<&Source<'_>>,
        flow: Option<&Flow<'_>>,
        steps: u32,
    ) -> Result<(), String> {
        let bind = self.bind(volume, self.current, heat, flow)?;
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        self.dispatch(&mut enc, &bind, "flux", groups(self.params.dims[3]));
        self.dispatch(&mut enc, &bind, "collect", groups(self.params.dims[3]));
        self.dispatch(&mut enc, &bind, "reduce", [1, 1, 1]);
        self.context.queue.submit(Some(enc.finish()));
        let bytes = read_bytes(
            &self.context,
            &self.stats,
            self.params.dims[3].div_ceil(256) as u64 * 48,
            48,
        )?;
        let v = bytemuck::cast_slice::<u8, f32>(&bytes);
        if v.iter().any(|v| !v.is_finite()) || v[7] > 0. {
            return Err("Thermal step is invalid: check temperature/phase data (latent heat is required before crossing a melting interval)".into());
        }
        self.diagnostics = Diagnostics {
            enthalpy_j: v[0] as f64 - v[9] as f64,
            joule_j: v[1] as f64,
            external_j: v[2] as f64,
            liquid_volume_m3: v[3] as f64,
            min_temperature_k: v[4],
            max_temperature_k: v[5],
            cells: v[6] as u64,
            substeps: steps,
            energy_error_j: 0.,
            delta_enthalpy_j: v[8] as f64,
            max_flux_w_m2: v[11],
        };
        Ok(())
    }
    pub fn is_current(&self, volume: &Volume) -> bool {
        self.versions == volume.inputs.versions()
    }
    pub fn working_bytes(&self) -> u64 {
        self.states.iter().map(wgpu::Buffer::size).sum::<u64>()
            + self.checkpoint.size()
            + self.catalog.size()
            + self.stats.size()
            + self.uniform.size()
            + self.zero.size()
    }
    pub fn result_buffer(&self) -> &wgpu::Buffer {
        &self.states[self.current]
    }
    pub fn transport_destination(&self) -> &wgpu::Buffer {
        &self.states[1 - self.current]
    }
    /// Accept conservative mixture enthalpy carried with the liquid occupancy.
    pub fn accept_surface_transport(&mut self, volume: &Volume) -> Result<(), String> {
        let old = self.diagnostics.enthalpy_j;
        self.current = 1 - self.current;
        let bind = self.bind(volume, self.current, None, None)?;
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        self.dispatch(
            &mut enc,
            &bind,
            "surface_recover",
            groups(self.params.dims[3]),
        );
        self.context.queue.submit(Some(enc.finish()));
        self.refresh(volume, None, None, 0)?;
        self.transport_delta_j += self.diagnostics.enthalpy_j - old;
        Ok(())
    }
    pub fn step(
        &mut self,
        volume: &Volume,
        dt_s: f64,
        heat: Option<Source<'_>>,
        flow: Option<Flow<'_>>,
    ) -> Result<&Diagnostics, String> {
        if !self.is_current(volume) || !dt_s.is_finite() || dt_s <= 0. || !(dt_s as f32).is_normal()
        {
            return Err("Thermal inputs or timestep changed/invalid".into());
        }
        let h = self.grid.cell_m;
        let convection = self
            .params
            .boundary
            .iter()
            .chain([&self.params.exposed])
            .filter(|b| b[0] == 3.)
            .map(|b| b[1])
            .fold(0f32, f32::max);
        let speed = flow.as_ref().map_or(0., |f| f.max_face_speed);
        if !speed.is_finite() || speed < 0. {
            return Err("Invalid thermal advection speed bound".into());
        }
        let rate = 12. * self.alpha_max / (h * h)
            + 6. * convection / (self.capacity_min * h)
            + 3. * speed / h;
        let count = (dt_s * rate as f64 / 0.8).ceil().max(1.);
        if count > self.config.max_substeps as f64 {
            return Err("Thermal subcycling limit exceeded; reduce the common timestep".into());
        }
        let count = count as u32;
        self.params.step[0] = (dt_s / count as f64) as f32;
        self.params.meta[2] = u32::from(heat.is_some());
        self.params.meta[3] = u32::from(flow.is_some());
        self.context
            .queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&self.params));
        let binds = [
            self.bind(volume, 0, heat.as_ref(), flow.as_ref())?,
            self.bind(volume, 1, heat.as_ref(), flow.as_ref())?,
        ];
        let old = self.current;
        let old_diag = self.diagnostics.clone();
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(
            self.result_buffer(),
            0,
            &self.checkpoint,
            0,
            self.checkpoint.size(),
        );
        self.dispatch(
            &mut enc,
            &binds[self.current],
            "reset_budget",
            groups(self.params.dims[3]),
        );
        for _ in 0..count {
            self.dispatch(
                &mut enc,
                &binds[self.current],
                "integrate",
                groups(self.params.dims[3]),
            );
            self.dispatch(
                &mut enc,
                &binds[self.current],
                "enthalpy_difference",
                groups(self.params.dims[3]),
            );
            self.dispatch(
                &mut enc,
                &binds[self.current],
                "compensate",
                groups(self.params.dims[3]),
            );
            self.current = 1 - self.current;
        }
        self.context.queue.submit(Some(enc.finish()));
        if let Err(e) = self.refresh(volume, heat.as_ref(), flow.as_ref(), count) {
            let mut enc = self
                .context
                .device
                .create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(
                &self.checkpoint,
                0,
                &self.states[old],
                0,
                self.checkpoint.size(),
            );
            self.context.queue.submit(Some(enc.finish()));
            self.current = old;
            self.diagnostics = old_diag;
            return Err(e);
        }
        self.diagnostics.delta_enthalpy_j += self.transport_delta_j;
        self.transport_delta_j = 0.;
        self.diagnostics.energy_error_j = self.diagnostics.delta_enthalpy_j
            - self.diagnostics.joule_j
            - self.diagnostics.external_j;
        self.time_s += dt_s;
        Ok(&self.diagnostics)
    }
    pub fn snapshot(&self) -> ThermalFrame {
        let result = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Recorded temperature and phase"),
            size: self.result_buffer().size(),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(self.result_buffer(), 0, &result, 0, result.size());
        self.context.queue.submit(Some(enc.finish()));
        let mut frame = self.field_view();
        frame.result = crate::recording::FieldStorage::gpu(result);
        frame
    }
    pub(crate) fn field_view(&self) -> ThermalFrame {
        ThermalFrame {
            grid: self.grid.clone(),
            time_s: self.time_s,
            diagnostics: self.diagnostics.clone(),
            bytes: self.result_buffer().size(),
            result: crate::recording::FieldStorage::gpu(self.result_buffer().clone()),
            context: self.context.clone(),
            versions: self.versions,
        }
    }
    pub fn probe(&self, point_mm: [f32; 3]) -> Result<Option<ThermalCell>, String> {
        probe(&self.context, &self.grid, self.result_buffer(), point_mm)
    }
}
#[derive(Clone)]
pub struct ThermalFrame {
    pub grid: GridSi,
    pub time_s: f64,
    pub diagnostics: Diagnostics,
    pub bytes: u64,
    pub(crate) result: crate::recording::FieldStorage,
    context: GpuContext,
    versions: InputVersions,
}
impl ThermalFrame {
    pub fn result_buffer(&self) -> &wgpu::Buffer {
        self.result.buffer()
    }
    pub fn is_resident(&self) -> bool {
        self.result.is_resident()
    }
    pub fn is_current(&self, v: &Volume) -> bool {
        self.versions == v.inputs.versions()
    }
    pub fn probe(&self, p: [f32; 3]) -> Result<Option<ThermalCell>, String> {
        crate::recording::probe(&self.context, &self.grid, &self.result, p)
    }
}
fn probe(
    context: &GpuContext,
    grid: &GridSi,
    buffer: &wgpu::Buffer,
    p: [f32; 3],
) -> Result<Option<ThermalCell>, String> {
    let mut q = [0; 3];
    for i in 0..3 {
        let v = ((p[i] * 1e-3 - grid.origin_m[i]) / grid.cell_m).floor();
        if !v.is_finite() || v < 0. || v >= grid.dims[i] as f32 {
            return Ok(None);
        }
        q[i] = v as u64;
    }
    let i = q[0] + grid.dims[0] as u64 * (q[1] + grid.dims[1] as u64 * q[2]);
    Ok(Some(bytemuck::pod_read_unaligned(&read_bytes(
        context,
        buffer,
        i * 64,
        64,
    )?)))
}
#[cfg(test)]
mod tests;
