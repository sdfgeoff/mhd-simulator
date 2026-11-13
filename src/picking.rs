//! Screen-space curve picking and nearest clipped solid ray intersection.
use crate::{gizmo::Camera, model::Model, plane::SectionPlane};
use eframe::egui::Pos2;
use glam::Vec3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Object {
    Solid(usize),
    Curve(usize),
}
pub fn pick(
    model: &Model,
    camera: Camera,
    point: Pos2,
    cut: bool,
    plane: SectionPlane,
) -> Option<Object> {
    let (origin, direction) = camera.ray(point);
    let kept = |p: Vec3| !cut || (p - plane.position).dot(plane.normal()) <= 1e-4;
    let mut solid = None;
    let mut nearest = f32::INFINITY;
    for (id, body) in model.bodies.iter().enumerate().filter(|(_, b)| b.visible) {
        for t in body.triangles() {
            if let Some(d) = triangle(origin, direction, *t) {
                if d < nearest && kept(origin + direction * d) {
                    nearest = d;
                    solid = Some(Object::Solid(id));
                }
            }
        }
        // Match the rendered cap, so a cut through a body remains selectable.
        if cut {
            let denom = direction.dot(plane.normal());
            if denom.abs() > 1e-6 {
                let d = (plane.position - origin).dot(plane.normal()) / denom;
                if d >= 0. && d < nearest {
                    let p = origin + direction * d;
                    let mut intersections: Vec<f32> = body
                        .triangles()
                        .iter()
                        .filter_map(|t| triangle(p + direction * 1e-4, direction, *t))
                        .collect();
                    intersections.sort_by(f32::total_cmp);
                    intersections.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
                    if intersections.len() % 2 == 1 {
                        nearest = d;
                        solid = Some(Object::Solid(id));
                    }
                }
            }
        }
    }
    let mut best = (7., solid);
    for (id, curve) in model.curves.iter().enumerate() {
        for segment in curve.points_mm.windows(2) {
            let mut a = Vec3::from(segment[0]);
            let mut b = Vec3::from(segment[1]);
            if cut {
                let da = (a - plane.position).dot(plane.normal());
                let db = (b - plane.position).dot(plane.normal());
                if da > 0. && db > 0. {
                    continue;
                }
                if da > 0. {
                    a = a.lerp(b, da / (da - db));
                } else if db > 0. {
                    b = a.lerp(b, da / (da - db));
                }
            }
            let pa = camera.project(a);
            let pb = camera.project(b);
            let ab = pb - pa;
            let t = ((point - pa).dot(ab) / ab.length_sq().max(1e-8)).clamp(0., 1.);
            let p = a.lerp(b, t);
            let depth = (p - origin).dot(direction);
            let distance = point.distance(pa + ab * t);
            if depth >= 0. && depth <= nearest + 0.5 / camera.scale && distance < best.0 {
                best = (distance, Some(Object::Curve(id)));
            }
        }
    }
    best.1
}
fn triangle(o: Vec3, d: Vec3, t: [Vec3; 3]) -> Option<f32> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = o - t[0];
    let u = s.dot(p) / det;
    if !(0. ..=1.).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) / det;
    if v < 0. || u + v > 1. {
        return None;
    }
    let hit = e2.dot(q) / det;
    (hit >= 0.).then_some(hit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Rect, vec2};
    use glam::Quat;
    fn cube(y: f32) -> crate::model::Body {
        let v = [
            [-1., y, -1.],
            [1., y, -1.],
            [1., y + 1., -1.],
            [-1., y + 1., -1.],
            [-1., y, 1.],
            [1., y, 1.],
            [1., y + 1., 1.],
            [-1., y + 1., 1.],
        ]
        .map(Vec3::from);
        let f = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ];
        crate::model::Body::new("Box".into(), f.map(|t| t.map(|i| v[i])).to_vec()).unwrap()
    }
    #[test]
    fn picks_nearest_visible_solid_clipped_caps_and_front_centrelines() {
        let mut m = Model::default();
        m.bodies = vec![cube(-2.), cube(1.)];
        let camera = Camera {
            rect: Rect::from_min_size(Pos2::ZERO, vec2(400., 400.)),
            center: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 100.,
            size: 5.,
        };
        let plane = SectionPlane::new(Vec3::ZERO);
        let center = camera.project(Vec3::ZERO);
        assert_eq!(
            pick(&m, camera, center, false, plane),
            Some(Object::Solid(0))
        );
        m.bodies[0].visible = false;
        assert_eq!(
            pick(&m, camera, center, false, plane),
            Some(Object::Solid(1))
        );
        m.bodies[0].visible = true;
        assert_eq!(
            pick(
                &m,
                camera,
                camera.project(Vec3::new(0.5, 0., 0.)),
                true,
                plane
            ),
            None
        );
        // Slice normal points towards the camera; the cap now hides remaining back faces.
        let mut cap = plane;
        cap.orientation = Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2);
        cap.position.y = -1.5;
        assert_eq!(pick(&m, camera, center, true, cap), Some(Object::Solid(0)));
        m.curves.push(crate::model::CurvePath {
            name: "Wire".into(),
            points_mm: vec![[-0.8, -3., 0.], [0.8, -3., 0.]],
        });
        assert_eq!(
            pick(&m, camera, center, false, plane),
            Some(Object::Curve(0))
        );
        for p in &mut m.curves[0].points_mm {
            p[1] = 3.;
        }
        assert_eq!(
            pick(&m, camera, center, false, plane),
            Some(Object::Solid(0))
        );
        assert_eq!(pick(&m, camera, Pos2::new(10., 10.), false, plane), None);
    }
}
