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
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
enum Event {
    Frame(RunFrame),
    Done(Result<RunState, String>),
}
pub struct RunUi {
    pub config: RunConfig,
    pub steps: u32,
    pub timeline: Timeline<()>,
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
        let mut ec = em.config.clone();
        ec.dt_s = self.config.dt_s;
        let mut fc = fluid.config.clone();
        fc.dt_s = self.config.dt_s;
        fc.use_em_force = !ec.wires.is_empty() || !ec.magnets.is_empty() || !ec.uniform.is_empty();
        let continuing = self.state.as_ref().is_some_and(|s| {
            s.is_current(volume)
                && s.config == self.config
                && s.em_config == ec
                && s.fluid_config == fc
        });
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
                    None => RunState::new(context, &volume, &model, config, ec, fc)?,
                };
                if state.time_s == 0. {
                    tx.send(Event::Frame(state.snapshot()?))
                        .map_err(|_| "Run view closed")?;
                }
                for _ in 0..steps {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(state);
                    }
                    state.step(&model, &volume)?;
                    tx.send(Event::Frame(state.snapshot()?))
                        .map_err(|_| "Run view closed")?;
                }
                Ok(state)
            };
            let _ = tx.send(Event::Done(run()));
        });
    }
    pub fn poll(
        &mut self,
        ctx: &egui::Context,
        volume: Option<&Volume>,
        em: &mut EmUi,
        fluid: &mut FluidUi,
    ) -> bool {
        let mut changed = false;
        let mut done = false;
        if let Some(rx) = &self.receiver {
            loop {
                match rx.try_recv() {
                    Ok(Event::Frame(frame)) if self.accept => {
                        let bytes = frame.em.as_ref().map_or(0, |f| f.bytes)
                            + frame.fluid.as_ref().map_or(0, |f| f.bytes);
                        em.timeline.capacity_frames = self.timeline.capacity_frames;
                        fluid.timeline.capacity_frames = self.timeline.capacity_frames;
                        if let Some(f) = frame.em {
                            em.record(f);
                        }
                        if let Some(f) = frame.fluid {
                            fluid.record(f);
                        }
                        self.timeline.push(Sample {
                            time_s: frame.time_s,
                            bytes,
                            output: (),
                        });
                        changed = true;
                    }
                    Ok(Event::Frame(_)) => {}
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
            self.receiver = None;
            self.cancel = None;
        }
        self.sync_selection(em, fluid);
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        changed
    }
    fn sync_selection(&self, em: &mut EmUi, fluid: &mut FluidUi) {
        // Timestamp lookup also handles changing the history window while a job runs.
        if let Some(s) = self.timeline.samples.get(self.timeline.selected) {
            if let Some(i) = em
                .timeline
                .samples
                .iter()
                .position(|f| f.time_s == s.time_s)
            {
                em.timeline.selected = i;
            }
            if let Some(i) = fluid
                .timeline
                .samples
                .iter()
                .position(|f| f.time_s == s.time_s)
            {
                fluid.timeline.selected = i;
            }
        }
    }
    pub fn controls(&mut self, ui: &mut egui::Ui, em: &mut EmUi, fluid: &mut FluidUi) -> bool {
        let changed = self.timeline.controls(ui);
        // Trim backend output storage to the same retained time window.
        if let Some(first) = self.timeline.samples.first() {
            em.timeline.samples.retain(|f| f.time_s >= first.time_s);
            fluid.timeline.samples.retain(|f| f.time_s >= first.time_s);
        }
        self.sync_selection(em, fluid);
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
