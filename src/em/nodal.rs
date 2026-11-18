use crate::{gpu::GpuContext, simulation::GridSi};
use std::collections::HashMap;
pub(super) struct NodalCorrection {
    pub bind: wgpu::BindGroup,
    pub pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    pub nodes: u32,
    pub bytes: u64,
    _values: wgpu::Buffer,
}
impl NodalCorrection {
    pub fn new(
        context: &GpuContext,
        grid: &GridSi,
        uniform: &wgpu::Buffer,
        source: &wgpu::Buffer,
        work: &wgpu::Buffer,
        partials: &wgpu::Buffer,
        scalars: &wgpu::Buffer,
    ) -> Result<Self, String> {
        let nodes: u32 = grid
            .dims
            .into_iter()
            .map(|n| u64::from(n) + 1)
            .product::<u64>()
            .try_into()
            .map_err(|_| "Nodal indexing exceeds u32")?;
        let bytes = u64::from(nodes) * 4;
        let limits = context.device.limits();
        if bytes > u64::from(limits.max_storage_buffer_binding_size)
            || bytes > limits.max_buffer_size
        {
            return Err("EM nodal correction exceeds adapter buffer limits".into());
        }
        context.ensure_memory(bytes, "EM nodal correction")?;
        let device = &context.device;
        let values = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("EM nodal gradient correction"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        for binding in [3, 4, 6, 7, 9] {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("EM gradient correction"),
            entries: &entries,
        });
        let buffers = [
            (0, uniform),
            (3, source),
            (4, work),
            (6, partials),
            (7, scalars),
            (9, &values),
        ];
        let entries: Vec<_> = buffers
            .iter()
            .map(|(binding, b)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &entries,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("EM gradient-space correction"),
            source: wgpu::ShaderSource::Wgsl(super::shader_source().into()),
        });
        let mut pipelines = HashMap::new();
        for name in [
            "nodal_build_initial",
            "nodal_build",
            "nodal_initial",
            "nodal_update",
        ] {
            pipelines.insert(
                name,
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(name),
                    layout: Some(&pl),
                    module: &shader,
                    entry_point: Some(name),
                    compilation_options: Default::default(),
                    cache: None,
                }),
            );
        }
        Ok(Self {
            bind,
            pipelines,
            nodes,
            bytes,
            _values: values,
        })
    }
}
