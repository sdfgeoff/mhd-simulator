use crate::{
    domain::Volume,
    flow::{VectorField, VectorLayout},
    gpu::GpuContext,
    plane::SectionPlane,
    render::FieldView,
    thermal::{Boundary, ThermalCell, ThermalConfig, ThermalFrame},
    vector_ui::VectorUi,
};
use eframe::egui;
pub struct ThermalUi {
    pub kind: u32,
    pub show: bool,
    pub overlay: VectorUi,
    error: String,
    range: f32,
    auto: bool,
    probe: Option<([u32; 3], u64, ThermalCell)>,
}
impl Default for ThermalUi {
    fn default() -> Self {
        Self {
            kind: 10,
            show: true,
            overlay: Default::default(),
            error: String::new(),
            range: 300.,
            auto: true,
            probe: None,
        }
    }
}
impl ThermalUi {
    pub fn invalidate(&mut self) {
        self.overlay.clear();
        self.probe = None;
        self.error.clear();
    }
    pub fn ready(&self, f: Option<&ThermalFrame>, v: Option<&Volume>) -> bool {
        self.show && f.zip(v).is_some_and(|(f, v)| f.is_current(v))
    }
    fn range(&self, f: Option<&ThermalFrame>) -> f32 {
        if !self.auto {
            return self.range.max(1e-20);
        }
        match self.kind {
            10 => f
                .map_or(1., |f| {
                    (f.diagnostics.max_temperature_k - 273.15)
                        .abs()
                        .max((f.diagnostics.min_temperature_k - 273.15).abs())
                })
                .max(1.),
            11 => 1.,
            _ => f.map_or(1., |f| f.diagnostics.max_flux_w_m2).max(1e-20),
        }
    }
    pub fn view<'a>(
        &'a self,
        f: Option<&'a ThermalFrame>,
        v: Option<&Volume>,
    ) -> Option<FieldView<'a>> {
        if !self.ready(f, v) {
            return None;
        }
        let f = f?;
        Some(FieldView {
            buffer: f.result_buffer(),
            kind: self.kind,
            component: 0,
            range: self.range(Some(f)),
            vectors: &self.overlay.arrows,
            trails: if self.kind == 12 {
                self.overlay.draw()
            } else {
                None
            },
        })
    }
    pub fn controls(ui: &mut egui::Ui, c: &mut ThermalConfig) {
        ui.label("Thermal transport and latent heat");
        ui.small("Temperature drives enthalpy/phase. Heat diffusion subcycles within the common timestep.");
        egui::CollapsingHeader::new("Thermal boundary conditions").show(ui, |ui| {
            for (i, name) in ["X−", "X+", "Y−", "Y+", "Z−", "Z+"].iter().enumerate() {
                ui.push_id(i, |ui| {
                    ui.label(format!("Domain face {name}"));
                    boundary(ui, &mut c.boundaries[i]);
                });
            }
            ui.separator();
            ui.label("CAD surfaces exposed to air");
            boundary(ui, &mut c.exposed_boundary);
            ui.add(
                egui::DragValue::new(&mut c.initial_temperature_c)
                    .prefix("Default initial ")
                    .suffix(" °C"),
            );
            ui.add(
                egui::DragValue::new(&mut c.max_substeps)
                    .range(1..=100000)
                    .prefix("Max thermal substeps "),
            );
        });
    }
    pub fn visualization_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = ui
            .checkbox(&mut self.auto, "Automatic colour range")
            .changed();
        if !self.auto || self.kind == 12 {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.range)
                        .speed(1.)
                        .prefix("Range ")
                        .suffix(unit(self.kind)),
                )
                .changed();
        }
        if self.kind == 12 {
            changed |= self.overlay.controls(ui, "W/m²");
        }
        changed
    }
    pub fn update(
        &mut self,
        time: f64,
        plane: SectionPlane,
        span: f32,
        gpu: &GpuContext,
        dirty: bool,
        frame: Option<&ThermalFrame>,
    ) {
        if !self.show || self.kind != 12 {
            return;
        }
        if let Some(f) = frame {
            if let Err(e) = self.overlay.update(
                time,
                gpu,
                VectorField {
                    buffer: f.result_buffer(),
                    grid: &f.grid,
                    layout: VectorLayout {
                        stride: 4,
                        offset: 1,
                        subtract: None,
                        interpolate: false,
                        validity: Some((3, 0)),
                    },
                },
                plane,
                span,
                self.range(Some(f)),
                dirty,
            ) {
                self.error = e;
            }
        }
    }
    pub fn panel(&self, ui: &mut egui::Ui, f: Option<&ThermalFrame>) {
        if let Some(f) = f {
            let d = &f.diagnostics;
            ui.label(format!("{} · {:.3} µs", label(self.kind), f.time_s * 1e6));
            crate::vector_ui::color_legend(
                ui,
                self.range(Some(f)),
                unit(self.kind),
                self.kind == 10,
            );
            ui.monospace(format!("T {:.3}…{:.3} °C\nEnthalpy {:.4e} J\nLiquid volume {:.4e} m³\nJoule input {:.4e} J\nExternal heat {:.4e} J\nEnergy error {:.4e} J\n{} thermal substeps",d.min_temperature_k-273.15,d.max_temperature_k-273.15,d.enthalpy_j,d.liquid_volume_m3,d.joule_j,d.external_j,d.energy_error_j,d.substeps));
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
            }
        }
    }
    pub fn hover_probe(&mut self, ui: &mut egui::Ui, p: [f32; 3], f: Option<&ThermalFrame>) {
        if let Some(f) = f {
            let key = (p.map(f32::to_bits), f.time_s.to_bits());
            if self.probe.as_ref().is_none_or(|(p, t, _)| (*p, *t) != key) {
                self.probe = f.probe(p).ok().flatten().map(|c| (key.0, key.1, c));
            }
            if let Some((_, _, c)) = &self.probe {
                if c.state[3] > 0.5 {
                    ui.monospace(format!(
                        "T {:.4} °C · liquid {:.2}%\nHeat flux {:.3e} W/m²",
                        c.state[0] - 273.15,
                        c.state[1] * 100.,
                        c.flux[3]
                    ));
                } else {
                    ui.small("Outside thermal material");
                }
            }
        }
    }
}
fn boundary(ui: &mut egui::Ui, b: &mut Boundary) {
    let mut kind = match b {
        Boundary::Insulated => 0,
        Boundary::Temperature { .. } => 1,
        Boundary::HeatFlux { .. } => 2,
        Boundary::Convection { .. } => 3,
    };
    let old = kind;
    egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(["Insulated", "Temperature", "Heat flux inward", "Convection"][kind])
        .show_ui(ui, |ui| {
            for (i, name) in ["Insulated", "Temperature", "Heat flux inward", "Convection"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut kind, i, *name);
            }
        });
    if old != kind {
        *b = match kind {
            1 => Boundary::Temperature { celsius: 20. },
            2 => Boundary::HeatFlux { w_m2: 0. },
            3 => Boundary::Convection {
                h_w_m2_k: 10.,
                ambient_c: 20.,
            },
            _ => Boundary::Insulated,
        };
    }
    match b {
        Boundary::Temperature { celsius } => {
            ui.add(egui::DragValue::new(celsius).suffix(" °C"));
        }
        Boundary::HeatFlux { w_m2 } => {
            ui.add(egui::DragValue::new(w_m2).suffix(" W/m²"));
        }
        Boundary::Convection {
            h_w_m2_k,
            ambient_c,
        } => {
            ui.add(
                egui::DragValue::new(h_w_m2_k)
                    .range(0. ..=1e8)
                    .suffix(" W/(m² K)"),
            );
            ui.add(egui::DragValue::new(ambient_c).suffix(" °C ambient"));
        }
        _ => {}
    }
}
pub fn label(k: u32) -> &'static str {
    match k {
        10 => "Temperature",
        11 => "Liquid fraction",
        _ => "Heat flux",
    }
}
fn unit(k: u32) -> &'static str {
    match k {
        10 => " °C",
        11 => " fraction",
        _ => " W/m²",
    }
}
