//! Streamed raster integral of closed, oriented CAD solids into liquid fractions.
use super::*;
use crate::gpu::read_bytes;
pub(crate) const SAMPLES: u32 = 16;
const TILE: u32 = 64;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FractionParams {
    minimum: [f32; 4],
    region: [u32; 4],
    grid: [u32; 4],
    tile: [u32; 4],
    flags: [u32; 4],
}
#[derive(Clone, Copy)]
pub(crate) struct InitialThermal<'a> {
    pub cells: &'a wgpu::Buffer,
    pub catalog: &'a wgpu::Buffer,
    pub body_offset: u32,
}
/// Returns the logical transient workspace size; output storage is caller-owned.
pub(crate) fn initialize(
    context: &GpuContext,
    volume: &Volume,
    model: &Model,
    material: u32,
    output: &wgpu::Buffer,
    thermal: Option<InitialThermal<'_>>,
) -> Result<u64, String> {
    initialize_impl(context, volume, model, material, output, thermal, true)
}
#[cfg(test)]
pub(crate) fn initialize_raw(
    context: &GpuContext,
    volume: &Volume,
    model: &Model,
    material: u32,
    output: &wgpu::Buffer,
    thermal: Option<InitialThermal<'_>>,
) -> Result<u64, String> {
    initialize_impl(context, volume, model, material, output, thermal, false)
}
fn initialize_impl(
    context: &GpuContext,
    volume: &Volume,
    model: &Model,
    material: u32,
    output: &wgpu::Buffer,
    thermal: Option<InitialThermal<'_>>,
    conditioned: bool,
) -> Result<u64, String> {
    if volume.overlaps > 0 {
        return Err(
            "Resolve overlapping CAD components before moving-interface initialization".into(),
        );
    }
    let snapshot = volume.inputs.snapshot();
    if snapshot.geometry_ids
        != model
            .bodies
            .iter()
            .map(|b| b.geometry_id())
            .collect::<Vec<_>>()
    {
        return Err(
            "CAD geometry changed; rebuild the domain before initializing liquid fractions".into(),
        );
    }
    let mut selected: Vec<_> = model
        .bodies
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            snapshot.components[*i].included && snapshot.components[*i].material_id == material
        })
        .collect();
    if selected.is_empty() {
        return Err("No included CAD solid is assigned to the moving material".into());
    }
    if conditioned {
        let liquid_bounds: Vec<_> = selected.iter().map(|(_, body)| body.bounds()).collect();
        let padding = glam::Vec3::splat(volume.settings.cell_mm);
        selected.extend(model.bodies.iter().enumerate().filter(|(i, body)| {
            let component = &snapshot.components[*i];
            let (lo, hi) = body.bounds();
            component.included
                && component.material_id != 0
                && component.material_id != material
                && liquid_bounds.iter().any(|&(liquid_lo, liquid_hi)| {
                    (hi.cmpge(liquid_lo - padding)).all() && (lo.cmple(liquid_hi + padding)).all()
                })
        }));
    }
    for (_, body) in &selected {
        if !body.closed() || !body.consistently_oriented() {
            return Err(format!(
                "{} needs closed, consistently oriented faces for CAD fraction initialization",
                body.name
            ));
        }
    }
    let limits = context.device.limits();
    let tile = TILE.min(limits.max_texture_dimension_2d / SAMPLES);
    if tile == 0 {
        return Err("GPU raster target is too small for CAD fraction sampling".into());
    }
    let [nx, ny, nz] = volume.dims;
    let width = nx.min(tile) * SAMPLES;
    let height = ny.min(tile) * SAMPLES;
    let mut tiles = Vec::new();
    for y in (0..ny).step_by(tile as usize) {
        for x in (0..nx).step_by(tile as usize) {
            tiles.push([x, y, (nx - x).min(tile), (ny - y).min(tile)]);
        }
    }
    let stride = (std::mem::size_of::<FractionParams>() as u64)
        .next_multiple_of(limits.min_uniform_buffer_offset_alignment as u64);
    let uniform_bytes = tiles.len() as u64 * nz as u64 * selected.len() as u64 * stride;
    let vertex_bytes = selected
        .iter()
        .map(|(_, b)| b.triangles().len() as u64 * 48)
        .sum::<u64>();
    let coverage_bytes = width as u64 * height as u64 * 8;
    if uniform_bytes > limits.max_buffer_size
        || uniform_bytes > u32::MAX as u64
        || coverage_bytes > limits.max_storage_buffer_binding_size as u64
        || selected
            .iter()
            .any(|(_, b)| b.triangles().len() as u64 * 48 > limits.max_buffer_size)
    {
        return Err("CAD fraction workspace exceeds GPU buffer limits; simplify geometry or increase cell spacing".into());
    }
    let workspace =
        uniform_bytes + vertex_bytes + coverage_bytes + width as u64 * height as u64 + 140;
    context.ensure_memory(workspace, "CAD fraction initialization workspace")?;
    let device = &context.device;
    let buffer = |label, size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    };
    let coverage = buffer("Fixed-point CAD ray integrals", coverage_bytes);
    let invalid = buffer("CAD fraction validity flags and excess diagnostics", 12);
    let absent_thermal = buffer("Absent initial thermal state", 64);
    let absent_catalog = buffer("Absent initial thermal catalog", 64);
    let mut uniforms = vec![0u8; uniform_bytes as usize];
    for (tile_index, &[x, y, tw, th]) in tiles.iter().enumerate() {
        for z in 0..nz {
            for (ordinal, (body, _)) in selected.iter().enumerate() {
                let params = FractionParams {
                    minimum: [
                        volume.settings.min_mm[0] + x as f32 * volume.settings.cell_mm,
                        volume.settings.min_mm[1] + y as f32 * volume.settings.cell_mm,
                        volume.settings.min_mm[2],
                        volume.settings.cell_mm,
                    ],
                    region: [tw * SAMPLES, th * SAMPLES, z, *body as u32],
                    grid: [nx, ny, nz, SAMPLES],
                    tile: [x, y, material, thermal.map_or(0, |t| t.body_offset)],
                    flags: [
                        u32::from(ordinal == 0),
                        u32::from(ordinal + 1 == selected.len()),
                        u32::from(thermal.is_some()),
                        u32::from(
                            conditioned && snapshot.components[*body].material_id != material,
                        ),
                    ],
                };
                let row = (tile_index * nz as usize + z as usize) * selected.len() + ordinal;
                let offset = row * stride as usize;
                uniforms[offset..offset + 80].copy_from_slice(bytemuck::bytes_of(&params));
            }
        }
    }
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("CAD integration tiles"),
        contents: &uniforms,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let vertices: Vec<_> = selected
        .iter()
        .map(|(_, body)| {
            let values: Vec<[f32; 4]> = body
                .triangles()
                .iter()
                .flat_map(|t| {
                    let z = (t[1] - t[0]).cross(t[2] - t[0]).z;
                    let sign = if z > 0. {
                        body.winding
                    } else if z < 0. {
                        -body.winding
                    } else {
                        0.
                    };
                    t.iter().map(move |p| p.extend(sign).to_array())
                })
                .collect();
            let count = values.len() as u32;
            (
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Oriented CAD triangles"),
                    contents: bytemuck::cast_slice(&values),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                count,
            )
        })
        .collect();
    let visibility = wgpu::ShaderStages::COMPUTE;
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Portable CAD integral layout"),
        entries: &[
            buffer_entry(
                0,
                wgpu::BufferBindingType::Uniform,
                true,
                wgpu::ShaderStages::VERTEX_FRAGMENT | visibility,
            ),
            buffer_entry(
                1,
                wgpu::BufferBindingType::Storage { read_only: false },
                false,
                wgpu::ShaderStages::FRAGMENT | visibility,
            ),
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
                wgpu::BufferBindingType::Storage { read_only: true },
                false,
                visibility,
            ),
            buffer_entry(
                6,
                wgpu::BufferBindingType::Storage { read_only: false },
                false,
                visibility,
            ),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let accumulator = include_str!("fractions/accumulate.wgsl");
    #[cfg(test)]
    let accumulator = if conditioned {
        accumulator
    } else {
        include_str!("fractions/raw_accumulate.wgsl")
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Oriented CAD volume integral"),
        source: wgpu::ShaderSource::Wgsl(
            format!(
                "{}\n{}\n{}",
                include_str!("../thermal/enthalpy.wgsl"),
                include_str!("fractions.wgsl"),
                accumulator
            )
            .into(),
        ),
    });
    let raster = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Integrate CAD crossings in fixed point"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0=>Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::R8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    });
    let accumulate = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Integrate initial liquid fraction and enthalpy"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("accumulate"),
        compilation_options: Default::default(),
        cache: None,
    });
    let buffers = [
        &coverage,
        output,
        thermal.map_or(&absent_thermal, |t| t.cells),
        thermal.map_or(&absent_catalog, |t| t.catalog),
        volume.material_ids(),
        &invalid,
    ];
    let mut entries = vec![wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &uniform,
            offset: 0,
            size: NonZeroU64::new(80),
        }),
    }];
    entries.extend(
        buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32 + 1,
                resource: b.as_entire_binding(),
            }),
    );
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &entries,
    });
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("CAD raster extent"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target = target.create_view(&Default::default());
    context.queue.write_buffer(&invalid, 0, &[0; 12]);
    for (tile_index, &[_, _, tw, th]) in tiles.iter().enumerate() {
        for start in (0..nz).step_by(8) {
            let mut encoder = device.create_command_encoder(&Default::default());
            for z in start..(start + 8).min(nz) {
                for (ordinal, (vertex, count)) in vertices.iter().enumerate() {
                    let offset = (((tile_index * nz as usize + z as usize) * selected.len()
                        + ordinal) as u64
                        * stride) as u32;
                    encoder.clear_buffer(&coverage, 0, None);
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("CAD subcell column integral"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &target,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Discard,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        pass.set_viewport(
                            0.,
                            0.,
                            (tw * SAMPLES) as f32,
                            (th * SAMPLES) as f32,
                            0.,
                            1.,
                        );
                        pass.set_scissor_rect(0, 0, tw * SAMPLES, th * SAMPLES);
                        pass.set_pipeline(&raster);
                        pass.set_bind_group(0, &bind, &[offset]);
                        pass.set_vertex_buffer(0, vertex.slice(..));
                        pass.draw(0..*count, 0..1);
                    }
                    {
                        let mut pass = encoder.begin_compute_pass(&Default::default());
                        pass.set_pipeline(&accumulate);
                        pass.set_bind_group(0, &bind, &[offset]);
                        pass.dispatch_workgroups(tw.div_ceil(8), th.div_ceil(8), 1);
                    }
                }
            }
            context.queue.submit(Some(encoder.finish()));
        }
    }
    let flags = read_bytes(context, &invalid, 0, 12)?;
    let diagnostics: &[u32] = bytemuck::cast_slice(&flags);
    let flags = diagnostics[0];
    if flags & 1 != 0 {
        return Err("CAD liquid integration found invalid oriented lengths; check outward surface winding and cavity orientation".into());
    }
    if flags & 2 != 0 {
        return Err(
            "CAD liquid fractions exceed one; resolve intersecting liquid components".into(),
        );
    }
    if flags & 4 != 0 {
        return Err(format!(
            "Wall-conditioned initialization has unresolved open volume or overlapping liquid/solid fractions (maximum total fraction excess {:.6e}, maximum liquid/open ratio excess {:.6e}); refine the grid or resolve CAD intersections",
            f32::from_bits(diagnostics[1]),
            f32::from_bits(diagnostics[2])
        ));
    }
    Ok(workspace)
}

#[cfg(test)]
mod tests;
