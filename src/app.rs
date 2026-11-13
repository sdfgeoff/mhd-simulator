use crate::{
    caps::{self, Cap},
    gizmo::{Camera, Gizmo},
    gpu::Gpu,
    model::Model,
    plane::SectionPlane,
    render::Viewport,
};
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use glam::{EulerRot, Quat};
pub const COLORS: [[u8; 3]; 6] = [
    [75, 196, 208],
    [246, 180, 82],
    [181, 132, 238],
    [235, 109, 124],
    [112, 194, 140],
    [143, 171, 218],
];
pub fn color(id: u32) -> [u8; 3] {
    if id == 0 {
        [17, 26, 39]
    } else if id == 255 {
        [255, 50, 90]
    } else {
        COLORS[(id as usize - 1) % COLORS.len()]
    }
}
pub struct App {
    pub model: Model,
    path: String,
    error: String,
    gpu: Option<Gpu>,
    viewport: Option<Viewport>,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    cut: bool,
    plane: SectionPlane,
    gizmo: Gizmo,
    show_gizmo: bool,
    hatched: bool,
    resolution: u32,
    texture: Option<egui::TextureHandle>,
    caps: Vec<Cap>,
    dirty: bool,
    section_time: f64,
    overlaps: usize,
}
impl App {
    pub fn new(path: String, state: Option<egui_wgpu::RenderState>) -> Self {
        let (model, error) = match Model::load(std::path::Path::new(&path)) {
            Ok(m) => (m, String::new()),
            Err(e) => (Model::default(), e),
        };
        let (lo, hi) = model.bounds();
        let (gpu, gpu_error) = match Gpu::new() {
            Ok(g) => (Some(g), String::new()),
            Err(e) => (None, e),
        };
        Self {
            model,
            path,
            error: format!("{error}{gpu_error}"),
            gpu,
            viewport: state.map(Viewport::new),
            yaw: -0.7,
            pitch: 0.45,
            zoom: 1.,
            cut: true,
            plane: SectionPlane::new((lo + hi) * 0.5),
            gizmo: Gizmo::default(),
            show_gizmo: true,
            hatched: true,
            resolution: 128,
            texture: None,
            caps: Vec::new(),
            dirty: true,
            section_time: 0.,
            overlaps: 0,
        }
    }
    fn load(&mut self) {
        match Model::load(std::path::Path::new(&self.path)) {
            Ok(m) => {
                self.model = m;
                self.reset_plane();
                self.error.clear();
            }
            Err(e) => self.error = e,
        }
    }
    fn reset_plane(&mut self) {
        let (lo, hi) = self.model.bounds();
        self.plane = SectionPlane::new((lo + hi) * 0.5);
        self.gizmo = Gizmo::default();
        self.dirty = true;
    }
    fn update_section(&mut self, ctx: &egui::Context) {
        let (lo, hi) = self.model.bounds();
        let span = (hi - lo).max_element().max(1.) * 0.75;
        self.caps = self
            .model
            .bodies
            .iter()
            .map(|b| {
                if b.visible {
                    caps::build(b, self.plane, (hi - lo).max_element().max(1.) * 1e-6)
                } else {
                    Cap::default()
                }
            })
            .collect();
        if self.model.bodies.is_empty() {
            self.texture = None;
            return;
        }
        let start = std::time::Instant::now();
        let Some(gpu) = &self.gpu else {
            return;
        };
        match gpu.section(
            &self.model,
            self.plane.position,
            self.plane.u() * span,
            self.plane.v() * span,
            self.resolution,
        ) {
            Ok(labels) => {
                self.overlaps = labels.iter().filter(|&&v| v == 255).count();
                let n = self.resolution as usize;
                let pixels = (0..n)
                    .rev()
                    .flat_map(|y| {
                        labels[y * n..(y + 1) * n].iter().map(|&id| {
                            let c = color(id);
                            Color32::from_rgb(c[0], c[1], c[2])
                        })
                    })
                    .collect();
                self.texture = Some(ctx.load_texture(
                    "GPU material section",
                    egui::ColorImage {
                        size: [n, n],
                        pixels,
                    },
                    egui::TextureOptions::NEAREST,
                ));
                self.section_time = start.elapsed().as_secs_f64() * 1000.;
                self.error.clear();
            }
            Err(e) => {
                self.texture = None;
                self.error = e;
            }
        }
    }
    fn camera(&self, rect: egui::Rect) -> Camera {
        let (lo, hi) = self.model.bounds();
        let size = (hi - lo).max_element().max(1.);
        Camera {
            rect,
            center: (lo + hi) * 0.5,
            rotation: Quat::from_rotation_x(self.pitch) * Quat::from_rotation_z(self.yaw),
            scale: rect.width().min(rect.height()) * 0.68 * self.zoom / size,
            size,
        }
    }
    fn viewport(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
        if rect.width() < 10. || rect.height() < 10. {
            return;
        }
        let mut camera = self.camera(rect);
        let (changed, captured) = if self.show_gizmo {
            self.gizmo.interact(ui, &response, camera, &mut self.plane)
        } else {
            (false, false)
        };
        self.dirty |= changed;
        if response.dragged_by(egui::PointerButton::Secondary)
            || (response.dragged_by(egui::PointerButton::Primary) && !captured)
        {
            let d = ui.input(|i| i.pointer.delta());
            self.yaw += d.x * 0.008;
            self.pitch = (self.pitch + d.y * 0.008).clamp(-1.5, 1.5);
        }
        if response.hovered() {
            self.zoom =
                (self.zoom * ui.input(|i| (i.smooth_scroll_delta.y * 0.001).exp())).clamp(0.2, 8.);
        }
        camera = self.camera(rect);
        if self.dirty {
            self.update_section(ui.ctx());
            self.dirty = false;
            ui.ctx().request_repaint();
        }
        let painter = ui.painter_at(rect);
        if let Some(viewport) = &mut self.viewport {
            let texture = viewport.draw(
                camera,
                self.plane,
                self.cut,
                self.hatched,
                &self.model,
                &self.caps,
                ui.ctx().pixels_per_point(),
            );
            painter.image(
                texture,
                rect,
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
        let span = camera.size * 0.75;
        let o = self.plane.position;
        let u = self.plane.u() * span;
        let v = self.plane.v() * span;
        let corners = [o - u - v, o + u - v, o + u + v, o - u + v];
        for i in 0..4 {
            painter.line_segment(
                [
                    camera.project(corners[i]),
                    camera.project(corners[(i + 1) % 4]),
                ],
                Stroke::new(1., Color32::from_rgb(64, 119, 130)),
            );
        }
        if self.show_gizmo {
            self.gizmo.draw(ui, camera, self.plane);
        }
        painter.text(
            rect.left_top() + Vec2::splat(16.),
            egui::Align2::LEFT_TOP,
            if self.hatched {
                "CAD / HATCHED SECTION"
            } else {
                "CAD / SECTION OUTLINES"
            },
            egui::FontId::monospace(12.),
            Color32::from_gray(180),
        );
        painter.text(rect.right_bottom()-Vec2::splat(14.),egui::Align2::RIGHT_BOTTOM,"Drag handles: move / rotate plane\nDrag background or right-drag: orbit · Scroll: zoom",egui::FontId::proportional(12.),Color32::from_gray(180));
    }
    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.);
        ui.heading("01 / Assembly");
        ui.label("glTF, GLB, STL · STEP metadata only");
        ui.text_edit_singleline(&mut self.path);
        if ui.button("Load file").clicked() {
            self.load();
        }
        ui.small("Or drop a file into this window.");
        let (lo, hi) = self.model.bounds();
        let d = hi - lo;
        if !self.model.bodies.is_empty() {
            ui.monospace(format!("{:.3} × {:.3} × {:.3} mm", d.x, d.y, d.z));
        }
        ui.add_space(12.);
        ui.separator();
        ui.heading("02 / Section plane");
        ui.checkbox(&mut self.cut, "Cut geometry");
        ui.checkbox(&mut self.show_gizmo, "Show move / rotate handles");
        ui.small(
            "Arrows translate along world X/Y/Z. Rings rotate around those axes at the pivot.",
        );
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.hatched, true, "Hatched CAD");
            ui.selectable_value(&mut self.hatched, false, "Outlines");
        });
        ui.small("Outline mode is ready for field overlays; no field solver is present yet.");
        ui.horizontal(|ui| {
            if ui.button("Reset plane").clicked() {
                self.reset_plane();
            }
            if ui.button("Flip cut side").clicked() {
                self.plane.orientation *= Quat::from_rotation_y(std::f32::consts::PI);
                self.dirty = true;
            }
        });
        egui::CollapsingHeader::new("Exact transform").show(ui, |ui| {
            for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Position {axis}"));
                    self.dirty |= ui
                        .add(
                            egui::DragValue::new(&mut self.plane.position[i])
                                .speed(0.02)
                                .suffix(" mm"),
                        )
                        .changed();
                });
            }
            let (x, y, z) = self.plane.orientation.to_euler(EulerRot::XYZ);
            let mut angles = [x.to_degrees(), y.to_degrees(), z.to_degrees()];
            let mut changed = false;
            for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Rotation {axis}"));
                    changed |= ui
                        .add(egui::DragValue::new(&mut angles[i]).speed(0.5).suffix("°"))
                        .changed();
                });
            }
            if changed {
                self.plane.orientation = Quat::from_euler(
                    EulerRot::XYZ,
                    angles[0].to_radians(),
                    angles[1].to_radians(),
                    angles[2].to_radians(),
                );
                self.dirty = true;
            }
        });
        egui::ComboBox::from_label("Section pixels")
            .selected_text(self.resolution.to_string())
            .show_ui(ui, |ui| {
                for n in [64, 128, 256] {
                    self.dirty |= ui
                        .selectable_value(&mut self.resolution, n, n.to_string())
                        .changed();
                }
            });
        if ui.button("Reset camera").clicked() {
            self.yaw = -0.7;
            self.pitch = 0.45;
            self.zoom = 1.;
        }
        ui.add_space(10.);
        ui.separator();
        egui::CollapsingHeader::new("Bodies and roles")
            .default_open(true)
            .show(ui, |ui| {
                for (i, b) in self.model.bodies.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let c = COLORS[i % 6];
                        ui.colored_label(Color32::from_rgb(c[0], c[1], c[2]), "■");
                        self.dirty |= ui.checkbox(&mut b.visible, &b.name).changed();
                    });
                    let roles = [
                        "Unassigned",
                        "Chamber wall",
                        "Molten metal",
                        "Coil",
                        "Permanent magnet",
                    ];
                    egui::ComboBox::from_id_salt(i)
                        .selected_text(roles[b.role])
                        .show_ui(ui, |ui| {
                            for (r, name) in roles.iter().enumerate() {
                                ui.selectable_value(&mut b.role, r, *name);
                            }
                        });
                    ui.small(format!(
                        "{} triangles · {}",
                        b.triangles.len(),
                        if b.closed() {
                            "edge closure passed"
                        } else {
                            "OPEN / INVALID"
                        }
                    ));
                    ui.add_space(5.);
                }
            });
        ui.small("Role labels and plane pose are session-only in this preview.");
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(9.);
            ui.horizontal(|ui| {
                ui.heading("MHD / WORKBENCH");
                ui.add_space(20.);
                ui.colored_label(Color32::from_rgb(75, 196, 208), "6DOF SECTION PLANE");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ctx.screen_rect().width() > 850. {
                        ui.label("Geometry preview · No physics yet");
                    }
                });
            });
            ui.add_space(9.);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.small(
                    self.gpu
                        .as_ref()
                        .map(|g| format!("Compute: {}", g.name))
                        .unwrap_or("Compute unavailable".into()),
                );
                ui.separator();
                ui.small(format!(
                    "GPU section: {:.1} ms · {} overlap pixels",
                    self.section_time, self.overlaps
                ));
            });
        });
        egui::SidePanel::left("controls")
            .exact_width((ctx.screen_rect().width() * 0.25).clamp(160., 290.))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()))
        {
            self.path = path.display().to_string();
            self.load();
        }
        egui::SidePanel::right("section")
            .exact_width((ctx.screen_rect().width() * 0.30).clamp(160., 340.))
            .show(ctx, |ui| {
                ui.add_space(12.);
                ui.heading("03 / GPU cross-section");
                ui.label("Closed-body occupancy on the plane");
                ui.add_space(8.);
                if let Some(t) = &self.texture {
                    ui.add(
                        egui::Image::new(t)
                            .fit_to_exact_size(Vec2::splat(ui.available_width().min(320.))),
                    );
                } else {
                    ui.label("No valid solid section available.");
                }
                ui.small("Background = air / void · Pink = overlap");
                ui.small("Body colours identify geometry, not physical fields.");
                ui.separator();
                if !self.error.is_empty() {
                    ui.colored_label(Color32::LIGHT_RED, &self.error);
                }
                for (i, cap) in self.caps.iter().enumerate() {
                    if let Some(w) = &cap.warning {
                        ui.colored_label(
                            Color32::LIGHT_RED,
                            format!("{}: {w}", self.model.bodies[i].name),
                        );
                    }
                }
                ui.small(format!("Source: {}", self.model.source));
                ui.heading("Import notes");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for s in &self.model.notices {
                        ui.label(s);
                        ui.add_space(5.);
                    }
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| self.viewport(ui));
    }
}
