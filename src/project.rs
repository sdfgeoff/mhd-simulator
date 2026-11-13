use crate::{model::Model, plane::SectionPlane};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SimulationProperties {
    pub included: bool,
    pub material_id: u32,
    pub temperature_c: Option<f32>,
    pub current_a: Option<f32>,
}
impl Default for SimulationProperties {
    fn default() -> Self {
        Self {
            included: true,
            material_id: 1,
            temperature_c: None,
            current_a: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Material {
    pub id: u32,
    pub name: String,
    pub conductivity_s_m: Option<f32>,
    pub relative_permeability: Option<f32>,
    pub density_kg_m3: Option<f32>,
}
pub fn default_materials() -> Vec<Material> {
    [
        "Air / void",
        "Unassigned",
        "Copper",
        "Sn60Pb40",
        "Sn99.3Cu0.7",
        "Permanent magnet",
    ]
    .iter()
    .enumerate()
    .map(|(i, n)| Material {
        id: i as u32,
        name: (*n).into(),
        conductivity_s_m: None,
        relative_permeability: None,
        density_kg_m3: None,
    })
    .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DomainSettings {
    pub min_mm: [f32; 3],
    pub max_mm: [f32; 3],
    pub cell_mm: f32,
}
impl DomainSettings {
    pub fn fit(model: &Model) -> Self {
        let (lo, hi) = model.bounds();
        let pad = (hi - lo).max_element().max(1.) * 0.1;
        let min = lo - Vec3::splat(pad);
        let max = hi + Vec3::splat(pad);
        Self {
            min_mm: min.to_array(),
            max_mm: max.to_array(),
            cell_mm: (max - min).max_element() / 128.,
        }
    }
    pub fn dims(&self) -> Result<[u32; 3], String> {
        if !self.cell_mm.is_finite() || self.cell_mm <= 0. {
            return Err("Cell spacing must be finite and positive".into());
        }
        let mut d = [0; 3];
        for (i, cell) in d.iter_mut().enumerate() {
            let span = self.max_mm[i] as f64 - self.min_mm[i] as f64;
            if !self.min_mm[i].is_finite() || !self.max_mm[i].is_finite() || span <= 0. {
                return Err("Domain maximum must exceed minimum on every axis".into());
            }
            let n = (span / self.cell_mm as f64 - 1e-6).ceil();
            if !(1.0..=2048.).contains(&n) {
                return Err("Each domain dimension must be 1..2048 cells".into());
            }
            *cell = n as u32;
        }
        Ok(d)
    }
    pub fn actual_max(&self) -> Result<Vec3, String> {
        let d = self.dims()?;
        Ok(Vec3::from(self.min_mm)
            + Vec3::new(d[0] as f32, d[1] as f32, d[2] as f32) * self.cell_mm)
    }
    pub fn cells(&self) -> Result<u64, String> {
        Ok(self.dims()?.iter().map(|&n| n as u64).product())
    }
    pub fn estimated_bytes(&self, bodies: usize) -> Result<u64, String> {
        let d = self.dims()?;
        Ok(self.cells()? * 9
            + d[0] as u64 * d[1] as u64 * 4
            + d[2] as u64 * bodies as u64 * 256
            + 4096)
    }
}
pub fn validate_materials(materials: &[Material]) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    for m in materials {
        if m.id > 254 || m.name.trim().is_empty() || !ids.insert(m.id) {
            return Err("Material IDs must be unique in 0..254, with nonempty names".into());
        }
        for (name, v, allow_zero) in [
            ("conductivity", m.conductivity_s_m, true),
            ("permeability", m.relative_permeability, false),
            ("density", m.density_kg_m3, false),
        ] {
            if v.is_some_and(|v| !v.is_finite() || v < 0. || (!allow_zero && v == 0.)) {
                return Err(format!("Invalid {name} for material {}", m.name));
            }
        }
    }
    if !ids.contains(&0) {
        return Err("Material 0 must identify air / void".into());
    }
    Ok(())
}
pub fn validate_bodies(model: &Model, materials: &[Material]) -> Result<(), String> {
    validate_materials(materials)?;
    for b in &model.bodies {
        if !materials.iter().any(|m| m.id == b.simulation.material_id)
            || b.simulation.material_id == 0
        {
            return Err(format!("{} needs a non-air material", b.name));
        }
        if b.simulation
            .temperature_c
            .is_some_and(|v| !v.is_finite() || v < -273.15)
            || b.simulation.current_a.is_some_and(|v| !v.is_finite())
        {
            return Err(format!("Invalid temperature or current for {}", b.name));
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct SavedBody {
    name: String,
    triangles: usize,
    visible: bool,
    simulation: SimulationProperties,
}
#[derive(Serialize, Deserialize)]
struct SavedPlane {
    position_mm: [f32; 3],
    quaternion_xyzw: [f32; 4],
}
#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    geometry: String,
    domain: DomainSettings,
    materials: Vec<Material>,
    bodies: Vec<SavedBody>,
    plane: SavedPlane,
}
pub struct Loaded {
    pub model: Model,
    pub settings: DomainSettings,
    pub materials: Vec<Material>,
    pub plane: SectionPlane,
}
pub fn save(
    path: &Path,
    model: &Model,
    settings: &DomainSettings,
    materials: &[Material],
    plane: SectionPlane,
) -> Result<(), String> {
    if !path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".mhd.json"))
    {
        return Err("Project files must end in .mhd.json".into());
    }
    settings.dims()?;
    validate_bodies(model, materials)?;
    let absolute = std::fs::canonicalize(&model.source).map_err(|e| e.to_string())?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
    let geometry = absolute
        .strip_prefix(&parent)
        .unwrap_or(&absolute)
        .to_string_lossy()
        .into_owned();
    let f = File {
        version: 1,
        geometry,
        domain: settings.clone(),
        materials: materials.to_vec(),
        bodies: model
            .bodies
            .iter()
            .map(|b| SavedBody {
                name: b.name.clone(),
                triangles: b.triangles.len(),
                visible: b.visible,
                simulation: b.simulation.clone(),
            })
            .collect(),
        plane: SavedPlane {
            position_mm: plane.position.to_array(),
            quaternion_xyzw: plane.orientation.to_array(),
        },
    };
    let data = serde_json::to_vec_pretty(&f).map_err(|e| e.to_string())?;
    // Stage beside the project, then rename; a failed save preserves the old file.
    let stage = path.with_extension(format!("mhd-writing-{}", std::process::id()));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(&data)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&stage, path)
    })()
    .map_err(|e: std::io::Error| e.to_string());
    if result.is_err() {
        let _ = std::fs::remove_file(stage);
    }
    result
}
pub fn load(path: &Path) -> Result<Loaded, String> {
    let f: File = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if f.version != 1 {
        return Err("Unsupported project version".into());
    }
    f.domain.dims()?;
    validate_materials(&f.materials)?;
    let geometry = PathBuf::from(&f.geometry);
    let geometry = if geometry.is_absolute() {
        geometry
    } else {
        path.parent().unwrap_or(Path::new(".")).join(geometry)
    };
    let mut model = Model::load(&geometry)?;
    if model.bodies.len() != f.bodies.len() {
        return Err("Geometry changed: component count differs from the saved project".into());
    }
    for (b, s) in model.bodies.iter_mut().zip(f.bodies) {
        if b.name != s.name || b.triangles.len() != s.triangles {
            return Err(
                "Geometry changed: component identities differ from the saved project".into(),
            );
        }
        b.visible = s.visible;
        b.simulation = s.simulation;
    }
    validate_bodies(&model, &f.materials)?;
    let position = Vec3::from(f.plane.position_mm);
    let q = Quat::from_array(f.plane.quaternion_xyzw);
    if !position.is_finite() || !q.is_finite() || q.length() < 1e-6 {
        return Err("Invalid plane transform".into());
    }
    Ok(Loaded {
        model,
        settings: f.domain,
        materials: f.materials,
        plane: SectionPlane {
            position,
            orientation: q.normalize(),
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_roundtrip_and_invalid_values() {
        let mut m = Model::load(Path::new("assets/hotend.glb")).unwrap();
        m.bodies[1].simulation.material_id = 3;
        m.bodies[1].simulation.temperature_c = Some(250.);
        m.bodies[2].simulation.current_a = Some(-12.);
        m.bodies[0].visible = false;
        let s = DomainSettings::fit(&m);
        let mut materials = default_materials();
        materials[2].conductivity_s_m = Some(5e7);
        let mut p = SectionPlane::new(Vec3::new(1., 2., 3.));
        p.rotate_world(Vec3::Z, 0.4);
        std::fs::create_dir_all("artifacts").unwrap();
        let path = Path::new("artifacts/roundtrip.mhd.json");
        save(path, &m, &s, &materials, p).unwrap();
        let r = load(path).unwrap();
        assert_eq!(r.settings, s);
        assert_eq!(r.materials, materials);
        assert_eq!(r.model.bodies[1].simulation, m.bodies[1].simulation);
        assert_eq!(r.model.bodies[2].simulation, m.bodies[2].simulation);
        assert!(!r.model.bodies[0].visible);
        assert!(r.plane.orientation.dot(p.orientation).abs() > 0.9999);
        let mut invalid = s;
        invalid.cell_mm = 0.;
        assert!(invalid.dims().is_err());
    }
}
