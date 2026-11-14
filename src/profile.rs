//! Optional compute-pass GPU timestamps plus exclusive CPU wall time per solver stage.
//! Query readback is deferred; profiling never waits for the current tick to finish on the CPU.
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};
const QUERY_COUNT: u32 = wgpu::QUERY_SET_MAX_QUERIES;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Other,
    EmPreparation,
    EmSources,
    EmCoefficients,
    EmSolve,
    EmFields,
    Fluid,
    Pressure,
    Surface,
    Thermal,
    Phase,
    Energy,
}
impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Other => "Other",
            Self::EmPreparation => "EM preparation",
            Self::EmSources => "Applied sources",
            Self::EmCoefficients => "EM coefficients",
            Self::EmSolve => "EM solve",
            Self::EmFields => "EM output",
            Self::Fluid => "Fluid transport/output",
            Self::Pressure => "Pressure solve",
            Self::Surface => "Interface/heat transport",
            Self::Thermal => "Thermal evolution",
            Self::Phase => "Phase/topology",
            Self::Energy => "Energy accounting",
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct CpuTimings {
    pub step: u64,
    pub time_s: f64,
    pub accepted: bool,
    pub total_ms: f64,
    pub stages_ms: BTreeMap<Stage, f64>,
}
#[derive(Clone, Debug, Default)]
pub struct GpuTimings {
    pub step: u64,
    pub time_s: f64,
    pub stages_ms: BTreeMap<Stage, f64>,
    pub recorded_passes: u32,
    pub total_passes: u32,
}
#[derive(Clone, Debug, Default)]
pub struct ProfileSnapshot {
    pub cpu: CpuTimings,
    pub gpu: Option<GpuTimings>,
    pub timestamps_supported: bool,
    pub enabled: bool,
}
#[derive(Clone, Debug)]
pub struct CompletedTiming {
    pub cpu: CpuTimings,
    pub gpu: Option<GpuTimings>,
}
struct Sample {
    stage: Stage,
    begin: u32,
    end: u32,
}
struct Pending {
    buffer: wgpu::Buffer,
    ready: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    samples: Vec<Sample>,
    cpu: CpuTimings,
    total: u32,
}
struct Recording {
    active: bool,
    stage: Stage,
    stage_started: Instant,
    started: Instant,
    step: u64,
    time_s: f64,
    next: u32,
    passes: u32,
    samples: Vec<Sample>,
    cpu: BTreeMap<Stage, f64>,
}
impl Default for Recording {
    fn default() -> Self {
        Self {
            active: false,
            stage: Stage::Other,
            stage_started: Instant::now(),
            started: Instant::now(),
            step: 0,
            time_s: 0.,
            next: 0,
            passes: 0,
            samples: Vec::new(),
            cpu: BTreeMap::new(),
        }
    }
}
#[derive(Default)]
struct State {
    recording: Recording,
    pending: VecDeque<Pending>,
    latest: ProfileSnapshot,
    completed: VecDeque<CompletedTiming>,
}
pub struct Profiler {
    device: wgpu::Device,
    queue: wgpu::Queue,
    queries: Option<wgpu::QuerySet>,
    resolve: Option<wgpu::Buffer>,
    enabled: AtomicBool,
    state: Mutex<State>,
}
impl Profiler {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let supported = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let queries = supported.then(|| {
            device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("Solver compute-pass timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: QUERY_COUNT,
            })
        });
        let resolve = supported.then(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Solver timestamp resolution"),
                size: QUERY_COUNT as u64 * 8,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        });
        Self {
            device: device.clone(),
            queue: queue.clone(),
            queries,
            resolve,
            enabled: AtomicBool::new(true),
            state: Mutex::new(State::default()),
        }
    }
    pub fn set_enabled(&self, value: bool) {
        self.enabled.store(value, Ordering::Relaxed);
    }
    pub fn begin_step(&self, step: u64, time_s: f64) {
        self.collect(false);
        let mut s = self.state.lock().unwrap();
        let on = self.enabled.load(Ordering::Relaxed);
        s.recording = Recording {
            active: on,
            step,
            time_s,
            ..Default::default()
        };
    }
    pub fn stage(&self, stage: Stage) -> StageGuard<'_> {
        let mut s = self.state.lock().unwrap();
        let r = &mut s.recording;
        let previous = r.stage;
        if r.active {
            let ms = r.stage_started.elapsed().as_secs_f64() * 1000.;
            *r.cpu.entry(r.stage).or_default() += ms;
            r.stage = stage;
            r.stage_started = Instant::now();
        }
        StageGuard {
            profiler: self,
            previous,
        }
    }
    pub fn timestamps(&self) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let mut s = self.state.lock().unwrap();
        let r = &mut s.recording;
        if !r.active {
            return None;
        }
        r.passes += 1;
        let queries = self.queries.as_ref()?;
        if r.next + 2 > QUERY_COUNT {
            return None;
        }
        let begin = r.next;
        let end = begin + 1;
        r.next += 2;
        r.samples.push(Sample {
            stage: r.stage,
            begin,
            end,
        });
        Some(wgpu::ComputePassTimestampWrites {
            query_set: queries,
            beginning_of_pass_write_index: Some(begin),
            end_of_pass_write_index: Some(end),
        })
    }
    pub fn end_step(&self, accepted: bool) {
        self.collect(false);
        let mut s = self.state.lock().unwrap();
        let r = &mut s.recording;
        if !r.active {
            s.latest.enabled = false;
            return;
        }
        let ms = r.stage_started.elapsed().as_secs_f64() * 1000.;
        *r.cpu.entry(r.stage).or_default() += ms;
        r.active = false;
        let cpu = CpuTimings {
            step: r.step,
            time_s: r.time_s,
            accepted,
            total_ms: r.started.elapsed().as_secs_f64() * 1000.,
            stages_ms: std::mem::take(&mut r.cpu),
        };
        let count = r.next;
        let total = r.passes;
        let samples = std::mem::take(&mut r.samples);
        s.latest.cpu = cpu.clone();
        s.latest.timestamps_supported = self.queries.is_some();
        s.latest.enabled = true;
        if count > 0 && s.pending.len() < 2 {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Asynchronous solver timestamps"),
                size: count as u64 * 8,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut enc = self.device.create_command_encoder(&Default::default());
            enc.resolve_query_set(
                self.queries.as_ref().unwrap(),
                0..count,
                self.resolve.as_ref().unwrap(),
                0,
            );
            enc.copy_buffer_to_buffer(
                self.resolve.as_ref().unwrap(),
                0,
                &buffer,
                0,
                count as u64 * 8,
            );
            self.queue.submit(Some(enc.finish()));
            let (tx, ready) = mpsc::channel();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            s.pending.push_back(Pending {
                buffer,
                ready,
                samples,
                cpu,
                total,
            });
        } else {
            s.completed.push_back(CompletedTiming { cpu, gpu: None });
        }
        while s.completed.len() > 32 {
            s.completed.pop_front();
        }
    }
    pub fn collect(&self, wait: bool) {
        self.device.poll(if wait {
            wgpu::Maintain::Wait
        } else {
            wgpu::Maintain::Poll
        });
        let mut s = self.state.lock().unwrap();
        loop {
            let Some(front) = s.pending.front() else {
                break;
            };
            let status = match front.ready.try_recv() {
                Ok(r) => r,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(_) => Err(wgpu::BufferAsyncError),
            };
            let p = s.pending.pop_front().unwrap();
            let gpu = if status.is_ok() {
                let data = p.buffer.slice(..).get_mapped_range();
                let ticks = bytemuck::cast_slice::<u8, u64>(&data);
                let mut stages = BTreeMap::new();
                let mut valid = 0;
                for sample in p.samples {
                    let a = ticks[sample.begin as usize];
                    let b = ticks[sample.end as usize];
                    if b >= a {
                        *stages.entry(sample.stage).or_default() +=
                            (b - a) as f64 * self.queue.get_timestamp_period() as f64 / 1e6;
                        valid += 1;
                    }
                }
                drop(data);
                p.buffer.unmap();
                Some(GpuTimings {
                    step: p.cpu.step,
                    time_s: p.cpu.time_s,
                    stages_ms: stages,
                    recorded_passes: valid,
                    total_passes: p.total,
                })
            } else {
                None
            };
            if let Some(g) = &gpu {
                s.latest.gpu = Some(g.clone());
            }
            s.completed.push_back(CompletedTiming { cpu: p.cpu, gpu });
            while s.completed.len() > 32 {
                s.completed.pop_front();
            }
        }
    }
    pub fn snapshot(&self) -> ProfileSnapshot {
        self.collect(false);
        self.state.lock().unwrap().latest.clone()
    }
    pub fn take_completed(&self, wait: bool) -> Vec<CompletedTiming> {
        self.collect(wait);
        self.state.lock().unwrap().completed.drain(..).collect()
    }
}
pub struct StageGuard<'a> {
    profiler: &'a Profiler,
    previous: Stage,
}
impl Drop for StageGuard<'_> {
    fn drop(&mut self) {
        let mut s = self.profiler.state.lock().unwrap();
        let r = &mut s.recording;
        if r.active {
            let ms = r.stage_started.elapsed().as_secs_f64() * 1000.;
            *r.cpu.entry(r.stage).or_default() += ms;
            r.stage = self.previous;
            r.stage_started = Instant::now();
        }
    }
}

impl CompletedTiming {
    pub fn csv_rows(&self) -> String {
        let mut rows = String::new();
        let mut stages: std::collections::BTreeSet<Stage> =
            self.cpu.stages_ms.keys().copied().collect();
        if let Some(g) = &self.gpu {
            stages.extend(g.stages_ms.keys().copied());
        }
        for stage in stages {
            rows.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                self.cpu.step,
                self.cpu.time_s,
                self.cpu.accepted,
                stage.label(),
                self.cpu.stages_ms.get(&stage).copied().unwrap_or(0.),
                self.gpu
                    .as_ref()
                    .and_then(|g| g.stages_ms.get(&stage))
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                self.gpu.as_ref().map_or(0, |g| g.recorded_passes),
                self.gpu.as_ref().map_or(0, |g| g.total_passes)
            ));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_profiling_tags_gpu_passes_and_preserves_results() {
        let _lock = crate::gpu_test_lock();
        let context = crate::domain::GpuDomain::new().unwrap().context;
        let shader = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Profiler fixture"), source: wgpu::ShaderSource::Wgsl("@group(0) @binding(0) var<storage,read_write> value:array<u32>; @compute @workgroup_size(1) fn main(){value[0]=123u;}".into()),
        });
        let pipeline = context
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let output = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output.as_entire_binding(),
                }],
            });
        for enabled in [true, false] {
            context.profiler.set_enabled(enabled);
            context.profiler.begin_step(7, 0.125);
            {
                let _outer = context.profiler.stage(Stage::Fluid);
                let _inner = context.profiler.stage(Stage::Pressure);
                let mut enc = context.device.create_command_encoder(&Default::default());
                {
                    let mut pass = context.compute_pass(&mut enc, Some("Profiled fixture"));
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &bind, &[]);
                    pass.dispatch_workgroups(1, 1, 1);
                }
                context.queue.submit(Some(enc.finish()));
            }
            context.profiler.end_step(true);
            assert_eq!(
                crate::gpu::read_bytes(&context, &output, 0, 4).unwrap(),
                123u32.to_ne_bytes()
            );
            let reports = context.profiler.take_completed(true);
            if enabled {
                assert_eq!(reports.len(), 1);
                let report = &reports[0];
                assert_eq!(report.cpu.step, 7);
                assert!(report.cpu.accepted);
                let sum: f64 = report.cpu.stages_ms.values().sum();
                assert!((report.cpu.total_ms - sum).abs() < 0.5);
                if context
                    .device
                    .features()
                    .contains(wgpu::Features::TIMESTAMP_QUERY)
                {
                    let gpu = report.gpu.as_ref().unwrap();
                    assert_eq!(gpu.recorded_passes, 1);
                    assert_eq!(gpu.total_passes, 1);
                    assert_eq!(gpu.step, 7);
                    assert!(gpu.stages_ms[&Stage::Pressure] >= 0.);
                } else {
                    assert!(report.gpu.is_none());
                }
            } else {
                assert!(reports.is_empty());
            }
        }
        // Exercise the same CPU-only path used on adapters without timestamp support.
        let mut cpu_only = Profiler::new(&context.device, &context.queue);
        cpu_only.queries = None;
        cpu_only.resolve = None;
        cpu_only.begin_step(8, 0.25);
        {
            let _stage = cpu_only.stage(Stage::Thermal);
        }
        cpu_only.end_step(false);
        let reports = cpu_only.take_completed(true);
        assert_eq!(reports.len(), 1);
        assert!(reports[0].gpu.is_none());
        assert!(!reports[0].cpu.accepted);
        assert!(!cpu_only.snapshot().timestamps_supported);
    }
}
