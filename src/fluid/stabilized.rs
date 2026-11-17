//! Damped second-order Runge–Kutta–Chebyshev integration for a fixed stress operator.
//! Verwer, Hundsdorfer & Sommeijer (1990), Numer. Math. 57, 157–178.
//! Stage storage alternates the existing two face buffers; no volume allocation.
use super::*;
const DAMPING: f64 = 0.05;
pub(super) const MAX_EULER_STEPS: u32 = 4096;
pub(super) fn capacity(euler_limit: u32) -> u32 {
    (3. * (euler_limit as f64).sqrt()).ceil() as u32 + 2
}
pub(super) fn bytes(alignment: u32) -> u64 {
    capacity(MAX_EULER_STEPS) as u64 * 32u64.next_multiple_of(alignment as u64)
}
/// Rows contain (mu,nu,initial weight,operator time) and (initial operator time,0,0,0).
pub(super) fn schedule(dt: f64, euler_steps: u32) -> Vec<[f32; 8]> {
    assert!(dt.is_finite() && dt > 0. && euler_steps > 0);
    let stiffness = 1.8 * euler_steps as f64;
    for stages in 2..=capacity(euler_steps) {
        let w0 = 1. + DAMPING / (stages as f64).powi(2);
        let mut t = vec![1., w0];
        let mut d = vec![0., 1.];
        let mut dd = vec![0., 0.];
        for j in 2..=stages as usize {
            t.push(2. * w0 * t[j - 1] - t[j - 2]);
            d.push(2. * t[j - 1] + 2. * w0 * d[j - 1] - d[j - 2]);
            dd.push(4. * d[j - 1] + 2. * w0 * dd[j - 1] - dd[j - 2]);
        }
        let last = stages as usize;
        let w1 = d[last] / dd[last];
        if stiffness > 0.9 * (1. + w0) / w1 {
            continue;
        }
        let mut b = vec![0.; t.len()];
        for j in 2..t.len() {
            b[j] = dd[j] / d[j].powi(2);
        }
        b[0] = b[2];
        b[1] = b[2];
        let a: Vec<f64> = b.iter().zip(&t).map(|(b, t)| 1. - b * t).collect();
        let mut rows = vec![[1., 0., 0., (dt * b[1] * w1) as f32, 0., 0., 0., 0.]];
        for j in 2..=last {
            let mu = (2. * w0 * b[j] / b[j - 1]) as f32;
            let nu = (-b[j] / b[j - 2]) as f32;
            let op = dt * 2. * w1 * b[j] / b[j - 1];
            rows.push([
                mu,
                nu,
                1. - mu - nu,
                op as f32,
                (-a[j - 1] * op) as f32,
                0.,
                0.,
                0.,
            ]);
        }
        return rows;
    }
    unreachable!("RKC stage-capacity bound");
}
pub(super) struct StabilizedGpu {
    uniform: wgpu::Buffer,
    bind: [wgpu::BindGroup; 2],
    pipeline: wgpu::ComputePipeline,
    initialize: wgpu::ComputePipeline,
    stride: u64,
    capacity: u32,
}
impl StabilizedGpu {
    pub(super) fn new(
        context: &GpuContext,
        shader: &wgpu::ShaderModule,
        buffers: [&wgpu::Buffer; 9],
    ) -> Self {
        let device = &context.device;
        let stride =
            32u64.next_multiple_of(device.limits().min_uniform_buffer_offset_alignment as u64);
        let capacity = capacity(MAX_EULER_STEPS);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Chebyshev stress stage coefficients"),
            size: stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entries: Vec<_> = (0..10)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 || binding == 9 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage {
                            read_only: binding == 1 || binding == 8,
                        }
                    },
                    has_dynamic_offset: binding == 9,
                    min_binding_size: if binding == 9 {
                        std::num::NonZeroU64::new(32)
                    } else {
                        None
                    },
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Stabilized stress layout"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Stabilized symmetric stress"),
            layout: Some(&pipeline_layout),
            module: shader,
            entry_point: Some("stress_stabilized"),
            compilation_options: Default::default(),
            cache: None,
        });
        let initialize = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Initial stress stage state"),
            layout: Some(&pipeline_layout),
            module: shader,
            entry_point: Some("stress_save_initial"),
            compilation_options: Default::default(),
            cache: None,
        });
        let bind = std::array::from_fn(|parity| {
            let mut state = buffers;
            if parity == 1 {
                state.swap(2, 3);
            }
            let mut entries: Vec<_> = state
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect();
            entries.push(wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform,
                    offset: 0,
                    size: std::num::NonZeroU64::new(32),
                }),
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Alternating Chebyshev face buffers"),
                layout: &layout,
                entries: &entries,
            })
        });
        Self {
            uniform,
            bind,
            pipeline,
            initialize,
            stride,
            capacity,
        }
    }
    pub(super) fn encode(
        &self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        rows: &[[f32; 8]],
        faces: [u32; 3],
        velocity: &wgpu::Buffer,
        scratch: &wgpu::Buffer,
    ) {
        assert!(rows.len() <= self.capacity as usize);
        let mut data = vec![0u8; rows.len() * self.stride as usize];
        for (i, row) in rows.iter().enumerate() {
            data[i * self.stride as usize..i * self.stride as usize + 32]
                .copy_from_slice(bytemuck::bytes_of(row));
        }
        context.queue.write_buffer(&self.uniform, 0, &data);
        {
            let mut pass = context.compute_pass(encoder, Some("Initial stress stage state"));
            pass.set_pipeline(&self.initialize);
            pass.set_bind_group(0, &self.bind[0], &[0]);
            // Face dispatch covers every cell too; the kernel guards by cell count.
            pass.dispatch_workgroups(faces[0], faces[1], faces[2]);
        }
        for i in 0..rows.len() {
            let mut pass = context.compute_pass(encoder, Some("Chebyshev stress stage"));
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind[i % 2], &[(i as u64 * self.stride) as u32]);
            pass.dispatch_workgroups(faces[0], faces[1], faces[2]);
        }
        // The forcing kernel consumes scratch. An odd stage count ends in velocity.
        if rows.len() % 2 == 1 {
            encoder.copy_buffer_to_buffer(velocity, 0, scratch, 0, velocity.size());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn amplification(rows: &[[f32; 8]], lambda: f64) -> f64 {
        let mut previous = 1.;
        let mut older = 1.;
        for r in rows {
            let next = (r[0] as f64 + r[3] as f64 * lambda) * previous
                + r[1] as f64 * older
                + r[2] as f64
                + r[4] as f64 * lambda;
            older = previous;
            previous = next;
        }
        previous
    }
    #[test]
    fn chebyshev_schedule_preserves_constants_stabilizes_interval_and_converges() {
        for steps in [1, 9, 64, 1000, 4096] {
            let rows = schedule(1., steps);
            assert!(rows.len() <= capacity(steps) as usize);
            assert!((amplification(&rows, 0.) - 1.).abs() < 2e-5);
            for i in 0..=2000 {
                let z = -(i as f64 / 2000.) * 1.8 * steps as f64;
                assert!(amplification(&rows, z).abs() <= 1.0001, "N={steps} z={z}");
            }
        }
        let mut errors = Vec::new();
        for n in [1, 2, 4, 8] {
            let rows = schedule(1. / n as f64, 1000);
            let value = amplification(&rows, -1.).powi(n);
            errors.push((value - (-1f64).exp()).abs());
        }
        assert!(errors.windows(2).all(|e| e[1] < 0.3 * e[0]), "{errors:?}");
    }
}
