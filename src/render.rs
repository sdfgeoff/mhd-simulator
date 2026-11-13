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
}
pub struct DomainView {
    pub span: f32,
    pub bounds: Option<(Vec3, Vec3)>,
    pub grid: Option<(Vec3, f32)>,
}
pub struct Viewport {
    slice_texture: wgpu::Texture,
    slice_sampler: wgpu::Sampler,
    slice_layout: wgpu::BindGroupLayout,
    slice_size: u32,
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let slice_texture = make_slice(device, 1);
        let slice_view = slice_texture.create_view(&Default::default());
        let slice_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
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
                    resource: wgpu::BindingResource::TextureView(&slice_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&slice_sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let make = |lines: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some(if lines{"Section outlines"}else{"Depth-tested CAD"}),layout:Some(&pipeline_layout),vertex:wgpu::VertexState{module:&shader,entry_point:Some("vertex"),compilation_options:Default::default(),buffers:&[wgpu::VertexBufferLayout{array_stride:std::mem::size_of::<Vertex>()as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x4,2=>Float32x2,3=>Float32]}]},fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fragment"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format:wgpu::TextureFormat::Rgba8UnormSrgb,blend:Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),primitive:wgpu::PrimitiveState{topology:if lines{wgpu::PrimitiveTopology::LineList}else{wgpu::PrimitiveTopology::TriangleList},cull_mode:if lines{None}else{Some(wgpu::Face::Back)},..Default::default()},depth_stencil:Some(wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:!lines,depth_compare:wgpu::CompareFunction::LessEqual,stencil:Default::default(),bias:wgpu::DepthBiasState{constant:if lines{-2}else{0},..Default::default()}}),multisample:Default::default(),multiview:None,cache:None})
        };
        let triangle = make(false);
        let line = make(true);
        Self {
            slice_texture,
            slice_sampler,
            slice_layout: layout,
            slice_size: 1,
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
    pub fn set_section(&mut self, pixels: &[u8], n: u32) {
        if pixels.len() != n as usize * n as usize * 4 {
            return;
        }
        if self.slice_size != n {
            self.slice_texture = make_slice(&self.state.device, n);
            self.slice_size = n;
            let view = self.slice_texture.create_view(&Default::default());
            self.bind = self
                .state
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.slice_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.slice_sampler),
                        },
                    ],
                });
        }
        self.state.queue.write_texture(
            self.slice_texture.as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(n * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: n,
                height: n,
                depth_or_array_layers: 1,
            },
        );
    }
    pub fn set_shared_section(&mut self, view: &wgpu::TextureView) {
        self.bind = self
            .state
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shared section image in 3D"),
                layout: &self.slice_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.slice_sampler),
                    },
                ],
            });
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
            let create = |format, usage| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("CAD viewport target"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
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
            );
            let depth = create(
                wgpu::TextureFormat::Depth32Float,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
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
            self.target = Some((color, depth, cv, dv));
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
                .and_then(|d| d.grid)
                .map(|(m, _)| m.extend(0.).to_array())
                .unwrap_or([0.; 4]),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&params));
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
        for (i, body) in model.bodies.iter().enumerate().filter(|(_, b)| b.visible) {
            let c = COLORS[i % COLORS.len()].map(|v| v as f32 / 255.);
            for t in &body.triangles {
                let mut t = *t;
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
                    vertices.push(vertex(
                        p,
                        [c[0] * shade, c[1] * shade, c[2] * shade, 1.],
                        0.,
                    ));
                }
            }
            if cut {
                if let Some(cap) = caps.get(i) {
                    if hatched && domain.is_none() {
                        let color = [
                            c[0] * 0.45 + 0.55,
                            c[1] * 0.45 + 0.55,
                            c[2] * 0.45 + 0.55,
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
                    if domain.is_none() {
                        for segment in &cap.contours {
                            for &p in segment {
                                lines.push(vertex(p, [0.7, 0.85, 0.9, 1.], 2.));
                            }
                        }
                    }
                }
            }
        }
        if let Some(domain) = &domain {
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
        for i in -5..=5 {
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
                contents: bytemuck::cast_slice(v),
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        let vb = (!vertices.is_empty()).then(|| buffer(&vertices));
        let lb = buffer(&lines);
        let (_, _, color, depth) = self.target.as_ref().unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CAD with depth"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    resolve_target: None,
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

fn make_slice(device: &wgpu::Device, n: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Voxel section overlay"),
        size: wgpu::Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
