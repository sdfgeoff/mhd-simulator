//! Stationary magnetoquasistatic electromagnetics on an oriented edge/face grid.
//! See docs/ELECTROMAGNETICS.md for the discrete formulation and limitations.
#[cfg(test)]
mod coil_response_tests;
mod grid;
mod nodal;
mod sources;
use crate::gpu::read_bytes;
use crate::{
    domain::Volume,
    gpu::GpuContext,
    inputs::InputVersions,
    model::Model,
    simulation::{GridSi, MaterialProperty},
};
use bytemuck::{Pod, Zeroable};
use grid::Params;
pub use sources::{EmConfig, Magnet, UniformField, Waveform, Wire, cad_wires};
use std::collections::HashMap;
use wgpu::util::DeviceExt;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Applied,
    Induction,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct CellField {
    pub b: [f32; 4],
    pub j: [f32; 4],
    pub force_heat: [f32; 4],
    pub applied: [f32; 4],
}
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub maxima: [f32; 4],
    pub force_n: [f32; 3],
    pub power_w: f32,
    pub divergence_j_scaled: f32,
    pub max_edge_j: f32,
    pub divergence_b_scaled: f32,
    pub magnetic_energy_j: f32,
    pub applied_max_t: f32,
    pub reaction_max_t: f32,
    pub iterations: u32,
    pub relative_residual: f32,
    pub elapsed_ms: f64,
}
#[derive(Clone, Debug)]
pub struct BodyDiagnostic {
    pub name: String,
    pub force_n: [f32; 3],
    pub power_w: f32,
}
/// Per-step energy integrals of the actual staggered operator (joules).
#[derive(Clone, Debug, Default)]
pub struct EnergyStep {
    pub magnetic_j: f64,
    pub delta_magnetic_j: f64,
    pub joule_j: f64,
    pub mechanical_j: f64,
    pub external_j: f64,
    pub backward_euler_j: f64,
    pub balance_error_j: f64,
}
/// Passive conducting-body response to one prescribed winding, with mu_r=1.
#[derive(Clone, Debug, Default)]
pub struct CoilResponse {
    /// None only when both present and preceding winding current are zero.
    pub reaction_linkage_wb: Option<f64>,
    pub reaction_magnetic_j: f64,
    pub reaction_backward_euler_j: f64,
}
struct CoilObserver {
    bind: wgpu::BindGroup,
    bytes: u64,
}
pub struct EmState {
    coil_observer: std::sync::Mutex<Option<CoilObserver>>,
    pub bodies: Vec<BodyDiagnostic>,
    body_collect: wgpu::ComputePipeline,
    body_reduce: wgpu::ComputePipeline,
    body_bind: wgpu::BindGroup,
    body_output: wgpu::Buffer,
    context: GpuContext,
    pub grid: GridSi,
    pub input_versions: InputVersions,
    config: EmConfig,
    source_elements: Vec<sources::Element>,
    source_valid: bool,
    nodal: Option<nodal::NodalCorrection>,
    #[cfg(test)]
    use_nodal_preconditioner: bool,
    #[cfg(test)]
    nodal_switch_after: u32,
    #[cfg(test)]
    cache_source_fields: bool,
    #[cfg(test)]
    source_reused: bool,
    cad_curves: Vec<crate::model::CurvePath>,
    pub mode: Mode,
    pub time_s: f64,
    last_step_dt_s: f64,
    can_predict_increment: bool,
    #[cfg(test)]
    pub(crate) predict_increments: bool,
    #[cfg(test)]
    pub(crate) prediction_diagnostic: [f32; 3],
    pub diagnostics: Diagnostics,
    conductivity_range: [f32; 2],
    params: Params,
    uniform: wgpu::Buffer,
    _source: wgpu::Buffer,
    _work: wgpu::Buffer,
    _previous: wgpu::Buffer,
    _partials: wgpu::Buffer,
    scalars: wgpu::Buffer,
    result: wgpu::Buffer,
    checkpoint: wgpu::Buffer,
    elements: wgpu::Buffer,
    source_pipeline: wgpu::ComputePipeline,
    motion_pipeline: Option<wgpu::ComputePipeline>,
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    bind: wgpu::BindGroup,
    source_bind: wgpu::BindGroup,
    pub bytes: u64,
}
fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
impl EmState {
    pub fn new(
        context: GpuContext,
        volume: &Volume,
        model: &Model,
        config: EmConfig,
        mode: Mode,
    ) -> Result<Self, String> {
        Self::new_with_source_work_budget(context, volume, model, config, mode, 50_000_000_000)
    }
    /// Override the computation cap without changing source quadrature or memory checks.
    pub fn new_with_source_work_budget(
        context: GpuContext,
        volume: &Volume,
        model: &Model,
        config: EmConfig,
        mode: Mode,
        source_work_budget: u64,
    ) -> Result<Self, String> {
        config.validate(model)?;
        validate_model_inputs(volume, model)?;
        if volume.overlaps > 0 {
            return Err("Resolve overlapping components before electromagnetics".into());
        }
        if volume.inputs.snapshot().geometry_ids
            != model
                .bodies
                .iter()
                .map(|b| b.geometry_id())
                .collect::<Vec<_>>()
        {
            return Err("Geometry changed; rebuild the domain".into());
        }
        if mode == Mode::Induction {
            let air = volume
                .inputs
                .snapshot()
                .materials
                .iter()
                .find(|m| m.id == 0)
                .unwrap();
            if air.conductivity_s_m.is_some_and(|v| v != 0.)
                || air.relative_permeability.is_some_and(|v| v != 1.)
            {
                return Err("EM background is vacuum: material 0 must have zero/unset conductivity and unit/unset relative permeability".into());
            }
            for wire in &config.wires {
                let points = wire.points(model)?;
                if glam::Vec3::from(points[0]).distance(glam::Vec3::from(*points.last().unwrap()))
                    > 1e-4
                {
                    return Err(format!(
                        "Wire {} must form a closed current circuit, including its return lead",
                        wire.name
                    ));
                }
            }
            volume.inputs.solver_tables(
                &[
                    MaterialProperty::Conductivity,
                    MaterialProperty::RelativePermeability,
                ],
                true,
            )?;
            for magnet in &config.magnets {
                let c = &volume.inputs.snapshot().components[magnet.body];
                if c.included {
                    let m = volume
                        .inputs
                        .snapshot()
                        .materials
                        .iter()
                        .find(|m| m.id == c.material_id)
                        .unwrap();
                    if m.relative_permeability != Some(1.) {
                        return Err("Prescribed-magnetization CAD bodies currently require relative permeability 1; other bodies support linear permeability contrast".into());
                    }
                }
            }
        }
        let grid = volume.inputs.snapshot().grid.clone();
        let params = grid::params(&grid)?;
        let edges = params.edges[3] as u64;
        let cells = params.dims[3] as u64;
        let groups = edges.div_ceil(256).max(cells.div_ceil(256));
        let sizes = [
            edges * 32,
            edges * 16,
            edges * 4,
            groups * 64,
            128,
            cells * 64,
            cells.div_ceil(256) * model.bodies.len() as u64 * 16,
            model.bodies.len() as u64 * 16,
            cells * 64,
        ];
        let source_elements = config.elements(model, 0.)?;
        let source_count = source_elements.len() as u32;
        let source_work = edges * source_count as u64;
        if source_work > source_work_budget {
            return Err(format!("Direct source summation needs {source_work} interactions, above work budget {source_work_budget}; reduce source refinement/segment count or increase cell spacing"));
        }
        let source_elements = if source_elements.is_empty() {
            vec![sources::Element::zeroed()]
        } else {
            source_elements
        };
        let bytes: u64 = sizes.iter().sum::<u64>() + source_elements.len() as u64 * 48;
        let limits = context.device.limits();
        if sizes.iter().any(|&s| {
            s > limits.max_storage_buffer_binding_size as u64 || s > limits.max_buffer_size
        }) {
            return Err(format!(
                "EM buffers need {:.0} MiB and exceed device buffer limits; increase cell spacing",
                bytes as f64 / 1048576.
            ));
        }
        context.ensure_memory(bytes, "EM working buffers")?;
        let create = |label, size| {
            context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let source = create("Applied A and edge coefficients", sizes[0]);
        let work = create("Reaction A and PCG vectors", sizes[1]);
        let previous = create("Previous reaction A", sizes[2]);
        let partials = create("GPU reduction partials", sizes[3]);
        let scalars = create("Convergence and field statistics", sizes[4]);
        let result = create("Cell B J Lorentz force heat applied B", sizes[5]);
        let checkpoint = create("Last accepted field checkpoint", sizes[8]);
        let uniform = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("EM SI grid and step"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let elements = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Persistent source geometry and amplitudes"),
                contents: bytemuck::cast_slice(&source_elements),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Edge source vector potential"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}",
                        include_str!("common.wgsl"),
                        include_str!("source.wgsl")
                    )
                    .into(),
                ),
            });
        let source_pipeline =
            context
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Wire and magnet applied field"),
                    layout: None,
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
        let source_bind = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &source_pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: elements.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: source.as_entire_binding(),
                    },
                ],
            });
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Implicit edge induction and GPU PCG"),
                source: wgpu::ShaderSource::Wgsl(shader_source().into()),
            });
        let mut entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        for binding in 1..=8 {
            entries.push(storage_entry(binding, binding <= 2));
        }
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Portable eight-storage-buffer EM layout"),
                entries: &entries,
            });
        let pl = context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
        let mut pipelines = HashMap::new();
        for name in [
            "coefficients",
            "coefficient_check",
            "prepare",
            "zero_guess",
            "initial_reduce",
            "apply_operator",
            "alpha_reduce",
            "update_residual",
            "beta_reduce",
            "update_search",
            "rollback",
            "fields",
            "field_reduce",
            "true_residual",
            "restart_reduce",
            "set_boundary",
            "hold_source",
            "begin_step",
            "commit_step",
            "energy_collect",
            "energy_reduce",
            "initialize_coil_basis",
            "coil_response_collect",
            "coil_response_reduce",
        ] {
            pipelines.insert(
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
            );
        }
        let properties = volume.inputs.solver_tables(&[], false)?.0;
        let bind = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("EM geometry and solver buffers"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: volume.material_ids().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: properties.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: source.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: work.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: previous.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: partials.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: scalars.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: result.as_entire_binding(),
                    },
                ],
            });
        let body_partials = create("Per-component force/power reduction", sizes[6]);
        let body_output = create("Component force and power totals", sizes[7]);
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Component Lorentz force and heating"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}",
                        include_str!("common.wgsl"),
                        include_str!("body-stats.wgsl")
                    )
                    .into(),
                ),
            });
        let layout = context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Component force/power layout"),
                entries: &entries[..5],
            });
        let pl = context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
        let body_collect =
            context
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Component integral partials"),
                    layout: Some(&pl),
                    module: &shader,
                    entry_point: Some("collect"),
                    compilation_options: Default::default(),
                    cache: None,
                });
        let body_reduce =
            context
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Component integral reduction"),
                    layout: Some(&pl),
                    module: &shader,
                    entry_point: Some("reduce"),
                    compilation_options: Default::default(),
                    cache: None,
                });
        let body_bind = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: volume.body_masks().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: result.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: body_partials.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: body_output.as_entire_binding(),
                    },
                ],
            });
        let bodies = model
            .bodies
            .iter()
            .map(|b| BodyDiagnostic {
                name: b.name.clone(),
                force_n: [0.; 3],
                power_w: 0.,
            })
            .collect();
        let mut state = Self {
            context,
            coil_observer: std::sync::Mutex::new(None),
            bodies,
            body_collect,
            body_reduce,
            body_bind,
            body_output,
            grid,
            input_versions: volume.inputs.versions(),
            config,
            source_elements,
            source_valid: false,
            nodal: None,
            #[cfg(test)]
            use_nodal_preconditioner: true,
            #[cfg(test)]
            nodal_switch_after: 1024,
            #[cfg(test)]
            cache_source_fields: true,
            #[cfg(test)]
            source_reused: false,
            cad_curves: model.curves.clone(),
            mode,
            time_s: 0.,
            last_step_dt_s: 0.,
            can_predict_increment: false,
            #[cfg(test)]
            predict_increments: true,
            #[cfg(test)]
            prediction_diagnostic: [0.; 3],
            diagnostics: Diagnostics::default(),
            conductivity_range: [0.; 2],
            params,
            uniform,
            _source: source,
            _work: work,
            _previous: previous,
            _partials: partials,
            scalars,
            result,
            checkpoint,
            elements,
            source_pipeline,
            motion_pipeline: None,
            pipelines,
            bind,
            source_bind,
            bytes,
        };
        state.params.control[0] = source_count;
        state.params.control[2] = u32::from(mode == Mode::Applied);
        state.params.control[1] = 1;
        state.write_params();
        state.dispatch(&[
            ("coefficients", state.edge_groups()),
            ("coefficient_check", [1, 1, 1]),
        ]);
        let check = state.read_f32(&state.scalars, 0, 4)?;
        state.conductivity_range = [if check[3] > 0. { check[1] } else { 0. }, check[2]];
        if mode == Mode::Induction && check[0] > 0. {
            return Err(
                "A conductor touches the EM outer boundary; add air padding to the domain".into(),
            );
        }
        state.advance_to(model, 0., true)?;
        Ok(state)
    }
    pub fn config(&self) -> &EmConfig {
        &self.config
    }
    pub fn result_buffer(&self) -> &wgpu::Buffer {
        &self.result
    }
    pub fn is_current(&self, volume: &Volume) -> bool {
        self.input_versions == volume.inputs.versions()
    }
    pub(crate) fn set_timestep(&mut self, dt_s: f64) {
        self.config.dt_s = dt_s;
    }
    fn edge_groups(&self) -> [u32; 3] {
        groups(self.params.edges[3])
    }
    fn write_params(&self) {
        self.context
            .queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&self.params));
    }
    fn dispatch(&self, commands: &[(&str, [u32; 3])]) {
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = self
                .context
                .compute_pass(&mut encoder, Some("EM compute batch"));
            pass.set_bind_group(0, &self.bind, &[]);
            let mut nodal_bound = false;
            for (name, g) in commands {
                if name.starts_with("nodal_") {
                    let node = self.nodal.as_ref().expect("Nodal workspace");
                    if !nodal_bound {
                        pass.set_bind_group(0, &node.bind, &[]);
                        nodal_bound = true;
                    }
                    pass.set_pipeline(&node.pipelines[name]);
                } else {
                    if nodal_bound {
                        pass.set_bind_group(0, &self.bind, &[]);
                        nodal_bound = false;
                    }
                    pass.set_pipeline(&self.pipelines[name]);
                }
                pass.dispatch_workgroups(g[0], g[1], g[2]);
            }
        }
        self.context.queue.submit(Some(encoder.finish()));
    }
    fn ensure_nodal(&mut self) -> Result<(), String> {
        if self.nodal.is_none() {
            let node = nodal::NodalCorrection::new(
                &self.context,
                &self.grid,
                &self.uniform,
                &self._source,
                &self._work,
                &self._partials,
                &self.scalars,
            )?;
            self.bytes += node.bytes;
            self.nodal = Some(node);
        }
        Ok(())
    }
    fn append_nodal(&self, commands: &mut Vec<(&'static str, [u32; 3])>, initial: bool) {
        if self.params.control[3] == 1 {
            let node = self.nodal.as_ref().expect("Nodal workspace");
            commands.push((
                if initial {
                    "nodal_build_initial"
                } else {
                    "nodal_build"
                },
                groups(node.nodes),
            ));
            commands.push((
                if initial {
                    "nodal_initial"
                } else {
                    "nodal_update"
                },
                self.edge_groups(),
            ));
        }
    }
    fn update_sources(&mut self, model: &Model, t: f64) -> Result<(), String> {
        let _stage = self
            .context
            .profiler
            .stage(crate::profile::Stage::EmSources);
        if self.cad_curves != model.curves {
            return Err("CAD centrelines changed; initialize electromagnetics again".into());
        }
        // Metadata is trustworthy only while source.x still holds that field.
        // Invalidate before fallible waveform evaluation; rollback also clears it.
        let mut changed = !self.source_valid;
        self.source_valid = false;
        #[cfg(test)]
        {
            changed |= !self.cache_source_fields;
            self.source_reused = false;
        }
        let mut offset = 0;
        for wire in &self.config.wires {
            let count = wire.points(model)?.len() - 1;
            let value = wire.current.value(t);
            if !value.is_finite() {
                return Err("Nonfinite source current at this time".into());
            }
            for e in &mut self.source_elements[offset..offset + count] {
                changed |= e.c[2].to_bits() != value.to_bits();
                e.c[2] = value;
            }
            offset += count;
        }
        let uniform_start = self.params.control[0] as usize - self.config.uniform.len();
        for (i, field) in self.config.uniform.iter().enumerate() {
            let value = field.scale.value(t);
            if !value.is_finite() {
                return Err("Nonfinite uniform source scale".into());
            }
            changed |= self.source_elements[uniform_start + i].c[2].to_bits() != value.to_bits();
            self.source_elements[uniform_start + i].c[2] = value;
        }
        if !changed {
            self.params.control[3] = 0;
            self.write_params();
            self.dispatch(&[("hold_source", self.edge_groups())]);
            self.source_valid = true;
            #[cfg(test)]
            {
                self.source_reused = true;
            }
            return Ok(());
        }
        self.context.queue.write_buffer(
            &self.elements,
            0,
            bytemuck::cast_slice(&self.source_elements),
        );
        for begin in (0..self.params.control[0].max(1)).step_by(32) {
            self.params.control[3] = begin;
            self.write_params();
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            {
                let mut pass = self.context.compute_pass(&mut encoder, None);
                pass.set_pipeline(&self.source_pipeline);
                pass.set_bind_group(0, &self.source_bind, &[]);
                let g = self.edge_groups();
                pass.dispatch_workgroups(g[0], g[1], g[2]);
            }
            self.context.queue.submit(Some(encoder.finish()));
        }
        self.params.control[3] = 0;
        self.write_params();
        self.source_valid = true;
        Ok(())
    }
    /// Prepare lagged motional EMF from the current fluid velocity and total B.
    /// The next induction step enforces the same discrete current continuity as at rest.
    /// Rebuild edge material coefficients after a conservative interface update.
    pub fn update_material_geometry(&mut self) -> Result<(), String> {
        let _stage = self
            .context
            .profiler
            .stage(crate::profile::Stage::EmCoefficients);
        self.dispatch(&[
            ("coefficients", self.edge_groups()),
            ("coefficient_check", [1, 1, 1]),
        ]);
        let check = self.read_f32(&self.scalars, 0, 4)?;
        self.conductivity_range = [if check[3] > 0. { check[1] } else { 0. }, check[2]];
        if check[0] > 0. {
            return Err(
                "Moving conductor reached the EM outer boundary; enlarge the domain".into(),
            );
        }
        Ok(())
    }
    pub fn set_velocity(
        &mut self,
        volume: &Volume,
        velocity: &wgpu::Buffer,
        grid: &GridSi,
    ) -> Result<(), String> {
        if grid != &self.grid
            || velocity.size() < self.params.dims[3] as u64 * 64
            || !velocity.usage().contains(wgpu::BufferUsages::STORAGE)
        {
            return Err("Motional induction requires a fluid packet on the same grid".into());
        }
        if self.motion_pipeline.is_none() {
            let shader = self
                .context
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Fluid velocity to motional EMF"),
                    source: wgpu::ShaderSource::Wgsl(
                        format!(
                            "{}\n{}",
                            include_str!("common.wgsl"),
                            include_str!("motion.wgsl")
                        )
                        .into(),
                    ),
                });
            self.motion_pipeline = Some(self.context.device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("u cross B edge projection"),
                    layout: None,
                    module: &shader,
                    entry_point: Some("motion"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            ));
        }
        let pipeline = self.motion_pipeline.as_ref().unwrap();
        let properties = volume.inputs.solver_tables(&[], false)?.0;
        let buffers = [
            &self.uniform,
            velocity,
            &self.result,
            &self._source,
            volume.material_ids(),
            properties,
        ];
        let entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(binding, b)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Motional feedback"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &entries,
            });
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = self.context.compute_pass(&mut encoder, None);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            let g = self.edge_groups();
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
        self.context.queue.submit(Some(encoder.finish()));
        Ok(())
    }
    pub fn step(&mut self, model: &Model, volume: &Volume) -> Result<&Diagnostics, String> {
        validate_model_inputs(volume, model)?;
        if !self.is_current(volume) {
            return Err("Simulation inputs changed; initialize electromagnetics again".into());
        }
        if self.mode == Mode::Applied {
            return Err("Applied-only mode uses evaluate_applied(time)".into());
        }
        let t = self.time_s + self.config.dt_s;
        self.advance_to(model, t, false)?;
        Ok(&self.diagnostics)
    }
    pub fn evaluate_applied(
        &mut self,
        model: &Model,
        time_s: f64,
        volume: &Volume,
    ) -> Result<&Diagnostics, String> {
        validate_model_inputs(volume, model)?;
        if !self.is_current(volume) {
            return Err("Simulation inputs changed; initialize electromagnetics again".into());
        }
        if self.mode != Mode::Applied || !time_s.is_finite() || time_s < 0. {
            return Err("Applied field time must be nonnegative and mode Applied".into());
        }
        self.advance_to(model, time_s, true)?;
        Ok(&self.diagnostics)
    }
    fn advance_to(&mut self, model: &Model, time: f64, initial: bool) -> Result<(), String> {
        let started = std::time::Instant::now();
        self.params.control[1] = u32::from(initial);
        // Reuse two accepted potentials to predict the next increment. The
        // physical RHS norm below remains independent of this initial guess.
        #[cfg(not(test))]
        let predict_increments = true;
        #[cfg(test)]
        let predict_increments = self.predict_increments;
        let prediction_factor = if predict_increments
            && !initial
            && self.can_predict_increment
            && self.last_step_dt_s > 0.
        {
            let ratio = ((time - self.time_s) / self.last_step_dt_s) as f32;
            if ratio.is_finite() && ratio > 0. {
                ratio
            } else {
                0.
            }
        } else {
            0.
        };
        self.params.origin[3] = prediction_factor;
        self.params.physical[0] = self.config.dt_s as f32;
        self.params.physical[3] = self.config.relative_tolerance.powi(2);
        #[cfg(not(test))]
        let nodal_switch_after = 1024;
        #[cfg(test)]
        let nodal_switch_after = self.nodal_switch_after;
        let allow_nodal = !initial && self.mode == Mode::Induction;
        #[cfg(test)]
        let allow_nodal = allow_nodal && self.use_nodal_preconditioner;
        let allow_nodal = allow_nodal && self.config.max_iterations > nodal_switch_after;
        if allow_nodal && nodal_switch_after == 0 {
            self.ensure_nodal()?;
        }
        self.update_sources(model, time)?;
        if allow_nodal && nodal_switch_after == 0 {
            self.params.control[3] = 1;
            self.write_params();
        }
        let solve_context = self.context.clone();
        let solve_stage = solve_context.profiler.stage(crate::profile::Stage::EmSolve);
        let eg = self.edge_groups();
        let mut commands = vec![("begin_step", eg), ("prepare", eg)];
        self.append_nodal(&mut commands, true);
        commands.push(("initial_reduce", [1, 1, 1]));
        self.dispatch(&commands);
        let mut status = self.read_f32(&self.scalars, 0, 8)?;
        if status[4] != 0. {
            // initial_reduce selected an exact power-of-two normalization.
            // Rebuild from physical b and x; original RHS-relative acceptance
            // and its existing absolute numeric floor transform together.
            let mut commands = vec![("prepare", eg)];
            self.append_nodal(&mut commands, true);
            commands.push(("initial_reduce", [1, 1, 1]));
            self.dispatch(&commands);
            status = self.read_f32(&self.scalars, 0, 8)?;
        }
        #[cfg(test)]
        {
            self.prediction_diagnostic = [
                prediction_factor,
                (status[1] / status[2].max(1e-38)).sqrt(),
                0.,
            ];
        }
        // A bad extrapolation must not start farther from the same physical RHS.
        if prediction_factor > 0.
            && (status.iter().any(|v| !v.is_finite()) || status[1] > status[2])
        {
            #[cfg(test)]
            {
                self.prediction_diagnostic[2] = 1.;
            }
            self.params.origin[3] = 0.;
            self.write_params();
            let mut commands = vec![("zero_guess", eg), ("prepare", eg)];
            self.append_nodal(&mut commands, true);
            commands.push(("initial_reduce", [1, 1, 1]));
            self.dispatch(&commands);
            status = self.read_f32(&self.scalars, 0, 8)?;
        }
        loop {
            while status[7] == 0.
                && status.iter().all(|v| v.is_finite())
                && (status[6] as u32) < self.config.max_iterations
            {
                let batch = (self.config.max_iterations - status[6] as u32).min(32);
                let mut commands = Vec::new();
                for _ in 0..batch {
                    commands.extend_from_slice(&[
                        ("apply_operator", eg),
                        ("alpha_reduce", [1, 1, 1]),
                        ("update_residual", eg),
                    ]);
                    self.append_nodal(&mut commands, false);
                    commands
                        .extend_from_slice(&[("beta_reduce", [1, 1, 1]), ("update_search", eg)]);
                }
                self.dispatch(&commands);
                status = self.read_f32(&self.scalars, 0, 8)?;
                if allow_nodal
                    && self.params.control[3] == 0
                    && status[7] == 0.
                    && status.iter().all(|v| v.is_finite())
                    && status[6] as u32 >= nodal_switch_after
                    && (status[6] as u32) < self.config.max_iterations
                {
                    if let Err(error) = self.ensure_nodal() {
                        self.dispatch(&[("rollback", eg)]);
                        self.can_predict_increment = false;
                        self.source_valid = false;
                        return Err(error);
                    }
                    // Switch only at a true-residual restart. The original RHS
                    // norm, acceptance threshold and total iteration count stay
                    // unchanged; ordinary Jacobi solves keep their exact path.
                    self.params.control[3] = 1;
                    self.write_params();
                    let mut restart = vec![("true_residual", eg)];
                    self.append_nodal(&mut restart, true);
                    restart.push(("restart_reduce", [1, 1, 1]));
                    self.dispatch(&restart);
                    status = self.read_f32(&self.scalars, 0, 8)?;
                }
            }
            if status[7] != 1. {
                break;
            }
            let mut commands = vec![("true_residual", eg)];
            self.append_nodal(&mut commands, true);
            commands.push(("restart_reduce", [1, 1, 1]));
            self.dispatch(&commands);
            status = self.read_f32(&self.scalars, 0, 8)?;
            if status[7] == 1. || status[6] as u32 >= self.config.max_iterations {
                break;
            }
        }
        // A cap or finite CG breakdown must still inspect the actual equation,
        // rather than reporting only the recursively accumulated residual.
        if status[7] != 1. && status.iter().all(|v| v.is_finite()) {
            let mut commands = vec![("true_residual", eg)];
            self.append_nodal(&mut commands, true);
            commands.push(("restart_reduce", [1, 1, 1]));
            self.dispatch(&commands);
            status = self.read_f32(&self.scalars, 0, 8)?;
        }
        if status[7] != 1. || status.iter().any(|v| !v.is_finite()) {
            self.dispatch(&[("rollback", eg)]);
            self.can_predict_increment = false;
            self.source_valid = false;
            return Err(format!(
                "EM solve did not converge: {} iterations, true relative residual {:.3e} (requested {:.3e}); conducting-edge σ range {:.3e}…{:.3e} S/m; state restored",
                status[6],
                (status[1] / status[2].max(1e-38)).sqrt(),
                self.config.relative_tolerance,
                self.conductivity_range[0],
                self.conductivity_range[1]
            ));
        }
        drop(solve_stage);
        let _fields_stage = self.context.profiler.stage(crate::profile::Stage::EmFields);
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.result, 0, &self.checkpoint, 0, self.result.size());
        self.context.queue.submit(Some(encoder.finish()));
        self.dispatch(&[
            ("fields", groups(self.params.dims[3])),
            ("field_reduce", [1, 1, 1]),
            ("commit_step", eg),
        ]);
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = self.context.compute_pass(&mut encoder, None);
            pass.set_bind_group(0, &self.body_bind, &[]);
            pass.set_pipeline(&self.body_collect);
            let g = groups(self.params.dims[3]);
            pass.dispatch_workgroups(g[0], g[1], self.bodies.len() as u32);
            pass.set_pipeline(&self.body_reduce);
            pass.dispatch_workgroups(1, 1, self.bodies.len() as u32);
        }
        self.context.queue.submit(Some(encoder.finish()));
        let totals = self.read_f32(&self.body_output, 0, self.bodies.len() * 4)?;
        let stats = self.read_f32(&self.scalars, 32, 16)?;
        if stats.iter().chain(&totals).any(|v| !v.is_finite()) {
            self.dispatch(&[("rollback", eg)]);
            self.can_predict_increment = false;
            self.source_valid = false;
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(&self.checkpoint, 0, &self.result, 0, self.result.size());
            self.context.queue.submit(Some(encoder.finish()));
            return Err("Nonfinite EM field output; state restored".into());
        }
        for (i, body) in self.bodies.iter_mut().enumerate() {
            body.force_n.copy_from_slice(&totals[i * 4..i * 4 + 3]);
            body.power_w = totals[i * 4 + 3];
        }
        self.last_step_dt_s = if initial { 0. } else { time - self.time_s };
        // A zero RHS has no useful nonzero increment to extrapolate. A failed
        // trial also overwrites the older history, so rollback clears this flag.
        self.can_predict_increment = !initial && status[2] > 0.;
        self.time_s = time;
        self.diagnostics = Diagnostics {
            maxima: stats[0..4].try_into().unwrap(),
            force_n: stats[4..7].try_into().unwrap(),
            power_w: stats[7],
            divergence_j_scaled: stats[8],
            max_edge_j: stats[9],
            divergence_b_scaled: stats[10],
            magnetic_energy_j: stats[11],
            applied_max_t: stats[12],
            reaction_max_t: stats[13],
            iterations: status[6] as u32,
            relative_residual: (status[1] / status[2].max(1e-38)).sqrt(),
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
        };
        Ok(())
    }
    fn read_f32(
        &self,
        buffer: &wgpu::Buffer,
        offset: u64,
        count: usize,
    ) -> Result<Vec<f32>, String> {
        Ok(bytemuck::cast_slice(&read_bytes(
            &self.context,
            buffer,
            offset,
            count as u64 * 4,
        )?)
        .to_vec())
    }
    /// Immutable field snapshot for analysis playback; solver state remains at its latest time.
    pub fn energy_step(&self) -> Result<EnergyStep, String> {
        self.dispatch(&[
            ("energy_collect", self.edge_groups()),
            ("energy_reduce", [1, 1, 1]),
        ]);
        let v = self.read_f32(&self.scalars, 0, 8)?;
        if v.iter().any(|v| !v.is_finite()) {
            return Err("Nonfinite EM energy integral".into());
        }
        let mut e = EnergyStep {
            magnetic_j: v[0] as f64,
            delta_magnetic_j: v[1] as f64,
            backward_euler_j: v[2] as f64,
            joule_j: v[3] as f64,
            mechanical_j: v[4] as f64,
            external_j: v[5] as f64,
            balance_error_j: 0.,
        };
        if self.time_s == 0. {
            return Ok(EnergyStep {
                magnetic_j: e.magnetic_j,
                ..Default::default()
            });
        }
        e.balance_error_j =
            e.external_j - e.delta_magnetic_j - e.backward_euler_j - e.joule_j - e.mechanical_j;
        Ok(e)
    }
    /// Reaction flux linkage by reciprocity, for a single winding and mu_r=1.
    /// This observes accepted fields; it does not impose or solve a coil voltage.
    /// The first observed nonzero interval captures an optional unit-winding basis,
    /// allowing linkage measurement after the applied current has returned to zero.
    /// Without that capture, linkage is unavailable during zero-current intervals.
    pub fn coil_response(&self, volume: &Volume) -> Result<CoilResponse, String> {
        if !self.is_current(volume)
            || self.mode != Mode::Induction
            || self.config.wires.len() != 1
            || !self.config.magnets.is_empty()
            || !self.config.uniform.is_empty()
        {
            return Err(
                "Coil response requires current inputs and exactly one winding source".into(),
            );
        }
        let snapshot = volume.inputs.snapshot();
        if snapshot.materials.iter().any(|m| {
            (m.id == 0
                || snapshot
                    .components
                    .iter()
                    .any(|c| c.included && c.material_id == m.id))
                && m.relative_permeability != Some(1.)
        }) {
            return Err("Coil response currently requires unit material permeability".into());
        }
        if self.last_step_dt_s == 0. {
            return Ok(CoilResponse::default());
        }
        let wave = &self.config.wires[0].current;
        let now = wave.value(self.time_s);
        let old = wave.value(self.time_s - self.last_step_dt_s);
        let mut observer = self.coil_observer.lock().unwrap();
        let mut initialize = false;
        if observer.is_none() {
            if now == 0. && old == 0. {
                // Without a previously captured winding basis linkage is unavailable.
                // Applied A is zero here, so total/reaction energy coincide.
                let energy = self.energy_step()?;
                return Ok(CoilResponse {
                    reaction_linkage_wb: None,
                    reaction_magnetic_j: energy.magnetic_j,
                    reaction_backward_euler_j: energy.backward_euler_j,
                });
            }
            let bytes = u64::from(self.params.edges[3].div_ceil(16)) * 64;
            self.context
                .ensure_memory(bytes, "Optional winding observation basis")?;
            let basis = self.context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Unit winding A, packed independently of accepted fields"),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            let properties = volume.inputs.solver_tables(&[], false)?.0;
            let buffers = [
                &self.uniform,
                volume.material_ids(),
                properties,
                &self._source,
                &self._work,
                &self._previous,
                &self._partials,
                &self.scalars,
                &basis,
            ];
            let layout = self.pipelines["coil_response_collect"].get_bind_group_layout(0);
            let bind = self
                .context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Read-only winding observation with private basis output"),
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
            let (amplitude, use_old) = if now.abs() >= old.abs() && now != 0. {
                (now, 0.)
            } else {
                (old, 1.)
            };
            self.context.queue.write_buffer(
                &self.scalars,
                112,
                bytemuck::cast_slice(&[amplitude, use_old, 0_f32, 0.]),
            );
            *observer = Some(CoilObserver { bind, bytes });
            initialize = true;
        }
        let observer = observer.as_ref().unwrap();
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = self
                .context
                .compute_pass(&mut encoder, Some("Winding reaction observation"));
            pass.set_bind_group(0, &observer.bind, &[]);
            if initialize {
                let g = groups(self.params.edges[3].div_ceil(16));
                pass.set_pipeline(&self.pipelines["initialize_coil_basis"]);
                pass.dispatch_workgroups(g[0], g[1], g[2]);
            }
            pass.set_pipeline(&self.pipelines["coil_response_collect"]);
            let g = self.edge_groups();
            pass.dispatch_workgroups(g[0], g[1], g[2]);
            pass.set_pipeline(&self.pipelines["coil_response_reduce"]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        self.context.queue.submit(Some(encoder.finish()));
        let sums = self.read_f32(&self.scalars, 112, 4)?;
        if sums.iter().any(|v| !v.is_finite()) {
            return Err("Nonfinite winding response integral".into());
        }
        let linkage = Some(f64::from(sums[0]));
        Ok(CoilResponse {
            reaction_linkage_wb: linkage,
            reaction_magnetic_j: f64::from(sums[2]),
            reaction_backward_euler_j: f64::from(sums[3]),
        })
    }
    /// Optional observer storage, allocated only after a nonzero winding interval.
    pub fn observation_bytes(&self) -> u64 {
        self.coil_observer
            .lock()
            .unwrap()
            .as_ref()
            .map_or(0, |o| o.bytes)
    }
    pub fn snapshot(&self) -> EmFrame {
        let result = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Recorded analysis fields"),
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
        EmFrame {
            context: self.context.clone(),
            grid: self.grid.clone(),
            input_versions: self.input_versions,
            result: crate::recording::FieldStorage::gpu(result),
            time_s: self.time_s,
            mode: self.mode,
            diagnostics: self.diagnostics.clone(),
            bodies: self.bodies.clone(),
            bytes: self.result.size(),
        }
    }
    pub(crate) fn field_view(&self) -> EmFrame {
        EmFrame {
            context: self.context.clone(),
            grid: self.grid.clone(),
            input_versions: self.input_versions,
            result: crate::recording::FieldStorage::gpu(self.result.clone()),
            time_s: self.time_s,
            mode: self.mode,
            diagnostics: self.diagnostics.clone(),
            bodies: self.bodies.clone(),
            bytes: self.result.size(),
        }
    }
    pub fn probe(&self, position_mm: [f32; 3]) -> Result<Option<CellField>, String> {
        self.field_view().probe(position_mm)
    }
    #[cfg(test)]
    fn all_fields(&self) -> Vec<CellField> {
        let bytes = read_bytes(
            &self.context,
            &self.result,
            0,
            self.params.dims[3] as u64 * 64,
        )
        .unwrap();
        bytemuck::cast_slice(&bytes).to_vec()
    }
}
/// GPU-resident output and diagnostics at one recorded time; no resumable solver state.
#[derive(Clone)]
pub struct EmFrame {
    input_versions: InputVersions,
    context: GpuContext,
    grid: GridSi,
    pub(crate) result: crate::recording::FieldStorage,
    pub time_s: f64,
    pub mode: Mode,
    pub diagnostics: Diagnostics,
    pub bodies: Vec<BodyDiagnostic>,
    pub bytes: u64,
}
impl EmFrame {
    pub fn grid(&self) -> &GridSi {
        &self.grid
    }
    pub fn is_current(&self, volume: &Volume) -> bool {
        self.input_versions == volume.inputs.versions()
    }
    pub fn result_buffer(&self) -> &wgpu::Buffer {
        self.result.buffer()
    }
    pub fn probe(&self, position_mm: [f32; 3]) -> Result<Option<CellField>, String> {
        crate::recording::probe(&self.context, &self.grid, &self.result, position_mm)
    }
    pub fn is_resident(&self) -> bool {
        self.result.is_resident()
    }
}

fn groups(n: u32) -> [u32; 3] {
    let n = n.div_ceil(256);
    [n.min(65535), n.div_ceil(65535), 1]
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod team6;

fn validate_model_inputs(volume: &Volume, model: &Model) -> Result<(), String> {
    let snapshot = crate::simulation::SimulationSnapshot::from_model(
        model,
        &volume.settings,
        &volume.inputs.snapshot().materials,
    )?;
    if snapshot != *volume.inputs.snapshot() {
        return Err(
            "Component or geometry inputs changed; update inputs/rebuild before electromagnetics"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod prediction_tests;

fn shader_source() -> String {
    format!(
        "{}\n{}\n{}\n{}",
        include_str!("common.wgsl"),
        include_str!("induction.wgsl"),
        include_str!("energy.wgsl"),
        include_str!("nodal.wgsl")
    )
}
