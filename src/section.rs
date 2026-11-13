//! One antialiased section image is used by the flat panel and the 3D cut face.
use crate::{
    app::COLORS,
    caps::Cap,
    domain::{Cell, Volume},
    model::Model,
    plane::SectionPlane,
};
use bytemuck::{Pod, Zeroable};
use eframe::egui;
use glam::{Vec2, Vec3};
#[cfg(test)]
use std::collections::BTreeSet;
use wgpu::util::DeviceExt;
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Style {
    HatchedCad,
    Outlines,
    Voxels,
}
impl Style {
    pub fn label(self) -> &'static str {
        match self {
            Self::HatchedCad => "Hatched CAD",
            Self::Outlines => "CAD outlines",
            Self::Voxels => "Voxel cells",
        }
    }
}
#[derive(Clone, Copy)]
pub struct Options {
    pub style: Style,
    pub edges: bool,
    pub material_colors: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    p: [f32; 2],
    color: [f32; 4],
    uv: [f32; 2],
    mode: f32,
}
fn linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn color(id: u32) -> [f32; 4] {
    let c = if id == 255 {
        [255, 50, 90]
    } else {
        COLORS[(id as usize).saturating_sub(1) % COLORS.len()]
    };
    [
        linear(c[0] as f32 / 255.),
        linear(c[1] as f32 / 255.),
        linear(c[2] as f32 / 255.),
        1.,
    ]
}
/// Polygon where an infinite plane cuts one cube; every edge lies on a voxel face.
pub fn footprint(min: Vec3, h: f32, plane: SectionPlane) -> Vec<Vec2> {
    let corners: [Vec3; 8] = std::array::from_fn(|i| {
        min + Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32) * h
    });
    let n = plane.normal();
    let eps = h * 1e-5;
    let mut points = Vec::new();
    let mut push = |p: Vec3| {
        let r = p - plane.position;
        let uv = Vec2::new(r.dot(plane.u()), r.dot(plane.v()));
        if points.iter().all(|q: &Vec2| q.distance(uv) > eps) {
            points.push(uv);
        }
    };
    for i in 0..8 {
        for axis in [1, 2, 4] {
            let j = i ^ axis;
            if i >= j {
                continue;
            }
            let a = corners[i];
            let b = corners[j];
            let da = (a - plane.position).dot(n);
            let db = (b - plane.position).dot(n);
            if da.abs() <= eps {
                push(a);
            }
            if (da < -eps && db > eps) || (da > eps && db < -eps) {
                push(a + (b - a) * (da / (da - db)));
            }
            if db.abs() <= eps {
                push(b);
            }
        }
    }
    if points.len() < 3 {
        return Vec::new();
    }
    let centre = points.iter().copied().sum::<Vec2>() / points.len() as f32;
    points.sort_by(|a, b| {
        (a.y - centre.y)
            .atan2(a.x - centre.x)
            .total_cmp(&(b.y - centre.y).atan2(b.x - centre.x))
    });
    let area = (0..points.len())
        .map(|i| points[i].perp_dot(points[(i + 1) % points.len()]))
        .sum::<f32>()
        .abs()
        * 0.5;
    if area < h * h * 1e-7 {
        points.clear();
    }
    points
}
fn line(out: &mut Vec<Vertex>, a: Vec2, b: Vec2, width: f32, c: [f32; 4], span: f32) {
    let direction = b - a;
    if direction.length_squared() < 1e-16 {
        return;
    }
    let offset = Vec2::new(-direction.y, direction.x).normalize() * width * 0.5;
    let p = [a + offset, b + offset, b - offset, a - offset];
    for i in [0, 1, 2, 0, 2, 3] {
        out.push(Vertex {
            p: (p[i] / span).to_array(),
            color: c,
            uv: [0.; 2],
            mode: 0.,
        });
    }
}
fn geometry(
    model: &Model,
    caps: &[Cap],
    plane: SectionPlane,
    volume: Option<&Volume>,
    cells: &[Cell],
    options: Options,
    span: f32,
    res: u32,
) -> Vec<Vertex> {
    let mut out = Vec::new();
    let uv = |p: Vec3| {
        let r = p - plane.position;
        Vec2::new(r.dot(plane.u()), r.dot(plane.v()))
    };
    if options.style == Style::Voxels {
        if let Some(v) = volume {
            let h = v.settings.cell_mm;
            let min = Vec3::from(v.settings.min_mm);
            for cell in cells {
                let p = footprint(
                    min + Vec3::new(
                        cell.index[0] as f32,
                        cell.index[1] as f32,
                        cell.index[2] as f32,
                    ) * h,
                    h,
                    plane,
                );
                if p.len() < 3 {
                    continue;
                }
                let c = color(if options.material_colors {
                    cell.material
                } else {
                    cell.body
                });
                for i in 1..p.len() - 1 {
                    for q in [p[0], p[i], p[i + 1]] {
                        out.push(Vertex {
                            p: (q / span).to_array(),
                            color: c,
                            uv: [0.; 2],
                            mode: 0.,
                        });
                    }
                }
            }
        }
    } else if options.style == Style::HatchedCad {
        for (i, cap) in caps.iter().enumerate() {
            let c = COLORS[i % COLORS.len()];
            let c = [
                linear(c[0] as f32 / 255. * 0.45 + 0.55),
                linear(c[1] as f32 / 255. * 0.45 + 0.55),
                linear(c[2] as f32 / 255. * 0.45 + 0.55),
                1.,
            ];
            for t in &cap.triangles {
                for &p in t {
                    let q = uv(p);
                    out.push(Vertex {
                        p: (q / span).to_array(),
                        color: c,
                        uv: [if i % 2 == 0 { q.x } else { -q.x }, q.y],
                        mode: 1.,
                    });
                }
            }
        }
    }
    // CAD contours provide the same geometry reference in both views.
    for (i, cap) in caps.iter().enumerate() {
        if !model.bodies.get(i).is_some_and(|b| b.visible) {
            continue;
        }
        for [a, b] in &cap.contours {
            line(
                &mut out,
                uv(*a),
                uv(*b),
                2. * span / res as f32 * 0.9,
                [0.62, 0.77, 0.84, 0.95],
                span,
            );
        }
    }
    out
}
pub struct SectionRenderer {
    state: egui_wgpu::RenderState,
    panel_pipeline: wgpu::RenderPipeline,
    panel_uniform: wgpu::Buffer,
    panel_target: Option<wgpu::Texture>,
    panel_size: u32,
    requested_panel_size: u32,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    mip: wgpu::RenderPipeline,
    mip_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    target: Option<(
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::TextureView,
        wgpu::Texture,
    )>,
    size: u32,
    id: Option<egui::TextureId>,
}
impl SectionRenderer {
    pub fn new(state: egui_wgpu::RenderState) -> Self {
        let device = &state.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shared CAD/voxel section"),
            source: wgpu::ShaderSource::Wgsl(include_str!("section-render.wgsl").into()),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("Antialiased section faces"),layout:None,vertex:wgpu::VertexState{module:&shader,entry_point:Some("vertex"),compilation_options:Default::default(),buffers:&[wgpu::VertexBufferLayout{array_stride:std::mem::size_of::<Vertex>()as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x4,2=>Float32x2,3=>Float32]}]},fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fragment"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format:wgpu::TextureFormat::Rgba8UnormSrgb,blend:Some(wgpu::BlendState::ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},depth_stencil:None,multisample:wgpu::MultisampleState{count:4,..Default::default()},multiview:None,cache:None});
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Section mipmaps"),
            source: wgpu::ShaderSource::Wgsl(include_str!("section-mip.wgsl").into()),
        });
        let mip = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Minification antialiasing"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let mip_layout = mip.get_bind_group_layout(0);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native-pixel section panel"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}",
                    include_str!("voxel-grid.wgsl"),
                    include_str!("section-panel.wgsl")
                )
                .into(),
            ),
        });
        let panel_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Panel grid at framebuffer resolution"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let panel_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Panel world/grid transform"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            panel_pipeline,
            panel_uniform,
            panel_target: None,
            panel_size: 0,
            requested_panel_size: 0,
            state,
            pipeline,
            uniform,
            bind,
            mip,
            mip_layout,
            sampler,
            target: None,
            size: 0,
            id: None,
        }
    }
    pub fn set_panel_resolution(&mut self, n: u32) {
        self.requested_panel_size = n.clamp(32, 4096);
    }
    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.target.as_ref().map(|t| &t.1)
    }
    pub fn draw(
        &mut self,
        model: &Model,
        caps: &[Cap],
        plane: SectionPlane,
        volume: Option<&Volume>,
        cells: &[Cell],
        options: Options,
        span: f32,
        n: u32,
    ) -> egui::TextureId {
        let device = &self.state.device;
        let queue = &self.state.queue;
        if self.size != n {
            let count = n.ilog2() + 1;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Shared section image"),
                size: wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 1,
                },
                mip_level_count: count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let all = texture.create_view(&Default::default());
            let base = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let msaa = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("4x section coverage"),
                size: wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 4,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.target = Some((texture, all, base, msaa));
            self.size = n;
        }
        let panel_n = if self.requested_panel_size == 0 {
            n
        } else {
            self.requested_panel_size
        };
        if self.panel_size != panel_n {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Panel native-pixel image"),
                size: wgpu::Extent3d {
                    width: panel_n,
                    height: panel_n,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let mut renderer = self.state.renderer.write();
            if let Some(id) = self.id {
                renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    wgpu::FilterMode::Linear,
                    id,
                );
            } else {
                self.id =
                    Some(renderer.register_native_texture(device, &view, wgpu::FilterMode::Linear));
            }
            self.panel_target = Some(texture);
            self.panel_size = panel_n;
        }
        let (lo, hi) = model.bounds();
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&[(hi - lo).max_element().max(1.) * 0.023, 0., 0., 0.]),
        );
        let vertices = geometry(model, caps, plane, volume, cells, options, span, n);
        let buffer = (!vertices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Exact section polygons"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let (texture, _, base, msaa) = self.target.as_ref().unwrap();
        let msaa_view = msaa.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shared section"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(base),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if let Some(buffer) = &buffer {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        }
        for level in 1..texture.mip_level_count() {
            let source = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let target = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.mip_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Section mip level"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.mip);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        let grid = volume
            .map(|v| (Vec3::from(v.settings.min_mm), v.settings.cell_mm))
            .unwrap_or((Vec3::ZERO, 1.));
        let panel_data = [
            plane.position.extend(0.).to_array(),
            (plane.u() * span).extend(0.).to_array(),
            (plane.v() * span).extend(0.).to_array(),
            grid.0.extend(grid.1).to_array(),
            [
                if options.style == Style::Voxels && options.edges {
                    1.
                } else {
                    0.
                },
                0.,
                0.,
                0.,
            ],
        ];
        queue.write_buffer(&self.panel_uniform, 0, bytemuck::cast_slice(&panel_data));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.panel_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.panel_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.target.as_ref().unwrap().1),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let target = self
            .panel_target
            .as_ref()
            .unwrap()
            .create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Native-resolution analytic panel grid"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.panel_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        self.id.unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::GpuDomain,
        project::{DomainSettings, default_materials},
    };
    use glam::{EulerRot, Quat};
    pub(super) fn state() -> egui_wgpu::RenderState {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter
                .request_device(&Default::default(), None)
                .await
                .unwrap();
            let renderer =
                egui_wgpu::Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, None, 1, false);
            egui_wgpu::RenderState {
                adapter,
                available_adapters: vec![],
                device,
                queue,
                target_format: wgpu::TextureFormat::Rgba8Unorm,
                renderer: std::sync::Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
            }
        })
    }
    pub(super) fn read(renderer: &SectionRenderer) -> Vec<u8> {
        read_texture(&renderer.state, renderer.panel_target.as_ref().unwrap())
    }
    pub(super) fn read_texture(state: &egui_wgpu::RenderState, texture: &wgpu::Texture) -> Vec<u8> {
        let device = &state.device;
        let n = texture.size().width;
        let height = texture.size().height;
        let stride = (n * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: stride as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: Default::default(),
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: n,
                height,
                depth_or_array_layers: 1,
            },
        );
        state.queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range();
        mapped
            .chunks(stride as usize)
            .flat_map(|row| row[..n as usize * 4].iter().copied())
            .collect()
    }
    #[test]
    fn cube_footprints_use_faces_not_cell_centres() {
        let plane = SectionPlane::new(Vec3::ZERO);
        let p = footprint(Vec3::splat(-0.5), 1., plane);
        assert_eq!(p.len(), 4);
        for q in &p {
            assert!((q.x.abs() - 0.5).abs() < 1e-5);
            assert!((q.y.abs() - 0.5).abs() < 1e-5);
        }
        let mut tilted = plane;
        tilted.orientation = Quat::from_rotation_arc(Vec3::X, Vec3::ONE.normalize());
        let p = footprint(Vec3::splat(-0.5), 1., tilted);
        assert_eq!(p.len(), 6);
        let area = (0..p.len())
            .map(|i| p[i].perp_dot(p[(i + 1) % p.len()]))
            .sum::<f32>()
            .abs()
            * 0.5;
        assert!((area - 3. * 3f32.sqrt() / 4.).abs() < 1e-5);
        for q in p {
            let w = tilted.position + tilted.u() * q.x + tilted.v() * q.y;
            assert!(w.dot(tilted.normal()).abs() < 1e-5);
            assert!((w.abs().max_element() - 0.5).abs() < 1e-5);
        }
    }
    #[test]
    fn gpu_slab_enumeration_matches_complete_oblique_intersections() {
        let _gpu_lock = crate::gpu_test_lock();
        let gpu = GpuDomain::new().unwrap();
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let settings = DomainSettings {
            min_mm: [-4., -4., -5.],
            max_mm: [4., 4., 5.],
            cell_mm: 0.5,
        };
        let v = gpu.build(&model, &settings, &default_materials()).unwrap();
        let masks = gpu.read_masks(&v);
        let mut plane = SectionPlane::new(Vec3::new(0.17, 0.23, 0.19));
        plane.orientation = Quat::from_euler(EulerRot::XYZ, 0.31, 0.47, 0.12);
        let cells = gpu.intersected_cells(&v, plane).unwrap();
        let found: BTreeSet<_> = cells
            .iter()
            .filter(|c| {
                !footprint(
                    Vec3::from(settings.min_mm)
                        + Vec3::new(c.index[0] as f32, c.index[1] as f32, c.index[2] as f32) * 0.5,
                    0.5,
                    plane,
                )
                .is_empty()
            })
            .map(|c| c.index)
            .collect();
        let mut expected = BTreeSet::new();
        for z in 0..v.dims[2] {
            for y in 0..v.dims[1] {
                for x in 0..v.dims[0] {
                    let i = ((z * v.dims[1] + y) * v.dims[0] + x) as usize;
                    if masks[i] != 0
                        && !footprint(
                            Vec3::from(settings.min_mm)
                                + Vec3::new(x as f32, y as f32, z as f32) * 0.5,
                            0.5,
                            plane,
                        )
                        .is_empty()
                    {
                        expected.insert([x, y, z]);
                    }
                }
            }
        }
        assert_eq!(found, expected);
        for axis in 0..3 {
            let mut p = SectionPlane::new(Vec3::ZERO);
            p.orientation = Quat::from_rotation_arc(Vec3::X, [Vec3::X, Vec3::Y, Vec3::Z][axis]);
            let cells = gpu.intersected_cells(&v, p).unwrap();
            let selected = ((0. - settings.min_mm[axis]) / 0.5).floor() as u32;
            assert!(cells.iter().all(|c| c.index[axis] == selected));
        }
    }
    #[test]
    fn shared_gpu_image_switches_hatching_outlines_and_voxels_without_filling_holes() {
        let _gpu_lock = crate::gpu_test_lock();
        let mut renderer = SectionRenderer::new(state());
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let plane = SectionPlane {
            position: Vec3::ZERO,
            orientation: Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        };
        let caps = vec![crate::caps::build(&model.bodies[0], plane, 1e-5)];
        let opts = Options {
            style: Style::HatchedCad,
            edges: true,
            material_colors: false,
        };
        let id = renderer.draw(&model, &caps, plane, None, &[], opts, 4., 256);
        let image = read(&renderer);
        let hatched = image.clone();
        let at = |image: &[u8], x: usize, y: usize| {
            image[(y * 256 + x) * 4..(y * 256 + x + 1) * 4].to_vec()
        };
        assert_eq!(at(&image, 128, 128)[3], 0);
        assert!((185..215).any(|x| at(&image, x, 128)[1] > 180));
        assert!((185..215).any(|x| at(&image, x, 128)[0] < 130));
        let id2 = renderer.draw(
            &model,
            &caps,
            plane,
            None,
            &[],
            Options {
                style: Style::Outlines,
                ..opts
            },
            4.,
            256,
        );
        assert_eq!(id, id2);
        let outlines = read(&renderer);
        assert!((185..215).all(|x| at(&outlines, x, 128)[3] == 0));
        let gpu = GpuDomain::new().unwrap();
        let settings = DomainSettings {
            min_mm: [-4., -4., -5.],
            max_mm: [4., 4., 5.],
            cell_mm: 0.5,
        };
        let v = gpu.build(&model, &settings, &default_materials()).unwrap();
        let cells = gpu.intersected_cells(&v, plane).unwrap();
        renderer.draw(
            &model,
            &caps,
            plane,
            Some(&v),
            &cells,
            Options {
                style: Style::Voxels,
                ..opts
            },
            4.,
            256,
        );
        let image = read(&renderer);
        assert_eq!(at(&image, 128, 128)[3], 0);
        assert!((185..215).all(|x| at(&image, x, 128)[3] > 240));
        assert!(
            (185..215).all(|x| at(&image, x, 128)[0] > 50),
            "No black grid bands in cell interiors"
        );
        std::fs::create_dir_all("artifacts").unwrap();
        for (name, data) in [("shared-hatched", hatched), ("shared-outlines", outlines)] {
            let mut ppm = b"P6\n256 256\n255\n".to_vec();
            for p in data.chunks_exact(4) {
                ppm.extend(&p[..3]);
            }
            std::fs::write(format!("artifacts/{name}.ppm"), ppm).unwrap();
        }
    }
    #[test]
    fn three_dimensional_cut_face_matches_the_flat_hatched_image() {
        let _gpu_lock = crate::gpu_test_lock();
        let state = state();
        let mut flat = SectionRenderer::new(state.clone());
        let mut viewport = crate::render::Viewport::new(state.clone());
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let plane = SectionPlane {
            position: Vec3::ZERO,
            orientation: Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        };
        let caps = vec![crate::caps::build(&model.bodies[0], plane, 1e-5)];
        flat.draw(
            &model,
            &caps,
            plane,
            None,
            &[],
            Options {
                style: Style::HatchedCad,
                edges: true,
                material_colors: false,
            },
            4.,
            256,
        );
        viewport.set_shared_section(flat.view().unwrap());
        let camera = crate::gizmo::Camera {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(256.)),
            center: plane.position,
            rotation: Quat::from_mat3(
                &glam::Mat3::from_cols(plane.u(), -plane.normal(), plane.v()).transpose(),
            ),
            scale: 32.,
            size: 8.,
        };
        viewport.draw(
            camera,
            plane,
            true,
            true,
            &model,
            &caps,
            1.,
            Some(crate::render::DomainView {
                span: 4.,
                bounds: None,
                grid: None,
            }),
        );
        let a = read(&flat);
        let b = read_texture(&state, viewport.output_texture());
        for y in 110..146 {
            for x in 185..215 {
                let i = (y * 256 + x) * 4;
                for c in 0..3 {
                    assert!(
                        (a[i + c] as i16 - b[i + c] as i16).abs() <= 2,
                        "Shared hatch mismatch at ({x},{y}) channel {c}: {} vs {}",
                        a[i + c],
                        b[i + c]
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod grid_resolution_tests {
    use super::*;
    #[test]
    fn grid_is_native_resolution_and_independent_of_fill_texture() {
        let _gpu_lock = crate::gpu_test_lock();
        let state = super::tests::state();
        let mut flat = SectionRenderer::new(state.clone());
        flat.set_panel_resolution(512);
        let mut viewport = crate::render::Viewport::new(state.clone());
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let plane = SectionPlane {
            position: Vec3::ZERO,
            orientation: glam::Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        };
        let caps = vec![crate::caps::build(&model.bodies[0], plane, 1e-5)];
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let settings = crate::project::DomainSettings {
            min_mm: [-4., -4., -5.],
            max_mm: [4., 4., 5.],
            cell_mm: 0.5,
        };
        let volume = gpu
            .build(&model, &settings, &crate::project::default_materials())
            .unwrap();
        let cells = gpu.intersected_cells(&volume, plane).unwrap();
        let options = Options {
            style: Style::Voxels,
            edges: true,
            material_colors: false,
        };
        flat.draw(&model, &caps, plane, Some(&volume), &cells, options, 4., 64);
        assert_eq!(flat.panel_size, 512);
        assert_eq!(flat.size, 64);
        let low = super::tests::read(&flat);
        viewport.set_shared_section(flat.view().unwrap());
        let camera = crate::gizmo::Camera {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(512.)),
            center: plane.position,
            rotation: glam::Quat::from_mat3(
                &glam::Mat3::from_cols(plane.u(), -plane.normal(), plane.v()).transpose(),
            ),
            scale: 64.,
            size: 8.,
        };
        viewport.draw(
            camera,
            plane,
            true,
            false,
            &model,
            &caps,
            1.,
            Some(crate::render::DomainView {
                span: 4.,
                bounds: None,
                grid: Some((Vec3::from(settings.min_mm), settings.cell_mm)),
            }),
        );
        let native = super::tests::read_texture(&state, viewport.output_texture());
        flat.draw(
            &model,
            &caps,
            plane,
            Some(&volume),
            &cells,
            options,
            4.,
            512,
        );
        let high = super::tests::read(&flat);
        let y = 240;
        let brightness = |image: &[u8], x: usize| {
            image[(y * 512 + x) * 4..(y * 512 + x) * 4 + 3]
                .iter()
                .map(|&v| v as i32)
                .sum::<i32>()
        };
        assert!(
            brightness(&low, 384) > brightness(&low, 400) + 10,
            "Grid line at the true u=2 mm cell face must remain visible with a 64px fill"
        );
        for x in 377..391 {
            let i = (y * 512 + x) * 4;
            for c in 0..3 {
                assert!(
                    (low[i + c] as i16 - high[i + c] as i16).abs() <= 2,
                    "Fill resolution changed native grid at x={x}"
                );
                assert!(
                    (low[i + c] as i16 - native[i + c] as i16).abs() <= 2,
                    "3D and panel grid differ at x={x}"
                );
            }
        }
        let zoom_camera = crate::gizmo::Camera {
            center: plane.position + plane.u() * 2. + plane.v() * 0.25,
            scale: 128.,
            ..camera
        };
        viewport.draw(
            zoom_camera,
            plane,
            true,
            false,
            &model,
            &caps,
            1.,
            Some(crate::render::DomainView {
                span: 4.,
                bounds: None,
                grid: Some((Vec3::from(settings.min_mm), settings.cell_mm)),
            }),
        );
        let zoomed = super::tests::read_texture(&state, viewport.output_texture());
        for offset in -4i32..=4 {
            let source = (240 * 512 + (384 + offset) as usize) * 4;
            let zoom = (256 * 512 + (256 + offset) as usize) * 4;
            for c in 0..3 {
                assert!(
                    (low[source + c] as i16 - zoomed[zoom + c] as i16).abs() <= 2,
                    "Zoom changed grid pixel width at offset {offset}"
                );
            }
        }
        // The cached fill contains no lattice; only each destination shader adds it.
        let cached = super::tests::read_texture(&state, &flat.target.as_ref().unwrap().0);
        assert!((brightness(&cached, 384) - brightness(&cached, 400)).abs() <= 2);
    }
}
