pub use crate::simulation::{
    DomainSettings, Material, SimulationProperties, default_materials, validate_bodies,
};
use crate::simulation::{ensure_presets, validate_materials};
use crate::{model::Model, plane::SectionPlane};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
                triangles: b.triangles().len(),
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
    let mut f: File = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if f.version != 1 {
        return Err("Unsupported project version".into());
    }
    f.domain.dims()?;
    validate_materials(&f.materials)?;
    ensure_presets(&mut f.materials);
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
        if b.name != s.name || b.triangles().len() != s.triangles {
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

#[cfg(test)]
mod material_preset_tests {
    use super::*;
    #[test]
    fn stainless_preset_and_legacy_library_preserve_custom_ids() {
        let materials = default_materials();
        let steel = materials
            .iter()
            .find(|m| m.name == "316 stainless steel")
            .unwrap();
        assert_eq!(steel.density_kg_m3, Some(8000.));
        assert!((steel.conductivity_s_m.unwrap() - 1_333_333.4).abs() < 1.);
        assert!(steel.relative_permeability.is_none());
        let ceramic = materials.iter().find(|m| m.name == "Alumina").unwrap();
        assert!(ceramic.conductivity_s_m.is_none() && ceramic.density_kg_m3.is_none());
        let mut legacy: Vec<_> = materials.into_iter().filter(|m| m.id < 6).collect();
        let mut custom = legacy[1].clone();
        custom.id = 6;
        custom.name = "My custom material".into();
        legacy.push(custom.clone());
        ensure_presets(&mut legacy);
        assert_eq!(legacy.iter().find(|m| m.id == 6), Some(&custom));
        assert_eq!(
            legacy
                .iter()
                .find(|m| m.name == "316 stainless steel")
                .unwrap()
                .id,
            7
        );
        assert!(legacy.iter().any(|m| m.name == "Alumina" && m.id == 8));
        let text = r#"{"id":2,"name":"Copper","conductivity_s_m":null,"relative_permeability":null,"density_kg_m3":null}"#;
        let old: Material = serde_json::from_str(text).unwrap();
        assert!(old.notes.is_empty() && old.reference.is_none());
    }
}
