//! Exact triangle/plane intersection and hole-aware section triangulation.
use crate::{model::Body, plane::SectionPlane};
use glam::{DVec2, DVec3, Vec3};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default, Clone)]
pub struct Cap {
    pub triangles: Vec<[Vec3; 3]>,
    pub contours: Vec<[Vec3; 2]>,
    pub warning: Option<String>,
}
fn inside(p: DVec2, poly: &[DVec2]) -> bool {
    let mut yes = false;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            yes = !yes;
        }
    }
    yes
}
fn area(p: &[DVec2]) -> f64 {
    (0..p.len())
        .map(|i| p[i].perp_dot(p[(i + 1) % p.len()]))
        .sum::<f64>()
        .abs()
        * 0.5
}
pub fn build(body: &Body, plane: SectionPlane, tolerance: f32) -> Cap {
    let mut cap = Cap::default();
    let eps = tolerance as f64;
    let o = plane.position.as_dvec3();
    let n = plane.normal().as_dvec3();
    let u = plane.u().as_dvec3();
    let v = plane.v().as_dvec3();
    let mut points = Vec::<DVec2>::new();
    let mut keys = BTreeMap::new();
    let mut segments = BTreeSet::new();
    for t in body.triangles() {
        let t = t.map(|p| p.as_dvec3());
        let d = t.map(|p| (p - o).dot(n));
        if d.iter().all(|x| x.abs() <= eps) {
            continue;
        }
        let mut hits = Vec::<DVec3>::new();
        for i in 0..3 {
            let j = (i + 1) % 3;
            if d[i].abs() <= eps {
                hits.push(t[i]);
            }
            if (d[i] > eps && d[j] < -eps) || (d[i] < -eps && d[j] > eps) {
                // Use a canonical endpoint order to keep shared-edge arithmetic identical.
                let (a, b) = if t[i].to_array() < t[j].to_array() {
                    (i, j)
                } else {
                    (j, i)
                };
                hits.push(t[a] + (t[b] - t[a]) * (d[a] / (d[a] - d[b])));
            }
        }
        hits.dedup_by(|a, b| a.distance(*b) <= eps);
        let mut unique = Vec::<DVec3>::new();
        for h in hits {
            if unique.iter().all(|p| p.distance(h) > eps) {
                unique.push(h);
            }
        }
        if unique.len() != 2 {
            continue;
        }
        let mut ids = [0; 2];
        for i in 0..2 {
            let relative = unique[i] - o;
            let p = DVec2::new(relative.dot(u), relative.dot(v));
            let key = ((p.x / eps).round() as i64, (p.y / eps).round() as i64);
            ids[i] = *keys.entry(key).or_insert_with(|| {
                points.push(p);
                points.len() - 1
            });
        }
        if ids[0] != ids[1] {
            ids.sort();
            segments.insert((ids[0], ids[1]));
        }
    }
    let world = |p: DVec2| (o + u * p.x + v * p.y).as_vec3();
    cap.contours = segments
        .iter()
        .map(|&(a, b)| [world(points[a]), world(points[b])])
        .collect();
    if !body.closed() {
        cap.warning = Some("Open mesh: outlines only".into());
        return cap;
    }
    let mut adjacency = vec![Vec::new(); points.len()];
    for &(a, b) in &segments {
        adjacency[a].push(b);
        adjacency[b].push(a);
    }
    if adjacency.iter().any(|a| a.len() != 2) {
        cap.warning =
            Some("Ambiguous section junction: outlines only; move the plane slightly".into());
        return cap;
    }
    let mut remaining = segments;
    let mut loops = Vec::new();
    while let Some(&(first, next)) = remaining.iter().next() {
        let mut poly = vec![points[first]];
        let mut previous = first;
        let mut current = next;
        remaining.remove(&(first.min(next), first.max(next)));
        while current != first {
            poly.push(points[current]);
            let next = *adjacency[current].iter().find(|&&p| p != previous).unwrap();
            if !remaining.remove(&(current.min(next), current.max(next))) {
                cap.warning = Some("Unclosed section loop: outlines only".into());
                return cap;
            }
            previous = current;
            current = next;
        }
        if poly.len() >= 3 && area(&poly) > eps * eps {
            loops.push(poly);
        }
    }
    let areas: Vec<_> = loops.iter().map(|p| area(p)).collect();
    let parents: Vec<_> = (0..loops.len())
        .map(|i| {
            (0..loops.len())
                .filter(|&j| areas[j] > areas[i] && inside(loops[i][0], &loops[j]))
                .min_by(|&a, &b| areas[a].total_cmp(&areas[b]))
        })
        .collect();
    let depth = |mut i: usize| {
        let mut d = 0;
        while let Some(p) = parents[i] {
            d += 1;
            i = p;
        }
        d
    };
    for i in 0..loops.len() {
        if depth(i) % 2 != 0 {
            continue;
        }
        let mut polygon = loops[i].clone();
        let mut holes = Vec::new();
        for j in 0..loops.len() {
            if parents[j] == Some(i) {
                holes.push(polygon.len());
                polygon.extend(&loops[j]);
            }
        }
        let coords: Vec<_> = polygon.iter().flat_map(|p| [p.x, p.y]).collect();
        match earcutr::earcut(&coords, &holes, 2) {
            Ok(indices) => {
                for ix in indices.chunks_exact(3) {
                    let mut t = [
                        world(polygon[ix[0]]),
                        world(polygon[ix[1]]),
                        world(polygon[ix[2]]),
                    ];
                    if (t[1] - t[0]).cross(t[2] - t[0]).dot(plane.normal()) < 0. {
                        t.swap(1, 2);
                    }
                    cap.triangles.push(t);
                }
            }
            Err(e) => {
                cap.triangles.clear();
                cap.warning = Some(format!("Section triangulation failed: {e:?}"));
                return cap;
            }
        }
    }
    cap
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    #[test]
    fn annular_cap_preserves_hole_and_oblique_area() {
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        for angle in [0., 0.4] {
            let mut p = SectionPlane::new(Vec3::ZERO);
            p.orientation = glam::Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 + angle);
            let cap = build(&model.bodies[0], p, 1e-5);
            assert!(cap.warning.is_none(), "{:?}", cap.warning);
            assert!(!cap.triangles.is_empty());
            let actual: f32 = cap
                .triangles
                .iter()
                .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5)
                .sum();
            let exact = std::f32::consts::PI * (9. - 2.25) / angle.cos();
            assert!((actual - exact).abs() / exact < 0.003);
            for t in &cap.triangles {
                let a = t[0].truncate();
                let b = t[1].truncate();
                let c = t[2].truncate();
                let sides = [a.perp_dot(b), b.perp_dot(c), c.perp_dot(a)];
                assert!(
                    !(sides.iter().all(|&v| v > 1e-6) || sides.iter().all(|&v| v < -1e-6)),
                    "Triangle must not cover the cavity centre"
                );
            }
        }
    }
    #[test]
    fn tangent_plane_has_no_cap_and_roll_preserves_cut() {
        let model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let mut p = SectionPlane::new(Vec3::new(4., 0., 0.));
        assert!(build(&model.bodies[0], p, 1e-5).triangles.is_empty());
        p.rotate_world(Vec3::X, 0.7);
        assert!((p.normal() - Vec3::X).length() < 1e-5);
        assert!((p.u().dot(p.v())).abs() < 1e-5);
    }
}
