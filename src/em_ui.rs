use crate::{
    domain::Volume,
    em::{self, CellField, EmConfig, EmState, Mode, Waveform},
    gpu::GpuContext,
    model::Model,
    plane::SectionPlane,
    render::FieldView,
};
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use glam::Vec3;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
struct Trace {
    time: f64,
    field: CellField,
}
enum Event {
    Progress(u32, u32),
    Done(Result<(EmState, Vec<Trace>), String>),
}
pub struct EmUi {
    pub config: EmConfig,
    pub state: Option<EmState>,
    receiver: Option<mpsc::Receiver<Event>>,
    cancel: Option<Arc<AtomicBool>>,
    progress: String,
    pub show: bool,
    kind: u32,
    component: u32,
    auto_range: bool,
    range: f32,
    pub arrows: bool,
    vectors: Vec<[Vec3; 2]>,
    pub source_lines: Vec<[Vec3; 2]>,
    probe_mm: [f32; 3],
    history: Vec<Trace>,
    pub error: String,
    radius_mm: f32,
    height_mm: f32,
    turns: u32,
    json: String,
    run_steps: u32,
    preview_time_us: f64,
}
impl Default for EmUi {
    fn default() -> Self {
        Self {
            config: EmConfig::default(),
            state: None,
            receiver: None,
            cancel: None,
            progress: String::new(),
            show: true,
            kind: 1,
            component: 0,
            auto_range: true,
            range: 1e-3,
            arrows: false,
            vectors: Vec::new(),
            source_lines: Vec::new(),
            probe_mm: [1., 0., 4.],
            history: Vec::new(),
            error: String::new(),
            radius_mm: 5.,
            height_mm: 8.,
            turns: 4,
            json: String::new(),
            run_steps: 10,
            preview_time_us: 50.,
        }
    }
}
impl EmUi {
    pub fn invalidate(&mut self) {
        self.state = None;
        self.history.clear();
        self.vectors.clear();
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Relaxed);
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context, volume: Option<&Volume>) -> bool {
        let mut changed = false;
        let mut finished = false;
        if let Some(rx) = &self.receiver {
            loop {
                let event = match rx.try_recv() {
                    Ok(e) => e,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !finished {
                            self.error = "Electromagnetic worker ended unexpectedly.".into();
                            changed = true;
                        }
                        finished = true;
                        break;
                    }
                };
                match event {
                    Event::Progress(i, n) => self.progress = format!("Computing step {i}/{n}…"),
                    Event::Done(result) => {
                        finished = true;
                        changed = true;
                        match result {
                            Ok((state, trace)) => {
                                if volume.is_some_and(|v| state.is_current(v))
                                    && state.config() == &self.config
                                {
                                    if state.time_s == 0. {
                                        self.history.clear();
                                    }
                                    self.history.extend(trace);
                                    if self.history.len() > 512 {
                                        self.history.drain(..self.history.len() - 512);
                                    }
                                    self.show = true;
                                    self.state = Some(state);
                                    self.error.clear();
                                } else {
                                    self.error =
                                        "Inputs changed during computation; initialize again."
                                            .into();
                                }
                            }
                            Err(e) => self.error = e,
                        }
                    }
                }
            }
        }
        if finished {
            self.receiver = None;
            self.cancel = None;
            self.progress.clear();
        }
        if self.receiver.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        changed
    }
    pub(crate) fn start(
        &mut self,
        mode: Mode,
        steps: u32,
        model: &Model,
        volume: &Volume,
        context: &GpuContext,
    ) {
        self.show = true;
        if mode == Mode::Applied {
            self.kind = 2;
        }
        let old = if steps > 0 { self.state.take() } else { None };
        let config = self.config.clone();
        let model = model.clone();
        let volume = volume.clone();
        let context = context.clone();
        let probe = self.probe_mm;
        let preview_time = self.preview_time_us * 1e-6;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.error.clear();
        self.progress = "Preparing GPU electromagnetics…".into();
        std::thread::spawn(move || {
            let run = || -> Result<(EmState, Vec<Trace>), String> {
                let mut state = match old {
                    Some(s) if s.mode == mode && s.config() == &config && s.is_current(&volume) => {
                        s
                    }
                    _ => EmState::new(context, &volume, &model, config, mode)?,
                };
                if cancel.load(Ordering::Relaxed) {
                    return Err("Electromagnetic computation cancelled.".into());
                }
                if steps == 0 && mode == Mode::Applied {
                    state.evaluate_applied(&model, preview_time, &volume)?;
                }
                let mut trace = Vec::new();
                if steps == 0 {
                    if let Some(field) = state.probe(probe)? {
                        trace.push(Trace {
                            time: state.time_s,
                            field,
                        });
                    }
                }
                for i in 0..steps {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Electromagnetic computation cancelled.".into());
                    }
                    let _ = tx.send(Event::Progress(i + 1, steps));
                    if mode == Mode::Induction {
                        state.step(&model, &volume)?;
                    } else {
                        let t = state.time_s + state.config().dt_s;
                        state.evaluate_applied(&model, t, &volume)?;
                    }
                    if let Some(field) = state.probe(probe)? {
                        trace.push(Trace {
                            time: state.time_s,
                            field,
                        });
                    }
                }
                Ok((state, trace))
            };
            let _ = tx.send(Event::Done(run()));
        });
    }
    pub fn ready(&self, volume: Option<&Volume>) -> bool {
        self.show
            && self
                .state
                .as_ref()
                .zip(volume)
                .is_some_and(|(s, v)| s.is_current(v))
    }
    fn scale(&self) -> f32 {
        if !self.auto_range {
            return self.range.max(1e-30);
        }
        self.state
            .as_ref()
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
        self.state.as_ref().map(|s| FieldView {
            buffer: s.result_buffer(),
            kind: self.kind,
            component: self.component,
            range: self.scale(),
            vectors: &self.vectors,
        })
    }
    pub fn update_vectors(&mut self, plane: SectionPlane, span: f32) {
        self.vectors.clear();
        self.source_lines = self
            .config
            .wires
            .iter()
            .flat_map(|w| {
                w.points_mm
                    .windows(2)
                    .map(|p| [Vec3::from(p[0]), Vec3::from(p[1])])
            })
            .collect();
        if !self.arrows || self.kind == 6 {
            return;
        }
        let Some(state) = &self.state else {
            return;
        };
        let n = 11;
        match state.sample_plane(plane, span, n, self.kind) {
            Ok(samples) => {
                for y in 0..n {
                    for x in 0..n {
                        let v = samples[(y * n + x) as usize];
                        let v = Vec3::new(v[0], v[1], v[2]);
                        let magnitude = v.length();
                        if magnitude <= self.scale() * 1e-6 {
                            continue;
                        }
                        let start = plane.position
                            + plane.u() * (((x as f32 + 0.5) / n as f32 * 2. - 1.) * span)
                            + plane.v() * (((y as f32 + 0.5) / n as f32 * 2. - 1.) * span);
                        let direction = v / magnitude;
                        let end =
                            start + direction * (span * 0.11 * (magnitude / self.scale()).min(1.));
                        self.vectors.push([start, end]);
                        let side = direction
                            .cross(plane.normal())
                            .try_normalize()
                            .unwrap_or(plane.u());
                        let head = (end - start).length() * 0.25;
                        self.vectors
                            .push([end, end - direction * head + side * head * 0.45]);
                        self.vectors
                            .push([end, end - direction * head - side * head * 0.45]);
                    }
                }
            }
            Err(e) => self.error = e,
        }
    }
    /// Returns (view changed, apply illustrative demo materials).
    pub fn controls(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        volume: Option<&Volume>,
        context: Option<&GpuContext>,
    ) -> (bool, bool) {
        ui.separator();
        ui.heading("05 / Electromagnetics");
        ui.small("Stationary geometry · linear materials · vacuum background");
        ui.small("Reaction A is zero at the outer box. Compare results with more air padding before interpreting forces.");
        let busy = self.receiver.is_some();
        let mut changed = false;
        let mut config_changed = false;
        let mut demo = false;
        ui.add_enabled_ui(!busy,|ui|{

            egui::CollapsingHeader::new("Sources").default_open(self.config.wires.is_empty()).show(ui,|ui|{

                ui.horizontal(|ui|{
ui.label("Coil R / height (mm)");
ui.add(egui::DragValue::new(&mut self.radius_mm).range(0.1..=100.).speed(0.1));
ui.add(egui::DragValue::new(&mut self.height_mm).range(0.1..=100.).speed(0.1));

}
);

                ui.horizontal(|ui|{
ui.label("Turns");
ui.add(egui::DragValue::new(&mut self.turns).range(1..=100));

}
);

                if ui.button("Add closed coil at model centre").clicked(){
let (lo,hi)=model.bounds();
self.config.wires.push(em::helix(((lo+hi)*0.5).to_array(),self.radius_mm,self.height_mm,self.turns,Waveform::Pulse{
peak:100.,delay_s:0.,rise_s:50e-6,hold_s:50e-6,fall_s:50e-6
}
));
config_changed=true;

}

                if ui.button("Use example coil + material inputs").clicked(){
let (lo,hi)=model.bounds();
self.config.wires=vec![em::helix(((lo+hi)*0.5).to_array(),5.,8.,4,Waveform::Pulse{
peak:100.,delay_s:0.,rise_s:50e-6,hold_s:50e-6,fall_s:50e-6
}
)];
self.config.magnets.clear();
self.config.uniform.clear();
config_changed=true;
demo=true;

}

                ui.small("Example solder conductivity is illustrative; replace it for quantitative predictions.");

                let mut remove=None;

                for (i,wire) in self.config.wires.iter_mut().enumerate() {
ui.push_id(("wire",i),|ui|{
egui::CollapsingHeader::new(&wire.name).show(ui,|ui|{

                    config_changed|=ui.text_edit_singleline(&mut wire.name).changed();
config_changed|=ui.add(egui::DragValue::new(&mut wire.core_radius_mm).speed(0.01).range(1e-4..=100.).prefix("Core/smoothing ").suffix(" mm")).changed();

                    let before=wire.body_current;

                    egui::ComboBox::from_id_salt("current_link").selected_text(wire.body_current.and_then(|i|model.bodies.get(i)).map(|b|b.name.as_str()).unwrap_or("Direct waveform (A)")).show_ui(ui,|ui|{
ui.selectable_value(&mut wire.body_current,None,"Direct waveform (A)");
for (i,b) in model.bodies.iter().enumerate(){
ui.selectable_value(&mut wire.body_current,Some(i),format!("× current: {}",b.name));

}

}
);

                    if before!=wire.body_current {
wire.current=Waveform::Constant{
value:if wire.body_current.is_some(){
1.
}
else{
100.
}

}
;
config_changed=true;

}

                    config_changed|=waveform(ui,&mut wire.current,if wire.body_current.is_some(){
"× component A"
}
else{
"A"
}
);

                    egui::CollapsingHeader::new(format!("{} path points (mm)",wire.points_mm.len())).show(ui,|ui|{
for p in &mut wire.points_mm {
ui.horizontal(|ui|{
for v in p {
config_changed|=ui.add(egui::DragValue::new(v).speed(0.1)).changed();

}

}
);

}

}
);

                    if ui.button("Remove wire").clicked(){
remove=Some(i);

}


}
);

}
);

}

                if let Some(i)=remove{
self.config.wires.remove(i);
config_changed=true;

}

                if !model.bodies.is_empty()&&ui.button("Add magnetized CAD body").clicked(){
self.config.magnets.push(em::Magnet{
body:model.bodies.iter().position(|b|b.name.to_ascii_lowercase().contains("magnet")).unwrap_or(0),magnetization_a_m:[0.,0.,8e5],subdivisions:1,smoothing_mm:0.05
}
);
config_changed=true;

}

                let mut remove=None;

                for (i,magnet) in self.config.magnets.iter_mut().enumerate() {
ui.push_id(("magnet",i),|ui|{
egui::CollapsingHeader::new(format!("Magnet {}",i+1)).show(ui,|ui|{

                    egui::ComboBox::from_id_salt("body").selected_text(model.bodies.get(magnet.body).map(|b|b.name.as_str()).unwrap_or("Missing body")).show_ui(ui,|ui|{
for (j,b) in model.bodies.iter().enumerate(){
config_changed|=ui.selectable_value(&mut magnet.body,j,&b.name).changed();

}

}
);

                    ui.label("Magnetization M (A/m), world XYZ");
for v in &mut magnet.magnetization_a_m {
config_changed|=ui.add(egui::DragValue::new(v).speed(1e4)).changed();

}

                    config_changed|=ui.add(egui::DragValue::new(&mut magnet.subdivisions).range(0..=4).prefix("Surface refinement ")).changed();
config_changed|=ui.add(egui::DragValue::new(&mut magnet.smoothing_mm).range(1e-4..=100.).speed(0.01).suffix(" mm smoothing")).changed();

                    if ui.button("Remove magnet").clicked(){
remove=Some(i);

}


}
);

}
);

}

                if let Some(i)=remove {
self.config.magnets.remove(i);
config_changed=true;

}

                egui::CollapsingHeader::new("Advanced source/settings JSON").show(ui,|ui|{

                    if ui.button("Read current settings").clicked(){
self.json=serde_json::to_string_pretty(&self.config).unwrap_or_default();

}

                    ui.add(egui::TextEdit::multiline(&mut self.json).desired_rows(6).code_editor());

                    if ui.button("Apply JSON").clicked(){
match serde_json::from_str::<EmConfig>(&self.json).map_err(|e|e.to_string()).and_then(|c|{
c.validate(model)?;
Ok(c)
}
) {
Ok(c)=>{
self.config=c;
config_changed=true;

}
,Err(e)=>self.error=e
}

}


}
);


}
);

            let mut dt_us=self.config.dt_s*1e6;

            if ui.add(egui::DragValue::new(&mut dt_us).speed(1.).range(1e-5..=1e8).prefix("Δt ").suffix(" µs")).changed(){
self.config.dt_s=dt_us*1e-6;
config_changed=true;

}

            egui::CollapsingHeader::new("Numerical solve").show(ui,|ui|{
config_changed|=ui.add(egui::DragValue::new(&mut self.config.relative_tolerance).speed(1e-6).range(1e-7..=1e-2).prefix("Relative residual ")).changed();
config_changed|=ui.add(egui::DragValue::new(&mut self.config.max_iterations).range(1..=20000).prefix("Max iterations ")).changed();

}
);

            if config_changed {
self.invalidate();
self.error.clear();
changed=true;

}

            if let (Some(volume),Some(context))=(volume,context) {

                ui.add(egui::DragValue::new(&mut self.preview_time_us).speed(1.).range(0. ..=1e12).prefix("Applied preview ").suffix(" µs"));

                if ui.button("Compute applied B (vacuum)").clicked(){
self.start(Mode::Applied,0,model,volume,context);
changed=true;

}

                if ui.button("Initialize induction at t = 0").clicked(){
self.start(Mode::Induction,0,model,volume,context);
changed=true;

}

                if let Some(mode)=self.state.as_ref().map(|s|s.mode) {

                    ui.horizontal(|ui|{
if ui.button("Step").clicked(){
self.start(mode,1,model,volume,context);
changed=true;

}
ui.add(egui::DragValue::new(&mut self.run_steps).range(1..=10000));
if ui.button("Run steps").clicked(){
self.start(mode,self.run_steps,model,volume,context);
changed=true;

}

}
);


}


}
 else {
ui.label("Build the domain first.");

}


}
);

        if busy {
            ui.spinner();
            ui.label(&self.progress);
            if ui.button("Cancel after current step").clicked() {
                if let Some(c) = &self.cancel {
                    c.store(true, Ordering::Relaxed);
                }
            }
        }
        changed |= ui
            .checkbox(&mut self.show, "Show electromagnetic section")
            .changed();
        egui::ComboBox::from_label("Field")
            .selected_text(label(self.kind))
            .show_ui(ui, |ui| {
                for kind in 1..=6 {
                    changed |= ui
                        .selectable_value(&mut self.kind, kind, label(kind))
                        .changed();
                }
            });
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
        changed |= ui
            .checkbox(&mut self.arrows, "Vector arrows on plane")
            .changed();
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
        (changed, demo)
    }
    pub fn panel(&mut self, ui: &mut egui::Ui) {
        let Some(state) = &self.state else {
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
        let scale = self.scale();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 12.), egui::Sense::hover());
        for i in 0..64 {
            let t = i as f32 / 63.;
            let rgb = [
                (1.5 - (4. * t - 3.).abs()).clamp(0., 1.),
                (1.5 - (4. * t - 2.).abs()).clamp(0., 1.),
                (1.5 - (4. * t - 1.).abs()).clamp(0., 1.),
            ];
            let c = if self.component > 0 && self.kind < 6 {
                let x = t * 2. - 1.;
                let end = if x < 0. {
                    [0.12, 0.32, 0.95]
                } else {
                    [0.95, 0.18, 0.12]
                };
                std::array::from_fn(|j| 0.93 * (1. - x.abs()) + end[j] * x.abs())
            } else {
                rgb
            };
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    Pos2::new(rect.min.x + rect.width() * i as f32 / 64., rect.min.y),
                    Pos2::new(rect.min.x + rect.width() * (i + 1) as f32 / 64., rect.max.y),
                ),
                0.,
                Color32::from_rgb(
                    (c[0] * 255.) as u8,
                    (c[1] * 255.) as u8,
                    (c[2] * 255.) as u8,
                ),
            );
        }
        let min = if self.component > 0 && self.kind < 6 {
            -scale
        } else {
            0.
        };
        ui.small(format!("{min:.3e} … {scale:.3e} {}", unit(self.kind)));
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
        if moved {
            self.history.clear();
        }
        if let Ok(Some(p)) = state.probe(self.probe_mm) {
            show_probe(ui, p);
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
    pub fn hover_probe(&self, ui: &mut egui::Ui, point: [f32; 3]) {
        if let Some(state) = &self.state {
            if let Ok(Some(p)) = state.probe(point) {
                show_probe(ui, p);
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
fn label(kind: u32) -> &'static str {
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
        Waveform::Table { .. } => {
            ui.small("Edit time/current samples in advanced JSON.");
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_field_job_finishes_and_cancel_does_not_publish_results() {
        let _lock = crate::gpu_test_lock();
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let settings = crate::simulation::DomainSettings {
            min_mm: [-5.; 3],
            max_mm: [5.; 3],
            cell_mm: 1.,
        };
        let volume = gpu
            .build(&model, &settings, &crate::simulation::default_materials())
            .unwrap();
        let mut ui = EmUi::default();
        ui.config.uniform.push(em::UniformField {
            tesla: [0., 0., 0.001],
            scale: Waveform::Constant { value: 1. },
        });
        let context = egui::Context::default();
        let wait = |ui: &mut EmUi| {
            let start = std::time::Instant::now();
            while ui.receiver.is_some() {
                ui.poll(&context, Some(&volume));
                assert!(start.elapsed().as_secs() < 10, "worker did not finish");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        };
        ui.show = false;
        ui.start(Mode::Applied, 0, &model, &volume, &gpu.context);
        wait(&mut ui);
        assert!(ui.error.is_empty(), "{}", ui.error);
        assert!(ui.ready(Some(&volume)));
        assert!(ui.state.as_ref().unwrap().diagnostics.maxima[0] > 0.);
        ui.start(Mode::Applied, 1000, &model, &volume, &gpu.context);
        ui.cancel.as_ref().unwrap().store(true, Ordering::Relaxed);
        wait(&mut ui);
        assert!(ui.state.is_none());
        assert!(ui.error.contains("cancelled"));
    }
}
