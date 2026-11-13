use crate::model::Model;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Triangle {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    origin: [f32; 4],
    u: [f32; 4],
    v: [f32; 4],
    shape: [u32; 4],
}
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    pub name: String,
}
impl Gpu {
    pub fn new() -> Result<Self, String> {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .ok_or("No compatible compute adapter")?;
            let name = adapter.get_info().name;
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("MHD section compute"),
                        ..Default::default()
                    },
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Solid cross-section"),
                source: wgpu::ShaderSource::Wgsl(include_str!("section.wgsl").into()),
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("GPU point-in-solid"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
            Ok(Self {
                device,
                queue,
                pipeline,
                name,
            })
        })
    }
    /// CPU prepares geometry and plane; classification runs only on the GPU.
    /// Points and triangles are normalized by the model's largest extent.
    pub fn section(
        &self,
        model: &Model,
        origin: Vec3,
        u: Vec3,
        v: Vec3,
        n: u32,
    ) -> Result<Vec<u32>, String> {
        if !(8..=512).contains(&n) {
            return Err("Section resolution must be 8..512".into());
        }
        if model.bodies.len() > 31 {
            return Err("Prototype supports at most 31 bodies".into());
        }
        let (lo, hi) = model.bounds();
        let center = (lo + hi) * 0.5;
        let size = (hi - lo).max_element().max(1e-6);
        let mut triangles = Vec::new();
        for (i, b) in model.bodies.iter().enumerate() {
            if !b.visible {
                continue;
            }
            if !b.closed() {
                return Err(format!(
                    "{} is not closed; solid classification disabled",
                    b.name
                ));
            }
            for t in &b.triangles {
                let p = t.map(|p| (p - center) / size);
                triangles.push(Triangle {
                    a: [p[0].x, p[0].y, p[0].z, i as f32],
                    b: p[1].extend(0.).to_array(),
                    c: p[2].extend(0.).to_array(),
                });
            }
        }
        if triangles.is_empty() {
            return Err("No visible closed triangle bodies".into());
        }
        let tri_bytes = bytemuck::cast_slice(&triangles);
        if tri_bytes.len() as u64 > self.device.limits().max_storage_buffer_binding_size as u64 {
            return Err("Triangle buffer exceeds device limits".into());
        }
        let geometry = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Triangles"),
                contents: tri_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let p = Params {
            origin: ((origin - center) / size).extend(0.).to_array(),
            u: (u / size).extend(0.).to_array(),
            v: (v / size).extend(0.).to_array(),
            shape: [n, triangles.len() as u32, 0, 0],
        };
        let params = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Plane"),
                contents: bytemuck::bytes_of(&p),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bytes = n as u64 * n as u64 * 4;
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Section labels"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Section readback"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: geometry.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Section classification"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(n.div_ceil(8), n.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        self.queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |v| {
            let _ = tx.send(v);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let data = readback.slice(..).get_mapped_range();
        let result = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        readback.unmap();
        Ok(result)
    }
}
