use glam::{Mat4, Vec3};
use std::{collections::HashMap, path::Path};

#[derive(Clone)]
pub struct Body {
    pub name: String,
    triangles: std::sync::Arc<[[Vec3; 3]]>, // immutable mm, shared by worker snapshots
    geometry_id: u64,
    bounds: (Vec3, Vec3),
    pub visible: bool,
    pub simulation: crate::simulation::SimulationProperties,
    pub boundary_edges: usize,
    pub nonmanifold_edges: usize,
    pub winding: f32,
}
impl Body {
    pub fn new(name: String, triangles: Vec<[Vec3; 3]>) -> Result<Self, String> {
        if triangles.is_empty() {
            return Err(format!("{name}: no triangles"));
        }
        if triangles.iter().flatten().any(|p| !p.is_finite()) {
            return Err(format!("{name}: nonfinite coordinates"));
        }
        if triangles
            .iter()
            .any(|t| (t[1] - t[0]).cross(t[2] - t[0]).length_squared() < 1e-18)
        {
            return Err(format!("{name}: degenerate triangles"));
        }
        // Weld positional seams at one nanometre (coordinates are in mm).
        let key = |p: Vec3| {
            [
                (p.x as f64 * 1e6).round() as i64,
                (p.y as f64 * 1e6).round() as i64,
                (p.z as f64 * 1e6).round() as i64,
            ]
        };
        let mut edges = HashMap::new();
        for t in &triangles {
            for i in 0..3 {
                let a = key(t[i]);
                let b = key(t[(i + 1) % 3]);
                let e = if a < b { (a, b) } else { (b, a) };
                *edges.entry(e).or_insert(0usize) += 1;
            }
        }
        let signed_volume: f64 = triangles
            .iter()
            .map(|t| t[0].as_dvec3().dot(t[1].as_dvec3().cross(t[2].as_dvec3())) / 6.)
            .sum();
        static NEXT_GEOMETRY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let bounds = triangles.iter().flatten().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        Ok(Self {
            geometry_id: NEXT_GEOMETRY.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            bounds,
            winding: if signed_volume < 0. { -1. } else { 1. },
            name,
            triangles: triangles.into(),
            visible: true,
            simulation: Default::default(),
            boundary_edges: edges.values().filter(|&&n| n == 1).count(),
            nonmanifold_edges: edges.values().filter(|&&n| n > 2).count(),
        })
    }
    pub fn triangles(&self) -> &[[Vec3; 3]] {
        &self.triangles
    }
    pub fn geometry_id(&self) -> u64 {
        self.geometry_id
    }
    pub fn volume_mm3(&self) -> f64 {
        self.triangles
            .iter()
            .map(|t| t[0].as_dvec3().dot(t[1].as_dvec3().cross(t[2].as_dvec3())) / 6.)
            .sum::<f64>()
            .abs()
    }
    pub fn closed(&self) -> bool {
        self.boundary_edges == 0 && self.nonmanifold_edges == 0
    }
}
#[derive(Clone, Debug)]
pub struct CurvePath {
    pub name: String,
    pub points_mm: Vec<[f32; 3]>,
}
#[derive(Default, Clone)]
pub struct Model {
    pub bodies: Vec<Body>,
    pub curves: Vec<CurvePath>,
    pub notices: Vec<String>,
    pub source: String,
}
impl Model {
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for body in &self.bodies {
            lo = lo.min(body.bounds.0);
            hi = hi.max(body.bounds.1);
        }
        for curve in &self.curves {
            for &p in &curve.points_mm {
                lo = lo.min(Vec3::from(p));
                hi = hi.max(Vec3::from(p));
            }
        }
        if !lo.is_finite() {
            (Vec3::splat(-5.), Vec3::splat(5.))
        } else {
            (lo, hi)
        }
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut m = Self {
            source: path.display().to_string(),
            ..Default::default()
        };
        match ext.as_str() {
            "gltf" | "glb" => {
                let (doc, buffers, _) = gltf::import(path).map_err(|e| e.to_string())?;
                fn visit(
                    node: gltf::Node,
                    parent: Mat4,
                    buffers: &[gltf::buffer::Data],
                    m: &mut Model,
                ) -> Result<(), String> {
                    let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
                    if let Some(mesh) = node.mesh() {
                        for (pi, p) in mesh.primitives().enumerate() {
                            if p.morph_targets().next().is_some() || node.skin().is_some() {
                                return Err(
                                    "Skinned/morphed geometry is unsupported; export a static mesh"
                                        .into(),
                                );
                            }
                            let r = p.reader(|b| Some(&buffers[b.index()]));
                            let pos: Vec<_> = r
                                .read_positions()
                                .ok_or("Missing positions")?
                                .map(|v| world.transform_point3(Vec3::from(v)) * 1000.)
                                .collect();
                            let ix: Vec<_> = r
                                .read_indices()
                                .map(|v| v.into_u32().collect())
                                .unwrap_or_else(|| (0..pos.len() as u32).collect());
                            if p.mode() != gltf::mesh::Mode::Triangles {
                                let paths = centreline_paths(p.mode(), &pos, &ix)?;
                                let name = node.name().or(mesh.name()).unwrap_or("CAD centreline");
                                for (j, points_mm) in paths.into_iter().enumerate() {
                                    m.curves.push(CurvePath {
                                        name: format!("{name} / {pi}:{j}"),
                                        points_mm,
                                    });
                                }
                                continue;
                            }
                            if ix.len() % 3 != 0 {
                                return Err("Incomplete triangle indices".into());
                            }
                            let mut ts = Vec::new();
                            for i in ix.chunks_exact(3) {
                                let get = |j: u32| {
                                    pos.get(j as usize)
                                        .copied()
                                        .ok_or("Triangle index out of range")
                                };
                                ts.push([get(i[0])?, get(i[1])?, get(i[2])?]);
                            }
                            m.bodies.push(Body::new(
                                format!(
                                    "{} / {}",
                                    node.name().or(mesh.name()).unwrap_or("Body"),
                                    pi
                                ),
                                ts,
                            )?);
                        }
                    }
                    for c in node.children() {
                        visit(c, world, buffers, m)?;
                    }
                    Ok(())
                }
                let scene = doc
                    .default_scene()
                    .or_else(|| doc.scenes().next())
                    .ok_or("No scene")?;
                for n in scene.nodes() {
                    visit(n, Mat4::IDENTITY, &buffers, &mut m)?;
                }
                m.notices.push("glTF metres converted to millimetres; node transforms applied. Visual materials are not physical properties.".into());
            }
            "stl" => {
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                let mut ts = Vec::new();
                let count = bytes
                    .get(80..84)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
                if count.is_some_and(|n| 84usize.checked_add(n * 50) == Some(bytes.len())) {
                    for tri in bytes[84..].chunks_exact(50) {
                        let mut t = [Vec3::ZERO; 3];
                        for (j, p) in t.iter_mut().enumerate() {
                            let f = |k: usize| {
                                f32::from_le_bytes(
                                    tri[12 + j * 12 + k * 4..16 + j * 12 + k * 4]
                                        .try_into()
                                        .unwrap(),
                                )
                            };
                            *p = Vec3::new(f(0), f(1), f(2));
                        }
                        ts.push(t);
                    }
                } else {
                    let s = std::str::from_utf8(&bytes).map_err(|_| "Invalid STL")?;
                    let mut points = Vec::new();
                    for line in s.lines() {
                        let w: Vec<_> = line.split_whitespace().collect();
                        if w.first() == Some(&"vertex") {
                            if w.len() != 4 {
                                return Err("Invalid STL vertex".into());
                            }
                            let f = |i: usize| w[i].parse::<f32>().map_err(|e| e.to_string());
                            points.push(Vec3::new(f(1)?, f(2)?, f(3)?));
                        }
                    }
                    if points.len() % 3 != 0 {
                        return Err("Incomplete STL triangle".into());
                    }
                    for t in points.chunks_exact(3) {
                        ts.push([t[0], t[1], t[2]]);
                    }
                }
                m.bodies.push(Body::new(
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    ts,
                )?);
                m.notices.push(
                    "STL is unitless: interpreted as millimetres. Verify dimensions before use."
                        .into(),
                );
            }
            "step" | "stp" => {
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                let (data, report) = step_io::read(file).map_err(|e| format!("{e:?}"))?;
                let scene = data.scene();
                m.notices.push(format!(
                    "STEP inspection: {} solids, {} faces, {} edges, {} products.",
                    scene.all_solids().count(),
                    scene.all_faces().count(),
                    scene.all_edges().count(),
                    scene.all_products().count()
                ));
                m.notices.push(format!(
                    "Import report: {} dropped entities; {} parser warnings.",
                    report.dropped.len(),
                    report.parse_warnings.len()
                ));
                m.notices.push(format!("Units: {:?}", scene.units()));
                for p in scene.all_products() {
                    m.notices.push(format!("Product: {}", p.name()));
                }
                m.notices.push("STEP tessellation is not implemented in this prototype. Export glTF/GLB or STL to inspect solid geometry. No simulation geometry has been loaded.".into());
                for w in report.parse_warnings.iter().take(10) {
                    m.notices.push(format!("{w:?}"));
                }
            }
            _ => return Err("Supported: .glb, .gltf, .stl; .step/.stp inspection only".into()),
        }
        for b in &m.bodies {
            if !b.closed() {
                m.notices.push(format!(
                    "{}: {} open edges, {} nonmanifold edges. GPU solid classification disabled.",
                    b.name, b.boundary_edges, b.nonmanifold_edges
                ));
            }
        }
        Ok(m)
    }
}

/// Assemble exported line segments into unbranched, ordered current paths.
fn centreline_paths(
    mode: gltf::mesh::Mode,
    positions: &[Vec3],
    indices: &[u32],
) -> Result<Vec<Vec<[f32; 3]>>, String> {
    use gltf::mesh::Mode;
    if positions.iter().any(|p| !p.is_finite())
        || indices.iter().any(|&i| i as usize >= positions.len())
    {
        return Err("Invalid CAD centreline positions or indices".into());
    }
    if matches!(mode, Mode::LineStrip | Mode::LineLoop) {
        let mut path: Vec<_> = indices
            .iter()
            .map(|&i| positions[i as usize].to_array())
            .collect();
        if mode == Mode::LineLoop && path.first() != path.last() && path.len() > 2 {
            path.push(path[0]);
        }
        if path.len() < 2
            || path
                .windows(2)
                .any(|p| Vec3::from(p[0]).distance(Vec3::from(p[1])) < 1e-6)
        {
            return Err(
                "CAD centreline needs at least two points and finite nonzero segments".into(),
            );
        }
        return Ok(vec![path]);
    }
    let edges: Vec<_> = match mode {
        Mode::Lines => {
            if indices.len() % 2 != 0 {
                return Err("Incomplete centreline edge".into());
            }
            indices.chunks_exact(2).map(|p| (p[0], p[1])).collect()
        }
        _ => return Err("Export triangle bodies or line/line-strip/line-loop centrelines".into()),
    };
    let mut points: Vec<Vec3> = Vec::new();
    let mut lookup = HashMap::new();
    let mut links: Vec<Vec<usize>> = Vec::new();
    for (a, b) in edges {
        let mut pair = [0; 2];
        for (j, index) in [a, b].into_iter().enumerate() {
            let p = positions[index as usize];
            let key = p.to_array().map(|v| (v as f64 * 1e5).round() as i64);
            let next = points.len();
            pair[j] = *lookup.entry(key).or_insert_with(|| {
                points.push(p);
                links.push(Vec::new());
                next
            });
        }
        let [a, b] = pair;
        if a == b {
            return Err("Zero-length CAD centreline segment".into());
        }
        if links[a].contains(&b) {
            return Err("Duplicate CAD centreline segment".into());
        }
        links[a].push(b);
        links[b].push(a);
    }
    if links.iter().any(|l| l.len() > 2) {
        return Err(
            "Branched CAD centreline: export each current circuit as an unbranched path".into(),
        );
    }
    let mut paths = Vec::new();
    while let Some(start) = links
        .iter()
        .position(|l| l.len() == 1)
        .or_else(|| links.iter().position(|l| !l.is_empty()))
    {
        let mut path = vec![points[start].to_array()];
        let mut at = start;
        while !links[at].is_empty() {
            let next = links[at].remove(0);
            links[next].retain(|&i| i != at);
            path.push(points[next].to_array());
            at = next;
        }
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(test)]
mod curve_tests {
    use super::*;
    #[test]
    fn cad_curves_keep_order_units_and_assembly_placement() {
        let path = Path::new("assets/coil-centreline.gltf");
        let model = Model::load(path).unwrap();
        assert!(model.bodies.is_empty());
        assert_eq!(model.curves.len(), 1);
        let curve = &model.curves[0];
        assert_eq!(curve.points_mm.first(), curve.points_mm.last());
        assert!((Vec3::from(curve.points_mm[0]) - Vec3::new(5., 0., 0.)).length() < 1e-5);
        assert!(curve.points_mm[1][1] > 0.); // Current direction follows exported vertex order.
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        doc["nodes"][0]["translation"] = serde_json::json!([0.001, 0.002, 0.003]);
        doc["nodes"][0]["scale"] = serde_json::json!([2., 2., 2.]);
        std::fs::create_dir_all("artifacts").unwrap();
        std::fs::write(
            "artifacts/transformed-centreline.gltf",
            serde_json::to_vec(&doc).unwrap(),
        )
        .unwrap();
        let transformed = Model::load(Path::new("artifacts/transformed-centreline.gltf")).unwrap();
        assert!(
            (Vec3::from(transformed.curves[0].points_mm[0]) - Vec3::new(11., 2., 3.)).length()
                < 1e-5
        );
    }
    #[test]
    fn disjoint_segment_paths_are_stitched_and_branches_rejected() {
        use gltf::mesh::Mode;
        let p = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::new(0., 0., 0.)];
        let paths = centreline_paths(Mode::Lines, &p, &[0, 1, 1, 2, 2, 3]).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].len(), 4);
        assert_eq!(paths[0].first(), paths[0].last());
        assert_eq!(paths[0][1], Vec3::X.to_array());
        assert!(centreline_paths(Mode::Lines, &p, &[0, 1, 0, 2, 0, 3]).is_err());
        assert!(centreline_paths(Mode::Lines, &p, &[0, 1, 1, 0]).is_err());
    }
}
