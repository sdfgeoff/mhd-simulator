use glam::{Mat4, Vec3};
use std::{collections::HashMap, path::Path};

#[derive(Clone)]
pub struct Body {
    pub name: String,
    pub triangles: Vec<[Vec3; 3]>, // millimetres
    pub visible: bool,
    pub role: usize,
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
        Ok(Self {
            winding: if signed_volume < 0. { -1. } else { 1. },
            name,
            triangles,
            visible: true,
            role: 0,
            boundary_edges: edges.values().filter(|&&n| n == 1).count(),
            nonmanifold_edges: edges.values().filter(|&&n| n > 2).count(),
        })
    }
    pub fn closed(&self) -> bool {
        self.boundary_edges == 0 && self.nonmanifold_edges == 0
    }
}
#[derive(Default)]
pub struct Model {
    pub bodies: Vec<Body>,
    pub notices: Vec<String>,
    pub source: String,
}
impl Model {
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for p in self
            .bodies
            .iter()
            .flat_map(|b| b.triangles.iter().flatten())
        {
            lo = lo.min(*p);
            hi = hi.max(*p);
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
                            if p.mode() != gltf::mesh::Mode::Triangles {
                                return Err("Only triangle primitives are supported".into());
                            }
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
