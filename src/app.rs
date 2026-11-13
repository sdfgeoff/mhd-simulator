use crate::{
    caps::{self, Cap},
    domain::{GpuDomain as Gpu, Volume},
    em_ui::EmUi,
    fluid_ui::FluidUi,
    gizmo::{Camera, Gizmo},
    gpu::GpuContext,
    inputs::FieldState,
    model::Model,
    plane::SectionPlane,
    project,
    render::Viewport,
    section::{Options as SectionOptions, SectionRenderer, Style},
    simulation::{DomainSettings, Material, SimulationSnapshot},
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum Analysis {
    Electromagnetics,
    Fluid,
    Thermal,
}
pub struct App {
    pub model: Model,
    settings: DomainSettings,
    volume: Option<Volume>,
    materials: Vec<Material>,
    domain_pending: bool,
    inputs_pending: bool,
    fields: FieldState,
    em_ui: EmUi,
    fluid_ui: FluidUi,
    thermal_ui: crate::thermal_ui::ThermalUi,
    analysis: Analysis,
    run_ui: crate::run_ui::RunUi,
    setup: bool,
    selection: Option<crate::picking::Object>,
    material_selection: u32,
    file_picker: crate::file_picker::FilePicker,
    voxel_overlay: bool,
    show_domain: bool,
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
        let (gpu, gpu_error) = match state
            .as_ref()
            .map(|s| {
                Gpu::from_context(GpuContext::new(
                    s.device.clone(),
                    s.queue.clone(),
                    s.adapter.get_info().name,
                ))
            })
            .unwrap_or_else(Gpu::new)
        {
            Ok(g) => (Some(g), String::new()),
            Err(e) => (None, e),
        };
        let settings = DomainSettings::fit(&model);
        let mut app = Self {
            settings,
            volume: None,
            materials: crate::simulation::default_materials(),
            domain_pending: true,
            inputs_pending: false,
            fields: FieldState::default(),
            em_ui: EmUi::default(),
            fluid_ui: FluidUi::default(),
            thermal_ui: Default::default(),
            analysis: Analysis::Electromagnetics,
            run_ui: Default::default(),
            setup: true,
            selection: None,
            material_selection: 2,
            file_picker: Default::default(),
            voxel_overlay: true,
            show_domain: true,
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
    pub fn start_em_preview(&mut self, steps: u32) {
        self.run_ui.steps = steps;
        self.setup = false;
        self.simulate();
    }
    fn clear_derived(&mut self) {
        self.volume = None;
        self.fields = FieldState::default();
        self.em_ui.invalidate();
        self.fluid_ui.invalidate();
        self.thermal_ui.invalidate();
        self.run_ui.invalidate();
        self.caps.clear();
        self.texture = None;

        self.dirty = true;
    }
    fn load(&mut self) {
        self.selection = None;
        self.notice.clear();
        let path = std::path::Path::new(&self.path);
        if path.extension().is_some_and(|e| e == "json") {
            match project::load(path) {
                Ok(p) => {
                    self.model = p.model;
                    self.settings = p.settings;
                    self.materials = p.materials;
                    self.plane = p.plane;
                    self.em_ui.config = p.electromagnetics;
                    self.fluid_ui.config = p.fluid;
                    self.run_ui.config = p.run;
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
                    self.em_ui.config = crate::em::EmConfig::default();
                    self.fluid_ui.config = crate::fluid::FluidConfig::default();
                    self.run_ui.config = Default::default();
                    self.settings = DomainSettings::fit(&self.model);
                    self.materials = crate::simulation::default_materials();
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
                self.inputs_pending = false;
                self.fields = FieldState::default();
                self.em_ui.invalidate();
                self.fluid_ui.invalidate();
                self.thermal_ui.invalidate();
                self.run_ui.invalidate();
                self.domain_pending = false;
                self.dirty = true;
                self.error.clear();
            }
            Err(e) => self.error = e,
        }
    }
    fn simulate(&mut self) {
        self.setup = false;
        if self.run_ui.busy() {
            return;
        }
        if self.domain_pending || self.volume.is_none() {
            self.build_domain();
        }
        self.sync_inputs();
        if self.domain_pending || self.inputs_pending {
            return;
        }
        if self.run_ui.config.thermal_enabled {
            self.analysis = Analysis::Thermal;
            self.thermal_ui.show = true;
            self.em_ui.show = false;
            self.fluid_ui.show = false;
        } else if !self.em_ui.config.has_sources() {
            self.analysis = Analysis::Fluid;
            self.fluid_ui.show = true;
            self.em_ui.show = false;
        } else {
            self.analysis = Analysis::Electromagnetics;
            self.em_ui.show = true;
            self.fluid_ui.show = false;
        }
        if let (Some(gpu), Some(volume)) = (&self.gpu, &self.volume) {
            self.run_ui.start(
                &self.model,
                volume,
                &gpu.context,
                &mut self.em_ui,
                &mut self.fluid_ui,
            );
        }
        self.dirty = true;
    }
    fn sync_inputs(&mut self) {
        if self.domain_pending {
            self.dirty |= self.run_ui.em_frame().is_some() || self.run_ui.fluid_frame().is_some();
            self.em_ui.invalidate();
            self.fluid_ui.invalidate();
            self.thermal_ui.invalidate();
            self.run_ui.invalidate();
            self.fields.invalidate();
            return;
        }
        if !self.inputs_pending {
            return;
        }
        self.fields.invalidate();
        self.dirty |= self.run_ui.em_frame().is_some() || self.run_ui.fluid_frame().is_some();
        self.em_ui.invalidate();
        self.fluid_ui.invalidate();
        self.thermal_ui.invalidate();
        self.run_ui.invalidate();
        let (Some(gpu), Some(volume)) = (&self.gpu, &mut self.volume) else {
            return;
        };
        let result = SimulationSnapshot::from_model(&self.model, &volume.settings, &self.materials)
            .and_then(|s| gpu.update_inputs(volume, s));
        match result {
            Ok(_) => {
                self.inputs_pending = false;
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
    fn fields_ready(&self) -> bool {
        if self.setup {
            return false;
        }
        match self.analysis {
            Analysis::Electromagnetics => self
                .em_ui
                .ready(self.run_ui.em_frame(), self.volume.as_ref()),
            Analysis::Fluid => self
                .fluid_ui
                .ready(self.run_ui.fluid_frame(), self.volume.as_ref()),
            Analysis::Thermal => self
                .thermal_ui
                .ready(self.run_ui.thermal_frame(), self.volume.as_ref()),
        }
    }
    fn section_style(&self) -> Style {
        if self.fields_ready() {
            Style::Field
        } else if self.voxel_overlay {
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
        self.em_ui.update_source_lines(&self.model);
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
        self.draw_section_texture();
        self.section_time = started.elapsed().as_secs_f64() * 1000.;
    }
    fn draw_section_texture(&mut self) {
        let style = self.section_style();
        let fields_ready = self.fields_ready();
        let span = self.section_span();
        if let Some(renderer) = &mut self.section_renderer {
            self.texture = Some(renderer.draw_section_with_surface(
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
                if fields_ready {
                    match self.analysis {
                        Analysis::Electromagnetics => self
                            .em_ui
                            .view(self.run_ui.em_frame(), self.volume.as_ref()),
                        Analysis::Fluid => self
                            .fluid_ui
                            .view(self.run_ui.fluid_frame(), self.volume.as_ref()),
                        Analysis::Thermal => self
                            .thermal_ui
                            .view(self.run_ui.thermal_frame(), self.volume.as_ref()),
                    }
                } else {
                    None
                },
                if self.setup {
                    None
                } else {
                    self.run_ui
                        .timeline
                        .current()
                        .and_then(|f| f.surface.as_ref().zip(f.fluid.as_ref()))
                        .map(|(s, f)| (f.result_buffer(), s.material_id))
                },
            ));
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
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
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
        if self.setup && response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                if !self.show_gizmo || !self.gizmo.hit_test(camera, self.plane, p) {
                    self.selection =
                        crate::picking::pick(&self.model, camera, p, self.cut, self.plane);
                    if let Some(crate::picking::Object::Solid(i)) = self.selection {
                        self.material_selection = self.model.bodies[i].simulation.material_id;
                    }
                    self.dirty = true;
                }
            }
        }

        let ready = self.fields_ready();
        let animate = ready
            && match self.analysis {
                Analysis::Electromagnetics => self
                    .em_ui
                    .animation_running(self.run_ui.em_frame(), self.volume.as_ref()),
                Analysis::Fluid => self.fields_ready() && self.fluid_ui.overlay.running(),
                Analysis::Thermal => {
                    self.thermal_ui.kind == 12 && self.thermal_ui.overlay.running()
                }
            };
        if let Some(gpu) = self.gpu.as_ref().filter(|_| ready) {
            let time = ui.input(|i| i.time);
            let span = self.section_span();
            match self.analysis {
                Analysis::Electromagnetics => self.em_ui.update_trails(
                    time,
                    self.plane,
                    span,
                    &gpu.context,
                    self.dirty,
                    self.run_ui.em_frame(),
                ),
                Analysis::Fluid => self.fluid_ui.update_vectors(
                    time,
                    self.plane,
                    span,
                    &gpu.context,
                    self.dirty,
                    self.run_ui.fluid_frame(),
                ),
                Analysis::Thermal => self.thermal_ui.update(
                    time,
                    self.plane,
                    span,
                    &gpu.context,
                    self.dirty,
                    self.run_ui.thermal_frame(),
                ),
            }
        }
        if animate {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
        if self.dirty {
            self.update_section(ui.ctx());
            self.dirty = false;
            ui.ctx().request_repaint();
        } else if animate {
            self.draw_section_texture();
        }
        let painter = ui.painter_at(rect);
        let section_span = self.section_span();
        let preview_bounds = self.preview_bounds();
        let fields_ready = self.fields_ready();
        let moving_surface = if self.setup {
            None
        } else {
            self.run_ui
                .timeline
                .current()
                .and_then(|f| f.surface.as_ref().zip(f.fluid.as_ref()))
                .map(|(s, f)| (f.result_buffer(), s.material_id))
        };
        if let Some(viewport) = &mut self.viewport {
            let texture = viewport.draw(
                camera,
                self.plane,
                self.cut,
                self.hatched,
                &self.model,
                &self.caps,
                ui.ctx().pixels_per_point(),
                (self.show_domain
                    || self.voxel_overlay
                    || fields_ready
                    || moving_surface.is_some()
                    || !self.em_ui.source_lines.is_empty())
                .then_some(crate::render::DomainView {
                    cad: !self.voxel_overlay && !fields_ready,
                    surface: moving_surface,
                    field: if fields_ready {
                        match self.analysis {
                            Analysis::Electromagnetics => self
                                .em_ui
                                .view(self.run_ui.em_frame(), self.volume.as_ref()),
                            Analysis::Fluid => self
                                .fluid_ui
                                .view(self.run_ui.fluid_frame(), self.volume.as_ref()),
                            Analysis::Thermal => self
                                .thermal_ui
                                .view(self.run_ui.thermal_frame(), self.volume.as_ref()),
                        }
                    } else {
                        None
                    },
                    source_lines: &self.em_ui.source_lines,
                    volume: if self.voxel_overlay || fields_ready || moving_surface.is_some() {
                        self.volume.as_ref()
                    } else {
                        None
                    },
                    material_colors: self.material_view,
                    span: section_span,
                    grid: if self.voxel_overlay && self.grid_lines {
                        self.volume
                            .as_ref()
                            .map(|v| (Vec3::from(v.settings.min_mm), v.settings.cell_mm))
                    } else {
                        None
                    },
                    bounds: preview_bounds,
                }),
            );
            painter.image(
                texture,
                rect,
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::WHITE,
            );
        }
        if self.setup {
            let accent = Stroke::new(2., Color32::from_rgb(255, 220, 100));
            match self.selection {
                Some(crate::picking::Object::Curve(i)) => {
                    for p in self.model.curves[i].points_mm.windows(2) {
                        painter.line_segment(
                            [
                                camera.project(Vec3::from(p[0])),
                                camera.project(Vec3::from(p[1])),
                            ],
                            accent,
                        );
                    }
                }
                Some(crate::picking::Object::Solid(i)) => {
                    let b = &self.model.bodies[i];
                    let mut lo = Vec3::splat(f32::INFINITY);
                    let mut hi = Vec3::splat(f32::NEG_INFINITY);
                    for p in b.triangles().iter().flatten() {
                        lo = lo.min(*p);
                        hi = hi.max(*p);
                    }
                    for axis in 0..3 {
                        for a in 0..2 {
                            for b in 0..2 {
                                let mut p = lo;
                                p[(axis + 1) % 3] = if a == 0 {
                                    lo[(axis + 1) % 3]
                                } else {
                                    hi[(axis + 1) % 3]
                                };
                                p[(axis + 2) % 3] = if b == 0 {
                                    lo[(axis + 2) % 3]
                                } else {
                                    hi[(axis + 2) % 3]
                                };
                                let mut q = p;
                                q[axis] = hi[axis];
                                painter
                                    .line_segment([camera.project(p), camera.project(q)], accent);
                            }
                        }
                    }
                }
                None => {}
            }
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
            if self.fields_ready() {
                match self.analysis {
                    Analysis::Electromagnetics => "ELECTROMAGNETIC FIELD + CAD CONTOURS",
                    Analysis::Fluid => "FLUID FIELD + CAD CONTOURS",
                    Analysis::Thermal => "TEMPERATURE / PHASE + CAD CONTOURS",
                }
            } else if self.voxel_overlay {
                "VOXELS + CAD CONTOURS"
            } else if self.hatched {
                "CAD / HATCHED SECTION"
            } else {
                "CAD / SECTION OUTLINES"
            },
            egui::FontId::monospace(12.),
            Color32::from_gray(180),
        );
        if self.show_domain {
            painter.text(
                rect.left_top() + Vec2::new(16., 36.),
                egui::Align2::LEFT_TOP,
                if self.domain_pending {
                    "DOMAIN PREVIEW · changes apply on Simulate"
                } else {
                    "DOMAIN BOX"
                },
                egui::FontId::proportional(11.),
                Color32::from_rgb(235, 174, 70),
            );
        }
        painter.text(rect.right_bottom()-Vec2::splat(14.),egui::Align2::RIGHT_BOTTOM,"Drag handles: move / rotate plane\nDrag background or right-drag: orbit · Scroll: zoom",egui::FontId::proportional(12.),Color32::from_gray(180));
    }
    fn controls(&mut self, ui: &mut egui::Ui) {
        let previous_em_config = self.em_ui.config.clone();
        let previous_fluid_config = self.fluid_ui.config.clone();
        let previous_run_config = self.run_ui.config.clone();
        if self.setup {
            ui.add_space(12.);
            ui.heading("01 / Import");
            ui.label("glTF, GLB, STL · STEP metadata only");
            ui.text_edit_singleline(&mut self.path);
            if ui.button("Load file…").clicked() {
                self.file_picker.open(&self.path);
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
                        &self.em_ui.config,
                        &self.fluid_ui.config,
                        &self.run_ui.config,
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
            ui.heading("02 / Assign properties");
            self.property_controls(ui);
            ui.separator();
            egui::CollapsingHeader::new("Domain resolution and bounds")
                .show(ui, |ui| self.domain_controls(ui));
        } else {
            ui.heading("Simulation");
            ui.small("One clock · EM ↔ fluid ↔ thermal coupling");
            let mut us = self.run_ui.config.dt_s * 1e6;
            if ui
                .add(
                    egui::DragValue::new(&mut us)
                        .speed(1.)
                        .range(1e-5..=1e9)
                        .prefix("Δt ")
                        .suffix(" µs"),
                )
                .changed()
            {
                self.run_ui.config.dt_s = us * 1e-6;
            }
            ui.checkbox(
                &mut self.run_ui.config.thermal_enabled,
                "Thermal / phase evolution",
            );
            if self.run_ui.config.thermal_enabled {
                crate::thermal_ui::ThermalUi::controls(ui, &mut self.run_ui.config.thermal);
            }
            ui.checkbox(&mut self.run_ui.config.fluid_enabled, "Fluid motion");
            ui.small("EM participates when currents, magnets or an imposed field are configured. No electricity is required for fluid-only runs. Thermal evolution can run without electricity.");
            if self.run_ui.config.fluid_enabled {
                self.fluid_ui
                    .controls(ui, &self.materials, self.run_ui.config.thermal_enabled);
            }
            egui::CollapsingHeader::new("EM numerical settings").show(ui, |ui| {
                self.em_ui.controls(ui);
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.run_ui.steps)
                        .range(1..=10000)
                        .suffix(" steps"),
                );
                if ui
                    .add_enabled(!self.run_ui.busy(), egui::Button::new("Simulate"))
                    .clicked()
                {
                    self.simulate();
                }
            });
            ui.small("Repeat Simulate to continue. Changing inputs starts a fresh run.");
        }
        ui.separator();
        ui.heading("04 / View results");
        ui.checkbox(&mut self.cut, "Cut geometry");
        ui.checkbox(&mut self.show_gizmo, "Show move / rotate handles");
        ui.small(
            "Arrows translate along world X/Y/Z. Rings rotate around those axes at the pivot.",
        );
        let fields = self.fields_ready();
        let mut selected = if fields {
            match self.analysis {
                Analysis::Electromagnetics => self.em_ui.kind + 2,
                Analysis::Fluid => self.fluid_ui.kind + 2,
                Analysis::Thermal => self.thermal_ui.kind + 2,
            }
        } else if self.voxel_overlay {
            2
        } else if self.hatched {
            0
        } else {
            1
        };
        let previous = selected;
        let view_label = |id: u32| match id {
            0 => "Hatched CAD",
            1 => "CAD outlines",
            2 => "Voxel domain",
            15 => crate::fluid_ui::label(13),
            id if id < 9 => crate::em_ui::label(id - 2),
            id if id < 12 => crate::fluid_ui::label(id - 2),
            id => crate::thermal_ui::label(id - 2),
        };
        egui::ComboBox::from_label("Section view")
            .selected_text(view_label(selected))
            .show_ui(ui, |ui| {
                for id in 0..=if self.setup { 2 } else { 15 } {
                    ui.add_enabled_ui(
                        id < 3
                            || (id < 9 && self.run_ui.em_frame().is_some())
                            || (id >= 9 && id < 12 && self.run_ui.fluid_frame().is_some())
                            || (id >= 12 && id < 15 && self.run_ui.thermal_frame().is_some())
                            || (id == 15
                                && self
                                    .run_ui
                                    .timeline
                                    .current()
                                    .is_some_and(|f| f.surface.is_some())),
                        |ui| {
                            ui.selectable_value(&mut selected, id, view_label(id));
                        },
                    );
                }
            });
        if selected != previous {
            self.em_ui.show = selected >= 3 && selected < 9;
            self.fluid_ui.show = (selected >= 9 && selected < 12) || selected == 15;
            self.thermal_ui.show = selected >= 12 && selected < 15;
            if selected == 15 {
                self.analysis = Analysis::Fluid;
                self.fluid_ui.kind = 13;
            } else if selected >= 12 {
                self.analysis = Analysis::Thermal;
                self.thermal_ui.kind = selected - 2;
            } else if selected >= 9 {
                self.analysis = Analysis::Fluid;
                self.fluid_ui.kind = selected - 2;
            } else if selected >= 3 {
                self.analysis = Analysis::Electromagnetics;
                self.em_ui.kind = selected - 2;
            } else {
                self.voxel_overlay = selected == 2;
                self.hatched = selected == 0;
            }
            self.dirty = true;
        }
        if fields || selected >= 3 {
            self.dirty |= match self.analysis {
                Analysis::Electromagnetics => self.em_ui.visualization_controls(ui),
                Analysis::Fluid => self.fluid_ui.visualization_controls(ui),
                Analysis::Thermal => self.thermal_ui.visualization_controls(ui),
            };
        } else if self.voxel_overlay {
            self.dirty |= ui
                .checkbox(&mut self.grid_lines, "Subtle cell edges")
                .changed();
            self.dirty |= ui
                .checkbox(&mut self.material_view, "Colour by material ID")
                .changed();
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
        if self.em_ui.config != previous_em_config
            || self.fluid_ui.config != previous_fluid_config
            || self.run_ui.config != previous_run_config
        {
            self.em_ui.invalidate();
            self.fluid_ui.invalidate();
            self.thermal_ui.invalidate();
            self.run_ui.invalidate();
        }
    }
}
impl App {
    fn ui(&mut self, ctx: &egui::Context) {
        self.dirty |= self.run_ui.poll(ctx, self.volume.as_ref());
        let previous_setup = self.setup;
        ctx.set_visuals(egui::Visuals::dark());
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(9.);
            ui.horizontal(|ui| {
                ui.heading("MHD / WORKBENCH");
                ui.selectable_value(&mut self.setup, true, "Setup");
                ui.selectable_value(&mut self.setup, false, "Simulation");
                if ui
                    .add_enabled(
                        !self.run_ui.busy() && !self.model.bodies.is_empty(),
                        egui::Button::new(egui::RichText::new("Simulate").strong())
                            .fill(Color32::from_rgb(30, 105, 115)),
                    )
                    .clicked()
                {
                    self.simulate();
                }

                ui.add_space(20.);
                ui.colored_label(Color32::from_rgb(75, 196, 208), "COUPLED SIMULATION");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ctx.screen_rect().width() > 850. {
                        ui.label(if self.fluid_ui.config.surface.enabled {
                            "Moving liquid / air"
                        } else {
                            "Fixed material boundary"
                        });
                    }
                });
            });
            ui.add_space(9.);
        });
        self.dirty |= self.setup != previous_setup;
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            if !self.setup {
                self.dirty |= self.run_ui.controls(ui);
            }
            ui.horizontal(|ui| {
                ui.small(
                    self.gpu
                        .as_ref()
                        .map(|g| format!("Compute: {}", g.name))
                        .unwrap_or("Compute unavailable".into()),
                );
                ui.separator();
                ui.small(format!("Section update: {:.1} ms", self.section_time));
                if !self.em_ui.error.is_empty() {
                    ui.separator();
                    ui.colored_label(Color32::LIGHT_RED, &self.em_ui.error);
                }
                if !self.run_ui.error.is_empty() {
                    ui.colored_label(Color32::LIGHT_RED, &self.run_ui.error);
                }
                if !self.fluid_ui.error.is_empty() {
                    ui.separator();
                    ui.colored_label(Color32::LIGHT_RED, &self.fluid_ui.error);
                }
            });
        });
        egui::SidePanel::left("controls")
            .exact_width((ctx.screen_rect().width() * 0.25).clamp(160., 290.))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });
        if let Some(path) = self.file_picker.show(ctx) {
            self.path = path.display().to_string();
            self.load();
        }
        self.sync_inputs();
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
                    ui.heading("Section");
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
                            if self.fields_ready() {
                                match self.analysis {Analysis::Electromagnetics=>self.em_ui.hover_probe(ui,point.to_array(),self.run_ui.em_frame()),Analysis::Fluid=>self.fluid_ui.hover_probe(ui,point.to_array(),self.run_ui.fluid_frame()),Analysis::Thermal=>self.thermal_ui.hover_probe(ui,point.to_array(),self.run_ui.thermal_frame())}
                            }
                            let pixel =
                                if self.voxel_overlay && !self.fields_ready() {
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
                    if self.fields_ready() {
                        ui.small("Vectors interpolate grid samples; force/heating/divergence retain cell values.");
                    } else if self.voxel_overlay {
                        ui.small(
                            "Air = 0 · Overlap = 255 · CAD visibility does not change the domain.",
                        );
                    } else {
                        ui.small(
                            "CAD geometry; voxel diagnostics below refer to the built domain.",
                        );
                    }
                    if self.fields_ready() {
                        match self.analysis {Analysis::Electromagnetics=>self.em_ui.panel(ui,&self.run_ui.timeline),Analysis::Fluid=>self.fluid_ui.panel(ui,self.run_ui.fluid_frame()),Analysis::Thermal=>self.thermal_ui.panel(ui,self.run_ui.thermal_frame())}
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
    fn preview_bounds(&self) -> Option<(Vec3, Vec3)> {
        if !self.show_domain {
            return None;
        }
        let dims = self.settings.dims().ok()?;
        let lo = Vec3::from(self.settings.min_mm);
        Some((
            lo,
            lo + Vec3::new(dims[0] as f32, dims[1] as f32, dims[2] as f32) * self.settings.cell_mm,
        ))
    }
    fn domain_controls(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.show_domain, "Preview domain box in 3D");
        ui.label("Uniform cell spacing");
        self.domain_pending |= ui
            .add(
                egui::DragValue::new(&mut self.settings.cell_mm)
                    .speed(0.005)
                    .range(0.001..=100.)
                    .suffix(" mm"),
            )
            .on_hover_text(
                "Smaller cells increase resolution, memory and solve time. Rebuild to apply.",
            )
            .changed();
        ui.horizontal(|ui| {
            if ui.button("2× finer").clicked() {
                self.settings.cell_mm = (self.settings.cell_mm * 0.5).max(0.001);
                self.domain_pending = true;
            }
            if ui.button("2× coarser").clicked() {
                self.settings.cell_mm = (self.settings.cell_mm * 2.).min(100.);
                self.domain_pending = true;
            }
        });
        ui.small(format!(
            "0.5 mm droplet: {:.1} cells across",
            0.5 / self.settings.cell_mm
        ));
        egui::CollapsingHeader::new("Domain bounds (mm)").show(ui, |ui| {
            for axis in 0..3 {
                ui.horizontal(|ui| {
                    ui.label(["X", "Y", "Z"][axis]);
                    self.domain_pending |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.min_mm[axis])
                                .speed(0.1)
                                .prefix("min "),
                        )
                        .changed();
                    self.domain_pending |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.max_mm[axis])
                                .speed(0.1)
                                .prefix("max "),
                        )
                        .changed();
                });
            }
            if ui.button("Fit domain to geometry").clicked() {
                let spacing = self.settings.cell_mm;
                self.settings = DomainSettings::fit(&self.model);
                self.settings.cell_mm = spacing;
                self.domain_pending = true;
            }
        });
        match self.settings.dims() {
            Ok(d) => {
                ui.monospace(format!("{} × {} × {} cells", d[0], d[1], d[2]));
                let cells = d.iter().map(|&n| n as u64).product::<u64>();
                let frame_mib = cells as f64 * 64. / 1048576.;
                let frames = self.run_ui.timeline.capacity_frames;
                ui.small(format!(
                    "Field result: {frame_mib:.1} MiB/frame · {:.1} MiB for {} frames",
                    frame_mib * frames as f64,
                    frames
                ));
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
        use crate::picking::Object;
        ui.small("Click an object in the 3D view, or select it here.");
        let previous_selection = self.selection;
        egui::ScrollArea::vertical()
            .id_salt("object_list")
            .max_height(150.)
            .show(ui, |ui| {
                for (i, b) in self.model.bodies.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.selection,
                        Some(Object::Solid(i)),
                        format!("Solid · {}", b.name),
                    );
                }
                for (i, c) in self.model.curves.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.selection,
                        Some(Object::Curve(i)),
                        format!("Centreline · {}", c.name),
                    );
                }
            });
        if self.selection != previous_selection {
            if let Some(Object::Solid(i)) = self.selection {
                self.material_selection = self.model.bodies[i].simulation.material_id;
            }
        }
        match self.selection {
            Some(Object::Solid(i)) => {
                let body = &mut self.model.bodies[i];
                ui.strong(&body.name);
                self.dirty |= ui.checkbox(&mut body.visible, "Show CAD").changed();
                self.domain_pending |= ui
                    .checkbox(&mut body.simulation.included, "Include in domain")
                    .changed();
                let name = self
                    .materials
                    .iter()
                    .find(|m| m.id == body.simulation.material_id)
                    .map(|m| m.name.as_str())
                    .unwrap_or("Unassigned");
                let previous_material = body.simulation.material_id;
                egui::ComboBox::from_label("Material")
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for m in self.materials.iter().filter(|m| m.id != 0) {
                            self.inputs_pending |= ui
                                .selectable_value(&mut body.simulation.material_id, m.id, &m.name)
                                .changed();
                        }
                    });
                if body.simulation.material_id != previous_material {
                    self.material_selection = body.simulation.material_id;
                }
                if ui.button("Edit assigned material").clicked() {
                    self.material_selection = body.simulation.material_id;
                }
                self.inputs_pending |= optional(
                    ui,
                    "Initial temperature",
                    &mut body.simulation.temperature_c,
                    20.,
                    1.,
                    " °C",
                );
                if !self.run_ui.config.thermal_enabled {
                    ui.small(
                        "Enable Thermal / phase evolution in Simulation to evolve temperature.",
                    );
                }
                self.inputs_pending |= optional(
                    ui,
                    "Hold temperature (thermostat)",
                    &mut body.simulation.fixed_temperature_c,
                    250.,
                    1.,
                    " °C",
                );
                self.inputs_pending |= ui
                    .add(
                        egui::DragValue::new(&mut body.simulation.heat_source_w_m3)
                            .speed(1e5)
                            .prefix("Body heating ")
                            .suffix(" W/m³"),
                    )
                    .changed();
                if let Some(m) = self
                    .materials
                    .iter()
                    .find(|m| m.id == body.simulation.material_id)
                {
                    if let Some(f) = body
                        .simulation
                        .temperature_c
                        .and_then(|t| m.thermal.liquid_fraction(t))
                    {
                        ui.small(format!("Initial liquid fraction: {:.1}%", f * 100.));
                    }
                }
                self.dirty |= self.em_ui.body_source_properties(ui, i, body.closed());
            }
            Some(Object::Curve(i)) => {
                self.dirty |= self.em_ui.curve_properties(ui, &self.model, Some(i));
            }
            None => {
                ui.small("Select an object to edit its properties.");
            }
        }
        ui.separator();
        ui.heading("Materials");
        ui.small("Objects reference these shared materials. Edits affect every assigned object.");
        let name = self
            .materials
            .iter()
            .find(|m| m.id == self.material_selection)
            .map(|m| m.name.as_str())
            .unwrap_or("Select material");
        egui::ComboBox::from_id_salt("library_material")
            .selected_text(name)
            .show_ui(ui, |ui| {
                for m in &self.materials {
                    ui.selectable_value(&mut self.material_selection, m.id, &m.name);
                }
            });
        if let Some(m) = self
            .materials
            .iter_mut()
            .find(|m| m.id == self.material_selection)
        {
            let users = self
                .model
                .bodies
                .iter()
                .filter(|b| b.simulation.material_id == m.id)
                .count();
            ui.small(format!("Assigned to {users} objects"));
            self.inputs_pending |= ui.text_edit_singleline(&mut m.name).changed();
            self.inputs_pending |=
                optional(ui, "Conductivity", &mut m.conductivity_s_m, 0., 1e5, " S/m");
            self.inputs_pending |= optional(
                ui,
                "Relative permeability",
                &mut m.relative_permeability,
                1.,
                0.1,
                "",
            );
            self.inputs_pending |=
                optional(ui, "Density", &mut m.density_kg_m3, 1000., 10., " kg/m³");
            self.inputs_pending |= optional(
                ui,
                "Liquid viscosity",
                &mut m.dynamic_viscosity_pa_s,
                0.002,
                0.0001,
                " Pa·s",
            );
            egui::CollapsingHeader::new("Thermal / phase properties").show(ui, |ui| {
                ui.small("Thermal transport uses these values. Latent heat is required before crossing a defined melting interval.");
                self.inputs_pending |= optional(
                    ui,
                    "Solidus / melting onset",
                    &mut m.thermal.solidus_c,
                    200.,
                    1.,
                    " °C",
                );
                self.inputs_pending |= optional(
                    ui,
                    "Liquidus / fully liquid",
                    &mut m.thermal.liquidus_c,
                    200.,
                    1.,
                    " °C",
                );
                self.inputs_pending |= optional(
                    ui,
                    "Heat capacity",
                    &mut m.thermal.heat_capacity_j_kg_k,
                    1000.,
                    10.,
                    " J/(kg K)",
                );
                self.inputs_pending |= optional(
                    ui,
                    "Thermal conductivity",
                    &mut m.thermal.conductivity_w_m_k,
                    100.,
                    1.,
                    " W/(m K)",
                );
                self.inputs_pending |= optional(
                    ui,
                    "Latent heat",
                    &mut m.thermal.latent_heat_j_kg,
                    50000.,
                    100.,
                    " J/kg",
                );
            });
            if !m.notes.is_empty() {
                ui.small(&m.notes);
            }
            if let Some(r) = &m.reference {
                ui.hyperlink_to("Reference", r);
            }
        }
        if ui.button("Create material").clicked() {
            if let Some(id) = (1..255).find(|id| self.materials.iter().all(|m| m.id != *id)) {
                self.materials.push(Material {
                    id,
                    name: format!("Material {id}"),
                    conductivity_s_m: None,
                    relative_permeability: None,
                    density_kg_m3: None,
                    dynamic_viscosity_pa_s: None,
                    thermal: Default::default(),
                    notes: String::new(),
                    reference: None,
                });
                self.material_selection = id;
                self.inputs_pending = true;
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

#[cfg(test)]
mod project_ui_tests {
    use super::*;
    #[test]
    fn unified_run_records_synchronous_frames_and_continues_from_latest() {
        let _lock = crate::gpu_test_lock();
        let mut app = App::new("assets/hotend-cad.mhd.json".into(), None);
        assert!(app.error.is_empty(), "{}", app.error);
        let ctx = egui::Context::default();
        let wait = |app: &mut App| {
            let start = std::time::Instant::now();
            while app.run_ui.busy() {
                app.run_ui.poll(&ctx, app.volume.as_ref());
                assert!(start.elapsed().as_secs() < 30);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(app.run_ui.error.is_empty(), "{}", app.run_ui.error);
        };
        app.run_ui.steps = 3;
        app.simulate();
        wait(&mut app);
        assert_eq!(app.run_ui.timeline.samples.len(), 4);
        for sample in &app.run_ui.timeline.samples {
            let f = &sample.output;
            assert_eq!(
                f.em.as_ref().unwrap().time_s,
                f.fluid.as_ref().unwrap().time_s
            );
        }
        assert!(app.run_ui.fluid_frame().unwrap().diagnostics.max_speed_m_s > 0.);
        app.run_ui.timeline.selected = 0;
        app.run_ui.poll(&ctx, app.volume.as_ref());
        assert_eq!(app.run_ui.em_frame().unwrap().time_s, 0.);
        assert_eq!(app.run_ui.fluid_frame().unwrap().time_s, 0.);
        app.run_ui.steps = 1;
        app.simulate();
        wait(&mut app);
        assert_eq!(app.run_ui.timeline.samples.len(), 5);
        assert_eq!(app.run_ui.timeline.selected, 0);
        assert!((app.run_ui.timeline.samples.last().unwrap().time_s - 4e-5).abs() < 1e-12);
        app.run_ui.timeline.capacity_frames = 2;
        app.simulate();
        wait(&mut app);
        assert_eq!(app.run_ui.timeline.samples.len(), 2);
        for sample in &app.run_ui.timeline.samples {
            assert_eq!(sample.output.em.as_ref().unwrap().time_s, sample.time_s);
            assert_eq!(sample.output.fluid.as_ref().unwrap().time_s, sample.time_s);
        }
        let serial = app.volume.as_ref().unwrap().build_serial;
        app.materials[3].dynamic_viscosity_pa_s = Some(0.003);
        app.inputs_pending = true;
        app.sync_inputs();
        assert!(app.run_ui.timeline.samples.is_empty());
        assert!(app.run_ui.em_frame().is_none());
        assert!(app.run_ui.fluid_frame().is_none());
        assert_eq!(serial, app.volume.as_ref().unwrap().build_serial);
        app.em_ui.config.wires.clear();
        app.em_ui.config.magnets.clear();
        app.em_ui.config.uniform.clear();
        app.simulate();
        wait(&mut app);
        assert!(app.run_ui.em_frame().is_none());
        assert_eq!(app.run_ui.timeline.samples.len(), 2);
    }
    #[test]
    fn cad_import_properties_simulate_rebuilds_and_runs_headlessly() {
        let _lock = crate::gpu_test_lock();
        let mut app = App::new("assets/annulus.gltf".into(), None);
        let serial = app.volume.as_ref().unwrap().build_serial;
        app.model.curves = Model::load(std::path::Path::new("assets/coil-centreline.gltf"))
            .unwrap()
            .curves;
        app.em_ui.config.wires = crate::em::cad_wires(&app.model);
        app.em_ui.config.wires[0].current = crate::em::Waveform::Constant { value: 10. };
        app.run_ui.steps = 1;
        for material in &mut app.materials {
            material.conductivity_s_m = Some(0.);
            material.relative_permeability = Some(1.);
        }
        app.inputs_pending = true;
        app.settings.cell_mm *= 2.;
        app.settings.max_mm[0] += 1.;
        app.domain_pending = true;
        let proposed = app.preview_bounds().unwrap();
        assert_ne!(proposed.1, app.volume.as_ref().unwrap().max());
        app.simulate();
        assert!(!app.domain_pending && !app.inputs_pending, "{}", app.error);
        assert_ne!(serial, app.volume.as_ref().unwrap().build_serial);
        assert_eq!(proposed.1, app.volume.as_ref().unwrap().max());
        let context = egui::Context::default();
        let start = std::time::Instant::now();
        while app.run_ui.busy() {
            app.run_ui.poll(&context, app.volume.as_ref());
            assert!(start.elapsed().as_secs() < 20);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(app.em_ui.error.is_empty(), "{}", app.em_ui.error);
        assert!(app.em_ui.ready(app.run_ui.em_frame(), app.volume.as_ref()));
        assert_eq!(app.run_ui.timeline.samples.len(), 2);
        assert!(app.run_ui.em_frame().unwrap().diagnostics.maxima[0] > 1e-6);
        app.show_domain = false;
        assert!(app.preview_bounds().is_none());
    }
    #[test]
    fn property_and_source_ui_updates_invalidate_fields_without_rebuilding() {
        let _lock = crate::gpu_test_lock();
        let mut app = App::new("assets/annulus.gltf".into(), None);
        assert!(app.error.is_empty(), "{}", app.error);
        let serial = app.volume.as_ref().unwrap().build_serial;
        let original = app.volume.as_ref().unwrap().inputs.versions();
        app.fields.computed_from = Some(original);
        app.materials[1].conductivity_s_m = Some(1e6);
        app.inputs_pending = true;
        app.sync_inputs();
        assert!(!app.domain_pending && !app.inputs_pending);
        assert!(app.fields.computed_from.is_none());
        let after_material = app.volume.as_ref().unwrap().inputs.versions();
        assert_ne!(after_material.materials, original.materials);
        assert_eq!(
            after_material.initial_conditions,
            original.initial_conditions
        );
        app.fields.computed_from = Some(after_material);
        app.model.bodies[0].simulation.temperature_c = Some(250.);
        app.inputs_pending = true;
        app.sync_inputs();
        let after_temperature = app.volume.as_ref().unwrap().inputs.versions();
        assert_eq!(after_temperature.materials, after_material.materials);
        assert_ne!(
            after_temperature.initial_conditions,
            after_material.initial_conditions
        );
        assert!(app.fields.computed_from.is_none());
        assert_eq!(app.volume.as_ref().unwrap().build_serial, serial);
        app.fields.computed_from = Some(after_temperature);
        app.materials[1].conductivity_s_m = Some(-1.);
        app.inputs_pending = true;
        app.sync_inputs();
        assert!(app.inputs_pending);
        assert!(!app.error.is_empty());
        assert_eq!(
            app.volume.as_ref().unwrap().inputs.versions(),
            after_temperature
        );
        assert!(app.fields.computed_from.is_none());
        app.materials[1].conductivity_s_m = Some(1e6);
        app.sync_inputs();
        assert!(app.error.is_empty());
        app.model.bodies[0].simulation.included = false;
        app.domain_pending = true;
        app.sync_inputs();
        assert!(app.domain_pending);
        assert_eq!(app.volume.as_ref().unwrap().build_serial, serial);
    }
    #[test]
    fn project_startup_and_geometry_replacement_clear_cached_sections() {
        let _gpu_lock = crate::gpu_test_lock();
        let model = Model::load(std::path::Path::new("assets/hotend.glb")).unwrap();
        let settings = DomainSettings::fit(&model);
        let materials = crate::simulation::default_materials();
        std::fs::create_dir_all("artifacts").unwrap();
        let file = std::path::Path::new("artifacts/startup.mhd.json");
        project::save(
            file,
            &model,
            &settings,
            &materials,
            SectionPlane::new(Vec3::new(0.1, 0.2, 4.)),
            &Default::default(),
            &Default::default(),
            &Default::default(),
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

#[cfg(test)]
mod ui_render_tests {
    use super::*;
    #[test]
    fn setup_and_simulation_render_headlessly_with_file_picker() {
        let _lock = crate::gpu_test_lock();
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) = pollster::block_on(
            adapter.request_device(&crate::gpu::device_descriptor(&adapter), None),
        )
        .unwrap();
        let renderer =
            egui_wgpu::Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, None, 1, false);
        let state = egui_wgpu::RenderState {
            adapter,
            available_adapters: vec![],
            device,
            queue,
            target_format: wgpu::TextureFormat::Rgba8Unorm,
            renderer: std::sync::Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
        };
        let mut app = App::new("assets/hotend-cad.mhd.json".into(), Some(state.clone()));
        app.selection = Some(crate::picking::Object::Solid(1));
        app.material_selection = 3;
        let ctx = egui::Context::default();
        let mut tick = 0;
        for mode in 0..7 {
            app.setup = mode != 1 && mode < 3;
            if mode == 1 {
                app.run_ui.steps = 3;
                app.simulate();
                let start = std::time::Instant::now();
                while app.run_ui.busy() {
                    app.run_ui.poll(&ctx, app.volume.as_ref());
                    assert!(start.elapsed().as_secs() < 30);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                assert!(app.run_ui.error.is_empty(), "{}", app.run_ui.error);
                app.analysis = Analysis::Fluid;
                app.em_ui.show = false;
                app.fluid_ui.show = true;
                app.dirty = true;
            }
            if mode == 2 {
                app.file_picker.open("assets/hotend-cad.mhd.json");
            }
            if mode == 3 {
                app = App::new("assets/hotend-thermal.mhd.json".into(), Some(state.clone()));
                app.setup = false;
                app.run_ui.steps = 3;
                app.simulate();
                let start = std::time::Instant::now();
                while app.run_ui.busy() {
                    app.run_ui.poll(&ctx, app.volume.as_ref());
                    assert!(start.elapsed().as_secs() < 60);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                assert!(app.run_ui.error.is_empty(), "{}", app.run_ui.error);
                let frame = app.run_ui.thermal_frame().unwrap();
                assert!(frame.diagnostics.max_temperature_k > 523.);
                assert!(app.thermal_ui.ready(Some(frame), app.volume.as_ref()));
                assert!((frame.time_s - 0.003).abs() < 1e-8);
                app.analysis = Analysis::Thermal;
                app.thermal_ui.kind = 10;
                app.dirty = true;
            }
            if mode == 4 {
                app = App::new(
                    "assets/hotend-free-surface.mhd.json".into(),
                    Some(state.clone()),
                );
                app.setup = false;
                app.run_ui.steps = 2;
                app.simulate();
                let start = std::time::Instant::now();
                while app.run_ui.busy() {
                    app.run_ui.poll(&ctx, app.volume.as_ref());
                    assert!(start.elapsed().as_secs() < 90);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                assert!(app.run_ui.error.is_empty(), "{}", app.run_ui.error);
                assert!(app.run_ui.timeline.current().unwrap().surface.is_some());
                app.analysis = Analysis::Fluid;
                app.fluid_ui.kind = 13;
                app.fluid_ui.show = true;
                app.dirty = true;
            }
            if mode >= 5 {
                app.setup = false;
                app.fluid_ui.show = false;
                app.thermal_ui.show = false;
                app.em_ui.show = false;
                app.voxel_overlay = false;
                app.hatched = mode == 5;
                app.dirty = true;
            }
            for _ in 0..6 {
                tick += 1;
                let output = ctx.run(
                    egui::RawInput {
                        time: Some(tick as f64 / 30.),
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1280., 900.),
                        )),
                        ..Default::default()
                    },
                    |ctx| app.ui(ctx),
                );
                let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
                let target = state.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Headless workbench screenshot"),
                    size: wgpu::Extent3d {
                        width: 1280,
                        height: 900,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: state.target_format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 1280 * 900 * 4,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = state.device.create_command_encoder(&Default::default());
                let screen = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [1280, 900],
                    pixels_per_point: output.pixels_per_point,
                };
                let mut renderer = state.renderer.write();
                for (id, delta) in output.textures_delta.set {
                    renderer.update_texture(&state.device, &state.queue, id, &delta);
                }
                let commands = renderer.update_buffers(
                    &state.device,
                    &state.queue,
                    &mut encoder,
                    &jobs,
                    &screen,
                );
                {
                    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &target.create_view(&Default::default()),
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    renderer.render(&mut pass.forget_lifetime(), &jobs, &screen);
                }
                encoder.copy_texture_to_buffer(
                    target.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(1280 * 4),
                            rows_per_image: Some(900),
                        },
                    },
                    target.size(),
                );
                state
                    .queue
                    .submit(commands.into_iter().chain(Some(encoder.finish())));
                for id in output.textures_delta.free {
                    renderer.free_texture(&id);
                }
                drop(renderer);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |v| tx.send(v).unwrap());
                state.device.poll(wgpu::Maintain::Wait);
                rx.recv().unwrap().unwrap();
                let data = buffer.slice(..).get_mapped_range();
                let mut ppm = b"P6\n1280 900\n255\n".to_vec();
                for p in data.chunks_exact(4) {
                    ppm.extend_from_slice(&p[..3]);
                }
                std::fs::create_dir_all("artifacts").unwrap();
                std::fs::write(format!("artifacts/workflow-{mode}.ppm"), ppm).unwrap();
                drop(data);
                buffer.unmap();
            }
        }
    }
}

#[cfg(test)]
mod selection_ui_tests {
    use super::*;
    #[test]
    fn a_primary_click_selects_geometry_and_its_shared_material() {
        let _lock = crate::gpu_test_lock();
        let mut app = App::new("assets/annulus.gltf".into(), None);
        app.show_gizmo = false;
        app.model.bodies[0].simulation.material_id = 2;
        let ctx = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(600., 600.))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input(vec![]), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = ui.available_rect_before_wrap();
                app.viewport(ui);
            });
        });
        let camera = app.camera(rect);
        let point = (0..50)
            .flat_map(|y| {
                (0..50)
                    .map(move |x| rect.min + Vec2::new(x as f32 * 11. + 10., y as f32 * 11. + 10.))
            })
            .find(|p| {
                crate::picking::pick(&app.model, camera, *p, app.cut, app.plane)
                    == Some(crate::picking::Object::Solid(0))
            })
            .expect("visible annulus surface");
        for pressed in [true, false] {
            let _ = ctx.run(
                input(vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ]),
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        app.viewport(ui);
                    });
                },
            );
        }
        assert_eq!(app.selection, Some(crate::picking::Object::Solid(0)));
        assert_eq!(app.material_selection, 2);
    }
}
