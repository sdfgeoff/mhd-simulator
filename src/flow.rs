//! GPU-only animated direction cues for cell-centred vector fields.
//! Traces project the field onto a section plane; animation time is separate from solver time.
use crate::{gpu::GpuContext, plane::SectionPlane, simulation::GridSi};
use bytemuck::{Pod, Zeroable};

pub const STEPS: u32 = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorStyle {
    Off,
    Arrows,
    Trails,
    Dots,
}
impl VectorStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Arrows => "Arrows",
            Self::Trails => "Animated trails",
            Self::Dots => "Animated dots",
        }
    }
    pub fn animated(self) -> bool {
        matches!(self, Self::Trails | Self::Dots)
    }
}
/// Packed vec4 cells; offsets allow reuse with velocity buffers without EM-specific shaders.
#[derive(Clone, Copy)]
pub struct VectorLayout {
    pub stride: u32,
    pub offset: u32,
    pub subtract: Option<u32>,
    pub interpolate: bool,
}
pub struct VectorField<'a> {
    pub buffer: &'a wgpu::Buffer,
    pub grid: &'a GridSi,
    pub layout: VectorLayout,
}
#[derive(Clone, Copy)]
pub struct TrailSettings {
    pub density: u32,
    pub speed: f32,
    pub tail: f32,
    pub dots: bool,
}
impl Default for TrailSettings {
    fn default() -> Self {
        Self {
            density: 25,
            speed: 0.35,
            tail: 0.22,
            dots: false,
        }
    }
}
#[derive(Clone, Copy)]
pub struct TrailDraw<'a> {
    pub buffer: &'a wgpu::Buffer,
    pub vertices: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    origin: [f32; 4],
    u: [f32; 4],
    v: [f32; 4],
    grid: [f32; 4],
    dims: [u32; 4],
    source: [u32; 4],
    motion: [f32; 4],
}
pub struct FlowTrails {
    context: GpuContext,
    pipeline: wgpu::ComputePipeline,
    uniform: wgpu::Buffer,
    vertices: wgpu::Buffer,
    count: u32,
}
impl FlowTrails {
    pub fn new(context: GpuContext) -> Self {
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Section flow traces"),
                source: wgpu::ShaderSource::Wgsl(include_str!("flow.wgsl").into()),
            });
        let pipeline = context
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Projected RK2 streamlines"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let uniform = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Flow display parameters"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vertices = Self::buffer(&context, 32);
        Self {
            context,
            pipeline,
            uniform,
            vertices,
            count: 0,
        }
    }
    fn buffer(context: &GpuContext, bytes: u64) -> wgpu::Buffer {
        context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animated flow geometry"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    }
    pub fn draw(&self) -> TrailDraw<'_> {
        TrailDraw {
            buffer: &self.vertices,
            vertices: self.count,
        }
    }
    pub fn update(
        &mut self,
        field: VectorField<'_>,
        plane: SectionPlane,
        span_mm: f32,
        time_s: f64,
        settings: TrailSettings,
    ) -> Result<(), String> {
        let layout = field.layout;
        if field.grid.dims.iter().any(|&n| n == 0 || n > 2048) {
            self.count = 0;
            return Err("Invalid flow grid dimensions".into());
        }
        let cells = field.grid.dims.iter().map(|&n| n as u64).product::<u64>();
        if !field.buffer.usage().contains(wgpu::BufferUsages::STORAGE)
            || field.buffer.size()
                > self.context.device.limits().max_storage_buffer_binding_size as u64
            || layout.stride == 0
            || layout.offset >= layout.stride
            || layout.subtract.is_some_and(|i| i >= layout.stride)
            || cells
                .checked_mul(layout.stride as u64)
                .and_then(|n| n.checked_mul(16))
                .is_none_or(|n| n > field.buffer.size())
            || field.grid.dims.contains(&0)
            || !field.grid.cell_m.is_finite()
            || field.grid.cell_m <= 0.
            || field.grid.origin_m.iter().any(|v| !v.is_finite())
            || !span_mm.is_finite()
            || span_mm <= 0.
            || !plane.position.is_finite()
            || !plane.orientation.is_finite()
            || (plane.orientation.length_squared() - 1.).abs() > 1e-3
            || !time_s.is_finite()
            || !(1..=64).contains(&settings.density)
            || !settings.speed.is_finite()
            || settings.speed < 0.
            || !settings.tail.is_finite()
            || !(0.01..=1.).contains(&settings.tail)
        {
            self.count = 0;
            return Err("Invalid flow field layout, plane or animation settings".into());
        }
        self.count = settings.density * settings.density * STEPS * 2;
        let bytes = self.count as u64 * 32;
        if self.vertices.size() < bytes {
            self.vertices = Self::buffer(&self.context, bytes);
        }
        let minimum = field.grid.origin_m.map(|v| v * 1000.);
        let step = (span_mm * 0.6 / STEPS as f32).min(field.grid.cell_m * 1000. * 0.75);
        let params = Params {
            origin: plane.position.extend(span_mm).to_array(),
            u: plane.u().extend(0.).to_array(),
            v: plane.v().extend(0.).to_array(),
            grid: [
                minimum[0],
                minimum[1],
                minimum[2],
                field.grid.cell_m * 1000.,
            ],
            dims: [
                field.grid.dims[0],
                field.grid.dims[1],
                field.grid.dims[2],
                layout.stride,
            ],
            source: [
                layout.offset,
                layout.subtract.unwrap_or(u32::MAX),
                settings.density,
                u32::from(layout.interpolate),
            ],
            motion: [
                (time_s * settings.speed as f64).rem_euclid(1.) as f32,
                step,
                if settings.dots { 0.035 } else { settings.tail },
                0.,
            ],
        };
        self.context
            .queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&params));
        let bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: field.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.vertices.as_entire_binding(),
                    },
                ],
            });
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((settings.density * settings.density).div_ceil(64), 1, 1);
        }
        self.context.queue.submit(Some(encoder.finish()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::util::DeviceExt;
    #[test]
    fn gpu_streamlines_follow_curvature_animate_and_stop_at_zero_field() {
        let _lock = crate::gpu_test_lock();
        let context = GpuContext::headless().unwrap();
        let grid = GridSi {
            origin_m: [-0.008; 3],
            cell_m: 0.001,
            dims: [16; 3],
        };
        let vectors: Vec<[f32; 4]> = (0..16)
            .flat_map(|z| {
                (0..16).flat_map(move |y| {
                    (0..16).map(move |_x| [0., -(z as f32 - 7.5), y as f32 - 7.5, 0.])
                })
            })
            .collect();
        let field = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&vectors),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
            });
        let layout = VectorLayout {
            stride: 1,
            offset: 0,
            subtract: None,
            interpolate: true,
        };
        let plane = SectionPlane::new(glam::Vec3::ZERO);
        let settings = TrailSettings {
            density: 8,
            ..Default::default()
        };
        let mut trails = FlowTrails::new(context.clone());
        let read = |trails: &FlowTrails| {
            crate::em::read_bytes(&context, trails.draw().buffer, 0, trails.count as u64 * 32)
                .unwrap()
        };
        trails
            .update(
                VectorField {
                    buffer: &field,
                    grid: &grid,
                    layout,
                },
                plane,
                6.,
                0.,
                settings,
            )
            .unwrap();
        let a = read(&trails);
        let vertices = bytemuck::cast_slice::<u8, [f32; 8]>(&a);
        let mut visible = 0;
        for pair in vertices.chunks_exact(2) {
            let p = glam::Vec3::from_slice(&pair[0]);
            let q = glam::Vec3::from_slice(&pair[1]);
            assert!(p.is_finite() && q.is_finite());
            assert!(p.x.abs() < 1e-6 && q.x.abs() < 1e-6);
            if pair[0][7] > 0. || pair[1][7] > 0. {
                visible += 1;
                let tangent = glam::Vec3::new(0., -p.z, p.y);
                assert!(
                    (q - p).dot(tangent) > 0.,
                    "motion must follow the vector field"
                );
                assert!(
                    (q.length() - p.length()).abs() < 0.002,
                    "RK2 circular trace must retain radius"
                );
            }
        }
        assert!(visible > 100);
        trails
            .update(
                VectorField {
                    buffer: &field,
                    grid: &grid,
                    layout,
                },
                plane,
                6.,
                0.7,
                settings,
            )
            .unwrap();
        let b = read(&trails);
        assert_ne!(a, b, "animation must move the bright part of each trace");
        let unchanged = crate::em::read_bytes(&context, &field, 0, field.size()).unwrap();
        assert_eq!(unchanged, bytemuck::cast_slice::<_, u8>(&vectors));
        context.queue.write_buffer(
            &field,
            0,
            bytemuck::cast_slice(&vec![[0f32; 4]; vectors.len()]),
        );
        trails
            .update(
                VectorField {
                    buffer: &field,
                    grid: &grid,
                    layout,
                },
                plane,
                6.,
                0.7,
                settings,
            )
            .unwrap();
        let zero = read(&trails);
        assert!(
            bytemuck::cast_slice::<u8, [f32; 8]>(&zero)
                .iter()
                .all(|v| v[7] == 0.)
        );
        assert!(
            trails
                .update(
                    VectorField {
                        buffer: &field,
                        grid: &grid,
                        layout: VectorLayout {
                            offset: 3,
                            ..layout
                        }
                    },
                    plane,
                    6.,
                    0.,
                    settings
                )
                .is_err()
        );
        assert_eq!(trails.draw().vertices, 0);
    }
}
