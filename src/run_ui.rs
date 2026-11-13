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
    pub fn poll(&mut self, ctx: &egui::Context, volume: Option<&Volume>) -> bool {
        let mut changed = false;
        let mut done = false;
        if let Some(rx) = &self.receiver {
            loop {
                match rx.try_recv() {
                    Ok(Event::Frame(frame)) if self.accept => {
                        let bytes = frame.em.as_ref().map_or(0, |f| f.bytes)
                            + frame.fluid.as_ref().map_or(0, |f| f.bytes);
                        self.timeline.push(Sample {
                            time_s: frame.time_s,
                            bytes,
                            output: frame,
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
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        changed
    }
    pub fn em_frame(&self) -> Option<&crate::em::EmFrame> {
        self.timeline.current().and_then(|f| f.em.as_ref())
    }
    pub fn fluid_frame(&self) -> Option<&crate::fluid::FluidFrame> {
        self.timeline.current().and_then(|f| f.fluid.as_ref())
    }
    pub fn controls(&mut self, ui: &mut egui::Ui) -> bool {
        let changed = self.timeline.controls(ui);
        if let Some(frame) = self.timeline.current() {
            egui::CollapsingHeader::new("Energy accounting (J)").show(ui,|ui|{
            let e=&frame.energy;
            ui.monospace(format!("Magnetic {:.4e} · Kinetic {:.4e} · Joule generated {:.4e} · External work {:.4e}",e.magnetic_j,e.kinetic_j,e.joule_j,e.external_work_j));
            ui.monospace(format!("Induction numerical loss {:.4e} · Fluid loss (viscous + numerical) {:.4e} · Coupling work mismatch {:.4e} · EM closure error {:.4e}",e.induction_numerical_loss_j,e.fluid_loss_j,e.coupling_defect_j,e.em_balance_error_j));
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
