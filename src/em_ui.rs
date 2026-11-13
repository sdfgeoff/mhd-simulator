use crate::{
    domain::Volume,
    em::{self, CellField, EmConfig, EmFrame, Mode, Waveform},
    flow::{VectorField, VectorLayout},
    gpu::GpuContext,
    model::Model,
    plane::SectionPlane,
    render::FieldView,
    vector_ui::VectorUi,
};
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use glam::Vec3;
struct Trace {
    time: f64,
    field: CellField,
}
pub struct EmUi {
    pub config: EmConfig,
    pub show: bool,
    pub kind: u32,
    component: u32,
    auto_range: bool,
    range: f32,
    pub overlay: VectorUi,
    pub source_lines: Vec<[Vec3; 2]>,
    probe_mm: [f32; 3],
    history: Vec<Trace>,
    history_key: Option<([u32; 3], usize, u64, u64)>,
    hover_cache: Option<([u32; 3], u64, Option<CellField>)>,
    pub timeline: crate::timeline::Timeline<EmFrame>,
    accept_frames: bool,
    pub error: String,
}
impl Default for EmUi {
    fn default() -> Self {
        Self {
            config: EmConfig::default(),
            show: true,
            kind: 1,
            component: 0,
            auto_range: true,
            range: 1e-3,
            overlay: VectorUi::default(),
            source_lines: Vec::new(),
            probe_mm: [1., 0., 4.],
            history: Vec::new(),
            history_key: None,
            hover_cache: None,
            timeline: Default::default(),
            accept_frames: false,
            error: String::new(),
        }
    }
}
impl EmUi {
    pub fn invalidate(&mut self) {
        self.overlay.clear();
        self.timeline.clear();
        self.accept_frames = false;
        self.history.clear();
        self.history_key = None;
        self.hover_cache = None;
    }
    pub fn record(&mut self, frame: EmFrame) {
        self.accept_frames = true;
        self.timeline.push(crate::timeline::Sample {
            time_s: frame.time_s,
            bytes: frame.bytes,
            output: frame,
        });
    }
    pub fn ready(&self, volume: Option<&Volume>) -> bool {
        self.show
            && self.accept_frames
            && self
                .timeline
                .current()
                .zip(volume)
                .is_some_and(|(frame, volume)| frame.is_current(volume))
    }
    fn scale(&self) -> f32 {
        if !self.auto_range {
            return self.range.max(1e-30);
        }
        self.timeline
            .current()
            .map(|s| match self.kind {
                1 => s.diagnostics.maxima[0],
                2 => s.diagnostics.applied_max_t,
                3 => s.diagnostics.reaction_max_t,
                4 => s.diagnostics.maxima[1],
                5 => s.diagnostics.maxima[2],
                _ => s.diagnostics.maxima[3],
            })
            .unwrap_or(1.)
            .max(1e-30)
    }
    pub fn view(&self, volume: Option<&Volume>) -> Option<FieldView<'_>> {
        if !self.ready(volume) {
            return None;
        }
        self.timeline.current().map(|s| FieldView {
            buffer: s.result_buffer(),
            kind: self.kind,
            component: self.component,
            range: self.scale(),
            vectors: &self.overlay.arrows,
            trails: if self.kind < 6 {
                self.overlay.draw()
            } else {
                None
            },
        })
    }
    pub fn update_source_lines(&mut self, model: &Model) {
        self.source_lines = self
            .config
            .wires
            .iter()
            .flat_map(|w| {
                w.points(model)
                    .unwrap_or_default()
                    .windows(2)
                    .map(|p| [Vec3::from(p[0]), Vec3::from(p[1])])
                    .collect::<Vec<_>>()
            })
            .collect();
    }
    pub fn animation_running(&self, volume: Option<&Volume>) -> bool {
        self.ready(volume) && self.kind < 6 && self.overlay.running()
    }
    pub fn update_trails(
        &mut self,
        time: f64,
        plane: SectionPlane,
        span: f32,
        context: &GpuContext,
        dirty: bool,
    ) {
        if !self.show || self.kind >= 6 {
            return;
        }
        let Some(frame) = self.timeline.current() else {
            return;
        };
        let layout = VectorLayout {
            stride: 4,
            offset: match self.kind {
                2 => 3,
                4 => 1,
                5 => 2,
                _ => 0,
            },
            subtract: (self.kind == 3).then_some(3),
            interpolate: self.kind <= 3,
            validity: None,
        };
        let scale = self.scale();
        if let Err(error) = self.overlay.update(
            time,
            context,
            VectorField {
                buffer: frame.result_buffer(),
                grid: frame.grid(),
                layout,
            },
            plane,
            span,
            scale,
            dirty,
        ) {
            self.error = error;
        }
    }
    pub fn curve_properties(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        selected: Option<usize>,
    ) -> bool {
        let mut changed = false;
        for (index, curve) in model
            .curves
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) == selected)
        {
            let title = format!("Curve · {}", curve.name);
            egui::CollapsingHeader::new(title)
                .id_salt(("cad_curve", index)).default_open(true)
                .show(ui, |ui| {
                    let existing = self.config.wires.iter().position(|wire| wire.cad_curve == index);
                    let mut enabled = existing.is_some();
                    if ui.checkbox(&mut enabled, "Carry prescribed current").changed() {
                        if enabled {
                            self.config.wires.push(em::Wire::for_curve(index, curve));
                        } else if let Some(i) = existing {
                            self.config.wires.remove(i);
                        }
                        changed = true;
                    }
                    ui.small(format!("{} CAD points · positive current follows CAD curve direction", curve.points_mm.len()));
                    let Some(wire) = self.config.wires.iter_mut().find(|wire| wire.cad_curve == index) else { return; };
                    changed |= waveform(ui, &mut wire.current, " A");
                    changed |= ui.add(egui::DragValue::new(&mut wire.core_radius_mm)
                        .range(1e-4..=100.).speed(0.01).prefix("Wire/core radius ").suffix(" mm")).changed();
                    changed |= ui.checkbox(&mut wire.reverse, "Reverse current direction").changed();
                    if let Ok(points) = wire.points(model) {
                        let open = points.len() > 1 && Vec3::from(points[0]).distance(Vec3::from(*points.last().unwrap())) > 1e-4;
                        if open {
                            ui.colored_label(Color32::YELLOW, "Open CAD path: include the physical return lead in the CAD for induction.");
                        }
                    }
                });
        }
        if changed {
            self.invalidate();
            self.error.clear();
        }
        changed
    }
    pub fn body_source_properties(&mut self, ui: &mut egui::Ui, body: usize, closed: bool) -> bool {
        let existing = self.config.magnets.iter().position(|m| m.body == body);
        let mut enabled = existing.is_some();
        let mut changed = false;
        if ui
            .add_enabled(
                closed,
                egui::Checkbox::new(&mut enabled, "Permanent magnet source"),
            )
            .changed()
        {
            if enabled {
                self.config.magnets.push(em::Magnet {
                    body,
                    magnetization_a_m: [0., 0., 8e5],
                    subdivisions: 1,
                    smoothing_mm: 0.05,
                });
            } else {
                self.config.magnets.retain(|m| m.body != body);
            }
            changed = true;
        }
        if let Some(magnet) = self.config.magnets.iter_mut().find(|m| m.body == body) {
            ui.label("Magnetization M (A/m), world XYZ");
            for (axis, v) in ["X", "Y", "Z"]
                .into_iter()
                .zip(&mut magnet.magnetization_a_m)
            {
                changed |= ui
                    .add(
                        egui::DragValue::new(v)
                            .speed(1e4)
                            .prefix(format!("{axis} ")),
                    )
                    .changed();
            }
            egui::CollapsingHeader::new("Magnet discretization").show(ui, |ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut magnet.subdivisions)
                            .range(0..=4)
                            .prefix("Surface refinement "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut magnet.smoothing_mm)
                            .range(1e-4..=100.)
                            .speed(0.01)
                            .suffix(" mm smoothing"),
                    )
                    .changed();
            });
        }
        if changed {
            self.invalidate();
            self.error.clear();
        }
        changed
    }
    pub fn controls(&mut self, ui: &mut egui::Ui) -> bool {
        let old = self.config.clone();
        ui.add(
            egui::DragValue::new(&mut self.config.relative_tolerance)
                .speed(1e-6)
                .range(1e-7..=1e-2)
                .prefix("EM relative residual "),
        );
        ui.add(
            egui::DragValue::new(&mut self.config.max_iterations)
                .range(1..=20000)
                .prefix("EM max iterations "),
        );
        self.config != old
    }
    pub fn visualization_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        if self.kind < 6 {
            egui::ComboBox::from_label("Component")
                .selected_text(["Magnitude", "X", "Y", "Z"][self.component as usize])
                .show_ui(ui, |ui| {
                    for (i, label) in ["Magnitude", "X", "Y", "Z"].iter().enumerate() {
                        changed |= ui
                            .selectable_value(&mut self.component, i as u32, *label)
                            .changed();
                    }
                });
        }
        changed |= ui
            .checkbox(&mut self.auto_range, "Automatic colour range")
            .changed();
        if !self.auto_range {
            let speed = (self.range.abs() * 0.02).max(1e-9);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.range)
                        .speed(speed)
                        .prefix("Range ")
                        .suffix(unit(self.kind)),
                )
                .changed();
        }
        if self.kind < 6 {
            changed |= self.overlay.controls(ui, unit(self.kind));
        }
        changed
    }
    pub fn panel(&mut self, ui: &mut egui::Ui) {
        let Some(state) = self.timeline.current() else {
            return;
        };
        ui.label(if state.mode == Mode::Applied {
            "Applied-only · vacuum"
        } else {
            "Transient induction"
        });
        ui.label(format!(
            "{} · {}",
            label(self.kind),
            if self.kind == 6 {
                "Scalar"
            } else {
                ["Magnitude", "X", "Y", "Z"][self.component as usize]
            }
        ));
        ui.label(format!(
            "t = {:.3} µs · {} iterations",
            state.time_s * 1e6,
            state.diagnostics.iterations
        ));
        ui.small(format!(
            "Residual {:.2e} · {:.1} ms · {:.1} MiB",
            state.diagnostics.relative_residual,
            state.diagnostics.elapsed_ms,
            state.bytes as f64 / 1048576.
        ));
        crate::vector_ui::color_legend(
            ui,
            self.scale(),
            unit(self.kind),
            self.component > 0 && self.kind < 6,
        );
        egui::CollapsingHeader::new("Force / power / conservation").show(ui, |ui| {
            let d = &state.diagnostics;
            ui.monospace(format!(
                "F = [{:.3e}, {:.3e}, {:.3e}] N\nPower = {:.3e} W",
                d.force_n[0], d.force_n[1], d.force_n[2], d.power_w
            ));
            for body in &state.bodies {
                ui.label(&body.name);
                ui.monospace(format!(
                    "F [{:.3e}, {:.3e}, {:.3e}] N\nP {:.3e} W",
                    body.force_n[0], body.force_n[1], body.force_n[2], body.power_w
                ));
            }
            ui.small(format!(
                "max |div J| h / max edge J = {:.2e}",
                d.divergence_j_scaled / d.max_edge_j.max(1e-30)
            ));
            ui.small(format!(
                "max |div B| h / max B = {:.2e}",
                d.divergence_b_scaled / d.maxima[0].max(1e-30)
            ));
        });
        ui.separator();
        ui.label("Probe position (mm), nearest cell");
        let mut moved = false;
        ui.horizontal(|ui| {
            for p in &mut self.probe_mm {
                moved |= ui.add(egui::DragValue::new(p).speed(0.1)).changed();
            }
        });
        let key = (
            self.probe_mm.map(f32::to_bits),
            self.timeline.samples.len(),
            self.timeline
                .samples
                .first()
                .map(|s| s.time_s.to_bits())
                .unwrap_or(0),
            self.timeline
                .samples
                .last()
                .map(|s| s.time_s.to_bits())
                .unwrap_or(0),
        );
        if moved || self.history_key != Some(key) {
            self.history_key = Some(key);
            self.history = self
                .timeline
                .samples
                .iter()
                .filter_map(|s| {
                    s.output
                        .probe(self.probe_mm)
                        .ok()
                        .flatten()
                        .map(|field| Trace {
                            time: s.time_s,
                            field,
                        })
                })
                .collect();
        }
        if let Some(trace) = self.history.iter().find(|t| t.time == state.time_s) {
            show_probe(ui, trace.field);
        }
        if self.history.len() > 1 {
            ui.label(format!("{} probe history", label(self.kind)));
            let (r, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 90.), egui::Sense::hover());
            let mut values = Vec::new();
            for t in &self.history {
                values.push(value(t.field, self.kind, self.component));
            }
            let min = values.iter().copied().fold(0f32, f32::min);
            let max = values.iter().copied().fold(0f32, f32::max);
            let t0 = self.history[0].time;
            let t1 = self.history.last().unwrap().time;
            let points: Vec<_> = values
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    Pos2::new(
                        r.left()
                            + r.width()
                                * ((self.history[i].time - t0) / (t1 - t0).max(1e-30)) as f32,
                        r.bottom() - r.height() * (*v - min) / (max - min).max(1e-30),
                    )
                })
                .collect();
            let x = r.left() + r.width() * ((state.time_s - t0) / (t1 - t0).max(1e-30)) as f32;
            ui.painter().line_segment(
                [Pos2::new(x, r.top()), Pos2::new(x, r.bottom())],
                Stroke::new(1., Color32::WHITE),
            );
            ui.painter().add(egui::Shape::line(
                points,
                Stroke::new(1.5, Color32::from_rgb(75, 196, 208)),
            ));
            ui.small(format!(
                "{:.2}…{:.2} µs; {min:.2e}…{max:.2e} {}",
                t0 * 1e6,
                t1 * 1e6,
                unit(self.kind)
            ));
        }
    }
    pub fn hover_probe(&mut self, ui: &mut egui::Ui, point: [f32; 3]) {
        if let Some(frame) = self.timeline.current() {
            let key = (point.map(f32::to_bits), frame.time_s.to_bits());
            if self
                .hover_cache
                .as_ref()
                .is_none_or(|(p, t, _)| (*p, *t) != key)
            {
                self.hover_cache = Some((key.0, key.1, frame.probe(point).ok().flatten()));
            }
            if let Some((_, _, Some(field))) = self.hover_cache {
                show_probe(ui, field);
            }
        }
    }
}
fn show_probe(ui: &mut egui::Ui, p: CellField) {
    ui.monospace(format!(
        "|B| {:.3e} T · |J| {:.3e} A/m²\n|f| {:.3e} N/m³ · q {:.3e} W/m³",
        p.b[3],
        p.j[3],
        Vec3::new(p.force_heat[0], p.force_heat[1], p.force_heat[2]).length(),
        p.force_heat[3]
    ));
}
pub fn label(kind: u32) -> &'static str {
    match kind {
        1 => "Total B",
        2 => "Applied B (vacuum)",
        3 => "Reaction B",
        4 => "Induced J",
        5 => "Lorentz J×B",
        _ => "Joule heat",
    }
}
fn unit(kind: u32) -> &'static str {
    match kind {
        1..=3 => "T",
        4 => "A/m²",
        5 => "N/m³",
        _ => "W/m³",
    }
}
fn value(f: CellField, kind: u32, component: u32) -> f32 {
    let v = match kind {
        1 => Vec3::new(f.b[0], f.b[1], f.b[2]),
        2 => Vec3::new(f.applied[0], f.applied[1], f.applied[2]),
        3 => Vec3::new(
            f.b[0] - f.applied[0],
            f.b[1] - f.applied[1],
            f.b[2] - f.applied[2],
        ),
        4 => Vec3::new(f.j[0], f.j[1], f.j[2]),
        5 => Vec3::new(f.force_heat[0], f.force_heat[1], f.force_heat[2]),
        _ => return f.force_heat[3],
    };
    if component == 0 {
        v.length()
    } else {
        v[component as usize - 1]
    }
}
fn waveform(ui: &mut egui::Ui, w: &mut Waveform, unit: &str) -> bool {
    let mut changed = false;
    let mut kind = match w {
        Waveform::Constant { .. } => 0,
        Waveform::Sine { .. } => 1,
        Waveform::Pulse { .. } => 2,
        Waveform::Table { .. } => 3,
    };
    let before = kind;
    egui::ComboBox::from_id_salt("waveform")
        .selected_text(["Constant", "Sine", "Pulse", "Table"][kind])
        .show_ui(ui, |ui| {
            for (i, n) in ["Constant", "Sine", "Pulse", "Table"].iter().enumerate() {
                ui.selectable_value(&mut kind, i, *n);
            }
        });
    if kind != before {
        *w = match kind {
            0 => Waveform::Constant { value: 100. },
            1 => Waveform::Sine {
                amplitude: 100.,
                frequency_hz: 1000.,
                phase_rad: 0.,
            },
            2 => Waveform::Pulse {
                peak: 100.,
                delay_s: 0.,
                rise_s: 50e-6,
                hold_s: 50e-6,
                fall_s: 50e-6,
            },
            _ => Waveform::Table {
                samples: vec![[0., 0.], [50e-6, 100.], [150e-6, 0.]],
            },
        };
        changed = true;
    }
    match w {
        Waveform::Constant { value } => {
            changed |= ui
                .add(egui::DragValue::new(value).speed(1.).suffix(unit))
                .changed();
        }
        Waveform::Sine {
            amplitude,
            frequency_hz,
            phase_rad,
        } => {
            changed |= ui
                .add(egui::DragValue::new(amplitude).speed(1.).suffix(unit))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(frequency_hz).speed(100.).suffix(" Hz"))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(phase_rad).speed(0.1).suffix(" rad"))
                .changed();
        }
        Waveform::Pulse {
            peak,
            delay_s,
            rise_s,
            hold_s,
            fall_s,
        } => {
            changed |= ui
                .add(egui::DragValue::new(peak).speed(1.).suffix(unit))
                .changed();
            for (name, time) in [
                ("Delay", delay_s),
                ("Rise", rise_s),
                ("Hold", hold_s),
                ("Fall", fall_s),
            ] {
                let mut us = *time * 1e6;
                if ui
                    .add(
                        egui::DragValue::new(&mut us)
                            .speed(1.)
                            .prefix(format!("{name} "))
                            .suffix(" µs"),
                    )
                    .changed()
                {
                    *time = us * 1e-6;
                    changed = true;
                }
            }
        }
        Waveform::Table { samples } => {
            ui.small("Time/current samples; times must increase strictly.");
            let count = samples.len();
            let mut remove = None;
            for (i, sample) in samples.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal(|ui| {
                        let mut time_us = sample[0] * 1e6;
                        if ui
                            .add(egui::DragValue::new(&mut time_us).speed(1.).suffix(" µs"))
                            .changed()
                        {
                            sample[0] = time_us * 1e-6;
                            changed = true;
                        }
                        changed |= ui
                            .add(egui::DragValue::new(&mut sample[1]).speed(1.).suffix(unit))
                            .changed();
                        if ui.add_enabled(count > 2, egui::Button::new("×")).clicked() {
                            remove = Some(i);
                        }
                    })
                });
            }
            if let Some(i) = remove {
                samples.remove(i);
                changed = true;
            }
            if ui.button("Add sample").clicked() {
                let last = samples.last().copied().unwrap_or([0., 0.]);
                samples.push([last[0] + 50e-6, last[1]]);
                changed = true;
            }
        }
    }
    changed
}
