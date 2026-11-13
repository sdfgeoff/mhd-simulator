use crate::{gpu::Gpu, model::Model};
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use glam::{Quat, Vec3};
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
    yaw: f32,
    pitch: f32,
    zoom: f32,
    cut: bool,
    azimuth: f32,
    elevation: f32,
    offset: f32,
    resolution: u32,
    texture: Option<egui::TextureHandle>,
    dirty: bool,
    section_time: f64,
    overlaps: usize,
}
impl App {
    pub fn new(path: String) -> Self {
        let (model, error) = match Model::load(std::path::Path::new(&path)) {
            Ok(m) => (m, String::new()),
            Err(e) => (Model::default(), e),
        };
        let (gpu, gpu_error) = match Gpu::new() {
            Ok(g) => (Some(g), String::new()),
            Err(e) => (None, e),
        };
        Self {
            model,
            path,
            error: format!("{error}{gpu_error}"),
            gpu,
            yaw: -0.7,
            pitch: -0.45,
            zoom: 1.,
            cut: true,
            azimuth: 0.,
            elevation: 0.,
            offset: 0.,
            resolution: 128,
            texture: None,
            dirty: true,
            section_time: 0.,
            overlaps: 0,
        }
    }
    fn plane(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        let (lo, hi) = self.model.bounds();
        let center = (lo + hi) * 0.5;
        let span = (hi - lo).max_element() * 0.65;
        let a = self.azimuth.to_radians();
        let e = self.elevation.to_radians();
        let normal = Vec3::new(e.cos() * a.cos(), e.cos() * a.sin(), e.sin());
        let u = Vec3::new(-a.sin(), a.cos(), 0.);
        let v = normal.cross(u);
        (center + normal * self.offset, normal, u * span, v * span)
    }
    fn update_section(&mut self, ctx: &egui::Context) {
        if self.model.bodies.is_empty() {
            self.texture = None;
            return;
        }
        let (origin, _, u, v) = self.plane();
        let start = std::time::Instant::now();
        let Some(gpu) = &self.gpu else {
            return;
        };
        match gpu.section(&self.model, origin, u, v, self.resolution) {
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
    fn viewport(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8., Color32::from_rgb(14, 21, 32));
        if response.dragged() {
            let d = ui.input(|i| i.pointer.delta());
            if ui.input(|i| i.pointer.primary_down()) {
                self.yaw += d.x * 0.008;
                self.pitch = (self.pitch + d.y * 0.008).clamp(-1.5, 1.5);
            }
        }
        if response.hovered() {
            self.zoom =
                (self.zoom * ui.input(|i| (i.smooth_scroll_delta.y * 0.001).exp())).clamp(0.2, 8.);
        }
        let (lo, hi) = self.model.bounds();
        let center = (lo + hi) * 0.5;
        let size = (hi - lo).max_element().max(1.);
        let rot = Quat::from_rotation_x(self.pitch) * Quat::from_rotation_z(self.yaw);
        let scale = rect.width().min(rect.height()) * 0.68 * self.zoom / size;
        let project = |p: Vec3| {
            let q = rot * (p - center);
            (
                Pos2::new(rect.center().x + q.x * scale, rect.center().y - q.z * scale),
                q.y,
            )
        };
        for i in -5..=5 {
            let a = i as f32 * size * 0.1;
            let z = lo.z - size * 0.04;
            for (p, q) in [
                (
                    Vec3::new(center.x + a, center.y - size * 0.5, z),
                    Vec3::new(center.x + a, center.y + size * 0.5, z),
                ),
                (
                    Vec3::new(center.x - size * 0.5, center.y + a, z),
                    Vec3::new(center.x + size * 0.5, center.y + a, z),
                ),
            ] {
                painter.line_segment(
                    [project(p).0, project(q).0],
                    Stroke::new(1., Color32::from_rgb(29, 42, 57)),
                );
            }
        }
        let (origin, normal, plane_u, plane_v) = self.plane();
        let mut faces = Vec::new();
        for (bi, b) in self
            .model
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.visible)
        {
            for t in &b.triangles {
                let projected = t.map(|p| project(p).0);
                let a = projected[1] - projected[0];
                let b2 = projected[2] - projected[0];
                let signed_area = a.x * b2.y - a.y * b2.x;
                if signed_area * b.winding >= -1e-6 {
                    continue;
                }
                let poly = if self.cut {
                    clip(t, origin, normal)
                } else {
                    t.to_vec()
                };
                if poly.len() < 3 {
                    continue;
                }
                let shade = (0.4
                    + 0.6
                        * (t[1] - t[0])
                            .cross(t[2] - t[0])
                            .normalize()
                            .dot(Vec3::new(0.3, -0.6, 0.74))
                            .abs())
                .clamp(0., 1.);
                let c = COLORS[bi % COLORS.len()];
                let c = Color32::from_rgb(
                    (c[0] as f32 * shade) as u8,
                    (c[1] as f32 * shade) as u8,
                    (c[2] as f32 * shade) as u8,
                );
                let depth = poly.iter().map(|&p| project(p).1).sum::<f32>() / poly.len() as f32;
                faces.push((
                    depth,
                    poly.iter().map(|&p| project(p).0).collect::<Vec<_>>(),
                    c,
                ));
            }
        }
        faces.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, points, c) in faces {
            // A clipped face can become a very thin polygon. Use explicit
            // triangles: polygon antialiasing fringes are unstable on slivers.
            let mut mesh = egui::Mesh::default();
            for &point in &points {
                mesh.colored_vertex(point, c);
            }
            for i in 1..points.len() - 1 {
                mesh.add_triangle(0, i as u32, i as u32 + 1);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        let corners = [
            origin - plane_u - plane_v,
            origin + plane_u - plane_v,
            origin + plane_u + plane_v,
            origin - plane_u + plane_v,
        ];
        for i in 0..4 {
            painter.line_segment(
                [project(corners[i]).0, project(corners[(i + 1) % 4]).0],
                Stroke::new(1., Color32::from_rgb(64, 119, 130)),
            );
        }
        painter.circle_filled(project(origin).0, 3., Color32::from_rgb(75, 196, 208));
        let axes = [
            (Vec3::X, "X", Color32::LIGHT_RED),
            (Vec3::Y, "Y", Color32::LIGHT_GREEN),
            (Vec3::Z, "Z", Color32::LIGHT_BLUE),
        ];
        let base = rect.left_bottom() + Vec2::new(50., -50.);
        for (axis, name, c) in axes {
            let q = rot * axis;
            let tip = base + Vec2::new(q.x, -q.z) * 30.;
            painter.line_segment([base, tip], Stroke::new(2., c));
            painter.text(
                tip,
                egui::Align2::CENTER_CENTER,
                name,
                egui::FontId::monospace(12.),
                c,
            );
        }
        painter.text(
            rect.left_top() + Vec2::splat(16.),
            egui::Align2::LEFT_TOP,
            "GEOMETRY  /  mm",
            egui::FontId::monospace(12.),
            Color32::from_gray(160),
        );
        painter.text(
            rect.right_bottom() - Vec2::splat(16.),
            egui::Align2::RIGHT_BOTTOM,
            "Drag to orbit · Scroll to zoom",
            egui::FontId::proportional(12.),
            Color32::from_gray(150),
        );
    }
}
fn clip(t: &[Vec3; 3], origin: Vec3, normal: Vec3) -> Vec<Vec3> {
    let mut out = Vec::new();
    for i in 0..3 {
        let a = t[i];
        let b = t[(i + 1) % 3];
        let da = (a - origin).dot(normal);
        let db = (b - origin).dot(normal);
        if da <= 0. {
            out.push(a);
        }
        if (da <= 0.) != (db <= 0.) {
            out.push(a + (b - a) * (da / (da - db)));
        }
    }
    out
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(9.);
            ui.horizontal(|ui| {
                ui.heading("MHD / WORKBENCH");
                ui.add_space(20.);
                ui.label(
                    egui::RichText::new("CAD + GPU SECTIONS")
                        .color(Color32::from_rgb(75, 196, 208)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ctx.screen_rect().width() > 850. {
                        ui.label("Prototype 0.1 · Geometry only");
                    }
                });
            });
            ui.add_space(9.);
        });
        egui::SidePanel::left("controls").exact_width((ctx.screen_rect().width()*0.25).clamp(160.,290.)).show(ctx,|ui|{ egui::ScrollArea::vertical().show(ui,|ui|{
            ui.add_space(12.);ui.heading("01 / Assembly");ui.label("Load glTF, GLB or STL. STEP: metadata only.");
            ui.text_edit_singleline(&mut self.path);
            if ui.button("Load file").clicked(){match Model::load(std::path::Path::new(&self.path)){Ok(m)=>{self.model=m;self.error.clear();self.dirty=true;self.offset=0.;},Err(e)=>self.error=e}}
            ui.label("Or drop a file into this window.");
            let (lo,hi)=self.model.bounds();let d=hi-lo;if !self.model.bodies.is_empty(){ui.monospace(format!("{:.3} × {:.3} × {:.3} mm",d.x,d.y,d.z));}
            ui.add_space(12.);
            for (i,b) in self.model.bodies.iter_mut().enumerate(){ui.horizontal(|ui|{let c=COLORS[i%6];ui.colored_label(Color32::from_rgb(c[0],c[1],c[2]),"■");self.dirty|=ui.checkbox(&mut b.visible,&b.name).changed();});
                let roles=["Unassigned","Chamber wall","Molten metal","Coil","Permanent magnet"];
                egui::ComboBox::from_id_salt(i).selected_text(roles[b.role]).show_ui(ui,|ui|{for (r,name) in roles.iter().enumerate(){ui.selectable_value(&mut b.role,r,*name);}});
                ui.small(format!("{} triangles · {}",b.triangles.len(),if b.closed(){"edge closure passed"}else{"OPEN / INVALID"}));ui.add_space(5.);
            }
            ui.separator();ui.heading("02 / Cut plane");
            ui.checkbox(&mut self.cut,"Clip geometry at plane");
            self.dirty|=ui.add(egui::Slider::new(&mut self.azimuth,-180.0..=180.0).text("Azimuth °")).changed();
            self.dirty|=ui.add(egui::Slider::new(&mut self.elevation,-90.0..=90.0).text("Elevation °")).changed();
            let extent=d.length()*0.5;
            self.dirty|=ui.add(egui::Slider::new(&mut self.offset,-extent..=extent).text("Offset mm")).changed();
            ui.horizontal(|ui|{for (name,a,e) in [("YZ",0.,0.),("XZ",90.,0.),("XY",0.,90.)]{if ui.button(name).clicked(){self.azimuth=a;self.elevation=e;self.offset=0.;self.dirty=true;}}});
            egui::ComboBox::from_label("Section pixels").selected_text(self.resolution.to_string()).show_ui(ui,|ui|{for n in [64,128,256]{self.dirty|=ui.selectable_value(&mut self.resolution,n,n.to_string()).changed();}});
            if ui.button("Reset camera").clicked(){self.yaw=-0.7;self.pitch=-0.45;self.zoom=1.;}
            ui.add_space(12.);ui.small("Roles are session-only labels in this preview. Physical properties, field solvers and droplet dynamics come in later milestones.");
        }); });
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
                    "Section: {:.1} ms · {} overlap pixels",
                    self.section_time, self.overlaps
                ));
            });
        });
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()))
        {
            self.path = path.display().to_string();
            match Model::load(&path) {
                Ok(m) => {
                    self.model = m;
                    self.offset = 0.;
                    self.dirty = true;
                }
                Err(e) => self.error = e,
            }
        }
        if self.dirty {
            self.update_section(ctx);
            self.dirty = false;
        }
        egui::SidePanel::right("section").exact_width((ctx.screen_rect().width()*0.30).clamp(160.,340.)).show(ctx,|ui|{ui.add_space(12.);ui.heading("03 / GPU cross-section");ui.label("Closed-body occupancy on the cut plane");ui.add_space(8.);
            if let Some(t)=&self.texture{ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::splat(ui.available_width().min(320.))));}else{ui.label("No valid solid cross-section available.");}
            ui.small("Background = air / void · Pink = overlap");ui.small("Pixel centres sampled by GPU ray parity. This is geometry classification, not a magnetic field.");ui.separator();
            if !self.error.is_empty(){ui.colored_label(Color32::LIGHT_RED,&self.error);}
            ui.small(format!("Source: {}",self.model.source));ui.heading("Import notes");egui::ScrollArea::vertical().show(ui,|ui|{for s in &self.model.notices{ui.label(s);ui.add_space(5.);}});
        });
        egui::CentralPanel::default().show(ctx, |ui| self.viewport(ui));
    }
}
