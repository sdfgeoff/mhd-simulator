use crate::{
    caps::{self, Cap},
    domain::{GpuDomain as Gpu, Volume},
    gizmo::{Camera, Gizmo},
    model::Model,
    plane::SectionPlane,
    project::{self, DomainSettings, Material},
    render::Viewport,
    section::{Options as SectionOptions, SectionRenderer, Style},
};
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use glam::{EulerRot, Quat, Vec3};
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
    settings: DomainSettings,
    volume: Option<Volume>,
    materials: Vec<Material>,
    domain_pending: bool,
    voxel_overlay: bool,
    grid_lines: bool,
    material_view: bool,
    project_path: String,
    notice: String,
    section_renderer: Option<SectionRenderer>,
    panel_pixels: u32,
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
    texture: Option<egui::TextureId>,
    caps: Vec<Cap>,
    dirty: bool,
    section_time: f64,
}
impl App {
    pub fn new(path: String, state: Option<egui_wgpu::RenderState>) -> Self {
        let (model, error) = match Model::load(std::path::Path::new(&path)) {
            Ok(m) => (m, String::new()),
            Err(e) => (Model::default(), e),
        };
        let (lo, hi) = model.bounds();
        let (gpu, gpu_error) = match state.as_ref().map(Gpu::from_state).unwrap_or_else(Gpu::new) {
            Ok(g) => (Some(g), String::new()),
            Err(e) => (None, e),
        };
        let settings = DomainSettings::fit(&model);
        let mut app = Self {
            settings,
            volume: None,
            materials: project::default_materials(),
            domain_pending: true,
            voxel_overlay: true,
            grid_lines: true,
            material_view: false,
            project_path: "project.mhd.json".into(),
            notice: String::new(),
            section_renderer: state.clone().map(SectionRenderer::new),
            panel_pixels: 0,
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
            texture: None,
            caps: Vec::new(),
            dirty: true,
            section_time: 0.,
        };
        if app.path.ends_with(".json") {
            app.load();
        } else {
            app.build_domain();
        }
        app
    }
    fn clear_derived(&mut self) {
        self.volume = None;
        self.caps.clear();
        self.texture = None;

        self.dirty = true;
    }
    fn load(&mut self) {
        self.notice.clear();
        let path = std::path::Path::new(&self.path);
        if path.extension().is_some_and(|e| e == "json") {
            match project::load(path) {
                Ok(p) => {
                    self.model = p.model;
                    self.settings = p.settings;
                    self.materials = p.materials;
                    self.plane = p.plane;
                    self.project_path = self.path.clone();
                    self.clear_derived();
                    self.gizmo = Gizmo::default();
                    self.build_domain();
                }
                Err(e) => self.error = e,
            }
        } else {
            match Model::load(path) {
                Ok(m) => {
                    self.model = m;
                    self.settings = DomainSettings::fit(&self.model);
                    self.materials = project::default_materials();
                    self.clear_derived();
                    self.reset_plane();
                    self.build_domain();
                }
                Err(e) => self.error = e,
            }
        }
    }
    fn build_domain(&mut self) {
        self.domain_pending = true;
        let Some(gpu) = &self.gpu else {
            return;
        };
        match gpu.build(&self.model, &self.settings, &self.materials) {
            Ok(volume) => {
                self.volume = Some(volume);
                self.domain_pending = false;
                self.dirty = true;
                self.error.clear();
            }
            Err(e) => self.error = e,
        }
    }
    fn section_span(&self) -> f32 {
        if let Some(v) = &self.volume {
            (v.max() - Vec3::from(v.settings.min_mm)).max_element() * 0.65
        } else {
            let (lo, hi) = self.model.bounds();
            (hi - lo).max_element().max(1.) * 0.75
        }
    }
    fn reset_plane(&mut self) {
        let (lo, hi) = self.model.bounds();
        self.plane = SectionPlane::new((lo + hi) * 0.5);
        self.gizmo = Gizmo::default();
        self.dirty = true;
    }
    fn section_style(&self) -> Style {
        if self.voxel_overlay {
            Style::Voxels
        } else if self.hatched {
            Style::HatchedCad
        } else {
            Style::Outlines
        }
    }
    fn update_section(&mut self, _ctx: &egui::Context) {
        let started = std::time::Instant::now();
        let (lo, hi) = self.model.bounds();
        let span = self.section_span();
        let style = self.section_style();
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
        if let Some(renderer) = &mut self.section_renderer {
            self.texture = Some(renderer.draw_section(
                &self.model,
                &self.caps,
                self.plane,
                self.volume.as_ref(),
                SectionOptions {
                    style,
                    edges: self.grid_lines,
                    material_colors: self.material_view,
                },
                span,
                self.panel_pixels.max(32),
            ));
        }
        self.section_time = started.elapsed().as_secs_f64() * 1000.;
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
        let section_span = self.section_span();
        if let Some(viewport) = &mut self.viewport {
            let texture = viewport.draw(
                camera,
                self.plane,
                self.cut,
                self.hatched,
                &self.model,
                &self.caps,
                ui.ctx().pixels_per_point(),
                self.voxel_overlay.then_some(crate::render::DomainView {
                    volume: self.volume.as_ref(),
                    material_colors: self.material_view,
                    span: section_span,
                    grid: if self.voxel_overlay && self.grid_lines {
                        self.volume
                            .as_ref()
                            .map(|v| (Vec3::from(v.settings.min_mm), v.settings.cell_mm))
                    } else {
                        None
                    },
                    bounds: if self.voxel_overlay {
                        self.volume
                            .as_ref()
                            .map(|v| (Vec3::from(v.settings.min_mm), v.max()))
                    } else {
                        None
                    },
                }),
            );
            painter.image(
                texture,
                rect,
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
        let span = self.section_span();
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
            if self.voxel_overlay {
                "VOXELS + CAD CONTOURS"
            } else if self.hatched {
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
        ui.small("Or drop geometry / an .mhd.json project here.");
        ui.small("Project file");
        ui.text_edit_singleline(&mut self.project_path);
        ui.horizontal(|ui| {
            if ui.button("Save project").clicked() {
                match project::save(
                    std::path::Path::new(&self.project_path),
                    &self.model,
                    &self.settings,
                    &self.materials,
                    self.plane,
                ) {
                    Ok(()) => self.notice = "Project saved".into(),
                    Err(e) => self.error = e,
                }
            }
            if ui.button("Open project").clicked() {
                self.path = self.project_path.clone();
                self.load();
            }
        });
        if !self.notice.is_empty() {
            ui.small(&self.notice);
        }
        let (lo, hi) = self.model.bounds();
        let d = hi - lo;
        if !self.model.bodies.is_empty() {
            ui.monospace(format!("{:.3} × {:.3} × {:.3} mm", d.x, d.y, d.z));
        }
        ui.add_space(12.);
        ui.separator();
        ui.heading("02 / Simulation domain");
        self.domain_controls(ui);
        ui.separator();
        ui.heading("03 / Section plane");
        ui.checkbox(&mut self.cut, "Cut geometry");
        ui.checkbox(&mut self.show_gizmo, "Show move / rotate handles");
        ui.small(
            "Arrows translate along world X/Y/Z. Rings rotate around those axes at the pivot.",
        );
        ui.horizontal(|ui| {
            self.dirty |= ui
                .selectable_value(&mut self.voxel_overlay, false, "CAD")
                .changed();
            self.dirty |= ui
                .selectable_value(&mut self.voxel_overlay, true, "Voxel domain")
                .changed();
        });
        if self.voxel_overlay {
            self.dirty |= ui
                .checkbox(&mut self.grid_lines, "Subtle cell edges")
                .changed();
            self.dirty |= ui
                .checkbox(&mut self.material_view, "Colour by material ID")
                .changed();
        } else {
            ui.horizontal(|ui| {
                self.dirty |= ui
                    .selectable_value(&mut self.hatched, true, "Hatched CAD")
                    .changed();
                self.dirty |= ui
                    .selectable_value(&mut self.hatched, false, "Outlines")
                    .changed();
            });
        }
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
        if ui.button("Reset camera").clicked() {
            self.yaw = -0.7;
            self.pitch = 0.45;
            self.zoom = 1.;
        }
        ui.add_space(10.);
        ui.separator();
        self.property_controls(ui);
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
                ui.colored_label(
                    Color32::from_rgb(75, 196, 208),
                    "RASTERIZED MATERIAL DOMAIN",
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ctx.screen_rect().width() > 850. {
                        ui.label("Domain preparation · No physics yet");
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
                ui.small(format!("Section update: {:.1} ms", self.section_time));
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
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(12.);
                    ui.heading("04 / Section");
                    ui.label(self.section_style().label());
                    ui.small("Rendered directly at viewport resolution");
                    let panel_pixels = (ui.available_width().min(320.) * ctx.pixels_per_point())
                        .round()
                        .clamp(32., 4096.) as u32;
                    if panel_pixels != self.panel_pixels {
                        self.panel_pixels = panel_pixels;
                        self.dirty = true;
                    }
                    ui.add_space(8.);
                    if self.voxel_overlay && self.domain_pending {
                        ui.colored_label(
                            Color32::YELLOW,
                            "Settings changed — rebuild to update this grid.",
                        );
                    }
                    if let Some(t) = self.texture {
                        let response = ui.add(
                            egui::Image::new((t, Vec2::splat(self.panel_pixels as f32)))
                                .bg_fill(Color32::from_rgb(14, 21, 32))
                                .fit_to_exact_size(Vec2::splat(ui.available_width().min(320.))),
                        );
                        if let Some(pointer) = response.hover_pos() {
                            let uv = (pointer - response.rect.min) / response.rect.size();
                            let span = self.section_span();
                            let point = self.plane.position
                                + self.plane.u() * ((uv.x * 2. - 1.) * span)
                                + self.plane.v() * ((1. - uv.y * 2.) * span);
                            let pixel = if self.voxel_overlay {
                                self.gpu
                                    .as_ref()
                                    .zip(self.volume.as_ref())
                                    .and_then(|(gpu, v)| {
                                        gpu.section(
                                            v,
                                            SectionPlane {
                                                position: point,
                                                ..self.plane
                                            },
                                            0.,
                                            1,
                                        )
                                        .ok()
                                    })
                                    .and_then(|p| p.into_iter().next())
                            } else {
                                None
                            };
                            if let Some(pixel) = pixel {
                                if pixel.inside == 0 {
                                    ui.label("Outside the built domain");
                                } else {
                                    ui.monospace(format!(
                                        "Component {} · Material ID {}",
                                        pixel.body, pixel.material
                                    ));
                                    if let Some(v) = &self.volume {
                                        if let Some((_, name)) = v
                                            .material_names
                                            .iter()
                                            .find(|(id, _)| *id == pixel.material)
                                        {
                                            ui.label(name);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        ui.label("No built volume available.");
                    }
                    if self.voxel_overlay {
                        ui.small(
                            "Air = 0 · Overlap = 255 · CAD visibility does not change the domain.",
                        );
                    } else {
                        ui.small(
                            "CAD geometry; voxel diagnostics below refer to the built domain.",
                        );
                    }
                    self.domain_diagnostics(ui);
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
            });
        egui::CentralPanel::default().show(ctx, |ui| self.viewport(ui));
    }
}
fn optional(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<f32>,
    initial: f32,
    speed: f64,
    suffix: &str,
) -> bool {
    let mut enabled = value.is_some();
    let mut changed = ui.checkbox(&mut enabled, label).changed();
    if changed {
        *value = enabled.then_some(initial);
    }
    if let Some(v) = value {
        changed |= ui
            .add(egui::DragValue::new(v).speed(speed).suffix(suffix))
            .changed();
    }
    changed
}
impl App {
    fn domain_controls(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Bounds and cell spacing").show(ui, |ui| {
            for axis in 0..3 {
                ui.horizontal(|ui| {
                    ui.label(["X", "Y", "Z"][axis]);
                    self.domain_pending |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.min_mm[axis])
                                .speed(0.1)
                                .prefix("min ")
                                .suffix(" mm"),
                        )
                        .changed();
                    self.domain_pending |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.max_mm[axis])
                                .speed(0.1)
                                .prefix("max ")
                                .suffix(" mm"),
                        )
                        .changed();
                });
            }
            self.domain_pending |= ui
                .add(
                    egui::DragValue::new(&mut self.settings.cell_mm)
                        .speed(0.005)
                        .prefix("Cell ")
                        .suffix(" mm"),
                )
                .changed();
            if ui.button("Fit domain to geometry").clicked() {
                self.settings = DomainSettings::fit(&self.model);
                self.domain_pending = true;
            }
        });
        match self.settings.dims() {
            Ok(d) => {
                ui.monospace(format!("{} × {} × {} cells", d[0], d[1], d[2]));
                if let Ok(bytes) = self.settings.estimated_bytes(self.model.bodies.len()) {
                    ui.small(format!(
                        "Grid build working set: {:.1} MiB",
                        bytes as f64 / 1048576.
                    ));
                }
            }
            Err(e) => {
                ui.colored_label(Color32::LIGHT_RED, e);
            }
        }
        if ui
            .button(if self.domain_pending {
                "Build / rebuild domain"
            } else {
                "Rebuild domain"
            })
            .clicked()
        {
            self.build_domain();
        }
        if self.domain_pending && self.volume.is_some() {
            ui.colored_label(Color32::YELLOW, "Displaying the last built grid");
        }
    }
    fn domain_diagnostics(&self, ui: &mut egui::Ui) {
        if let Some(v) = &self.volume {
            ui.separator();
            ui.monospace(format!(
                "{} × {} × {} · {:.4} mm cells",
                v.dims[0], v.dims[1], v.dims[2], v.settings.cell_mm
            ));
            let lo = v.settings.min_mm;
            let hi = v.max();
            ui.small(format!(
                "Min: ({:.3}, {:.3}, {:.3}) mm",
                lo[0], lo[1], lo[2]
            ));
            ui.small(format!("Max: ({:.3}, {:.3}, {:.3}) mm", hi.x, hi.y, hi.z));
            ui.small(format!(
                "Persistent GPU data: {:.1} MiB",
                v.persistent_bytes as f64 / 1048576.
            ));
            ui.small(format!(
                "Raster build #{}: {:.1} ms",
                v.build_serial, v.build_ms
            ));
            ui.small(format!(
                "{} occupied · {} overlapping cells",
                v.occupied, v.overlaps
            ));
            egui::CollapsingHeader::new("Component volumes").show(ui, |ui| {
                for (i, &count) in v.counts.iter().enumerate() {
                    ui.small(format!(
                        "{}: {} cells / {:.3} mm³",
                        self.model.bodies[i].name,
                        count,
                        count as f64 * (v.settings.cell_mm as f64).powi(3)
                    ));
                }
            });
            for warning in &v.warnings {
                ui.colored_label(Color32::YELLOW, warning);
            }
        }
    }
    fn property_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Simulation properties");
        ui.small("Materials, initial temperatures and prescribed currents. Field and thermal solvers are not yet present.");
        for (i, b) in self.model.bodies.iter_mut().enumerate() {
            egui::CollapsingHeader::new(&b.name).id_salt(("component",i)).show(ui,|ui|{
            self.dirty|=ui.checkbox(&mut b.visible,"Show CAD").changed();self.domain_pending|=ui.checkbox(&mut b.simulation.included,"Include in domain").changed();
            let name=self.materials.iter().find(|m|m.id==b.simulation.material_id).map(|m|m.name.as_str()).unwrap_or("Unknown material");
            egui::ComboBox::from_id_salt(("material",i)).selected_text(format!("{} · {}",b.simulation.material_id,name)).show_ui(ui,|ui|{for material in self.materials.iter().filter(|m|m.id!=0){self.domain_pending|=ui.selectable_value(&mut b.simulation.material_id,material.id,format!("{} · {}",material.id,material.name)).changed();}});
            optional(ui,"Initial temperature",&mut b.simulation.temperature_c,20.,1.," °C");optional(ui,"Prescribed current",&mut b.simulation.current_a,0.,1.," A");
            ui.small("Temperature and current are saved configuration values; no excitation or thermal calculation yet.");
        });
        }
        egui::CollapsingHeader::new("Material library").show(ui, |ui| {
            for m in &mut self.materials {
                ui.push_id(m.id, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("ID {}", m.id));
                        self.domain_pending |= ui.text_edit_singleline(&mut m.name).changed();
                    });
                    self.domain_pending |=
                        optional(ui, "Conductivity", &mut m.conductivity_s_m, 0., 1e5, " S/m");
                    self.domain_pending |= optional(
                        ui,
                        "Relative permeability",
                        &mut m.relative_permeability,
                        1.,
                        0.1,
                        "",
                    );
                    self.domain_pending |=
                        optional(ui, "Density", &mut m.density_kg_m3, 1., 10., " kg/m³");
                    if !m.notes.is_empty() {
                        ui.small(&m.notes);
                    }
                    if let Some(reference) = &m.reference {
                        ui.hyperlink_to("Reference datasheet", reference);
                    }
                    ui.separator();
                });
            }
            if ui.button("Add material").clicked() {
                let id = (1..255).find(|id| self.materials.iter().all(|m| m.id != *id));
                if let Some(id) = id {
                    self.materials.push(Material {
                        id,
                        name: format!("Material {id}"),
                        conductivity_s_m: None,
                        relative_permeability: None,
                        density_kg_m3: None,
                        notes: String::new(),
                        reference: None,
                    });
                    self.domain_pending = true;
                }
            }
        });
    }
}

#[cfg(test)]
mod project_ui_tests {
    use super::*;
    #[test]
    fn project_startup_and_geometry_replacement_clear_cached_sections() {
        let _gpu_lock = crate::gpu_test_lock();
        let model = Model::load(std::path::Path::new("assets/hotend.glb")).unwrap();
        let settings = DomainSettings::fit(&model);
        let materials = project::default_materials();
        std::fs::create_dir_all("artifacts").unwrap();
        let file = std::path::Path::new("artifacts/startup.mhd.json");
        project::save(
            file,
            &model,
            &settings,
            &materials,
            SectionPlane::new(Vec3::new(0.1, 0.2, 4.)),
        )
        .unwrap();
        let mut app = App::new(file.display().to_string(), None);
        assert!(app.error.is_empty(), "{}", app.error);
        assert!(app.volume.is_some());
        assert_eq!(app.settings, settings);
        assert_eq!(app.plane.position, Vec3::new(0.1, 0.2, 4.));
        app.caps = vec![Cap::default(); 5];
        app.path = "assets/annulus.gltf".into();
        app.load();
        assert_eq!(app.model.bodies.len(), 1);
        assert!(app.caps.is_empty());
        assert_eq!(app.volume.as_ref().unwrap().counts.len(), 1);
    }
}
