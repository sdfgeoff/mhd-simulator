use crate::{app::COLORS, caps::Cap, gizmo::Camera, model::Model, plane::SectionPlane};
use bytemuck::{Pod, Zeroable};
use eframe::egui;
use glam::Vec3;
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
    uv: [f32; 2],
    mode: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    camera: [[f32; 4]; 4],
    origin: [f32; 4],
    normal: [f32; 4],
    settings: [f32; 4],
    grid_minimum: [f32; 4],
    dimensions: [u32; 4],
    palette: [[f32; 4]; 8],
    field: [f32; 4],
}
#[derive(Clone, Copy)]
pub struct FieldView<'a> {
    pub buffer: &'a wgpu::Buffer,
    pub kind: u32,
    pub component: u32,
    pub range: f32,
    pub vectors: &'a [[Vec3; 2]],
}
pub struct DomainView<'a> {
    pub field: Option<FieldView<'a>>,
    pub source_lines: &'a [[Vec3; 2]],
    pub span: f32,
    pub bounds: Option<(Vec3, Vec3)>,
    pub grid: Option<(Vec3, f32)>,
    pub volume: Option<&'a crate::domain::Volume>,
    pub material_colors: bool,
}
struct CachedBody {
    key: (u64, u32),
    buffer: wgpu::Buffer,
    count: u32,
}
pub struct Viewport {
    meshes: Vec<CachedBody>,
    mesh_uploads: u64,
    volume_layout: wgpu::BindGroupLayout,
    empty: wgpu::Buffer,
    state: egui_wgpu::RenderState,
    triangle: wgpu::RenderPipeline,
    line: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    target: Option<(
        wgpu::Texture,
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::TextureView,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
    size: [u32; 2],
    texture: Option<egui::TextureId>,
}
impl Viewport {
    pub fn new(state: egui_wgpu::RenderState) -> Self {
        let device = &state.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("CAD depth + hatching"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}",
                    include_str!("voxel-grid.wgsl"),
                    include_str!("viewport.wgsl")
                )
                .into(),
            ),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera and plane"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let empty = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Empty domain"),
            contents: &[0u8; 64],
            usage: wgpu::BufferUsages::STORAGE,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: empty.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: empty.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: empty.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let make = |lines: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some(if lines{"Section outlines"}else{"Depth-tested CAD"}),layout:Some(&pipeline_layout),vertex:wgpu::VertexState{module:&shader,entry_point:Some("vertex"),compilation_options:Default::default(),buffers:&[wgpu::VertexBufferLayout{array_stride:std::mem::size_of::<Vertex>()as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x4,2=>Float32x2,3=>Float32]}]},fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fragment"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format:wgpu::TextureFormat::Rgba8UnormSrgb,blend:Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),primitive:wgpu::PrimitiveState{topology:if lines{wgpu::PrimitiveTopology::LineList}else{wgpu::PrimitiveTopology::TriangleList},cull_mode:if lines{None}else{Some(wgpu::Face::Back)},..Default::default()},depth_stencil:Some(wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:!lines,depth_compare:wgpu::CompareFunction::LessEqual,stencil:Default::default(),bias:wgpu::DepthBiasState{constant:if lines{-2}else{0},..Default::default()}}),multisample:wgpu::MultisampleState{count:4,..Default::default()},multiview:None,cache:None})
        };
        let triangle = make(false);
        let line = make(true);
        Self {
            meshes: Vec::new(),
            mesh_uploads: 0,
            volume_layout: layout,
            empty,
            state,
            triangle,
            line,
            uniform,
            bind,
            target: None,
            size: [0, 0],
            texture: None,
        }
    }
    fn cache_meshes(&mut self, model: &Model) {
        if self.meshes.len() == model.bodies.len()
            && self
                .meshes
                .iter()
                .zip(&model.bodies)
                .all(|(m, b)| m.key == (b.geometry_id(), b.winding.to_bits()))
        {
            return;
        }
        self.meshes.clear();
        for (i, body) in model.bodies.iter().enumerate() {
            let c = COLORS[i % COLORS.len()].map(|v| v as f32 / 255.);
            let mut vertices = Vec::with_capacity(body.triangles().len() * 3);
            for triangle in body.triangles() {
                let mut t = *triangle;
                if body.winding < 0. {
                    t.swap(1, 2);
                }
                let shade = 0.4
                    + 0.6
                        * (t[1] - t[0])
                            .cross(t[2] - t[0])
                            .normalize()
                            .dot(Vec3::new(0.3, -0.6, 0.74))
                            .abs();
                for p in t {
                    vertices.push(Vertex {
                        position: p.to_array(),
                        color: [c[0] * shade, c[1] * shade, c[2] * shade, 1.],
                        uv: [0.; 2],
                        mode: 0.,
                    });
                }
            }
            let buffer = self
                .state
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Cached immutable CAD mesh"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            self.meshes.push(CachedBody {
                key: (body.geometry_id(), body.winding.to_bits()),
                buffer,
                count: vertices.len() as u32,
            });
            self.mesh_uploads += 1;
        }
    }
    #[cfg(test)]
    pub(crate) fn output_texture(&self) -> &wgpu::Texture {
        &self.target.as_ref().unwrap().0
    }
    pub fn draw(
        &mut self,
        camera: Camera,
        plane: SectionPlane,
        cut: bool,
        hatched: bool,
        model: &Model,
        caps: &[Cap],
        pixels_per_point: f32,
        domain: Option<DomainView>,
    ) -> egui::TextureId {
        self.draw_view(
            camera,
            plane,
            cut,
            hatched,
            model,
            caps,
            pixels_per_point,
            domain,
            false,
        )
    }
    pub fn draw_section(
        &mut self,
        model: &Model,
        caps: &[Cap],
        plane: SectionPlane,
        volume: Option<&crate::domain::Volume>,
        options: crate::section::Options,
        span: f32,
        n: u32,
        field: Option<FieldView>,
    ) -> egui::TextureId {
        let basis = glam::Mat3::from_cols(plane.u(), -plane.normal(), plane.v());
        let camera = Camera {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(n as f32)),
            center: plane.position,
            rotation: glam::Quat::from_mat3(&basis).inverse(),
            scale: n as f32 / (2. * span),
            size: {
                let (lo, hi) = model.bounds();
                (hi - lo).max_element().max(1.)
            },
        };
        let domain = matches!(
            options.style,
            crate::section::Style::Voxels | crate::section::Style::Field
        )
        .then_some(DomainView {
            field,
            source_lines: &[],
            span,
            bounds: None,
            grid: if options.edges {
                volume.map(|v| (Vec3::from(v.settings.min_mm), v.settings.cell_mm))
            } else {
                None
            },
            volume,
            material_colors: options.material_colors,
        });
        self.draw_view(
            camera,
            plane,
            true,
            options.style == crate::section::Style::HatchedCad,
            model,
            caps,
            1.,
            domain,
            true,
        )
    }
    fn draw_view(
        &mut self,
        camera: Camera,
        plane: SectionPlane,
        cut: bool,
        hatched: bool,
        model: &Model,
        caps: &[Cap],
        pixels_per_point: f32,
        domain: Option<DomainView>,
        panel: bool,
    ) -> egui::TextureId {
        if !panel {
            self.cache_meshes(model);
        }
        let device = &self.state.device;
        let queue = &self.state.queue;
        let size = [
            (camera.rect.width() * pixels_per_point)
                .round()
                .clamp(8., 4096.) as u32,
            (camera.rect.height() * pixels_per_point)
                .round()
                .clamp(8., 4096.) as u32,
        ];
        if size != self.size {
            let create = |format, usage, sample_count| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("CAD viewport target"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let color = create(
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                1,
            );
            let depth = create(
                wgpu::TextureFormat::Depth32Float,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
                4,
            );
            let cv = color.create_view(&Default::default());
            let dv = depth.create_view(&Default::default());
            let mut renderer = self.state.renderer.write();
            if let Some(id) = self.texture {
                renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &cv,
                    wgpu::FilterMode::Linear,
                    id,
                );
            } else {
                self.texture =
                    Some(renderer.register_native_texture(device, &cv, wgpu::FilterMode::Linear));
            }
            let msaa = create(
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
                4,
            );
            let mv = msaa.create_view(&Default::default());
            self.target = Some((color, depth, cv, dv, msaa, mv));
            self.size = size;
        }
        let params = Uniforms {
            camera: camera.matrix().to_cols_array_2d(),
            origin: plane.position.extend(0.).to_array(),
            normal: plane.normal().extend(0.).to_array(),
            settings: [
                if cut { 1. } else { 0. },
                camera.size * 0.023,
                if domain.as_ref().is_some_and(|d| d.grid.is_some()) {
                    1.
                } else {
                    0.
                },
                domain
                    .as_ref()
                    .and_then(|d| d.grid)
                    .map(|(_, h)| h)
                    .unwrap_or(1.),
            ],
            grid_minimum: domain
                .as_ref()
                .and_then(|d| d.volume)
                .map(|v| {
                    Vec3::from(v.settings.min_mm)
                        .extend(v.settings.cell_mm)
                        .to_array()
                })
                .unwrap_or([0., 0., 0., 1.]),
            dimensions: domain
                .as_ref()
                .and_then(|d| d.volume)
                .map(|v| {
                    [
                        v.dims[0],
                        v.dims[1],
                        v.dims[2],
                        u32::from(domain.as_ref().unwrap().material_colors),
                    ]
                })
                .unwrap_or([0; 4]),
            field: domain
                .as_ref()
                .and_then(|d| d.field)
                .map(|f| [f.kind as f32, f.component as f32, f.range.max(1e-30), 0.])
                .unwrap_or([0., 0., 1., 0.]),
            palette: std::array::from_fn(|i| {
                let c = COLORS[i % COLORS.len()];
                [
                    linear(c[0] as f32 / 255.),
                    linear(c[1] as f32 / 255.),
                    linear(c[2] as f32 / 255.),
                    1.,
                ]
            }),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&params));
        let volume = domain.as_ref().and_then(|d| d.volume);
        self.bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Resident 3D domain"),
            layout: &self.volume_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: domain
                        .as_ref()
                        .and_then(|d| d.field)
                        .map(|f| f.buffer)
                        .unwrap_or(&self.empty)
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: volume
                        .map(|v| v.body_masks())
                        .unwrap_or(&self.empty)
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: volume
                        .map(|v| v.material_ids())
                        .unwrap_or(&self.empty)
                        .as_entire_binding(),
                },
            ],
        });
        let mut vertices = Vec::new();
        let mut lines = Vec::new();
        let vertex = |p: Vec3, c: [f32; 4], mode: f32| {
            let r = p - plane.position;
            Vertex {
                position: p.to_array(),
                color: c,
                uv: [r.dot(plane.u()), r.dot(plane.v())],
                mode,
            }
        };
        for (i, _) in model.bodies.iter().enumerate().filter(|(_, b)| b.visible) {
            let c = COLORS[i % COLORS.len()].map(|v| v as f32 / 255.);
            if cut {
                if let Some(cap) = caps.get(i) {
                    if hatched && domain.is_none() {
                        let color = [
                            linear(c[0] * 0.45 + 0.55),
                            linear(c[1] * 0.45 + 0.55),
                            linear(c[2] * 0.45 + 0.55),
                            1.,
                        ];
                        for t in &cap.triangles {
                            for &p in t {
                                let mut v = vertex(p, color, 1.);
                                if i % 2 == 1 {
                                    v.uv[0] = -v.uv[0];
                                }
                                vertices.push(v);
                            }
                        }
                    }
                    for segment in &cap.contours {
                        for &p in segment {
                            lines.push(vertex(p, [0.7, 0.85, 0.9, 1.], 2.));
                        }
                    }
                }
            }
        }
        if let Some(domain) = &domain {
            if !panel {
                for segment in domain.source_lines {
                    for &p in segment {
                        lines.push(vertex(p, [1., 0.65, 0.12, 1.], 2.));
                    }
                }
            }
            if let Some(field) = domain.field {
                for segment in field.vectors {
                    for &p in segment {
                        lines.push(vertex(p, [0.95, 0.97, 1., 1.], 2.));
                    }
                }
            }
            if cut {
                let o = plane.position;
                let u = plane.u() * domain.span;
                let v = plane.v() * domain.span;
                let corners = [o - u + v, o + u + v, o + u - v, o - u - v];
                let uv = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
                for i in [0, 2, 1, 0, 3, 2] {
                    let mut vert = vertex(corners[i], [1.; 4], 3.);
                    vert.uv = uv[i];
                    vertices.push(vert);
                }
            }
            if let Some((lo, hi)) = domain.bounds {
                for axis in 0..3 {
                    for a in [0, 1] {
                        for b in [0, 1] {
                            let mut p = lo;
                            p[(axis + 1) % 3] = if a == 0 {
                                lo[(axis + 1) % 3]
                            } else {
                                hi[(axis + 1) % 3]
                            };
                            p[(axis + 2) % 3] = if b == 0 {
                                lo[(axis + 2) % 3]
                            } else {
                                hi[(axis + 2) % 3]
                            };
                            let mut q = p;
                            q[axis] = hi[axis];
                            lines.push(vertex(p, [0.12, 0.3, 0.36, 1.], 2.));
                            lines.push(vertex(q, [0.12, 0.3, 0.36, 1.], 2.));
                        }
                    }
                }
            }
        }
        let contour_count = lines.len() as u32;
        let (lo, _) = model.bounds();
        for i in (-5..=5).filter(|_| !panel) {
            let a = i as f32 * camera.size * 0.1;
            let z = lo.z - camera.size * 0.04;
            for (p, q) in [
                (
                    Vec3::new(camera.center.x + a, camera.center.y - camera.size * 0.5, z),
                    Vec3::new(camera.center.x + a, camera.center.y + camera.size * 0.5, z),
                ),
                (
                    Vec3::new(camera.center.x - camera.size * 0.5, camera.center.y + a, z),
                    Vec3::new(camera.center.x + camera.size * 0.5, camera.center.y + a, z),
                ),
            ] {
                lines.push(vertex(p, [0.025, 0.045, 0.075, 1.], 2.));
                lines.push(vertex(q, [0.025, 0.045, 0.075, 1.], 2.));
            }
        }
        let buffer = |v: &[Vertex]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("CAD draw vertices"),
                contents: if v.is_empty() {
                    &[0u8; std::mem::size_of::<Vertex>()]
                } else {
                    bytemuck::cast_slice(v)
                },
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        let vb = (!vertices.is_empty()).then(|| buffer(&vertices));
        let lb = buffer(&lines);
        let (_, _, color, depth, _, msaa) = self.target.as_ref().unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CAD with depth"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa,
                    resolve_target: Some(color),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0045,
                            g: 0.0075,
                            b: 0.014,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.bind, &[]);
            // Editor grid is a backdrop; it must not draw over section faces.
            pass.set_pipeline(&self.line);
            pass.set_vertex_buffer(0, lb.slice(..));
            pass.draw(contour_count..lines.len() as u32, 0..1);
            if !panel {
                pass.set_pipeline(&self.triangle);
                for (body, mesh) in model.bodies.iter().zip(&self.meshes) {
                    if body.visible {
                        pass.set_vertex_buffer(0, mesh.buffer.slice(..));
                        pass.draw(0..mesh.count, 0..1);
                    }
                }
            }
            if let Some(vb) = &vb {
                pass.set_pipeline(&self.triangle);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
            pass.set_pipeline(&self.line);
            pass.set_vertex_buffer(0, lb.slice(..));
            pass.draw(0..contour_count, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        self.texture.unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn viewport() -> Viewport {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter
                .request_device(&Default::default(), None)
                .await
                .unwrap();
            let renderer =
                egui_wgpu::Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, None, 1, false);
            Viewport::new(egui_wgpu::RenderState {
                adapter,
                available_adapters: vec![],
                device,
                queue,
                target_format: wgpu::TextureFormat::Rgba8Unorm,
                renderer: std::sync::Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
            })
        })
    }
    fn read(view: &Viewport) -> Vec<u8> {
        let device = &view.state.device;
        let target = &view.target.as_ref().unwrap().0;
        let size = target.size();
        let bytes_per_row = (size.width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes_per_row as u64 * size.height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
            },
            size,
        );
        view.state.queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();
        buffer.slice(..).get_mapped_range().to_vec()
    }
    #[test]
    fn electromagnetic_section_samples_gpu_fields_at_native_resolution() {
        let _lock = crate::gpu_test_lock();
        let mut panel = viewport();
        let mut scene = Viewport::new(panel.state.clone());
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let context = crate::gpu::GpuContext::new(
            panel.state.device.clone(),
            panel.state.queue.clone(),
            panel.state.adapter.get_info().name,
        );
        let gpu = crate::domain::GpuDomain::from_context(context.clone()).unwrap();
        let settings = crate::simulation::DomainSettings {
            min_mm: [-5.; 3],
            max_mm: [5.; 3],
            cell_mm: 0.5,
        };
        let volume = gpu
            .build(&model, &settings, &crate::simulation::default_materials())
            .unwrap();
        let config = crate::em::EmConfig {
            uniform: vec![crate::em::UniformField {
                tesla: [0., 0., 0.01],
                scale: crate::em::Waveform::Constant { value: 1. },
            }],
            ..Default::default()
        };
        let em =
            crate::em::EmState::new(context, &volume, &model, config, crate::em::Mode::Applied)
                .unwrap();
        let plane = SectionPlane {
            position: Vec3::ZERO,
            orientation: glam::Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        };
        let mut hidden = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        for b in &mut hidden.bodies {
            b.visible = false;
        }
        let basis = glam::Mat3::from_cols(plane.u(), -plane.normal(), plane.v());
        for n in [256, 512] {
            let field = FieldView {
                buffer: em.result_buffer(),
                kind: 1,
                component: 0,
                range: 0.02,
                vectors: &[],
            };
            panel.draw_section(
                &model,
                &[],
                plane,
                Some(&volume),
                crate::section::Options {
                    style: crate::section::Style::Field,
                    edges: false,
                    material_colors: false,
                },
                5.,
                n,
                Some(field),
            );
            let image = read(&panel);
            let i = (n as usize * n as usize / 2 + n as usize / 2) * 4;
            let color = &image[i..i + 3];
            assert!(
                (color[0] as i32 - 128).abs() < 3
                    && color[1] > 250
                    && (color[2] as i32 - 128).abs() < 3,
                "field colour {color:?}"
            );
            let camera = Camera {
                rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(n as f32)),
                center: plane.position,
                rotation: glam::Quat::from_mat3(&basis).inverse(),
                scale: n as f32 / 10.,
                size: 8.,
            };
            scene.draw(
                camera,
                plane,
                true,
                false,
                &hidden,
                &[],
                1.,
                Some(DomainView {
                    span: 5.,
                    bounds: None,
                    grid: None,
                    volume: Some(&volume),
                    material_colors: false,
                    field: Some(field),
                    source_lines: &[],
                }),
            );
            let image3d = read(&scene);
            assert_eq!(color, &image3d[i..i + 3]);
            panel.draw_section(
                &model,
                &[],
                plane,
                Some(&volume),
                crate::section::Options {
                    style: crate::section::Style::Field,
                    edges: false,
                    material_colors: false,
                },
                5.,
                n,
                Some(FieldView {
                    component: 1,
                    ..field
                }),
            );
            let signed = read(&panel);
            assert!(signed[i..i + 3].iter().all(|&v| (v as i32 - 237).abs() < 3));
        }
    }
    #[test]
    fn cad_mesh_uploads_are_reused_until_geometry_is_replaced() {
        let _lock = crate::gpu_test_lock();
        let mut view = viewport();
        let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let mut plane = SectionPlane::new(Vec3::ZERO);
        let mut camera = Camera {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(256.)),
            center: Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: 20.,
            size: 8.,
        };
        view.draw(camera, plane, true, false, &model, &[], 1., None);
        let uploaded = view.mesh_uploads;
        assert_eq!(uploaded, 1);
        camera.rotation = glam::Quat::from_rotation_z(0.3);
        plane.position.x = 0.1;
        model.bodies[0].simulation.current_a = Some(10.);
        view.draw(camera, plane, true, false, &model, &[], 1., None);
        model.bodies[0].visible = false;
        view.draw(camera, plane, true, false, &model, &[], 1., None);
        model.bodies[0].visible = true;
        view.draw(camera, plane, true, false, &model, &[], 1., None);
        assert_eq!(view.mesh_uploads, uploaded);
        let replacement = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        view.draw(camera, plane, true, false, &replacement, &[], 1., None);
        assert_eq!(view.mesh_uploads, uploaded + 1);
    }
    #[test]
    fn direct_voxel_fragments_match_resident_grid_at_two_viewport_sizes() {
        let _gpu_lock = crate::gpu_test_lock();
        let mut view = viewport();
        let gpu = crate::domain::GpuDomain::from_context(crate::gpu::GpuContext::new(
            view.state.device.clone(),
            view.state.queue.clone(),
            view.state.adapter.get_info().name,
        ))
        .unwrap();
        let mut scene = Viewport::new(view.state.clone());
        let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        model.bodies[0].simulation.material_id = 3;
        let settings = crate::project::DomainSettings {
            min_mm: [-4., -4., -5.],
            max_mm: [4., 4., 5.],
            cell_mm: 0.25,
        };
        let volume = gpu
            .build(&model, &settings, &crate::project::default_materials())
            .unwrap();
        let serial = volume.build_serial;
        // Isolate the clipping shader from CAD surface coverage along the cut boundary.
        let mut hidden_cad = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        for body in &mut hidden_cad.bodies {
            body.visible = false;
        }
        let plane = SectionPlane {
            position: Vec3::new(0.031, 0.017, 0.043),
            orientation: glam::Quat::from_euler(glam::EulerRot::XYZ, 0.23, 0.41, 0.13),
        };
        for n in [256, 512] {
            let options = crate::section::Options {
                style: crate::section::Style::Voxels,
                edges: false,
                material_colors: false,
            };
            view.draw_section(&model, &[], plane, Some(&volume), options, 5., n, None);
            assert_eq!(view.output_texture().width(), n);
            let image = read(&view);
            let reference = gpu.section(&volume, plane, 5., n).unwrap();
            let basis = glam::Mat3::from_cols(plane.u(), -plane.normal(), plane.v());
            let camera = Camera {
                rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(n as f32)),
                center: plane.position,
                rotation: glam::Quat::from_mat3(&basis).inverse(),
                scale: n as f32 / 10.,
                size: 8.,
            };
            scene.draw(
                camera,
                plane,
                true,
                false,
                &hidden_cad,
                &[],
                1.,
                Some(DomainView {
                    field: None,
                    source_lines: &[],
                    span: 5.,
                    bounds: None,
                    grid: None,
                    volume: Some(&volume),
                    material_colors: false,
                }),
            );
            let image_3d = read(&scene);
            let mut checked = 0;
            for y in 2..n as usize - 2 {
                for x in 2..n as usize - 2 {
                    let i = (n as usize - 1 - y) * n as usize + x;
                    let occupied = reference[i].body != 0;
                    // Skip antialiased cell/shape transitions, whose subpixel samples may straddle two cells.
                    if [i - 1, i + 1, i - n as usize, i + n as usize]
                        .iter()
                        .any(|&j| (reference[j].body != 0) != occupied)
                    {
                        continue;
                    }
                    let rgba = &image[(y * n as usize + x) * 4..][..4];
                    assert_eq!(
                        rgba[..3].iter().copied().max().unwrap() > 100,
                        occupied,
                        "native voxel lookup at {n}px ({x},{y})"
                    );
                    if occupied {
                        assert!((rgba[0] as i32 - COLORS[0][0] as i32).abs() <= 1);
                        assert_eq!(
                            rgba,
                            &image_3d[(y * n as usize + x) * 4..][..4],
                            "3D and panel voxel fragments must agree"
                        );
                    }
                    checked += 1;
                }
            }
            assert!(checked > n as usize * n as usize / 2);
            view.draw_section(
                &model,
                &[],
                plane,
                Some(&volume),
                crate::section::Options {
                    material_colors: true,
                    ..options
                },
                5.,
                n,
                None,
            );
            let material = read(&view);
            assert!(material.chunks_exact(4).any(|p| p[..3] == COLORS[2]));
            view.draw_section(
                &model,
                &[],
                plane,
                Some(&volume),
                crate::section::Options {
                    edges: true,
                    ..options
                },
                5.,
                n,
                None,
            );
            let grid = read(&view);
            assert!(
                grid.iter().zip(image.iter()).any(|(a, b)| a != b),
                "native grid must change occupied fragments"
            );
        }
        assert_eq!(volume.build_serial, serial);
    }
    #[test]
    fn direct_cad_section_and_3d_face_use_same_shader_and_viewport_pixels() {
        let _gpu_lock = crate::gpu_test_lock();
        let mut panel = viewport();
        let mut scene = Viewport::new(panel.state.clone());
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let plane = SectionPlane {
            position: Vec3::ZERO,
            orientation: glam::Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        };
        let caps = vec![crate::caps::build(&model.bodies[0], plane, 1e-5)];
        for n in [256, 512] {
            let basis = glam::Mat3::from_cols(plane.u(), -plane.normal(), plane.v());
            let camera = Camera {
                rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(n as f32)),
                center: plane.position,
                rotation: glam::Quat::from_mat3(&basis).inverse(),
                scale: n as f32 / 10.,
                size: 8.,
            };
            for style in [
                crate::section::Style::HatchedCad,
                crate::section::Style::Outlines,
            ] {
                panel.draw_section(
                    &model,
                    &caps,
                    plane,
                    None,
                    crate::section::Options {
                        style,
                        edges: false,
                        material_colors: false,
                    },
                    5.,
                    n,
                    None,
                );
                scene.draw(
                    camera,
                    plane,
                    true,
                    style == crate::section::Style::HatchedCad,
                    &model,
                    &caps,
                    1.,
                    None,
                );
                let a = read(&panel);
                let b = read(&scene);
                // Annular interior avoids background/editor grid and surface boundary pixels.
                let y = n as usize / 2;
                for x in n as usize * 7 / 10..n as usize * 77 / 100 {
                    if style == crate::section::Style::HatchedCad {
                        assert_eq!(
                            &a[(y * n as usize + x) * 4..][..3],
                            &b[(y * n as usize + x) * 4..][..3]
                        );
                    } else {
                        assert!(
                            a[(y * n as usize + x) * 4] < 100 && b[(y * n as usize + x) * 4] < 100
                        );
                    }
                }
                assert_eq!(panel.output_texture().width(), n);
                let center = (n as usize / 2 * n as usize + n as usize / 2) * 4;
                assert!(a[center] < 100, "CAD cavity must stay open");
            }
        }
    }
    #[test]
    fn gpu_hatched_caps_preserve_cavity_and_outline_mode_removes_fill() {
        let _gpu_lock = crate::gpu_test_lock();
        let mut view = viewport();
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let plane = SectionPlane {
            position: Vec3::ZERO,
            orientation: glam::Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        };
        let caps = vec![crate::caps::build(&model.bodies[0], plane, 1e-5)];
        let camera = Camera {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(256.)),
            center: Vec3::ZERO,
            rotation: glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            scale: 30.,
            size: 8.,
        };
        view.draw(camera, plane, true, true, &model, &caps, 1., None);
        let hatched = read(&view);
        let bright = |data: &[u8], x: usize, y: usize| {
            data[(y * 256 + x) * 4..(y * 256 + x) * 4 + 3]
                .iter()
                .copied()
                .max()
                .unwrap()
        };
        assert!(
            bright(&hatched, 128, 128) < 100,
            "The cavity must stay unfilled"
        );
        let ring: Vec<_> = (180..208).map(|x| bright(&hatched, x, 128)).collect();
        assert!(
            ring.iter().any(|&v| v > 180),
            "The cap must cover the annular section"
        );
        assert!(ring.iter().any(|&v| v < 160), "Hatch ink must be visible");
        view.draw(camera, plane, true, false, &model, &caps, 1., None);
        let outline = read(&view);
        assert!(
            (180..208).all(|x| bright(&outline, x, 128) < 100),
            "Outline mode must not leave cap fill"
        );
        std::fs::create_dir_all("artifacts").unwrap();
        let mut ppm = b"P6\n256 256\n255\n".to_vec();
        for p in hatched.chunks_exact(4) {
            ppm.extend(&p[..3]);
        }
        std::fs::write("artifacts/hatch-test.ppm", ppm).unwrap();
    }
}

fn linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
