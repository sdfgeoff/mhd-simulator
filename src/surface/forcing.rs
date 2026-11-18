//! Explicit prescribed-force nozzle benchmark; not a computed electromagnetic source.
use crate::{simulation::GridSi, surface::SurfaceState};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JetPulse {
    pub peak_n_m3: f32,
    pub duration_s: f64,
    pub radial_mm: [f32; 2],
    pub axial_mm: [f32; 4],
}
impl Default for JetPulse {
    fn default() -> Self {
        Self {
            peak_n_m3: 7e7,
            duration_s: 0.0005,
            radial_mm: [0.45, 0.55],
            axial_mm: [-0.85, -0.75, -0.25, -0.15],
        }
    }
}
impl JetPulse {
    pub fn validate(&self) -> Result<(), String> {
        if !self.peak_n_m3.is_finite()
            || self.peak_n_m3 < 0.
            || !self.duration_s.is_finite()
            || self.duration_s <= 0.
            || !(self.duration_s as f32).is_normal()
            || self
                .radial_mm
                .iter()
                .chain(&self.axial_mm)
                .any(|v| !v.is_finite())
            || self.radial_mm[0] < 0.
            || self.radial_mm[0] >= self.radial_mm[1]
            || self.axial_mm.windows(2).any(|v| v[0] >= v[1])
        {
            return Err("Invalid prescribed nozzle pulse: use finite nonnegative force, positive duration and ordered window bounds".into());
        }
        Ok(())
    }
    pub fn amplitude(&self, midpoint_s: f64) -> f32 {
        if midpoint_s <= 0. || midpoint_s >= self.duration_s {
            0.
        } else {
            self.peak_n_m3
                * (std::f64::consts::PI * midpoint_s / self.duration_s)
                    .sin()
                    .powi(2) as f32
        }
    }
}
pub(crate) struct Pulse {
    pub(crate) force: wgpu::Buffer,
    uniform: wgpu::Buffer,
    binds: [wgpu::BindGroup; 2],
    pipeline: wgpu::ComputePipeline,
    dims: [u32; 4],
    origin: [f32; 4],
    h: f32,
    config: JetPulse,
}
impl Pulse {
    pub(crate) fn new(surface: &SurfaceState, grid: &GridSi, config: JetPulse) -> Self {
        let device = &surface.context.device;
        let n = grid.dims.iter().product::<u32>();
        let force = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Prescribed jet force; not an EM solve"),
            size: n as u64 * 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Jet force pulse parameters"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Jet pulse"),
            source: wgpu::ShaderSource::Wgsl(include_str!("forcing.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Jet pulse"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let binds = std::array::from_fn(|current| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: surface.fields[current].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: force.as_entire_binding(),
                    },
                ],
            })
        });
        Self {
            force,
            uniform,
            binds,
            pipeline,
            dims: [grid.dims[0], grid.dims[1], grid.dims[2], n],
            origin: [grid.origin_m[0], grid.origin_m[1], grid.origin_m[2], 0.],
            h: grid.cell_m,
            config,
        }
    }
    pub(crate) fn update(&self, surface: &SurfaceState, force: f32) {
        let mut bytes = bytemuck::bytes_of(&self.dims).to_vec();
        bytes.extend_from_slice(bytemuck::bytes_of(&self.origin));
        bytes.extend_from_slice(bytemuck::bytes_of(&[self.h, force, 0., 0.]));
        bytes.extend_from_slice(bytemuck::bytes_of(&[
            self.config.radial_mm[0] * 0.001,
            self.config.radial_mm[1] * 0.001,
            0.,
            0.,
        ]));
        bytes.extend_from_slice(bytemuck::bytes_of(&self.config.axial_mm.map(|v| v * 0.001)));
        surface.context.queue.write_buffer(&self.uniform, 0, &bytes);
        let mut encoder = surface
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = surface.context.compute_pass(&mut encoder, None);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.binds[surface.current], &[]);
            let g = [
                self.dims[3].div_ceil(256).min(65535),
                self.dims[3].div_ceil(256).div_ceil(65535),
                1,
            ];
            pass.dispatch_workgroups(g[0], g[1], g[2]);
        }
        surface.context.queue.submit(Some(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_pulse_preserves_timing_and_rejects_invalid_windows() {
        let pulse = JetPulse::default();
        pulse.validate().unwrap();
        assert_eq!(pulse.amplitude(0.), 0.);
        assert_eq!(pulse.amplitude(pulse.duration_s), 0.);
        assert_eq!(pulse.amplitude(pulse.duration_s * 0.5), pulse.peak_n_m3);
        let run = crate::run::RunConfig {
            prescribed_jet: Some(pulse.clone()),
            ..Default::default()
        };
        let roundtrip: crate::run::RunConfig =
            serde_json::from_str(&serde_json::to_string(&run).unwrap()).unwrap();
        assert_eq!(run, roundtrip);
        let old: crate::run::RunConfig = serde_json::from_str("{}").unwrap();
        assert!(old.prescribed_jet.is_none());
        for invalid in [
            JetPulse {
                peak_n_m3: f32::NAN,
                ..pulse.clone()
            },
            JetPulse {
                duration_s: 0.,
                ..pulse.clone()
            },
            JetPulse {
                radial_mm: [0.55, 0.45],
                ..pulse.clone()
            },
            JetPulse {
                axial_mm: [-1., 0., 0., 1.],
                ..pulse
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }
}
