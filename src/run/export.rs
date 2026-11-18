//! Accepted solver fields for headless analysis; metadata publishes a complete frame.
use super::RunState;
use crate::{domain::Volume, gpu::read_bytes};
use std::path::Path;

impl RunState {
    /// Save the latest accepted physical fields. The destination must be new.
    /// Metadata is published after every field file has been written successfully.
    pub fn export_fields(
        &self,
        volume: &Volume,
        model: &crate::model::Model,
        directory: &Path,
    ) -> Result<(), String> {
        if !self.is_current(volume) {
            return Err("Only an accepted run with matching inputs can export fields".into());
        }
        let snapshot = volume.inputs.snapshot();
        let grid = &snapshot.grid;
        let mut fields = Vec::new();
        if let Some(s) = &self.em {
            fields.push(("em", s.result_buffer()));
        }
        if let Some(s) = &self.fluid {
            fields.push(("fluid", s.result_buffer()));
        }
        if let Some(s) = &self.thermal {
            fields.push(("thermal", s.result_buffer()));
        }
        self.context.ensure_memory(
            fields.iter().map(|(_, b)| b.size()).max().unwrap_or(0),
            "Headless field export readback",
        )?;
        if let Some(parent) = directory.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // Exclusive creation protects previous exports and concurrent writers.
        std::fs::create_dir(directory)
            .map_err(|e| format!("Create new frame directory {}: {e}", directory.display()))?;
        let write = || -> Result<(), String> {
            let mut files = Vec::new();
            for (name, buffer) in &fields {
                let bytes = read_bytes(&self.context, buffer, 0, buffer.size())?;
                let file = format!("{name}.bin");
                std::fs::write(directory.join(&file), &bytes).map_err(|e| e.to_string())?;
                files.push(serde_json::json!({"physics":name,"file":file,"bytes":bytes.len(),"f32_components_per_cell":16}));
            }
            let metadata = serde_json::json!({
                "format_version":1,"step":self.steps,"time_s":self.time_s,
                "geometry":std::fs::canonicalize(&model.source).ok().map(|p|p.to_string_lossy().into_owned()),
                "cad_curves":model.curves,
                "bodies":model.bodies.iter().map(|b|serde_json::json!({"name":b.name,"simulation":b.simulation})).collect::<Vec<_>>(),
                "origin_m":grid.origin_m,"cell_m":grid.cell_m,"dims":grid.dims,
                "byte_order":if cfg!(target_endian="little") {"little"} else {"big"},
                "cell_order":"x fastest, then y, then z","files":files,
                "moving_interface":self.surface.is_some(),
                "forcing_kind":if self.em.is_some() {"computed_em"} else if self.prescribed_force.is_some() {"prescribed"} else {"none"},
                "run":self.config,"electromagnetics":self.em_config,"fluid":self.fluid_config,
                "materials":&snapshot.materials[..],
                "material_volume_m3":self.surface.as_ref().map(|s|s.diagnostics.volume_m3),
                "energy_j":{"magnetic":self.energy.magnetic_j,"kinetic":self.energy.kinetic_j,"thermal":self.energy.thermal_j,"joule":self.energy.joule_j,"external_work":self.energy.external_work_j}
            });
            let bytes = serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?;
            let staged = directory.join("metadata.writing");
            std::fs::write(&staged, bytes).map_err(|e| e.to_string())?;
            std::fs::rename(staged, directory.join("metadata.json")).map_err(|e| e.to_string())
        };
        if let Err(error) = write() {
            // This directory was created exclusively by this export.
            let _ = std::fs::remove_dir_all(directory);
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_fields_export_with_matching_grid_and_protect_previous_output() {
        let _lock = crate::gpu_test_lock();
        let gpu = crate::domain::GpuDomain::new().unwrap();
        let mut model = crate::model::Model::load(Path::new("assets/annulus.gltf")).unwrap();
        model.bodies[0].simulation.material_id = 3;
        let volume = gpu
            .build(
                &model,
                &crate::simulation::DomainSettings {
                    min_mm: [-6.; 3],
                    max_mm: [6.; 3],
                    cell_mm: 1.,
                },
                &crate::simulation::default_materials(),
            )
            .unwrap();
        let mut run = RunState::new(
            gpu.context.clone(),
            &volume,
            &model,
            Default::default(),
            Default::default(),
            crate::fluid::FluidConfig {
                acceleration_m_s2: [0., 0., -0.1],
                ..Default::default()
            },
        )
        .unwrap();
        run.step(&model, &volume).unwrap();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::path::PathBuf::from(format!("artifacts/export-tests/{stamp}/frame-00000001"));
        run.export_fields(&volume, &model, &path).unwrap();
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(metadata["step"], 1);
        assert_eq!(metadata["time_s"].as_f64().unwrap(), run.time_s);
        assert_eq!(metadata["dims"], serde_json::json!(volume.dims));
        let source = run.fluid.as_ref().unwrap().result_buffer();
        let actual = std::fs::read(path.join("fluid.bin")).unwrap();
        assert_eq!(
            actual,
            read_bytes(&gpu.context, source, 0, source.size()).unwrap()
        );
        assert!(run.export_fields(&volume, &model, &path).is_err());
        assert_eq!(actual, std::fs::read(path.join("fluid.bin")).unwrap());
        run.failed = true;
        let rejected = path.with_file_name("rejected");
        assert!(run.export_fields(&volume, &model, &rejected).is_err());
        assert!(!rejected.exists());
    }
}
