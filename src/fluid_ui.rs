//! Background fluid analysis and retained output frames; visualization shares the vector overlay.
use crate::{
    domain::Volume,
    flow::{VectorField, VectorLayout},
    fluid::{FluidConfig, FluidFrame, FluidState, ForceField},
    gpu::GpuContext,
    plane::SectionPlane,
    render::FieldView,
    simulation::{GridSi, Material},
    timeline::{Sample, Timeline},
    vector_ui::VectorUi,
};
use eframe::egui::{self, Color32};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
pub struct FrozenForce {
    pub buffer: wgpu::Buffer,
    pub grid: GridSi,
    pub time_s: f64,
}
enum Event {
    Progress(u32, u32),
    Frame(FluidFrame),
    Done(Result<Box<FluidState>, String>),
}
pub struct FluidUi {
    pub config: FluidConfig,
    pub state: Option<FluidState>,
    pub timeline: Timeline<FluidFrame>,
    pub overlay: VectorUi,
    pub kind: u32,
    pub show: bool,
    pub run_steps: u32,
    pub error: String,
    receiver: Option<mpsc::Receiver<Event>>,
    cancel: Option<Arc<AtomicBool>>,
    accept: bool,
    progress: String,
    component: u32,
    auto_range: bool,
    range: f32,
    force_key: Option<wgpu::Buffer>,
    force_time: Option<f64>,
    probe_cache: Option<([u32; 3], u64, Option<crate::fluid::FluidCell>)>,
}
impl Default for FluidUi {
    fn default() -> Self {
        let mut overlay = VectorUi::default();
        overlay.minimum = 1e-7;
        Self {
            config: Default::default(),
            state: None,
            timeline: Default::default(),
            overlay,
            kind: 8,
            show: true,
            run_steps: 10,
            error: String::new(),
            receiver: None,
            cancel: None,
            accept: false,
            progress: String::new(),
            component: 0,
            auto_range: true,
            range: 1.,
            force_key: None,
            force_time: None,
            probe_cache: None,
        }
    }
}
impl FluidUi {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn invalidate(&mut self) {
        self.state = None;
        self.timeline.clear();
        self.overlay.clear();
        self.accept = false;
        self.probe_cache = None;
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Relaxed);
        }
    }
    pub fn ready(&self, volume: Option<&Volume>) -> bool {
        self.show
            && self.accept
            && self
                .timeline
                .current()
                .zip(volume)
                .is_some_and(|(f, v)| f.is_current(v))
    }
    pub fn start(&mut self, volume: &Volume, context: &GpuContext, force: Option<FrozenForce>) {
        if self.busy() {
            return;
        }
        let key = if self.config.use_em_force {
            force.as_ref().map(|f| f.buffer.clone())
        } else {
            None
        };
        let continuing = self.force_key == key
            && self
                .state
                .as_ref()
                .is_some_and(|s| s.is_current(volume) && s.config() == &self.config);
        let old = if continuing {
            self.state.take()
        } else {
            self.invalidate();
            None
        };
        self.force_key = key;
        self.force_time = force.as_ref().map(|f| f.time_s);
        self.show = true;
        self.accept = true;
        let context = context.clone();
        let volume = volume.clone();
        let config = self.config.clone();
        let steps = self.run_steps;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let (tx, rx) = mpsc::sync_channel(2);
        self.receiver = Some(rx);
        self.error.clear();
        self.progress = "Preparing fluid geometry…".into();
        std::thread::spawn(move || {
            let run = || -> Result<Box<FluidState>, String> {
                let mut state = match old {
                    Some(s) => s,
                    None => FluidState::new(
                        context,
                        &volume,
                        config,
                        force.as_ref().map(|f| ForceField {
                            buffer: &f.buffer,
                            grid: &f.grid,
                        }),
                    )?,
                };
                if state.time_s == 0. {
                    tx.send(Event::Frame(state.snapshot()))
                        .map_err(|_| "Fluid analysis view closed")?;
                }
                for i in 0..steps {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Fluid computation cancelled.".into());
                    }
                    let _ = tx.send(Event::Progress(i + 1, steps));
                    state.step(&volume)?;
                    tx.send(Event::Frame(state.snapshot()))
                        .map_err(|_| "Fluid analysis view closed")?;
                }
                Ok(Box::new(state))
            };
            let _ = tx.send(Event::Done(run()));
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context, volume: Option<&Volume>) -> bool {
        let mut changed = false;
        let mut finished = false;
        if let Some(rx) = &self.receiver {
            loop {
                match rx.try_recv() {
                    Ok(Event::Progress(i, n)) => self.progress = format!("Fluid step {i}/{n}…"),
                    Ok(Event::Frame(frame)) => {
                        if self.accept {
                            self.timeline.push(Sample {
                                time_s: frame.time_s,
                                bytes: frame.bytes,
                                output: frame,
                            });
                            changed = true;
                        }
                    }
                    Ok(Event::Done(result)) => {
                        finished = true;
                        changed = true;
                        match result {
                            Ok(state)
                                if self.accept
                                    && volume.is_some_and(|v| state.is_current(v))
                                    && state.config() == &self.config =>
                            {
                                self.state = Some(*state);
                                self.error.clear();
                            }
                            Ok(_) => {
                                self.error =
                                    "Fluid inputs changed during computation; simulate again."
                                        .into()
                            }
                            Err(error) => self.error = error,
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !finished {
                            self.error = "Fluid worker ended unexpectedly".into();
                        }
                        finished = true;
                        break;
                    }
                }
            }
        }
        if finished {
            self.receiver = None;
            self.cancel = None;
            self.progress.clear();
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        changed
    }
    pub fn timeline_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let changed = self.timeline.controls(ui);
        if self.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&self.progress);
            });
        }
        changed
    }
    pub fn controls(
        &mut self,
        ui: &mut egui::Ui,
        materials: &[Material],
        force_time: Option<f64>,
    ) -> bool {
        ui.label("Fixed liquid region · impermeable/no-slip walls");
        ui.small("This stage computes velocity and pressure inside the initial liquid geometry. Moving surfaces and ejection are not enabled.");
        let mut changed = false;
        ui.add_enabled_ui(!self.busy(),|ui| {
            let selected_material=self.config.material_id;
            let name=materials.iter().find(|m|m.id==self.config.material_id).map(|m|m.name.as_str()).unwrap_or("Select fluid material");
            egui::ComboBox::from_label("Liquid material").selected_text(name).show_ui(ui,|ui| {
                for m in materials.iter().filter(|m|m.id>1 && (m.dynamic_viscosity_pa_s.is_some() || m.id==selected_material)) {changed|=ui.selectable_value(&mut self.config.material_id,m.id,&m.name).changed();}
            });
            let mut us=self.config.dt_s*1e6;
            if ui.add(egui::DragValue::new(&mut us).speed(1.).range(1e-5..=1e9).prefix("Δt ").suffix(" µs")).changed() {self.config.dt_s=us*1e-6;changed=true;}
            changed|=ui.checkbox(&mut self.config.use_em_force,"Apply frozen EM force frame").changed();
            if self.config.use_em_force {
                if self.state.is_some() && self.force_time!=force_time {ui.small("Selected EM frame changed; Simulate restarts fluid history with that force.");}
                ui.small(force_time.map(|t|format!("Selected EM frame: {:.3} µs",t*1e6)).unwrap_or("Compute EM and select a force frame first.".into()));}
            egui::CollapsingHeader::new("Fluid driving and numerical settings").show(ui,|ui| {
                for (axis,v) in ["X","Y","Z"].into_iter().zip(&mut self.config.acceleration_m_s2) {changed|=ui.add(egui::DragValue::new(v).speed(0.1).prefix(format!("Acceleration {axis} ")).suffix(" m/s²")).changed();}
                for (axis,v) in ["X","Y","Z"].into_iter().zip(&mut self.config.initial_velocity_m_s) {changed|=ui.add(egui::DragValue::new(v).speed(0.001).prefix(format!("Initial velocity {axis} ")).suffix(" m/s")).changed();}
                for (axis,periodic) in ["X","Y","Z"].into_iter().zip(&mut self.config.periodic) {changed|=ui.checkbox(periodic,format!("Periodic {axis} boundaries")).on_hover_text("Connect fluid cells on opposite domain faces; use for matching periodic geometry.").changed();}
                changed|=ui.checkbox(&mut self.config.advection,"Velocity advection").changed();
                changed|=ui.add(egui::DragValue::new(&mut self.config.relative_tolerance).speed(1e-6).range(1e-7..=1e-2).prefix("Pressure relative tolerance ")).changed();
                changed|=ui.add(egui::DragValue::new(&mut self.config.absolute_divergence_tolerance).speed(1e-6).range(0. ..=1.).prefix("Divergence tolerance ").suffix(" s⁻¹")).changed();
                changed|=ui.add(egui::DragValue::new(&mut self.config.max_iterations).range(1..=20000).prefix("Max pressure iterations ")).changed();
                ui.small("Explicit viscosity and velocity CFL limits are checked each step. Density and liquid viscosity come from the selected material.");
            });
        });
        if changed {
            self.invalidate();
            self.error.clear();
        }
        if self.busy() {
            ui.spinner();
            ui.label(&self.progress);
            if ui.button("Cancel after current step").clicked() {
                if let Some(c) = &self.cancel {
                    c.store(true, Ordering::Relaxed);
                }
            }
        }
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
        changed
    }
    fn scale(&self) -> f32 {
        if !self.auto_range {
            return self.range.abs().max(1e-30);
        }
        self.timeline
            .current()
            .map(|f| match self.kind {
                7 => f.diagnostics.max_speed_m_s,
                8 => f.diagnostics.max_pressure_pa,
                _ => f.diagnostics.max_divergence_s_inv,
            })
            .unwrap_or(1.)
            .max(1e-30)
    }
    pub fn view(&self, volume: Option<&Volume>) -> Option<FieldView<'_>> {
        if !self.ready(volume) {
            return None;
        }
        self.timeline.current().map(|f| FieldView {
            buffer: f.result_buffer(),
            kind: self.kind,
            component: self.component,
            range: self.scale(),
            vectors: &self.overlay.arrows,
            trails: self.overlay.draw(),
        })
    }
    pub fn visualization_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        if self.kind == 7 {
            egui::ComboBox::from_label("Component")
                .selected_text(["Magnitude", "X", "Y", "Z"][self.component as usize])
                .show_ui(ui, |ui| {
                    for (i, name) in ["Magnitude", "X", "Y", "Z"].into_iter().enumerate() {
                        changed |= ui
                            .selectable_value(&mut self.component, i as u32, name)
                            .changed();
                    }
                });
        }
        changed |= ui
            .checkbox(&mut self.auto_range, "Automatic colour range")
            .changed();
        if !self.auto_range {
            let speed = (self.range as f64 * 0.02).max(1e-6);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.range)
                        .speed(speed)
                        .prefix("Range ")
                        .suffix(unit(self.kind)),
                )
                .changed();
        }
        changed |= self.overlay.controls(ui, "m/s");
        ui.small("Trails/arrows use velocity; the background can show pressure or divergence.");
        changed
    }
    pub fn update_vectors(
        &mut self,
        time: f64,
        plane: SectionPlane,
        span: f32,
        context: &GpuContext,
        dirty: bool,
    ) {
        if !self.show {
            return;
        }
        let Some(frame) = self.timeline.current() else {
            return;
        };
        if let Err(error) = self.overlay.update(
            time,
            context,
            VectorField {
                buffer: frame.result_buffer(),
                grid: &frame.grid,
                layout: VectorLayout {
                    stride: 4,
                    offset: 0,
                    subtract: None,
                    interpolate: true,
                    validity: Some((3, 0)),
                },
            },
            plane,
            span,
            frame.diagnostics.max_speed_m_s,
            dirty,
        ) {
            self.error = error;
        }
    }
    pub fn panel(&self, ui: &mut egui::Ui) {
        let Some(frame) = self.timeline.current() else {
            return;
        };
        let d = &frame.diagnostics;
        ui.label(format!(
            "{} · {:.3} µs",
            label(self.kind),
            frame.time_s * 1e6
        ));
        crate::vector_ui::color_legend(
            ui,
            self.scale(),
            unit(self.kind),
            self.kind >= 8 || self.component > 0,
        );
        if self.kind == 8 {
            ui.small("Gauge pressure: one zero reference per connected fluid region.");
        }
        ui.monospace(format!("Speed {:.3e} m/s\nPressure |p| {:.3e} Pa\nmax |div u| {:.3e} s⁻¹\nrms div u {:.3e} s⁻¹\nNet flux {:.3e} m³/s\nKinetic energy {:.3e} J\nLiquid mass {:.3e} kg",d.max_speed_m_s,d.max_pressure_pa,d.max_divergence_s_inv,d.rms_divergence_s_inv,d.net_flux_m3_s,d.kinetic_energy_j,d.fluid_mass_kg));
        ui.small(format!(
            "Pressure solve: {} iterations · rel residual {:.2e} · {:.1} ms",
            d.iterations, d.relative_residual, d.elapsed_ms
        ));
        if self.config.use_em_force {
            ui.small(format!(
                "Frozen EM forcing at {:.3} µs",
                self.force_time.unwrap_or(0.) * 1e6
            ));
        }
    }
    pub fn hover_probe(&mut self, ui: &mut egui::Ui, point: [f32; 3]) {
        let Some(frame) = self.timeline.current() else {
            return;
        };
        let key = (point.map(f32::to_bits), frame.time_s.to_bits());
        if self
            .probe_cache
            .as_ref()
            .is_none_or(|(p, t, _)| (*p, *t) != key)
        {
            self.probe_cache = Some((key.0, key.1, frame.probe(point).ok().flatten()));
        }
        if let Some((_, _, Some(cell))) = self.probe_cache {
            if cell.reserved[0] < 0.5 {
                ui.small("Outside the selected liquid region");
                return;
            }
            ui.monospace(format!(
                "|u| {:.3e} m/s · p {:.3e} Pa\ndiv u {:.3e} s⁻¹",
                cell.velocity[3], cell.pressure[0], cell.pressure[1]
            ));
        }
    }
}
pub fn label(kind: u32) -> &'static str {
    match kind {
        7 => "Fluid velocity",
        8 => "Fluid pressure (gauge)",
        _ => "Velocity divergence",
    }
}
fn unit(kind: u32) -> &'static str {
    match kind {
        7 => "m/s",
        8 => "Pa",
        _ => "s⁻¹",
    }
}
