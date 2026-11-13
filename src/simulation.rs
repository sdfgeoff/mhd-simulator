//! Configuration shared by geometry preparation, GPU execution and project persistence.
use crate::model::Model;
use glam::Vec3;
use serde::{Deserialize, Serialize};
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
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub reference: Option<String>,
}
pub fn default_materials() -> Vec<Material> {
    let preset = |id, name: &str, sigma, mu, density, notes: &str, reference: &str| Material {
        id,
        name: name.into(),
        conductivity_s_m: Some(sigma),
        relative_permeability: Some(mu),
        density_kg_m3: Some(density),
        notes: notes.into(),
        reference: (!reference.is_empty()).then(|| reference.into()),
    };
    vec![
        preset(
            0,
            "Air / void",
            0.,
            1.,
            1.2,
            "EM vacuum/insulator approximation; approximate air density at room temperature. No temperature dependence.",
            "",
        ),
        Material {
            id: 1,
            name: "Unassigned".into(),
            conductivity_s_m: None,
            relative_permeability: None,
            density_kg_m3: None,
            notes: "Select a physical material before simulating this component.".into(),
            reference: None,
        },
        preset(
            2,
            "Copper",
            58e6,
            1.,
            8890.,
            "100% IACS reference conductivity and density at 20 °C; unit permeability approximation. Temperature dependence is not applied.",
            "https://help.copper.fyi/hc/en-us/articles/4410229175442-How-does-copper-set-the-standard-for-electrical-conductivity",
        ),
        preset(
            3,
            "Sn60Pb40",
            1. / 14.99e-8,
            1.,
            8520.,
            "Bulk solder datasheet defaults: resistivity 14.99 µΩ·cm and density 8.52 Mg/m³. Unit permeability approximation. Not calibrated molten-metal properties; temperature dependence is not applied.",
            "https://www.ametekinterconnect.com/-/media/ametek-ecp/v2/files/cw_datasheets_sds_cfsi/datasheets/sn60pb40%20data%20sheet.pdf?la=en",
        ),
        preset(
            4,
            "Sn99.3Cu0.7",
            1. / 11.99e-8,
            1.,
            7420.,
            "ALPHA VACULOY datasheet defaults: resistivity 11.99 µΩ·cm and density 7.42 g/cm³. Unit permeability approximation. Not calibrated molten-metal properties; temperature dependence is not applied.",
            "https://www.macdermidalpha.com/sites/default/files/2024-04/ALPHA-VACULOY-Sn99.3Cu0.7-Solid-Solder-EN-12Apr24-TB.pdf",
        ),
        preset(
            5,
            "Permanent magnet",
            1. / 1.6e-6,
            1.,
            7500.,
            "Representative NdFeB: resistivity 160 µΩ·cm, density 7.5 g/cm³. The solver uses μr=1 as an explicit approximation (typical recoil μr≈1.05). Set source magnetization separately per CAD body; no temperature dependence or demagnetization model.",
            "https://www.kjmagnetics.com/neodymium-magnet-specifications.asp",
        ),
        stainless316(6),
        alumina(7),
    ]
}
fn stainless316(id: u32) -> Material {
    Material {id,name:"316 stainless steel".into(),conductivity_s_m:Some(1./0.75e-6),relative_permeability:Some(1.),density_kg_m3:Some(8000.),
        notes:"Density and conductivity references at 20 °C. Unit permeability is an annealed/nonmagnetic approximation; processing can change it. No temperature dependence is applied.".into(),
        reference:Some("https://www.thyssenkrupp-materials.co.uk/stainless-steel-316-14401.html/index.html".into())}
}
fn alumina(id: u32) -> Material {
    Material {id,name:"Alumina".into(),conductivity_s_m:Some(0.),relative_permeability:Some(1.),density_kg_m3:Some(3900.),
        notes:"Representative CoorsTek AD-995 (99.5%) density. Ideal electrical insulator and unit-permeability approximations; room-temperature resistivity exceeds 10¹⁴ Ω·cm. Grade and temperature matter; no temperature dependence is applied.".into(),
        reference:Some("https://www2.coorstek.com/media/4235/advanced-alumina.pdf".into())}
}
/// Fill only missing scalar values on matching built-in materials; preserve overrides and custom materials.
pub fn fill_missing_defaults(materials: &mut [Material]) -> bool {
    let presets = default_materials();
    let mut changed = false;
    for material in materials {
        if let Some(preset) = presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&material.name))
        {
            let mut filled = false;
            for (target, value) in [
                (&mut material.conductivity_s_m, preset.conductivity_s_m),
                (
                    &mut material.relative_permeability,
                    preset.relative_permeability,
                ),
                (&mut material.density_kg_m3, preset.density_kg_m3),
            ] {
                if target.is_none() && value.is_some() {
                    *target = value;
                    filled = true;
                }
            }
            if filled && material.reference.is_none() {
                material.reference = preset.reference.clone();
            }
            if filled && !material.notes.contains(&preset.notes) {
                material.notes = format!("{} {}", material.notes, preset.notes).trim().into();
            }
            changed |= filled;
        }
    }
    changed
}
pub(crate) fn ensure_presets(materials: &mut Vec<Material>) {
    for mut preset in [stainless316(0), alumina(0)] {
        if materials
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case(&preset.name))
        {
            continue;
        }
        if let Some(id) = (1..255).find(|id| materials.iter().all(|m| m.id != *id)) {
            preset.id = id;
            materials.push(preset);
        }
    }
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
            + 8192
            + bodies as u64 * 32
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

/// Solver coordinates are metres and kelvin. CAD and saved projects remain mm / °C.
#[derive(Clone, Debug, PartialEq)]
pub struct GridSi {
    pub origin_m: [f32; 3],
    pub cell_m: f32,
    pub dims: [u32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}
impl Axis {
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}
/// Describes storage without committing any future solver to a discretization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldLocation {
    CellCentre,
    Face(Axis),
    Edge(Axis),
}
impl GridSi {
    pub fn from_settings(settings: &DomainSettings) -> Result<Self, String> {
        let grid = Self {
            origin_m: settings.min_mm.map(|v| v * 1e-3),
            cell_m: settings.cell_mm * 1e-3,
            dims: settings.dims()?,
        };
        if grid.cell_m == 0. {
            return Err("Cell spacing underflows in solver SI units".into());
        }
        Ok(grid)
    }
    pub fn sample_dims(&self, location: FieldLocation) -> [u32; 3] {
        let mut d = self.dims;
        for (i, v) in d.iter_mut().enumerate() {
            if match location {
                FieldLocation::CellCentre => false,
                FieldLocation::Face(a) => i == a.index(),
                FieldLocation::Edge(a) => i != a.index(),
            } {
                *v += 1;
            }
        }
        d
    }
    pub fn position_m(&self, index: [u32; 3], location: FieldLocation) -> Result<[f32; 3], String> {
        let dims = self.sample_dims(location);
        if (0..3).any(|i| index[i] >= dims[i]) {
            return Err("Field sample index is outside its layout".into());
        }
        Ok(std::array::from_fn(|i| {
            let half = match location {
                FieldLocation::CellCentre => true,
                FieldLocation::Face(a) => i != a.index(),
                FieldLocation::Edge(a) => i == a.index(),
            };
            self.origin_m[i] + (index[i] as f32 + if half { 0.5 } else { 0. }) * self.cell_m
        }))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentInput {
    pub name: String,
    pub included: bool,
    pub material_id: u32,
    pub initial_temperature_k: Option<f32>,
    pub current_a: Option<f32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SimulationSnapshot {
    pub geometry_ids: Vec<u64>,
    pub grid: GridSi,
    pub materials: Vec<Material>,
    pub components: Vec<ComponentInput>,
}
#[derive(Clone, Copy, Debug)]
pub enum MaterialProperty {
    Conductivity,
    RelativePermeability,
    Density,
}
impl SimulationSnapshot {
    pub fn from_model(
        model: &Model,
        settings: &DomainSettings,
        materials: &[Material],
    ) -> Result<Self, String> {
        validate_bodies(model, materials)?;
        let snapshot = Self {
            grid: GridSi::from_settings(settings)?,
            geometry_ids: model.bodies.iter().map(|b| b.geometry_id()).collect(),
            materials: materials.to_vec(),
            components: model
                .bodies
                .iter()
                .map(|b| ComponentInput {
                    name: b.name.clone(),
                    included: b.simulation.included,
                    material_id: b.simulation.material_id,
                    initial_temperature_k: b.simulation.temperature_c.map(|v| v + 273.15),
                    current_a: b.simulation.current_a,
                })
                .collect(),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
    pub fn validate(&self) -> Result<(), String> {
        validate_materials(&self.materials)?;
        if !self.grid.cell_m.is_finite()
            || self.grid.cell_m <= 0.
            || self.grid.origin_m.iter().any(|v| !v.is_finite())
            || self.grid.dims.iter().any(|&d| d == 0 || d > 2048)
        {
            return Err("Invalid SI grid".into());
        }
        if self.geometry_ids.len() != self.components.len() {
            return Err("Component geometry identities do not match the inputs".into());
        }
        for c in &self.components {
            if c.material_id == 0 || !self.materials.iter().any(|m| m.id == c.material_id) {
                return Err(format!("{} needs a non-air material", c.name));
            }
            if c.initial_temperature_k
                .is_some_and(|v| !v.is_finite() || v < 0.)
                || c.current_a.is_some_and(|v| !v.is_finite())
            {
                return Err(format!("Invalid SI temperature or current for {}", c.name));
            }
        }
        Ok(())
    }
    /// Call before creating solver bindings. Only explicitly required properties are checked.
    /// Unknown values remain unknown; a specified zero conductivity is valid.
    pub fn require_properties(
        &self,
        properties: &[MaterialProperty],
        include_air: bool,
    ) -> Result<(), String> {
        self.validate()?;
        for material in &self.materials {
            if !(include_air && material.id == 0)
                && !self
                    .components
                    .iter()
                    .any(|c| c.included && c.material_id == material.id)
            {
                continue;
            }
            for property in properties {
                let value = match property {
                    MaterialProperty::Conductivity => material.conductivity_s_m,
                    MaterialProperty::RelativePermeability => material.relative_permeability,
                    MaterialProperty::Density => material.density_kg_m3,
                };
                if value.is_none() {
                    return Err(format!(
                        "Material {} is missing required {:?}",
                        material.name, property
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn si_boundary_and_all_staggered_layouts_have_explicit_positions() {
        let settings = DomainSettings {
            min_mm: [-10., 20., 30.],
            max_mm: [-6., 26., 38.],
            cell_mm: 2.,
        };
        let grid = GridSi::from_settings(&settings).unwrap();
        assert_eq!(grid.dims, [2, 3, 4]);
        assert!((grid.cell_m - 0.002).abs() < 1e-9);
        let near = |a: [f32; 3], b: [f32; 3]| {
            for i in 0..3 {
                assert!((a[i] - b[i]).abs() < 1e-8, "{a:?} != {b:?}");
            }
        };
        near(
            grid.position_m([0; 3], FieldLocation::CellCentre).unwrap(),
            [-0.009, 0.021, 0.031],
        );
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let i = axis.index();
            let face = FieldLocation::Face(axis);
            let edge = FieldLocation::Edge(axis);
            let mut face_dims = [2, 3, 4];
            face_dims[i] += 1;
            assert_eq!(grid.sample_dims(face), face_dims);
            let mut face_first = [-0.009, 0.021, 0.031];
            face_first[i] = grid.origin_m[i];
            near(grid.position_m([0; 3], face).unwrap(), face_first);
            let mut edge_dims = [3, 4, 5];
            edge_dims[i] -= 1;
            assert_eq!(grid.sample_dims(edge), edge_dims);
            let mut edge_first = grid.origin_m;
            edge_first[i] += 0.001;
            near(grid.position_m([0; 3], edge).unwrap(), edge_first);
            assert!(grid.position_m(face_dims, face).is_err());
            assert!(grid.position_m(edge_dims, edge).is_err());
        }
        let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        model.bodies[0].simulation.temperature_c = Some(250.);
        let snapshot =
            SimulationSnapshot::from_model(&model, &settings, &default_materials()).unwrap();
        assert!((snapshot.components[0].initial_temperature_k.unwrap() - 523.15).abs() < 1e-4);
    }
    #[test]
    fn named_presets_are_complete_and_missing_fill_preserves_overrides() {
        let mut materials = default_materials();
        for material in materials.iter().filter(|m| m.id != 1) {
            assert!(
                material.conductivity_s_m.is_some()
                    && material.relative_permeability.is_some()
                    && material.density_kg_m3.is_some(),
                "{}",
                material.name
            );
        }
        materials[2].conductivity_s_m = Some(123.);
        materials[2].density_kg_m3 = None;
        materials[3].relative_permeability = None;
        let mut custom = materials[2].clone();
        custom.name = "Custom".into();
        custom.id = 55;
        materials.push(custom.clone());
        assert!(fill_missing_defaults(&mut materials));
        assert_eq!(materials[2].conductivity_s_m, Some(123.));
        assert_eq!(materials[2].density_kg_m3, Some(8890.));
        assert_eq!(materials[3].relative_permeability, Some(1.));
        assert_eq!(materials.last(), Some(&custom));
        assert!(!fill_missing_defaults(&mut materials));
        assert!(materials[1].conductivity_s_m.is_none());
    }
    #[test]
    fn required_materials_reject_unknown_and_allow_explicit_zero() {
        let mut model = Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let settings = DomainSettings::fit(&model);
        let mut materials = default_materials();
        let mut snapshot = SimulationSnapshot::from_model(&model, &settings, &materials).unwrap();
        assert!(
            snapshot
                .require_properties(&[MaterialProperty::Conductivity], false)
                .unwrap_err()
                .contains("Unassigned")
        );
        materials[1].conductivity_s_m = Some(0.);
        snapshot = SimulationSnapshot::from_model(&model, &settings, &materials).unwrap();
        snapshot
            .require_properties(&[MaterialProperty::Conductivity], false)
            .unwrap();
        snapshot.materials[0].conductivity_s_m = None;
        assert!(
            snapshot
                .require_properties(&[MaterialProperty::Conductivity], true)
                .unwrap_err()
                .contains("Air")
        );
        assert!(
            snapshot
                .require_properties(
                    &[
                        MaterialProperty::RelativePermeability,
                        MaterialProperty::Density
                    ],
                    false
                )
                .is_err()
        );
        model.bodies[0].simulation.included = false;
        SimulationSnapshot::from_model(&model, &settings, &default_materials())
            .unwrap()
            .require_properties(&[MaterialProperty::Density], false)
            .unwrap();
        snapshot.components[0].initial_temperature_k = Some(-1.);
        assert!(snapshot.validate().is_err());
    }
}
