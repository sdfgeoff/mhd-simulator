//! Fixed symmetric GPU aggregation preconditioner for the existing fine pressure equation.
use super::*;
const COARSE_SWEEPS: u32 = 8;
pub(super) const MIN_CELLS: u32 = 96 * 96 * 96;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Level {
    dims: [u32; 4],
    child: [u32; 4],
    faces: [u32; 4],
    meta: [u32; 4],
}
pub(super) struct Multigrid {
    levels: Vec<Level>,
    binds: Vec<wgpu::BindGroup>,
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    #[cfg(test)]
    coeff: wgpu::Buffer,
    #[cfg(test)]
    value: wgpu::Buffer,
    pub(super) bytes: u64,
    cycle: Vec<(usize, &'static str)>,
}
fn hierarchy(dims: [u32; 4], faces: [u32; 4], periodic: u32) -> Result<(Vec<Level>, u64), String> {
    let fine = dims;
    let count = dims[..3]
        .iter()
        .try_fold(1u64, |n, &d| n.checked_mul(d as u64))
        .ok_or("Pressure grid count overflow")?;
    if dims[..3].iter().any(|&n| n > i32::MAX as u32) {
        return Err("Pressure grid dimensions exceed signed shader coordinates".into());
    }
    if dims[..3].contains(&0) || count != dims[3] as u64 {
        return Err("Invalid pressure grid dimensions".into());
    }
    let mut levels = vec![Level {
        dims: fine,
        child: fine,
        faces,
        meta: [0, 0, 0, periodic],
    }];
    let mut offset = 0u64;
    while levels.last().unwrap().dims[..3].iter().any(|&n| n > 4) {
        let child = *levels.last().unwrap();
        let d = [child.dims[0], child.dims[1], child.dims[2]].map(|n| n.div_ceil(2));
        let count = d.iter().product();
        levels.push(Level {
            dims: [d[0], d[1], d[2], count],
            child: child.dims,
            faces,
            meta: [
                u32::try_from(offset).map_err(|_| "Coarse pressure indices exceed u32")?,
                child.meta[0],
                levels.len() as u32,
                periodic,
            ],
        });
        offset += count as u64;
        if offset > u32::MAX as u64 {
            return Err("Coarse pressure indices exceed u32".into());
        }
    }
    Ok((levels, offset))
}

fn cycle_stages(level_count: usize) -> Vec<(usize, &'static str)> {
    let mut stages = Vec::new();
    for l in 0..level_count - 1 {
        stages.extend([(l, "diagonal_start"), (l + 1, "restrict_residual")]);
    }
    let coarsest = level_count - 1;
    stages.push((coarsest, "coarse_solve"));
    for l in (0..coarsest).rev() {
        stages.extend([
            (l + 1, "prolong"),
            (l, "smooth_to_temp"),
            (l, "commit_smooth"),
        ]);
    }
    stages
}

impl Multigrid {
    pub(super) fn new(state: &FluidState) -> Result<Self, String> {
        let context = &state.context;
        let device = &context.device;
        let (levels, offset) =
            hierarchy(state.params.dims, state.params.faces, state.params.flags[0])?;
        let size = (offset * 16).max(16);
        let limits = device.limits();
        if size > limits.max_storage_buffer_binding_size as u64 || size > limits.max_buffer_size {
            return Err("Coarse pressure workspace exceeds the GPU buffer binding limit".into());
        }
        let bytes = 2 * size + levels.len() as u64 * std::mem::size_of::<Level>() as u64;
        context.ensure_memory(bytes, "Optional pressure preconditioner")?;
        let buffer = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let coeff = buffer("Coarse pressure graph");
        let value = buffer("Coarse pressure correction");
        let entries: Vec<_> = (0..9)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage {
                            read_only: binding == 1 || binding == 2 || binding == 8,
                        }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Pressure multigrid"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let source = include_str!("multigrid.wgsl").to_owned();
        #[cfg(test)]
        let source = if state.floating_pressure {
            let old = "return (mask[i]&3u)==1u;";
            assert_eq!(source.matches(old).count(), 1);
            let source = source.replace(old, "return mask[i]!=0u;");
            let old =
                "if fine && (mask[index(n,p.child.xyz)]&3u)==2u {c.w+=beta;diag+=beta;continue;}";
            assert_eq!(source.matches(old).count(), 1);
            source.replace(old, "")
        } else {
            source
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Aggregation pressure preconditioner"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "const COARSE_SWEEPS:u32={COARSE_SWEEPS}u;\n{}\n{}\n{}",
                    include_str!("../periodic.wgsl"),
                    source,
                    if cfg!(test) {
                        include_str!("multigrid_probes.wgsl")
                    } else {
                        ""
                    }
                )
                .into(),
            ),
        });
        let pipelines = [
            "build",
            "smooth_to_temp",
            "restrict_residual",
            "prolong",
            "initial_direction",
            "preconditioned_dot",
            "preconditioned_search",
            "coarse_solve",
            "diagonal_start",
            "commit_smooth",
        ]
        .into_iter()
        .chain(
            if cfg!(test) {
                &["clear", "smooth_from_temp", "probe_action"][..]
            } else {
                &[][..]
            }
            .iter()
            .copied(),
        )
        .map(|name| {
            (
                name,
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(name),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some(name),
                    compilation_options: Default::default(),
                    cache: None,
                }),
            )
        })
        .collect();
        let binds = levels
            .iter()
            .map(|level| {
                let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Pressure level"),
                    contents: bytemuck::bytes_of(level),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let buffers = [
                    &uniform,
                    &state._mask,
                    &state.velocity,
                    &state._work,
                    &state.scratch,
                    &coeff,
                    &value,
                    &state._partials,
                    &state.scalars,
                ];
                let entries: Vec<_> = buffers
                    .iter()
                    .enumerate()
                    .map(|(i, b)| wgpu::BindGroupEntry {
                        binding: i as u32,
                        resource: b.as_entire_binding(),
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &layout,
                    entries: &entries,
                })
            })
            .collect();
        let cycle = cycle_stages(levels.len());
        Ok(Self {
            levels,
            binds,
            pipelines,
            #[cfg(test)]
            coeff,
            #[cfg(test)]
            value,
            bytes,
            cycle,
        })
    }
    fn encode(
        &self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        stages: &[(usize, &str)],
    ) {
        let mut pass = context.compute_pass(encoder, Some("Pressure aggregation"));
        self.encode_stages(&mut pass, stages);
    }
    fn encode_stages(&self, pass: &mut wgpu::ComputePass<'_>, stages: &[(usize, &str)]) {
        for &(level, stage) in stages {
            pass.set_bind_group(0, &self.binds[level], &[]);
            pass.set_pipeline(&self.pipelines[stage]);
            let n = if stage == "prolong" {
                self.levels[level].child[3]
            } else {
                self.levels[level].dims[3]
            };
            let g = if stage == "coarse_solve" {
                [1, 1, 1]
            } else {
                groups(n)
            };
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
    }
    fn dispatch(&self, context: &GpuContext, stages: &[(usize, &str)]) {
        let mut encoder = context.device.create_command_encoder(&Default::default());
        self.encode(context, &mut encoder, stages);
        context.queue.submit(Some(encoder.finish()));
    }
    pub(super) fn build(&self, context: &GpuContext) {
        self.dispatch(
            context,
            &(1..self.levels.len())
                .map(|l| (l, "build"))
                .collect::<Vec<_>>(),
        );
    }
    fn apply(&self, context: &GpuContext) {
        self.dispatch(context, &self.cycle);
    }
}

fn encode_fluid(state: &FluidState, pass: &mut wgpu::ComputePass<'_>, stages: &[(&str, [u32; 3])]) {
    pass.set_bind_group(0, &state.bind, &[]);
    for (stage, g) in stages {
        pass.set_pipeline(&state.pipelines[stage]);
        pass.dispatch_workgroups(g[0], g[1], g[2]);
    }
}
impl Multigrid {
    pub(super) fn initialize(&self, state: &FluidState) {
        self.apply(&state.context);
        self.dispatch(&state.context, &[(0, "initial_direction")]);
        #[cfg(test)]
        if state.floating_pressure {
            state.dispatch(&[("floating_direction_collect", groups(state.params.dims[3]))]);
        }
        state.dispatch(&[("initial_reduce", [1, 1, 1])]);
    }
    pub(super) fn iterate(&self, state: &FluidState, batch: u32) {
        let cells = groups(state.params.dims[3]);
        let mut encoder = state
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = state
                .context
                .compute_pass(&mut encoder, Some("Multigrid-preconditioned CG batch"));
            for _ in 0..batch {
                encode_fluid(
                    state,
                    &mut pass,
                    &[
                        ("pressure_apply", cells),
                        ("alpha_reduce", [1, 1, 1]),
                        ("update", cells),
                    ],
                );
                #[cfg(test)]
                if state.floating_pressure {
                    encode_fluid(
                        state,
                        &mut pass,
                        &[
                            ("floating_residual_reduce", [1, 1, 1]),
                            ("floating_residual_project", cells),
                        ],
                    );
                }
                self.encode_stages(&mut pass, &self.cycle);
                self.encode_stages(&mut pass, &[(0, "preconditioned_dot")]);
                encode_fluid(state, &mut pass, &[("beta_reduce", [1, 1, 1])]);
                self.encode_stages(&mut pass, &[(0, "preconditioned_search")]);
                #[cfg(test)]
                if state.floating_pressure {
                    encode_fluid(
                        state,
                        &mut pass,
                        &[
                            ("floating_direction_collect", cells),
                            ("floating_search_reduce", [1, 1, 1]),
                        ],
                    );
                }
            }
        }
        state.context.queue.submit(Some(encoder.finish()));
    }
}

#[cfg(test)]
mod tests;
