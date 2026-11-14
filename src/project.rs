#[cfg(test)]
use crate::simulation::default_materials;
use crate::simulation::validate_materials;
use crate::simulation::{DomainSettings, Material, SimulationProperties, validate_bodies};
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
    #[serde(default)]
    run: crate::run::RunConfig,
    #[serde(default)]
    cad_curves: Option<Vec<crate::model::CurvePath>>,
    #[serde(default)]
    electromagnetics: crate::em::EmConfig,
    #[serde(default)]
    fluid: crate::fluid::FluidConfig,
    version: u32,
    geometry: String,
    domain: DomainSettings,
    materials: Vec<Material>,
    bodies: Vec<SavedBody>,
    plane: SavedPlane,
}
pub struct Loaded {
    pub run: crate::run::RunConfig,
    pub electromagnetics: crate::em::EmConfig,
    pub fluid: crate::fluid::FluidConfig,
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
    electromagnetics: &crate::em::EmConfig,
    fluid: &crate::fluid::FluidConfig,
    run: &crate::run::RunConfig,
) -> Result<(), String> {
    electromagnetics.validate(model)?;
    fluid.validate()?;
    run.validate()?;
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
        run: run.clone(),
        cad_curves: Some(model.curves.clone()),
        electromagnetics: electromagnetics.clone(),
        fluid: fluid.clone(),
        version: 3,
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
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut f: File = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    if !matches!(f.version, 1 | 2 | 3) {
        return Err("Unsupported project version".into());
    }
    f.domain.dims()?;
    f.run.validate()?;
    crate::simulation::ensure_material_library(&mut f.materials);
    if f.version == 1 {
        let presets = crate::simulation::default_materials();
        for m in &mut f.materials {
            if let Some(p) = presets
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&m.name))
            {
                let mut changed = false;
                for (a, b) in [
                    (&mut m.thermal.solidus_c, p.thermal.solidus_c),
                    (&mut m.thermal.liquidus_c, p.thermal.liquidus_c),
                    (
                        &mut m.thermal.heat_capacity_j_kg_k,
                        p.thermal.heat_capacity_j_kg_k,
                    ),
                    (
                        &mut m.thermal.conductivity_w_m_k,
                        p.thermal.conductivity_w_m_k,
                    ),
                    (&mut m.thermal.latent_heat_j_kg, p.thermal.latent_heat_j_kg),
                ] {
                    if a.is_none() && b.is_some() {
                        *a = b;
                        changed = true;
                    }
                }
                if changed {
                    m.notes.push_str(" Thermal defaults added during project upgrade. Solder latent heats, Sn60Pb40 heat capacity and magnet thermal conductivity are illustrative; calibrate at operating temperature.");
                }
            }
        }
    }

    if f.version < 3 {
        // Material values are authoritative. Migrate old global interface settings only
        // when the new material field was absent; never replace a material override.
        let presets = crate::simulation::default_materials();
        let missing: std::collections::BTreeSet<u32> = raw["materials"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m.get("surface_tension_n_m").is_none())
            .filter_map(|m| m["id"].as_u64().map(|id| id as u32))
            .collect();
        for m in &mut f.materials {
            if missing.contains(&m.id) {
                m.surface_tension_n_m = presets
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&m.name))
                    .and_then(|p| p.surface_tension_n_m);
            }
        }
        let candidates: Vec<_> = f
            .materials
            .iter()
            .filter(|m| {
                m.dynamic_viscosity_pa_s.is_some()
                    && m.thermal.solidus_c.is_some()
                    && f.bodies
                        .iter()
                        .any(|b| b.simulation.included && b.simulation.material_id == m.id)
            })
            .map(|m| m.id)
            .collect();
        let liquid = if f.run.thermal_enabled && candidates.len() == 1 {
            candidates[0]
        } else {
            f.fluid.material_id
        };
        if let Some(t) = raw["fluid"]["surface"]["tension_n_m"]
            .as_f64()
            .or_else(|| f.fluid.surface.enabled.then_some(0.5))
        {
            if let Some(m) = f
                .materials
                .iter_mut()
                .find(|m| m.id == liquid && missing.contains(&m.id))
            {
                m.surface_tension_n_m = Some(t as f32);
                m.notes.push_str(
                    " Surface tension migrated from the former global interface setting.",
                );
            }
        }
        if let Some(d) = raw["fluid"]["surface"]["air_density_kg_m3"]
            .as_f64()
            .or_else(|| f.fluid.surface.enabled.then_some(1.2))
        {
            if let Some(m) = f.materials.iter_mut().find(|m| m.id == 0) {
                if m.density_kg_m3.is_none() || m.density_kg_m3 == Some(1.2) {
                    m.density_kg_m3 = Some(d as f32);
                } else if m.density_kg_m3 != Some(d as f32) {
                    m.notes.push_str(" Ambient material density takes precedence over the former global air-density setting.");
                }
            }
        }
    }

    validate_materials(&f.materials)?;
    let geometry = PathBuf::from(&f.geometry);
    let geometry = if geometry.is_absolute() {
        geometry
    } else {
        path.parent().unwrap_or(Path::new(".")).join(geometry)
    };
    let mut model = Model::load(&geometry)?;
    if f.cad_curves
        .as_ref()
        .is_some_and(|curves| curves != &model.curves)
    {
        return Err("Geometry changed: CAD centreline identities or coordinates differ from the saved project".into());
    }
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
    f.electromagnetics.validate(&model)?;
    f.fluid.validate()?;
    Ok(Loaded {
        run: f.run,
        electromagnetics: f.electromagnetics,
        fluid: f.fluid,
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
    fn thermal_upgrade_preserves_overrides_and_version_two_unknowns() {
        let mut data: serde_json::Value =
            serde_json::from_slice(&std::fs::read("assets/hotend-cad.mhd.json").unwrap()).unwrap();
        data["geometry"] = serde_json::json!(
            std::fs::canonicalize("assets/hotend-cad.gltf")
                .unwrap()
                .to_str()
                .unwrap()
        );
        data["version"] = serde_json::json!(1);
        let material = &mut data["materials"][2];
        material["thermal"]["heat_capacity_j_kg_k"] = serde_json::json!(123.);
        material["thermal"]["conductivity_w_m_k"] = serde_json::Value::Null;
        std::fs::create_dir_all("artifacts").unwrap();
        let path = Path::new("artifacts/thermal-upgrade.mhd.json");
        std::fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
        let upgraded = load(path).unwrap();
        assert_eq!(
            upgraded.materials[2].thermal.heat_capacity_j_kg_k,
            Some(123.)
        );
        assert_eq!(upgraded.materials[2].thermal.conductivity_w_m_k, Some(394.));
        data["version"] = serde_json::json!(2);
        std::fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert_eq!(
            load(path).unwrap().materials[2].thermal.conductivity_w_m_k,
            None
        );
    }
    #[test]
    fn interface_properties_migrate_to_materials_and_roundtrip_without_globals() {
        let source = Path::new("assets/hotend-free-surface.mhd.json");
        let mut raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
        let geometry = source
            .parent()
            .unwrap()
            .join(raw["geometry"].as_str().unwrap());
        raw["geometry"] = serde_json::json!(std::fs::canonicalize(geometry).unwrap());
        raw["version"] = serde_json::json!(2);
        for m in raw["materials"].as_array_mut().unwrap() {
            m.as_object_mut().unwrap().remove("surface_tension_n_m");
        }
        raw["fluid"]["surface"]["tension_n_m"] = serde_json::json!(0.72);
        raw["fluid"]["surface"]["air_density_kg_m3"] = serde_json::json!(2.5);
        std::fs::create_dir_all("artifacts").unwrap();
        let path = Path::new("artifacts/interface-upgrade.mhd.json");
        std::fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
        let p = load(path).unwrap();
        assert_eq!(p.materials[3].surface_tension_n_m, Some(0.72));
        assert_eq!(p.materials[0].density_kg_m3, Some(2.5));
        save(
            path,
            &p.model,
            &p.settings,
            &p.materials,
            p.plane,
            &p.electromagnetics,
            &p.fluid,
            &p.run,
        )
        .unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["version"], 3);
        assert!(saved["fluid"]["surface"].get("tension_n_m").is_none());
        assert_eq!(load(path).unwrap().materials, p.materials);
        raw["materials"][0]["density_kg_m3"] = serde_json::json!(3.);
        raw["materials"][3]["surface_tension_n_m"] = serde_json::json!(0.9);
        std::fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
        let p = load(path).unwrap();
        assert_eq!(p.materials[0].density_kg_m3, Some(3.));
        assert_eq!(p.materials[3].surface_tension_n_m, Some(0.9));
        let mut implicit = raw.clone();
        implicit["materials"][3]["name"] = serde_json::json!("Custom liquid");
        implicit["materials"][3]
            .as_object_mut()
            .unwrap()
            .remove("surface_tension_n_m");
        implicit["materials"][0]["density_kg_m3"] = serde_json::Value::Null;
        implicit["fluid"]["surface"]
            .as_object_mut()
            .unwrap()
            .remove("tension_n_m");
        implicit["fluid"]["surface"]
            .as_object_mut()
            .unwrap()
            .remove("air_density_kg_m3");
        std::fs::write(path, serde_json::to_vec(&implicit).unwrap()).unwrap();
        let p = load(path).unwrap();
        assert_eq!(p.materials[3].surface_tension_n_m, Some(0.5));
        assert_eq!(p.materials[0].density_kg_m3, Some(1.2));
        raw["version"] = serde_json::json!(3);
        raw["materials"][3]["surface_tension_n_m"] = serde_json::Value::Null;
        std::fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
        assert_eq!(load(path).unwrap().materials[3].surface_tension_n_m, None);
    }
    #[test]
    fn multiple_cad_bindings_roundtrip_and_guard_curve_identity() {
        let path = Path::new("assets/hotend-cad.mhd.json");
        let loaded = load(path).unwrap();
        std::fs::create_dir_all("artifacts").unwrap();
        let saved = Path::new("artifacts/multiple-cad.mhd.json");
        save(
            saved,
            &loaded.model,
            &loaded.settings,
            &loaded.materials,
            loaded.plane,
            &loaded.electromagnetics,
            &loaded.fluid,
            &loaded.run,
        )
        .unwrap();
        let restored = load(saved).unwrap();
        assert_eq!(restored.electromagnetics, loaded.electromagnetics);
        assert_eq!(restored.model.curves, loaded.model.curves);
        let mut data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(saved).unwrap()).unwrap();
        data["cad_curves"][0]["name"] = serde_json::json!("Wrong curve");
        std::fs::write(saved, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(load(saved).err().unwrap().contains("centreline identities"));
    }
    #[test]
    fn project_roundtrip_and_invalid_values() {
        let mut m = Model::load(Path::new("assets/hotend.glb")).unwrap();
        m.bodies[1].simulation.material_id = 3;
        m.bodies[1].simulation.temperature_c = Some(250.);
        m.bodies[0].visible = false;
        let s = DomainSettings::fit(&m);
        let mut materials = default_materials();
        materials[2].conductivity_s_m = Some(5e7);
        let mut p = SectionPlane::new(Vec3::new(1., 2., 3.));
        p.rotate_world(Vec3::Z, 0.4);
        std::fs::create_dir_all("artifacts").unwrap();
        let path = Path::new("artifacts/roundtrip.mhd.json");
        save(
            path,
            &m,
            &s,
            &materials,
            p,
            &Default::default(),
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
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
    fn named_material_values_match_reference_conversions() {
        let materials = default_materials();
        let steel = materials
            .iter()
            .find(|m| m.name == "316 stainless steel")
            .unwrap();
        assert_eq!(steel.density_kg_m3, Some(8000.));
        assert!((steel.conductivity_s_m.unwrap() - 1_333_333.4).abs() < 1.);
        assert_eq!(steel.relative_permeability, Some(1.));
        let ceramic = materials.iter().find(|m| m.name == "Alumina").unwrap();
        assert_eq!(ceramic.conductivity_s_m, Some(0.));
        assert_eq!(ceramic.density_kg_m3, Some(3900.));
    }
}
