//! Experimental fixed symmetric GPU aggregation preconditioner.
use super::*;
const COARSE_SWEEPS: u32 = 8;

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
    coeff: wgpu::Buffer,
    value: wgpu::Buffer,
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

impl Multigrid {
    fn new(state: &FluidState) -> Result<Self, String> {
        let context = &state.context;
        let device = &context.device;
        let (levels, offset) =
            hierarchy(state.params.dims, state.params.faces, state.params.flags[0])?;
        let size = (offset * 16).max(16);
        let limits = device.limits();
        if size > limits.max_storage_buffer_binding_size as u64 || size > limits.max_buffer_size {
            return Err("Coarse pressure workspace exceeds the GPU buffer binding limit".into());
        }
        context.ensure_memory(
            2 * size + levels.len() as u64 * std::mem::size_of::<Level>() as u64,
            "Optional pressure preconditioner",
        )?;
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Aggregation pressure prototype"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "const COARSE_SWEEPS:u32={COARSE_SWEEPS}u;\n{}\n{}",
                    include_str!("../periodic.wgsl"),
                    include_str!("multigrid.wgsl")
                )
                .into(),
            ),
        });
        let pipelines = [
            "build",
            "clear",
            "smooth_to_temp",
            "smooth_from_temp",
            "restrict_residual",
            "prolong",
            "probe_action",
            "initial_direction",
            "preconditioned_dot",
            "preconditioned_search",
            "coarse_solve",
            "diagonal_start",
            "commit_smooth",
        ]
        .into_iter()
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
        Ok(Self {
            levels,
            binds,
            pipelines,
            coeff,
            value,
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
    fn build(&self, context: &GpuContext) {
        self.dispatch(
            context,
            &(1..self.levels.len())
                .map(|l| (l, "build"))
                .collect::<Vec<_>>(),
        );
    }
    fn cycle_stages(&self) -> Vec<(usize, &'static str)> {
        let mut stages = Vec::new();
        for l in 0..self.levels.len() - 1 {
            stages.extend([(l, "diagonal_start"), (l + 1, "restrict_residual")]);
        }
        let coarsest = self.levels.len() - 1;
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
    fn apply(&self, context: &GpuContext) {
        self.dispatch(context, &self.cycle_stages());
    }
}

fn encode_fluid(state: &FluidState, pass: &mut wgpu::ComputePass<'_>, stages: &[(&str, [u32; 3])]) {
    pass.set_bind_group(0, &state.bind, &[]);
    for (stage, g) in stages {
        pass.set_pipeline(&state.pipelines[stage]);
        pass.dispatch_workgroups(g[0], g[1], g[2]);
    }
}
fn project_inner(
    state: &FluidState,
    mg: &Multigrid,
    warm_start: bool,
) -> Result<(u32, f32), String> {
    let cells = groups(state.params.dims[3]);
    let preparation = [
        ("pressure_warm", groups(state.params.faces[3])),
        ("pressure_coefficients", groups(state.params.faces[3])),
        ("pressure_guess", cells),
        ("pressure_refresh", cells),
        ("initial_reduce", [1, 1, 1]),
    ];
    state.dispatch(if warm_start {
        &preparation
    } else {
        &preparation[1..]
    });
    let mut residual = state.convergence()?;
    let initialize = || {
        mg.apply(&state.context);
        mg.dispatch(&state.context, &[(0, "initial_direction")]);
        state.dispatch(&[("initial_reduce", [1, 1, 1])]);
    };
    if !residual.1 {
        mg.build(&state.context);
        initialize();
    }
    let mut iterations = 0;
    let cycle_stages = mg.cycle_stages();
    while !residual.1 && iterations < state.config.max_iterations {
        let batch = 8
            .min(state.config.max_iterations - iterations)
            .min(64 - iterations % 64);
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
                mg.encode_stages(&mut pass, &cycle_stages);
                mg.encode_stages(&mut pass, &[(0, "preconditioned_dot")]);
                encode_fluid(state, &mut pass, &[("beta_reduce", [1, 1, 1])]);
                mg.encode_stages(&mut pass, &[(0, "preconditioned_search")]);
            }
        }
        state.context.queue.submit(Some(encoder.finish()));
        residual = state.convergence()?;
        if residual.2 <= iterations && !residual.1 {
            return Err("No prototype progress".into());
        }
        iterations = residual.2;
        if residual.1 {
            state.dispatch(&[("pressure_refresh", cells), ("initial_reduce", [1, 1, 1])]);
            residual = state.convergence()?;
            if !residual.1 {
                initialize();
            }
        } else if iterations % 64 == 0 || iterations == state.config.max_iterations {
            state.dispatch(&[("pressure_check", cells), ("check_reduce", [1, 1, 1])]);
            residual = state.convergence()?;
        }
    }
    if !residual.1 {
        return Err(format!(
            "Fluid pressure solve (experimental multigrid) did not converge after {iterations} iterations, residual {}",
            residual.0
        ));
    }
    let completion = [
        ("correct", groups(state.params.faces[3])),
        ("pressure_commit", cells),
    ];
    state.dispatch(if warm_start {
        &completion
    } else {
        &completion[..1]
    });
    Ok((iterations, residual.0))
}

pub(super) fn project(state: &mut FluidState, warm_start: bool) -> Result<(u32, f32), String> {
    let profiler = state.context.profiler.clone();
    let _stage = profiler.stage(crate::profile::Stage::Pressure);
    let mg = match state.multigrid.take() {
        Some(mg) => mg,
        None => Multigrid::new(state)?,
    };
    let result = project_inner(state, &mg, warm_start);
    state.multigrid = Some(mg);
    result
}

#[cfg(test)]
mod tests;
