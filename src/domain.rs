//! Solid voxelization by hardware-rasterized crossing parity, then GPU volume fill.
use crate::{
    gpu::GpuContext,
    inputs::{GpuInputs, InputChanges},
    model::Model,
    plane::SectionPlane,
    simulation::SimulationSnapshot,
    simulation::{DomainSettings, Material},
};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::num::NonZeroU64;
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    minimum: [f32; 4],
    dimensions: [u32; 4],
    control: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Slice {
    origin: [f32; 4],
    u: [f32; 4],
    v: [f32; 4],
    minimum: [f32; 4],
    dimensions: [u32; 4],
    options: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Pixel {
    pub body: u32,
    pub material: u32,
    pub _padding: f32,
    pub inside: u32,
}
#[derive(Clone)]
pub struct Volume {
    pub settings: DomainSettings,
    pub dims: [u32; 3],
    pub counts: Vec<u32>,
    pub overlaps: u32,
    pub occupied: u32,
    pub build_ms: f64,
    pub warnings: Vec<String>,
    pub persistent_bytes: u64,
    pub material_names: Vec<(u32, String)>,
    pub build_serial: u64,
    pub(crate) masks: wgpu::Buffer,
    pub(crate) materials: wgpu::Buffer,
    pub inputs: GpuInputs,
}
impl Volume {
    pub fn body_masks(&self) -> &wgpu::Buffer {
        &self.masks
    }
    pub fn material_ids(&self) -> &wgpu::Buffer {
        &self.materials
    }
    pub fn max(&self) -> Vec3 {
        Vec3::from(self.settings.min_mm)
            + Vec3::new(
                self.dims[0] as f32,
                self.dims[1] as f32,
                self.dims[2] as f32,
            ) * self.settings.cell_mm
    }
}
pub struct GpuDomain {
    pub context: GpuContext,
    raster_layout: wgpu::BindGroupLayout,
    build_layout: wgpu::BindGroupLayout,
    crossing: wgpu::RenderPipeline,
    fill: wgpu::RenderPipeline,
    merge: wgpu::ComputePipeline,
    finalize: wgpu::ComputePipeline,
    coverage: wgpu::ComputePipeline,
    slice: wgpu::ComputePipeline,
    assign: wgpu::ComputePipeline,
    pub name: String,
    serial: std::cell::Cell<u64>,
}
fn buffer_entry(
    binding: u32,
    ty: wgpu::BufferBindingType,
    dynamic: bool,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}
impl GpuDomain {
    pub fn new() -> Result<Self, String> {
        Self::from_context(GpuContext::headless()?)
    }
    pub fn from_context(context: GpuContext) -> Result<Self, String> {
        let device = context.device.clone();
        let name = context.name.clone();
        let raster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[buffer_entry(
                0,
                wgpu::BufferBindingType::Uniform,
                true,
                wgpu::ShaderStages::VERTEX_FRAGMENT,
            )],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&raster_layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rasterized volume parity"),
            source: wgpu::ShaderSource::Wgsl(include_str!("voxel-raster.wgsl").into()),
        });
        let pipeline = |fill: bool| {
            let face = wgpu::StencilFaceState {
                compare: if fill {
                    wgpu::CompareFunction::NotEqual
                } else {
                    wgpu::CompareFunction::Always
                },
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: if fill {
                    wgpu::StencilOperation::Keep
                } else {
                    wgpu::StencilOperation::Invert
                },
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if fill {
                    "Fill odd crossings"
                } else {
                    "Toggle crossing stencil"
                }),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(if fill { "quad" } else { "surface" }),
                    compilation_options: Default::default(),
                    buffers: if fill {
                        &[]
                    } else {
                        &[wgpu::VertexBufferLayout {
                            array_stride: 12,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0=>Float32x3],
                        }]
                    },
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(if fill { "filled" } else { "crossing" }),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::R8Unorm,
                        blend: None,
                        write_mask: if fill {
                            wgpu::ColorWrites::ALL
                        } else {
                            wgpu::ColorWrites::empty()
                        },
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState {
                        front: face,
                        back: face,
                        read_mask: 255,
                        write_mask: if fill { 0 } else { 255 },
                    },
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let crossing = pipeline(false);
        let fill = pipeline(true);
        let visibility = wgpu::ShaderStages::COMPUTE;
        let build_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                buffer_entry(0, wgpu::BufferBindingType::Uniform, true, visibility),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                buffer_entry(
                    2,
                    wgpu::BufferBindingType::Storage { read_only: false },
                    false,
                    visibility,
                ),
                buffer_entry(
                    3,
                    wgpu::BufferBindingType::Storage { read_only: false },
                    false,
                    visibility,
                ),
                buffer_entry(
                    4,
                    wgpu::BufferBindingType::Storage { read_only: true },
                    false,
                    visibility,
                ),
                buffer_entry(
                    5,
                    wgpu::BufferBindingType::Storage { read_only: false },
                    false,
                    visibility,
                ),
                buffer_entry(
                    6,
                    wgpu::BufferBindingType::Storage { read_only: true },
                    false,
                    visibility,
                ),
            ],
        });
        let cl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&build_layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Voxel merge and material assignment"),
            source: wgpu::ShaderSource::Wgsl(include_str!("voxel-build.wgsl").into()),
        });
        let compute = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&cl),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let merge = compute("merge");
        let finalize = compute("finalize");
        let coverage = compute("coverage");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Persistent volume slice"),
            source: wgpu::ShaderSource::Wgsl(include_str!("voxel-section.wgsl").into()),
        });
        let slice = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Volume lookup"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Refresh material assignments"),
            source: wgpu::ShaderSource::Wgsl(include_str!("voxel-assign.wgsl").into()),
        });
        let assign = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Reuse existing body masks"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            context,
            raster_layout,
            build_layout,
            crossing,
            fill,
            merge,
            finalize,
            coverage,
            slice,
            assign,
            name,
            serial: std::cell::Cell::new(0),
        })
    }
    fn read(&self, buffer: &wgpu::Buffer, bytes: u64) -> Result<Vec<u8>, String> {
        let output = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Small diagnostic readback"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(buffer, 0, &output, 0, bytes);
        self.context.queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        output.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.context.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(output.slice(..).get_mapped_range().to_vec())
    }
    pub fn build(
        &self,
        model: &Model,
        settings: &DomainSettings,
        materials: &[Material],
    ) -> Result<Volume, String> {
        let started = std::time::Instant::now();
        let snapshot = SimulationSnapshot::from_model(model, settings, materials)?;
        let dims = settings.dims()?;
        let cells = settings.cells()?;
        let limits = self.context.device.limits();
        if model.bodies.is_empty() || model.bodies.len() > 31 {
            return Err("The domain needs 1..31 mesh components".into());
        }
        if dims[0] > limits.max_texture_dimension_2d
            || dims[1] > limits.max_texture_dimension_2d
            || dims[2] > limits.max_texture_array_layers
        {
            return Err("Grid dimensions exceed this GPU's raster target limits".into());
        }
        if cells * 4 > limits.max_storage_buffer_binding_size as u64
            || cells * 4 > limits.max_buffer_size
        {
            return Err(
                "Grid exceeds this GPU's storage-buffer limit; increase cell spacing".into(),
            );
        }
        let triangles = model
            .bodies
            .iter()
            .filter(|b| b.simulation.included)
            .map(|b| b.triangles().len() as u64)
            .sum::<u64>();
        if (triangles * 16).max(16) > limits.max_storage_buffer_binding_size as u64
            || model.bodies.iter().any(|b| {
                b.simulation.included && b.triangles().len() as u64 * 36 > limits.max_buffer_size
            })
        {
            return Err("Mesh upload exceeds this GPU's buffer limits; simplify the mesh".into());
        }
        let mesh_bytes = model
            .bodies
            .iter()
            .filter(|b| b.simulation.included)
            .map(|b| b.triangles().len() as u64 * 52)
            .sum::<u64>();
        if settings.estimated_bytes(model.bodies.len())? + mesh_bytes > 2 * 1024 * 1024 * 1024 {
            return Err("Estimated build memory exceeds 2 GiB; increase cell spacing".into());
        }
        for b in model.bodies.iter().filter(|b| b.simulation.included) {
            if !b.closed() {
                return Err(format!(
                    "{} is not closed; repair it or exclude it from the domain",
                    b.name
                ));
            }
        }
        let create = |label, size| {
            self.context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let masks = create("Persistent body masks", cells * 4);
        let material_grid = create("Persistent material IDs", cells * 4);
        let stats = create("Volume diagnostics", 64 * 4);
        let assignments: Vec<_> = model
            .bodies
            .iter()
            .map(|b| b.simulation.material_id)
            .collect();
        let assignment =
            self.context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Body-to-material map"),
                    contents: bytemuck::cast_slice(&assignments),
                    usage: wgpu::BufferUsages::STORAGE,
                });
        let probe_count: usize = model
            .bodies
            .iter()
            .filter(|b| b.simulation.included)
            .map(|b| b.triangles().len())
            .sum();
        let mut probe_values: Vec<[f32; 4]> = Vec::with_capacity(probe_count.max(1));
        for (i, b) in model
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.simulation.included)
        {
            for t in b.triangles() {
                probe_values.push(((t[0] + t[1] + t[2]) / 3.).extend(i as f32).to_array());
            }
        }
        if probe_values.is_empty() {
            probe_values.push([0.; 4]);
        }
        let probes = self
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Surface coverage probes"),
                contents: bytemuck::cast_slice(&probe_values),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let mut uniforms = vec![0u8; model.bodies.len() * dims[2] as usize * 256];
        for body in 0..model.bodies.len() {
            for z in 0..dims[2] {
                let p = Params {
                    minimum: [
                        settings.min_mm[0],
                        settings.min_mm[1],
                        settings.min_mm[2],
                        settings.cell_mm,
                    ],
                    dimensions: [dims[0], dims[1], dims[2], 1u32 << body],
                    control: [z, model.bodies.len() as u32, probe_count as u32, 0],
                };
                let offset = (body * dims[2] as usize + z as usize) * 256;
                uniforms[offset..offset + 48].copy_from_slice(bytemuck::bytes_of(&p));
            }
        }
        let uniform = self
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Raster slice parameters"),
                contents: &uniforms,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let ub = || {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &uniform,
                offset: 0,
                size: NonZeroU64::new(48),
            })
        };
        let raster_bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.raster_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ub(),
                }],
            });
        let occupancy = self
            .context
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Rasterized per-body slice occupancy"),
                size: wgpu::Extent3d {
                    width: dims[0],
                    height: dims[1],
                    depth_or_array_layers: dims[2],
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
        let whole = occupancy.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layers: Vec<_> = (0..dims[2])
            .map(|z| {
                occupancy.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: z,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let stencil = self
            .context
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Crossing parity stencil"),
                size: wgpu::Extent3d {
                    width: dims[0],
                    height: dims[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
        let stencil_view = stencil.create_view(&Default::default());
        let bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.build_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: ub(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&whole),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: masks.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: material_grid.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: assignment.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: stats.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: probes.as_entire_binding(),
                    },
                ],
            });
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        encoder.clear_buffer(&masks, 0, None);
        encoder.clear_buffer(&stats, 0, None);
        for (body, b) in model
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.simulation.included)
        {
            let vertices: Vec<[f32; 3]> = b
                .triangles()
                .iter()
                .flatten()
                .map(|p| p.to_array())
                .collect();
            let vb = self
                .context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Voxelization triangles"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            for z in 0..dims[2] {
                let offset = ((body * dims[2] as usize + z as usize) * 256) as u32;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Parity-rasterize a volume layer"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &layers[z as usize],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &stencil_view,
                        depth_ops: None,
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Discard,
                        }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_stencil_reference(0);
                pass.set_bind_group(0, &raster_bind, &[offset]);
                pass.set_pipeline(&self.crossing);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
                pass.set_pipeline(&self.fill);
                pass.draw(0..3, 0..1);
            }
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.merge);
            pass.set_bind_group(0, &bind, &[(body * dims[2] as usize * 256) as u32]);
            pass.dispatch_workgroups(
                dims[0].div_ceil(8),
                dims[1].div_ceil(8),
                dims[2].div_ceil(4),
            );
        }
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.finalize);
            pass.set_bind_group(0, &bind, &[0]);
            pass.dispatch_workgroups(
                dims[0].div_ceil(8),
                dims[1].div_ceil(8),
                dims[2].div_ceil(4),
            );
        }
        if probe_count > 0 {
            let groups = (probe_count as u32).div_ceil(64);
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.coverage);
            pass.set_bind_group(0, &bind, &[0]);
            pass.dispatch_workgroups(groups.min(1024), groups.div_ceil(1024), 1);
        }
        self.context.queue.submit(Some(encoder.finish()));
        let data = self.read(&stats, 64 * 4)?;
        let totals: &[u32] = bytemuck::cast_slice(&data);
        let counts = totals[..model.bodies.len()].to_vec();
        let overlaps = totals[31];
        let occupied = totals[32];
        let mut warnings = Vec::new();
        if overlaps > 0 {
            warnings.push(format!(
                "{overlaps} cells contain overlapping components (material ID 255)."
            ));
        }
        let min = Vec3::from(settings.min_mm);
        let max = settings.actual_max()?;
        for (i, b) in model
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.simulation.included)
        {
            if totals[33 + i] > 0 {
                warnings.push(format!("{}: {} surface patches have no nearby occupied cells; local thin features may be lost.",b.name,totals[33+i]));
            }
            if counts[i] == 0 {
                warnings.push(format!(
                    "{} disappears at this resolution: no occupied cell centres.",
                    b.name
                ));
            }
            let expected = b.volume_mm3();
            let measured = counts[i] as f64 * (settings.cell_mm as f64).powi(3);
            if expected > 1e-12 && (measured - expected).abs() / expected > 0.1 {
                warnings.push(format!("{}: voxel volume differs from mesh volume by {:.0}%; check resolution, thin features or clipping.",b.name,100.*(measured-expected).abs()/expected));
            }
            let mut lo = Vec3::splat(f32::INFINITY);
            let mut hi = Vec3::splat(f32::NEG_INFINITY);
            for &p in b.triangles().iter().flatten() {
                lo = lo.min(p);
                hi = hi.max(p);
            }
            if (hi - lo).min_element() < 2. * settings.cell_mm {
                warnings.push(format!("{} has an axis-aligned extent below two cells; thin features may be unresolved.",b.name));
            }
            if lo.cmplt(min).any() || hi.cmpgt(max).any() {
                warnings.push(format!("{} extends outside the domain.", b.name));
            }
        }
        static NEXT_BUILD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        self.serial
            .set(NEXT_BUILD.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let inputs = GpuInputs::new(&self.context, snapshot, self.serial.get())?;
        Ok(Volume {
            settings: settings.clone(),
            dims,
            counts,
            overlaps,
            occupied,
            build_ms: started.elapsed().as_secs_f64() * 1000.,
            warnings,
            persistent_bytes: cells * 8 + inputs.bytes(),
            material_names: materials.iter().map(|m| (m.id, m.name.clone())).collect(),
            build_serial: self.serial.get(),
            masks,
            materials: material_grid,
            inputs,
        })
    }
    /// Update physical inputs and material IDs without repeating rasterization.
    pub fn update_inputs(
        &self,
        volume: &mut Volume,
        snapshot: SimulationSnapshot,
    ) -> Result<InputChanges, String> {
        let change = volume.inputs.changes(&snapshot)?;
        if change.assignments {
            // Keep material IDs immutable for active solver snapshots.
            volume.materials = self.context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Copy-on-write material assignments"),
                size: volume.materials.size(),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let assignments: Vec<u32> = snapshot.components.iter().map(|c| c.material_id).collect();
            let uniform =
                self.context
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: bytemuck::cast_slice(&[
                            volume.dims[0],
                            volume.dims[1],
                            volume.dims[2],
                            0,
                        ]),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
            let map = self
                .context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Updated body materials"),
                    contents: bytemuck::cast_slice(&assignments),
                    usage: wgpu::BufferUsages::STORAGE,
                });
            let bind = self
                .context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.assign.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: volume.masks.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: map.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: volume.materials.as_entire_binding(),
                        },
                    ],
                });
            let mut encoder = self
                .context
                .device
                .create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.assign);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(
                    volume.dims[0].div_ceil(8),
                    volume.dims[1].div_ceil(8),
                    volume.dims[2].div_ceil(4),
                );
            }
            self.context.queue.submit(Some(encoder.finish()));
        }
        volume.material_names = snapshot
            .materials
            .iter()
            .map(|m| (m.id, m.name.clone()))
            .collect();
        volume.inputs.update(&self.context, snapshot, change);
        volume.persistent_bytes =
            volume.dims.iter().map(|&d| d as u64).product::<u64>() * 8 + volume.inputs.bytes();
        Ok(change)
    }
    pub fn section(
        &self,
        volume: &Volume,
        plane: SectionPlane,
        span: f32,
        n: u32,
    ) -> Result<Vec<Pixel>, String> {
        if !(1..=1024).contains(&n) {
            return Err("Slice resolution must be 1..1024".into());
        }
        let p = Slice {
            origin: plane.position.extend(0.).to_array(),
            u: (plane.u() * span).extend(0.).to_array(),
            v: (plane.v() * span).extend(0.).to_array(),
            minimum: [
                volume.settings.min_mm[0],
                volume.settings.min_mm[1],
                volume.settings.min_mm[2],
                volume.settings.cell_mm,
            ],
            dimensions: [volume.dims[0], volume.dims[1], volume.dims[2], 0],
            options: [n, 0, 0, 0],
        };
        let uniform = self
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Slice plane"),
                contents: bytemuck::bytes_of(&p),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bytes = n as u64 * n as u64 * 16;
        let output = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volume slice"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind = self
            .context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.slice.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: volume.masks.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: volume.materials.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: output.as_entire_binding(),
                    },
                ],
            });
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.slice);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(n.div_ceil(8), n.div_ceil(8), 1);
        }
        self.context.queue.submit(Some(encoder.finish()));
        Ok(bytemuck::cast_slice(&self.read(&output, bytes)?).to_vec())
    }
    #[cfg(test)]
    pub fn read_masks(&self, v: &Volume) -> Vec<u32> {
        bytemuck::cast_slice(
            &self
                .read(&v.masks, v.settings.cells().unwrap() * 4)
                .unwrap(),
        )
        .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Body;
    use crate::simulation;
    use glam::{Mat4, Quat};
    fn annulus() -> Model {
        Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap()
    }
    #[test]
    fn rasterized_hollow_volume_converges_and_slices_do_not_rebuild() {
        let _gpu_lock = crate::gpu_test_lock();
        let gpu = GpuDomain::new().unwrap();
        let mut model = annulus();
        model.bodies[0].simulation.material_id = 3;
        let materials = simulation::default_materials();
        let mut errors = Vec::new();
        for h in [0.5, 0.25, 0.125] {
            let settings = DomainSettings {
                min_mm: [-4., -4., -5.],
                max_mm: [4., 4., 5.],
                cell_mm: h,
            };
            let volume = gpu.build(&model, &settings, &materials).unwrap();
            let actual = volume.occupied as f64 * (h as f64).powi(3);
            let exact = std::f64::consts::PI * (9. - 2.25) * 8.;
            let error = (actual - exact).abs() / exact;
            errors.push(error);
            eprintln!(
                "Raster {}x{}x{}: {:.1} ms, volume {actual:.5}, relative error {error:.5}",
                volume.dims[0], volume.dims[1], volume.dims[2], volume.build_ms
            );
            assert!(error < 0.1);
            let masks = gpu.read_masks(&volume);
            for z in 0..volume.dims[2] {
                for y in 0..volume.dims[1] {
                    for x in 0..volume.dims[0] {
                        let p = Vec3::from(settings.min_mm)
                            + (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5)) * h;
                        let r = p.truncate().length();
                        if (r - 1.5).abs() > 0.02
                            && (r - 3.).abs() > 0.02
                            && (p.z.abs() - 4.).abs() > 0.01
                        {
                            let expected = u32::from(r > 1.5 && r < 3. && p.z.abs() < 4.);
                            let i = ((z * volume.dims[1] + y) * volume.dims[0] + x) as usize;
                            assert_eq!(masks[i], expected, "{p:?}");
                        }
                    }
                }
            }
            let serial = gpu.serial.get();
            let mut plane = SectionPlane::new(Vec3::ZERO);
            plane.orientation = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
            let pixels = gpu.section(&volume, plane, 4., 64).unwrap();
            assert!(pixels.iter().any(|p| p.body == 1));
            assert!(
                pixels
                    .iter()
                    .filter(|p| p.body == 1)
                    .all(|p| p.material == 3)
            );
            plane.rotate_world(Vec3::Y, 0.4);
            gpu.section(&volume, plane, 4., 64).unwrap();
            assert_eq!(
                gpu.serial.get(),
                serial,
                "Moving the plane must not rebuild the volume"
            );
        }
        assert!(errors[2] < 0.01 && errors[2] < errors[0], "{errors:?}");
    }
    #[test]
    fn rotated_solid_matches_analytic_interior() {
        let _gpu_lock = crate::gpu_test_lock();
        let gpu = GpuDomain::new().unwrap();
        let mut model = annulus();
        let transform = Mat4::from_rotation_translation(
            Quat::from_rotation_y(0.43) * Quat::from_rotation_z(0.25),
            Vec3::new(0.13, -0.19, 0.27),
        );
        let inverse = transform.inverse();
        let triangles = model.bodies[0]
            .triangles()
            .iter()
            .map(|t| t.map(|p| transform.transform_point3(p)))
            .collect();
        model.bodies[0] = Body::new("Rotated hollow cylinder".into(), triangles).unwrap();
        let settings = DomainSettings {
            min_mm: [-6.; 3],
            max_mm: [6.; 3],
            cell_mm: 0.25,
        };
        let volume = gpu
            .build(&model, &settings, &simulation::default_materials())
            .unwrap();
        let masks = gpu.read_masks(&volume);
        for z in 0..48 {
            for y in 0..48 {
                for x in 0..48 {
                    let p = Vec3::splat(-6.)
                        + (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5)) * 0.25;
                    let q = inverse.transform_point3(p);
                    let r = q.truncate().length();
                    if (r - 1.5).abs() > 0.03
                        && (r - 3.).abs() > 0.03
                        && (q.z.abs() - 4.).abs() > 0.03
                    {
                        assert_eq!(
                            masks[((z * 48 + y) * 48 + x) as usize],
                            u32::from(r > 1.5 && r < 3. && q.z.abs() < 4.),
                            "{p:?} / {q:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn overlaps_visibility_inclusion_and_disappearing_features() {
        let _gpu_lock = crate::gpu_test_lock();
        let gpu = GpuDomain::new().unwrap();
        let settings = DomainSettings {
            min_mm: [-4., -4., -5.],
            max_mm: [4., 4., 5.],
            cell_mm: 0.25,
        };
        let mut model = annulus();
        model.bodies[0].visible = false;
        model.bodies.push(model.bodies[0].clone());
        let volume = gpu
            .build(&model, &settings, &simulation::default_materials())
            .unwrap();
        assert!(volume.occupied > 0);
        assert_eq!(volume.overlaps, volume.occupied);
        assert_eq!(volume.counts[0], volume.counts[1]);
        assert!(volume.warnings.iter().any(|w| w.contains("overlapping")));
        model.bodies[1].simulation.included = false;
        let single = gpu
            .build(&model, &settings, &simulation::default_materials())
            .unwrap();
        assert_eq!(single.occupied, volume.occupied);
        assert_eq!(single.overlaps, 0);
        assert_eq!(single.counts[1], 0);
        let mut tiny = Model::load(std::path::Path::new("assets/hotend.glb")).unwrap();
        let triangles = tiny.bodies[3]
            .triangles()
            .iter()
            .map(|t| t.map(|p| (p - Vec3::new(6., 0., 5.)) * 0.005))
            .collect();
        tiny.bodies = vec![Body::new("Subcell component".into(), triangles).unwrap()];
        let v = gpu
            .build(&tiny, &settings, &simulation::default_materials())
            .unwrap();
        assert_eq!(v.occupied, 0);
        assert!(v.warnings.iter().any(|w| w.contains("disappears")));
        let mut compound = annulus();
        let mut combined = compound.bodies[0].triangles().to_vec();
        combined.extend_from_slice(tiny.bodies[0].triangles());
        compound.bodies[0] = Body::new("Compound".into(), combined).unwrap();
        let v = gpu
            .build(&compound, &settings, &simulation::default_materials())
            .unwrap();
        assert!(v.occupied > 0);
        assert!(
            v.warnings.iter().any(|w| w.contains("surface patches")),
            "An isolated subcell patch in a larger body should be flagged"
        );
    }
    #[test]
    fn open_mesh_is_rejected_before_build() {
        let _gpu_lock = crate::gpu_test_lock();
        let gpu = GpuDomain::new().unwrap();
        let model = Model::load(std::path::Path::new("assets/open-triangle.gltf")).unwrap();
        assert!(
            gpu.build(
                &model,
                &DomainSettings::fit(&model),
                &simulation::default_materials()
            )
            .err()
            .unwrap()
            .contains("not closed")
        );
    }
}

#[cfg(test)]
mod larger_grid_test {
    use super::*;
    use crate::simulation;
    #[test]
    fn persistent_256_cubed_grid() {
        let _gpu_lock = crate::gpu_test_lock();
        let gpu = GpuDomain::new().unwrap();
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let settings = DomainSettings {
            min_mm: [-5.; 3],
            max_mm: [5.; 3],
            cell_mm: 10. / 256.,
        };
        let v = gpu
            .build(&model, &settings, &simulation::default_materials())
            .unwrap();
        assert_eq!(v.dims, [256; 3]);
        assert!(v.persistent_bytes >= 128 * 1024 * 1024);
        let measured = v.occupied as f64 * (settings.cell_mm as f64).powi(3);
        let exact = std::f64::consts::PI * 6.75 * 8.;
        assert!((measured - exact).abs() / exact < 0.01);
        eprintln!(
            "256^3 raster build: {:.1} ms; {:.1} MiB persistent; occupied volume {:.5} mm^3",
            v.build_ms,
            v.persistent_bytes as f64 / 1048576.,
            measured
        );
    }
}

#[cfg(test)]
mod input_update_tests {
    use super::*;
    use crate::{
        inputs::{FieldState, TableEntry},
        simulation::{self, MaterialProperty},
    };
    #[test]
    fn tables_and_assignments_update_without_revoxelization_and_reject_stale_geometry() {
        let _lock = crate::gpu_test_lock();
        let gpu = GpuDomain::new().unwrap();
        let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let settings = DomainSettings {
            min_mm: [-4., -4., -5.],
            max_mm: [4., 4., 5.],
            cell_mm: 0.5,
        };
        let mut materials = simulation::default_materials();
        let mut volume = gpu.build(&model, &settings, &materials).unwrap();
        let masks = gpu.read_masks(&volume);
        let serial = volume.build_serial;
        let occupied = volume.occupied;
        assert!(
            volume
                .inputs
                .solver_tables(&[MaterialProperty::Conductivity], false)
                .is_err()
        );
        let mut fields = FieldState {
            computed_from: Some(volume.inputs.versions()),
            ..Default::default()
        };
        assert!(fields.is_current(&volume.inputs));
        let initial = volume.inputs.versions();
        materials[1].conductivity_s_m = Some(0.);
        let change = gpu
            .update_inputs(
                &mut volume,
                SimulationSnapshot::from_model(&model, &settings, &materials).unwrap(),
            )
            .unwrap();
        assert!(change.materials && !change.assignments && !change.excitation);
        assert_eq!(volume.inputs.versions().excitation, initial.excitation);
        assert!(!fields.is_current(&volume.inputs));
        fields.invalidate();
        assert!(fields.computed_from.is_none());
        volume
            .inputs
            .solver_tables(&[MaterialProperty::Conductivity], false)
            .unwrap();
        let raw = gpu
            .read(
                &volume.inputs.material_table,
                volume.inputs.material_table.size(),
            )
            .unwrap();
        let table = bytemuck::cast_slice::<u8, TableEntry>(&raw);
        assert_eq!(table[0].flags[0], 0);
        assert_eq!(table[1].flags[0], 1);
        assert_eq!(table[1].values[0], 0.);
        let before_source = volume.inputs.versions();
        model.bodies[0].simulation.current_a = Some(-12.);
        model.bodies[0].simulation.temperature_c = Some(250.);
        let change = gpu
            .update_inputs(
                &mut volume,
                SimulationSnapshot::from_model(&model, &settings, &materials).unwrap(),
            )
            .unwrap();
        assert!(change.excitation && !change.materials && !change.assignments);
        assert_eq!(volume.inputs.versions().materials, before_source.materials);
        let raw = gpu
            .read(
                &volume.inputs.component_table,
                volume.inputs.component_table.size(),
            )
            .unwrap();
        let entry = bytemuck::cast_slice::<u8, TableEntry>(&raw)[0];
        assert!((entry.values[0] - 523.15).abs() < 1e-4);
        assert_eq!(entry.values[1], -12.);
        assert_eq!(entry.flags[..2], [1, 1]);
        let old_snapshot = volume.clone();
        model.bodies[0].simulation.material_id = 6;
        let change = gpu
            .update_inputs(
                &mut volume,
                SimulationSnapshot::from_model(&model, &settings, &materials).unwrap(),
            )
            .unwrap();
        assert!(change.assignments && change.materials && !change.excitation);
        let pixels = gpu
            .section(&volume, SectionPlane::new(Vec3::ZERO), 4., 64)
            .unwrap();
        assert!(pixels.iter().any(|p| p.body == 1 && p.material == 6));
        assert!(pixels.iter().all(|p| p.body != 1 || p.material == 6));
        let old_pixels = gpu
            .section(&old_snapshot, SectionPlane::new(Vec3::ZERO), 4., 64)
            .unwrap();
        assert!(old_pixels.iter().any(|p| p.body == 1));
        assert!(
            old_pixels.iter().all(|p| p.body != 1 || p.material == 1),
            "an active snapshot must keep its original assignments"
        );
        assert_eq!(gpu.read_masks(&volume), masks);
        assert_eq!(volume.build_serial, serial);
        assert_eq!(volume.occupied, occupied);
        let stable = volume.inputs.versions();
        let snapshot = volume.inputs.snapshot().clone();
        assert_eq!(
            gpu.update_inputs(&mut volume, snapshot).unwrap(),
            InputChanges::default()
        );
        assert_eq!(volume.inputs.versions(), stable);
        model.bodies[0].simulation.included = false;
        assert!(
            gpu.update_inputs(
                &mut volume,
                SimulationSnapshot::from_model(&model, &settings, &materials).unwrap()
            )
            .is_err()
        );
        model.bodies[0].simulation.included = true;
        let mut finer = settings.clone();
        finer.cell_mm *= 0.5;
        assert!(
            gpu.update_inputs(
                &mut volume,
                SimulationSnapshot::from_model(&model, &finer, &materials).unwrap()
            )
            .is_err()
        );
        let replacement = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        assert!(
            gpu.update_inputs(
                &mut volume,
                SimulationSnapshot::from_model(&replacement, &settings, &materials).unwrap()
            )
            .is_err()
        );
        assert_eq!(volume.inputs.versions(), stable);
        assert_eq!(gpu.read_masks(&volume), masks);
    }
}
