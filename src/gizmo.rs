use crate::plane::SectionPlane;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use glam::{Mat4, Quat, Vec3, Vec4};
#[derive(Clone, Copy)]
pub struct Camera {
    pub rect: Rect,
    pub center: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    pub size: f32,
}
impl Camera {
    pub fn project(self, p: Vec3) -> Pos2 {
        let q = self.rotation * (p - self.center);
        Pos2::new(
            self.rect.center().x + q.x * self.scale,
            self.rect.center().y - q.z * self.scale,
        )
    }
    pub fn vector(self, v: Vec3) -> Vec2 {
        let q = self.rotation * v;
        Vec2::new(q.x, -q.z) * self.scale
    }
    pub fn ray(self, p: Pos2) -> (Vec3, Vec3) {
        let v = p - self.rect.center();
        (
            self.center
                + self.rotation.inverse()
                    * Vec3::new(v.x / self.scale, -self.size * 4., -v.y / self.scale),
            self.rotation.inverse() * Vec3::Y,
        )
    }
    pub fn matrix(self) -> Mat4 {
        let projection = Mat4::from_cols(
            Vec4::new(2. * self.scale / self.rect.width(), 0., 0., 0.),
            Vec4::new(0., 0., 1. / (self.size * 8.), 0.),
            Vec4::new(0., 2. * self.scale / self.rect.height(), 0., 0.),
            Vec4::new(0., 0., 0.5, 1.),
        );
        projection * Mat4::from_quat(self.rotation) * Mat4::from_translation(-self.center)
    }
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Handle {
    Move(usize),
    Rotate(usize),
}
#[derive(Clone, Copy)]
struct Drag {
    handle: Handle,
    initial: SectionPlane,
    start: Pos2,
    radial: Vec3,
}
#[derive(Default)]
pub struct Gizmo {
    drag: Option<Drag>,
}
const AXES: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
const COLORS: [Color32; 3] = [
    Color32::from_rgb(247, 113, 123),
    Color32::from_rgb(133, 218, 151),
    Color32::from_rgb(117, 179, 255),
];
fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0., 1.);
    p.distance(a + ab * t)
}
fn ring_point(axis: usize, angle: f32) -> Vec3 {
    AXES[(axis + 1) % 3] * angle.cos() + AXES[(axis + 2) % 3] * angle.sin()
}
fn radial(camera: Camera, pointer: Pos2, pivot: Vec3, axis: Vec3) -> Option<Vec3> {
    let (o, d) = camera.ray(pointer);
    let denom = d.dot(axis);
    if denom.abs() < 0.08 {
        return None;
    }
    let p = o + d * ((pivot - o).dot(axis) / denom);
    (p - pivot).try_normalize()
}
impl Gizmo {
    fn hit(&self, camera: Camera, plane: SectionPlane, p: Pos2) -> Option<Handle> {
        let pivot = camera.project(plane.position);
        let length = 82. / camera.scale;
        for (i, axis) in AXES.iter().enumerate() {
            let end = camera.project(plane.position + *axis * length);
            let start = pivot + (end - pivot) * 0.22;
            if end.distance(pivot) > 18. && segment_distance(p, start, end) < 8. {
                return Some(Handle::Move(i));
            }
        }
        let radius = 59. / camera.scale;
        let mut best = (8., None);
        for axis in 0..3 {
            for i in 0..96 {
                let a = camera.project(
                    plane.position
                        + ring_point(axis, i as f32 * std::f32::consts::TAU / 96.) * radius,
                );
                let b = camera.project(
                    plane.position
                        + ring_point(axis, (i + 1) as f32 * std::f32::consts::TAU / 96.) * radius,
                );
                let d = segment_distance(p, a, b);
                if d < best.0 {
                    best = (d, Some(Handle::Rotate(axis)));
                }
            }
        }
        best.1
    }
    /// Uses the viewport's drag response, so a handle drag cannot orbit the camera.
    pub fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        camera: Camera,
        plane: &mut SectionPlane,
    ) -> (bool, bool) {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mut changed = false;
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(start) = ui.input(|i| i.pointer.press_origin()) {
                if let Some(handle) = self.hit(camera, *plane, start) {
                    let radial = match handle {
                        Handle::Rotate(i) => radial(camera, start, plane.position, AXES[i])
                            .unwrap_or_else(|| {
                                let q = start - camera.project(plane.position);
                                let (_, d) = camera.ray(start);
                                let screen = camera.rotation.inverse() * Vec3::new(q.x, 0., -q.y);
                                let r = screen - AXES[i] * screen.dot(AXES[i]);
                                r.try_normalize()
                                    .unwrap_or(d.cross(AXES[i]).normalize_or_zero())
                            }),
                        _ => Vec3::ZERO,
                    };
                    self.drag = Some(Drag {
                        handle,
                        initial: *plane,
                        start,
                        radial,
                    });
                }
            }
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            if let (Some(drag), Some(p)) = (self.drag, pointer) {
                *plane = drag.initial;
                match drag.handle {
                    Handle::Move(i) => {
                        let screen = camera.vector(AXES[i]);
                        if screen.length_sq() > 1e-4 {
                            plane.position +=
                                AXES[i] * ((p - drag.start).dot(screen) / screen.length_sq());
                        }
                    }
                    Handle::Rotate(i) => {
                        let axis = AXES[i];
                        let angle = if let Some(current) = radial(camera, p, plane.position, axis) {
                            axis.dot(drag.radial.cross(current))
                                .atan2(drag.radial.dot(current))
                        } else {
                            let tangent =
                                camera.vector(axis.cross(drag.radial)) * (59. / camera.scale);
                            (p - drag.start).dot(tangent) / tangent.length_sq().max(1.)
                        };
                        plane.rotate_world(axis, angle);
                    }
                }
                changed = true;
            }
        }
        let captured = self.drag.is_some();
        if response.drag_stopped() {
            self.drag = None;
        }
        let hover = pointer
            .filter(|p| response.rect.contains(*p))
            .and_then(|p| self.hit(camera, *plane, p));
        if hover.is_some() || captured {
            ui.ctx().set_cursor_icon(if captured {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        (changed, captured)
    }
    pub fn draw(&self, ui: &egui::Ui, camera: Camera, plane: SectionPlane) {
        let painter = ui.painter_at(camera.rect);
        let pivot = camera.project(plane.position);
        let radius = 59. / camera.scale;
        let hovered = ui
            .input(|i| i.pointer.hover_pos())
            .and_then(|p| self.hit(camera, plane, p));
        for axis in 0..3 {
            let selected = self.drag.map(|d| d.handle).or(hovered) == Some(Handle::Rotate(axis));
            let color = if selected {
                Color32::WHITE
            } else {
                COLORS[axis]
            };
            let mut points = Vec::new();
            for i in 0..=96 {
                points.push(camera.project(
                    plane.position
                        + ring_point(axis, i as f32 * std::f32::consts::TAU / 96.) * radius,
                ));
            }
            painter.add(egui::Shape::line(
                points,
                Stroke::new(if selected { 3. } else { 1.7 }, color),
            ));
        }
        for (i, axis) in AXES.iter().enumerate() {
            let end = camera.project(plane.position + *axis * (82. / camera.scale));
            let selected = self.drag.map(|d| d.handle).or(hovered) == Some(Handle::Move(i));
            let color = if selected { Color32::WHITE } else { COLORS[i] };
            if end.distance(pivot) > 18. {
                painter.arrow(pivot, end - pivot, Stroke::new(3., color));
                painter.text(
                    end,
                    egui::Align2::LEFT_BOTTOM,
                    ["X", "Y", "Z"][i],
                    egui::FontId::monospace(13.),
                    color,
                );
            }
        }
        painter.circle_filled(pivot, 4., Color32::WHITE);
        if let Some(handle) = self.drag.map(|d| d.handle).or(hovered) {
            painter.text(
                pivot + Vec2::new(0., 80.),
                egui::Align2::CENTER_TOP,
                format!(
                    "{} {}",
                    if matches!(handle, Handle::Move(_)) {
                        "Move"
                    } else {
                        "Rotate"
                    },
                    ["X", "Y", "Z"][match handle {
                        Handle::Move(i) | Handle::Rotate(i) => i,
                    }]
                ),
                egui::FontId::proportional(12.),
                Color32::WHITE,
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn camera() -> Camera {
        Camera {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.)),
            center: Vec3::ZERO,
            rotation: Quat::from_rotation_x(-0.45) * Quat::from_rotation_z(-0.7),
            scale: 30.,
            size: 20.,
        }
    }
    #[test]
    fn projection_and_ray_agree() {
        let c = camera();
        let p = Vec3::new(1., 2., 3.);
        let (o, d) = c.ray(c.project(p));
        assert!((p - o).cross(d).length() < 1e-4);
        let ndc = c.matrix().project_point3(p);
        let pixel = Pos2::new((ndc.x + 1.) * 400., (1. - ndc.y) * 300.);
        assert!(pixel.distance(c.project(p)) < 1e-3);
    }
    fn drag(camera: Camera, start: Pos2, end: Pos2) -> SectionPlane {
        let ctx = egui::Context::default();
        let mut plane = SectionPlane::new(Vec3::ZERO);
        let mut gizmo = Gizmo::default();
        let frames = vec![
            vec![],
            vec![egui::Event::PointerMoved(start)],
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerMoved(end)],
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ];
        for (frame, events) in frames.into_iter().enumerate() {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(camera.rect),
                    time: Some(frame as f64 / 60.),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let response = ui.allocate_rect(camera.rect, egui::Sense::drag());
                            gizmo.interact(ui, &response, camera, &mut plane);
                        });
                },
            );
        }
        plane
    }
    #[test]
    fn synthetic_handle_drags_cover_all_six_degrees_of_freedom() {
        let c = camera();
        let g = Gizmo::default();
        let initial = SectionPlane::new(Vec3::ZERO);
        for axis in 0..3 {
            let start = c.project(AXES[axis] * (75. / c.scale));
            assert_eq!(g.hit(c, initial, start), Some(Handle::Move(axis)));
            let p = drag(c, start, start + c.vector(AXES[axis] * 1.7));
            assert!(
                (p.position - AXES[axis] * 1.7).length() < 1e-4,
                "Translation {axis}: {:?}",
                p.position
            );
            assert_eq!(p.orientation, Quat::IDENTITY);
            let radial = (0..96)
                .map(|i| ring_point(axis, i as f32 * std::f32::consts::TAU / 96.) * (59. / c.scale))
                .find(|&r| g.hit(c, initial, c.project(r)) == Some(Handle::Rotate(axis)))
                .expect("Every rotation ring must be selectable");
            let angle = 0.45;
            let expected = Quat::from_axis_angle(AXES[axis], angle);
            let p = drag(c, c.project(radial), c.project(expected * radial));
            assert!(p.position.length() < 1e-5);
            assert!(
                p.orientation.dot(expected).abs() > 0.9999,
                "Rotation {axis}: {:?}",
                p.orientation
            );
        }
    }
}
