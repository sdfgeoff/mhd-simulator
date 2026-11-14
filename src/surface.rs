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
#[derive(Clone, Debug, Default)]
pub struct SurfaceDiagnostics {
    pub material_id: u32,
    pub volume_m3: f64,
    pub area_m2: f64,
    pub min_fraction: f32,
    pub max_fraction: f32,
    pub transport_error_m3: f64,
    pub max_curvature_m_inv: f32,
}
pub struct SurfaceState {
    context: GpuContext,
    pub grid: GridSi,
    pub diagnostics: SurfaceDiagnostics,
    params: Params,
    uniform: wgpu::Buffer,
    fields: [wgpu::Buffer; 2],
    current: usize,
    normals: wgpu::Buffer,
    transported: wgpu::Buffer,
    checkpoint: wgpu::Buffer,
    stats: wgpu::Buffer,
    zero: wgpu::Buffer,
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    layout: wgpu::BindGroupLayout,
    id_pipeline: wgpu::ComputePipeline,
}
impl SurfaceState {
    pub fn new(
        context: GpuContext,
        volume: &Volume,
        material: u32,
    ) -> Result<(Self, Volume), String> {
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
            n * 64,
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
            choice: [material, 0, 0, 0],
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
        let transported = buffer("Co-transported enthalpy", n * 64);
        let checkpoint = buffer("Accepted interface checkpoint", n * 16);
        let stats = buffer("Interface reductions", (n.div_ceil(256) + 1) * 32);
        let zero = buffer("Absent interface input", 64);
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Conservative liquid interface"),
                source: wgpu::ShaderSource::Wgsl(include_str!("surface.wgsl").into()),
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
            "advect",
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
            uniform,
            fields,
            current: 0,
            normals,
            transported,
            checkpoint,
            stats,
            zero,
            pipelines,
            layout,
            id_pipeline,
        };
        s.dispatch(volume, None, None, &["initialize"]);
        s.current = 1;
        s.refresh(volume)?;
        s.update_materials(volume, &evolving);
        Ok((s, evolving))
    }
    pub fn field(&self) -> &wgpu::Buffer {
        &self.fields[self.current]
    }
    pub fn transported_thermal(&self) -> &wgpu::Buffer {
        &self.transported
    }
    fn dispatch(
        &self,
        volume: &Volume,
        velocity: Option<&wgpu::Buffer>,
        thermal: Option<&wgpu::Buffer>,
        entries: &[&str],
    ) {
        let buffers = [
            &self.uniform,
            volume.material_ids(),
            self.field(),
            &self.fields[1 - self.current],
            velocity.unwrap_or(&self.zero),
            thermal.unwrap_or(&self.zero),
            &self.transported,
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
            let mut pass = enc.begin_compute_pass(&Default::default());
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
        if s.iter().any(|v| !v.is_finite()) || s[3] > 0. || s[4] < -1e-5 || s[5] > 1.00001 {
            return Err(
                "Liquid interface transport left [0,1] or became nonfinite; reduce timestep".into(),
            );
        }
        self.diagnostics.material_id = self.params.choice[0];
        self.diagnostics.volume_m3 = s[0] as f64;
        self.diagnostics.area_m2 = s[1] as f64;
        self.diagnostics.min_fraction = s[4];
        self.diagnostics.max_fraction = s[5];
        self.diagnostics.max_curvature_m_inv = s[6];
        Ok(())
    }
    pub fn advance(
        &mut self,
        volume: &Volume,
        velocity: &wgpu::Buffer,
        thermal: Option<&wgpu::Buffer>,
        dt: f64,
        max_speed: f32,
    ) -> Result<(), String> {
        if dt <= 0.
            || !dt.is_finite()
            || !max_speed.is_finite()
            || max_speed < 0.
            || 3. * max_speed * dt as f32 / self.grid.cell_m > 0.45
        {
            return Err("Conservative interface CFL exceeds 0.45; reduce common timestep".into());
        }
        let old = self.diagnostics.volume_m3;
        let old_diagnostics = self.diagnostics.clone();
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(self.field(), 0, &self.checkpoint, 0, self.field().size());
        self.context.queue.submit(Some(enc.finish()));
        self.params.step[0] = dt as f32;
        self.params.choice[1] = u32::from(thermal.is_some());
        self.context
            .queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&self.params));
        self.dispatch(volume, Some(velocity), thermal, &["advect"]);
        self.current = 1 - self.current;
        if let Err(e) = self.refresh(volume) {
            let mut enc = self
                .context
                .device
                .create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(&self.checkpoint, 0, self.field(), 0, self.field().size());
            self.context.queue.submit(Some(enc.finish()));
            self.diagnostics = old_diagnostics;
            return Err(e);
        }
        self.diagnostics.transport_error_m3 += self.diagnostics.volume_m3 - old;
        Ok(())
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
            let mut pass = enc.begin_compute_pass(&Default::default());
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
mod tests;
