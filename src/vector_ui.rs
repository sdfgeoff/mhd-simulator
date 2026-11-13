//! Shared vector overlay controls for EM and fluid analysis.
use crate::{
    flow::{FlowTrails, TrailDraw, TrailSettings, VectorField, VectorStyle},
    gpu::GpuContext,
    plane::SectionPlane,
};
use eframe::egui;
use glam::Vec3;
pub struct VectorUi {
    pub style: VectorStyle,
    pub density: u32,
    pub paused: bool,
    pub minimum: f32,
    speed: f32,
    tail: f32,
    pub visual_time: f64,
    last_time: Option<f64>,
    flow: Option<FlowTrails>,
    pub arrows: Vec<[Vec3; 2]>,
    sampled: bool,
}
impl Default for VectorUi {
    fn default() -> Self {
        Self {
            style: VectorStyle::Trails,
            density: 25,
            paused: false,
            minimum: 0.,
            speed: 0.35,
            tail: 0.22,
            visual_time: 0.,
            last_time: None,
            flow: None,
            arrows: Vec::new(),
            sampled: false,
        }
    }
}
impl VectorUi {
    pub fn clear(&mut self) {
        self.flow = None;
        self.arrows.clear();
        self.sampled = false;
        self.last_time = None;
    }
    pub fn running(&self) -> bool {
        self.style.animated() && !self.paused
    }
    pub fn draw(&self) -> Option<TrailDraw<'_>> {
        if self.style.animated() {
            self.flow.as_ref().map(|f| f.draw())
        } else {
            None
        }
    }
    pub fn controls(&mut self, ui: &mut egui::Ui, unit: &str) -> bool {
        let mut changed = false;
        egui::ComboBox::from_label("Vector display")
            .selected_text(self.style.label())
            .show_ui(ui, |ui| {
                for style in [
                    VectorStyle::Off,
                    VectorStyle::Arrows,
                    VectorStyle::Trails,
                    VectorStyle::Dots,
                ] {
                    changed |= ui
                        .selectable_value(&mut self.style, style, style.label())
                        .changed();
                }
            });
        if self.style != VectorStyle::Off {
            changed |= ui
                .add(egui::Slider::new(&mut self.density, 8..=64).text("Vector density"))
                .changed();
        }
        if self.style != VectorStyle::Off {
            let speed = (self.minimum.abs() * 0.1).max(1e-12) as f64;
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.minimum)
                        .range(0. ..=f32::MAX)
                        .speed(speed)
                        .max_decimals(10)
                        .prefix("Hide below ")
                        .suffix(format!(" {unit}")),
                )
                .on_hover_text(
                    "Only affects vector display; useful for hiding numerical rest-state noise.",
                )
                .changed();
        }
        if self.style.animated() {
            changed |= ui.checkbox(&mut self.paused, "Pause animation").changed();
            changed |= ui
                .add(egui::Slider::new(&mut self.speed, 0.05..=2.).text("Visual speed"))
                .changed();
            if self.style == VectorStyle::Trails {
                changed |= ui
                    .add(egui::Slider::new(&mut self.tail, 0.05..=0.5).text("Trail length"))
                    .changed();
            }
            ui.small("Projected onto the section plane. Visual speed is independent of solver time and vector magnitude.");
        }
        changed
    }
    pub fn update(
        &mut self,
        time: f64,
        context: &GpuContext,
        field: VectorField<'_>,
        plane: SectionPlane,
        span: f32,
        scale: f32,
        dirty: bool,
    ) -> Result<(), String> {
        let delta = self
            .last_time
            .replace(time)
            .map(|last| (time - last).clamp(0., 0.1))
            .unwrap_or(0.);
        if !self.paused {
            self.visual_time += delta;
        }
        if self.style == VectorStyle::Off {
            self.arrows.clear();
            return Ok(());
        }
        let flow = self
            .flow
            .get_or_insert_with(|| FlowTrails::new(context.clone()));
        if self.style.animated() {
            self.arrows.clear();
            return flow.update(
                field,
                plane,
                span,
                self.visual_time,
                TrailSettings {
                    density: self.density,
                    speed: self.speed,
                    tail: self.tail,
                    dots: self.style == VectorStyle::Dots,
                    minimum_magnitude: self.minimum,
                },
            );
        }
        if !dirty && self.sampled {
            return Ok(());
        }
        self.arrows.clear();
        self.sampled = true;
        let n = self.density;
        let samples = flow.gather(field, plane, span, n)?;
        for y in 0..n {
            for x in 0..n {
                let v = Vec3::from_slice(&samples[(y * n + x) as usize]);
                let magnitude = v.length();
                if magnitude <= self.minimum.max(scale.max(1e-30) * 1e-6) {
                    continue;
                }
                let start = plane.position
                    + plane.u() * (((x as f32 + 0.5) / n as f32 * 2. - 1.) * span)
                    + plane.v() * (((y as f32 + 0.5) / n as f32 * 2. - 1.) * span);
                let direction = v / magnitude;
                let end = start
                    + direction
                        * (span * 1.3 / n as f32 * (magnitude / scale.max(1e-30)).sqrt().min(1.));
                self.arrows.push([start, end]);
                let side = direction
                    .cross(plane.normal())
                    .try_normalize()
                    .unwrap_or(plane.u());
                let head = (end - start).length() * 0.25;
                self.arrows
                    .push([end, end - direction * head + side * head * 0.45]);
                self.arrows
                    .push([end, end - direction * head - side * head * 0.45]);
            }
        }
        Ok(())
    }
}

/// Native-pixel legend matching the field shader palettes.
pub fn color_legend(ui: &mut egui::Ui, scale: f32, unit: &str, signed: bool) {
    use egui::{Color32, Pos2, Vec2};
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 12.), egui::Sense::hover());
    for i in 0..64 {
        let t = i as f32 / 63.;
        let rgb = [
            (1.5 - (4. * t - 3.).abs()).clamp(0., 1.),
            (1.5 - (4. * t - 2.).abs()).clamp(0., 1.),
            (1.5 - (4. * t - 1.).abs()).clamp(0., 1.),
        ];
        let c = if signed {
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
    let min = if signed { -scale } else { 0. };
    ui.small(format!("{min:.3e} … {scale:.3e} {}", unit));
}
