//! Background execution and a single timeline for all participating physics.
use crate::{
    domain::Volume,
    em_ui::EmUi,
    fluid_ui::FluidUi,
    gpu::GpuContext,
    model::Model,
    run::{RunConfig, RunFrame, RunState},
    timeline::{Sample, Timeline},
};
use eframe::egui;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::{collections::VecDeque, sync::atomic::Ordering as AtomicOrdering};
#[derive(Clone, Default)]
struct Statistics {
    memory: crate::run::WorkingMemory,
    profile: mhd_workbench::profile::ProfileSnapshot,
    capture: mhd_workbench::recording::CaptureTiming,
    step: u64,
    time_s: f64,
}

enum Event {
    Frame(RunFrame),
    Done(Result<RunState, String>),
}
pub struct RunUi {
    pub config: RunConfig,
    pub steps: u32,
    pub record_every: u32,
    pub profiling: bool,
    pub view_error: String,
    cache: Option<RunFrame>,
    context: Option<GpuContext>,
    recording_memory: Arc<mhd_workbench::recording::RecordingMemory>,
    statistics: Arc<Mutex<Statistics>>,
    pub timeline: Timeline<RunFrame>,
    pub error: String,
    state: Option<RunState>,
    receiver: Option<mpsc::Receiver<Event>>,
    cancel: Option<Arc<AtomicBool>>,
    accept: bool,
}
impl Default for RunUi {
    fn default() -> Self {
        Self {
            config: Default::default(),
            steps: 10,
            record_every: 1,
            profiling: true,
            view_error: String::new(),
            cache: None,
            context: None,
            recording_memory: Arc::new(Default::default()),
            statistics: Arc::new(Mutex::new(Statistics::default())),
            timeline: Default::default(),
            error: String::new(),
            state: None,
            receiver: None,
            cancel: None,
            accept: false,
        }
    }
}
impl RunUi {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn invalidate(&mut self) {
        self.state = None;
        self.timeline.clear();
        self.cache = None;
        self.view_error.clear();
        *self.statistics.lock().unwrap() = Statistics::default();
        self.accept = false;
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Relaxed);
        }
    }
    pub fn start(
        &mut self,
        model: &Model,
        volume: &Volume,
        context: &GpuContext,
        em: &mut EmUi,
        fluid: &mut FluidUi,
    ) {
        if self.busy() {
            return;
        }
        let ec = em.config.clone();
        let fc = fluid.config.clone();
        let continuing = self
            .state
            .as_ref()
            .is_some_and(|s| s.matches(volume, &self.config, &ec, &fc));
        let old = if continuing {
            self.state.take()
        } else {
            self.invalidate();
            em.invalidate();
            fluid.invalidate();
            None
        };
        self.accept = true;
        self.error.clear();
        self.context = Some(context.clone());
        context.profiler.set_enabled(self.profiling);
        let memory = self.recording_memory.clone();
        let statistics = self.statistics.clone();
        let record_every = self.record_every.max(1) as u64;

        let (context, model, volume, config, steps) = (
            context.clone(),
            model.clone(),
            volume.clone(),
            self.config.clone(),
            self.steps,
        );
        let (tx, rx) = mpsc::sync_channel(2);
        self.receiver = Some(rx);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        std::thread::spawn(move || {
            let run = || -> Result<RunState, String> {
                let mut state = match old {
                    Some(s) => s,
                    None => RunState::new(context.clone(), &volume, &model, config, ec, fc)?,
                };
                statistics.lock().unwrap().memory = state.working_memory(&volume);
                let mut pool =
                    mhd_workbench::recording::ReadbackPool::new(context.clone(), memory.clone());
                let mut pending = VecDeque::new();
                let mut last_capture = state.steps;
                let mut failure = None;
                let deliver = |pending: crate::run::PendingRunFrame,
                               pool: &mut mhd_workbench::recording::ReadbackPool|
                 -> Result<(), String> {
                    let (frame, timing) = pending.finish(pool)?;
                    statistics.lock().unwrap().capture = timing;
                    let bytes = frame.ram_bytes();
                    memory
                        .queued_ram_bytes
                        .fetch_add(bytes, AtomicOrdering::Relaxed);
                    if tx.send(Event::Frame(frame)).is_err() {
                        memory
                            .queued_ram_bytes
                            .fetch_sub(bytes, AtomicOrdering::Relaxed);
                        return Err("Run view closed".into());
                    }
                    Ok(())
                };
                if state.steps == 0 {
                    pending.push_back(state.begin_recording(&mut pool)?);
                }
                for _ in 0..steps {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = state.step(&model, &volume);
                    {
                        let mut stats = statistics.lock().unwrap();
                        stats.step = state.steps;
                        stats.time_s = state.time_s;
                        stats.profile = context.profiler.snapshot();
                        stats.memory = state.working_memory(&volume);
                    }
                    if let Err(e) = result {
                        failure = Some(e);
                        break;
                    }
                    if state.steps % record_every == 0 {
                        match state.begin_recording(&mut pool) {
                            Ok(f) => {
                                pending.push_back(f);
                                last_capture = state.steps;
                            }
                            Err(e) => {
                                failure = Some(e);
                                break;
                            }
                        }
                    }
                    // At most two full-precision downloads are in flight. Their maps overlap later solver work.
                    while pending.len() >= 2 || pending.front_mut().is_some_and(|p| p.ready()) {
                        if let Err(e) = deliver(pending.pop_front().unwrap(), &mut pool) {
                            failure = Some(e);
                            break;
                        }
                    }
                    if failure.is_some() {
                        break;
                    }
                }
                if failure.is_none() && last_capture != state.steps {
                    match state.begin_recording(&mut pool) {
                        Ok(f) => pending.push_back(f),
                        Err(e) => failure = Some(e),
                    }
                }
                // Earlier complete ticks must still reach the timeline if a later physical tick fails.
                while let Some(p) = pending.pop_front() {
                    if let Err(e) = deliver(p, &mut pool) {
                        if failure.is_none() {
                            failure = Some(e);
                        }
                        break;
                    }
                }
                context.profiler.collect(false);
                statistics.lock().unwrap().profile = context.profiler.snapshot();
                if let Some(e) = failure {
                    Err(e)
                } else {
                    Ok(state)
                }
            };
            let _ = tx.send(Event::Done(run()));
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context, volume: Option<&Volume>) -> bool {
        let mut changed = false;
        let mut done = false;
        if let Some(rx) = &self.receiver {
            loop {
                match rx.try_recv() {
                    Ok(Event::Frame(frame)) if self.accept => {
                        let bytes = frame.ram_bytes();
                        self.recording_memory
                            .queued_ram_bytes
                            .fetch_sub(bytes, AtomicOrdering::Relaxed);
                        self.timeline.push(Sample {
                            time_s: frame.time_s,
                            bytes,
                            output: frame,
                        });
                        changed = true;
                    }
                    Ok(Event::Frame(frame)) => {
                        self.recording_memory
                            .queued_ram_bytes
                            .fetch_sub(frame.ram_bytes(), AtomicOrdering::Relaxed);
                    }
                    Ok(Event::Done(result)) => {
                        done = true;
                        changed = true;
                        match result {
                            Ok(state)
                                if self.accept && volume.is_some_and(|v| state.is_current(v)) =>
                            {
                                self.state = Some(state)
                            }
                            Ok(_) => {}
                            Err(e) if self.accept => self.error = e,
                            Err(_) => {}
                        }
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if self.accept {
                            self.error = "Simulation worker disconnected".into();
                        }
                        done = true;
                        break;
                    }
                }
            }
        }
        if done {
            if self.state.is_none() {
                self.statistics.lock().unwrap().memory = Default::default();
            }
            self.receiver = None;
            self.cancel = None;
        }
        changed |= self.refresh_cache();
        if self.busy() || (!self.view_error.is_empty() && self.cache.is_none()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        changed
    }
    pub fn gpu_frame(&self) -> Option<&RunFrame> {
        self.cache
            .as_ref()
            .filter(|c| self.timeline.current().is_some_and(|f| f.id == c.id))
    }
    pub fn gpu_cache_bytes(&self) -> u64 {
        self.cache.as_ref().map_or(0, RunFrame::gpu_bytes)
    }
    fn refresh_cache(&mut self) -> bool {
        let Some(frame) = self.timeline.current() else {
            self.cache = None;
            return false;
        };
        if self.cache.as_ref().is_some_and(|c| c.id == frame.id) {
            return false;
        }
        let mut frame = frame.clone();
        self.cache = None;
        let Some(context) = &self.context else {
            return false;
        };
        match frame.make_resident(context) {
            Ok(()) => {
                self.cache = Some(frame);
                self.view_error.clear();
                true
            }
            Err(e) => {
                self.view_error = e;
                false
            }
        }
    }
    pub fn em_frame(&self) -> Option<&crate::em::EmFrame> {
        self.gpu_frame().and_then(|f| f.em.as_ref())
    }
    pub fn fluid_frame(&self) -> Option<&crate::fluid::FluidFrame> {
        self.gpu_frame().and_then(|f| f.fluid.as_ref())
    }
    pub fn thermal_frame(&self) -> Option<&crate::thermal::ThermalFrame> {
        self.gpu_frame().and_then(|f| f.thermal.as_ref())
    }
    fn performance_panel(&self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Performance and memory").show(ui, |ui| {
            let stats = self.statistics.lock().unwrap().clone();
            let m = &stats.memory;
            let rec = self.recording_memory.snapshot();
            let mib = |bytes: u64| bytes as f64 / 1048576.;
            ui.monospace(format!(
                "Working GPU buffers {:.1} MiB · selected-frame GPU cache {:.1} MiB",
                mib(m.total()), mib(self.gpu_cache_bytes())
            ));
            ui.small(format!(
                "Domain {:.1} · EM {:.1} · fluid {:.1} · thermal {:.1} · interface {:.1} MiB",
                mib(m.domain), mib(m.em), mib(m.fluid), mib(m.thermal), mib(m.surface)
            ));
            ui.monospace(format!(
                "History RAM {:.1} MiB · delivery queue RAM {:.1} MiB",
                mib(self.timeline.bytes()), mib(rec.queued_ram_bytes)
            ));
            ui.small(format!(
                "Transfer buffers {:.1} MiB ({:.1} MiB in flight)",
                mib(rec.staging_bytes), mib(rec.pending_bytes)
            ));
            if stats.capture.bytes > 0 {
                ui.small(format!(
                    "Latest readback completion latency {:.2} ms · RAM copy {:.2} ms",
                    stats.capture.completion_latency_ms, stats.capture.cpu_copy_ms
                ));
            }
            let p = &stats.profile;
            if !p.enabled { return; }
            ui.small(format!(
                "CPU wall: step {} at {:.3} µs · total {:.2} ms",
                p.cpu.step, p.cpu.time_s * 1e6, p.cpu.total_ms
            ));
            if let Some(g) = &p.gpu {
                ui.small(format!(
                    "GPU compute passes: step {} · {}/{} timed",
                    g.step, g.recorded_passes, g.total_passes
                ));
            } else if !p.timestamps_supported {
                ui.small("GPU timestamps unavailable on this device; CPU wall times are available.");
            }
            egui::Grid::new("stage_performance").striped(true).show(ui, |ui| {
                ui.strong("Stage");
                ui.strong("CPU wall ms");
                ui.strong("GPU pass ms");
                ui.end_row();
                for (stage, cpu) in &p.cpu.stages_ms {
                    ui.label(stage.label());
                    ui.monospace(format!("{cpu:.2}"));
                    let gpu = p.gpu.as_ref().and_then(|g| g.stages_ms.get(stage));
                    ui.monospace(gpu.map(|v| format!("{v:.2}")).unwrap_or_else(|| "—".into()));
                    ui.end_row();
                }
            });
            ui.small("CPU wall includes submission/waiting. GPU values sum compute passes; transfers and GPU idle gaps are excluded. GPU results arrive asynchronously; step numbers identify each sample.");
        });
    }
    pub fn controls(&mut self, ui: &mut egui::Ui) -> bool {
        let changed = self.timeline.controls(ui);
        if changed {
            self.refresh_cache();
        }
        ui.horizontal(|ui| {
            ui.add_enabled(
                !self.busy(),
                egui::DragValue::new(&mut self.record_every)
                    .range(1..=10000)
                    .prefix("Record every ")
                    .suffix(" steps"),
            );
            if ui
                .checkbox(&mut self.profiling, "Profile solver stages")
                .changed()
            {
                if let Some(c) = &self.context {
                    c.profiler.set_enabled(self.profiling);
                }
            }
        });
        if !self.view_error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.view_error);
        }
        self.performance_panel(ui);

        if let Some(frame) = self.timeline.current() {
            egui::CollapsingHeader::new("Energy accounting (J)").show(ui,|ui|{
            let e=&frame.energy;
            ui.monospace(format!("Magnetic {:.4e} · Kinetic {:.4e} · Thermal {:.4e} · Joule generated {:.4e} · Deposited heat {:.4e} · External work {:.4e}",e.magnetic_j,e.kinetic_j,e.thermal_j,e.joule_j,e.thermal_deposited_j,e.external_work_j));
            ui.monospace(format!("Induction numerical loss {:.4e} · Fluid loss (viscous + numerical) {:.4e} · Coupling work mismatch {:.4e} · EM closure error {:.4e} · Thermal closure error {:.4e}",e.induction_numerical_loss_j,e.fluid_loss_j,e.coupling_defect_j,e.em_balance_error_j,e.thermal_balance_error_j));
            if let Some(surface)=&frame.surface {
                ui.monospace(format!("Material volume {:.4e} m³ · Transport drift {:.3e} m³ · Occupancy {:.4}…{:.4}",surface.volume_m3,surface.transport_error_m3,surface.min_fraction,surface.max_fraction));
                ui.monospace(format!("Surface energy {:.4e} J · Capillary work {:.4e} J · Interface energy mismatch {:.4e} J",e.surface_j,e.surface_work_j,e.surface_balance_error_j));
            }
            ui.small("Work/loss terms accumulate from run start. Fluid loss is inferred from its work balance; it is not deposited as heat. EM closure uses staggered operator energy, including boundary/source work.");
        });
        }
        if self.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Advancing coupled simulation…");
                if ui.button("Stop after step").clicked() {
                    if let Some(c) = &self.cancel {
                        c.store(true, Ordering::Relaxed);
                    }
                }
            });
        }
        changed
    }
}
