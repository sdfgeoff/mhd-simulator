//! Headless design diagnostics from the normal RunState; no separate physics.
//! Static material labels are valid for the EM-only screening stage only.
use mhd_workbench::{domain::GpuDomain, em::CellField, gpu::read_bytes, project, run::RunState};
use std::{error::Error, path::Path};

pub(crate) fn main() -> Result<(), Box<dyn Error>> {
    run_with_expected_adapter(None)
}

pub(crate) fn run_with_expected_adapter(expected: Option<&str>) -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let input = Path::new(
        args.get(1)
            .ok_or("Expected PROJECT STEPS OUTPUT Z_LO_MM Z_HI_MM")?,
    );
    let steps: u32 = args.get(2).ok_or("Missing steps")?.parse()?;
    let out = Path::new(args.get(3).ok_or("Missing output")?);
    let zlo = args.get(4).ok_or("Missing ROI Z minimum")?.parse::<f64>()? * 0.001;
    let zhi = args.get(5).ok_or("Missing ROI Z maximum")?.parse::<f64>()? * 0.001;
    if steps == 0 || zlo >= zhi {
        return Err("Invalid probe range".into());
    }
    let p = project::load(input)?;
    if p.run.fluid_enabled || p.run.thermal_enabled || p.fluid.surface.enabled {
        return Err(
            "Static-label screening requires fluid, thermal and moving-interface modes disabled"
                .into(),
        );
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir(out)?;
    let gpu = GpuDomain::new()?;
    if expected.is_some_and(|name| gpu.name != name) {
        return Err("Native compute adapter differs from the expected physical device".into());
    }
    gpu.context.profiler.set_enabled(true);
    let volume = gpu.build(&p.model, &p.settings, &p.materials)?;
    if volume.overlaps > 0 {
        return Err(format!("{} overlapping cells", volume.overlaps).into());
    }
    let bytes = read_bytes(
        &gpu.context,
        volume.material_ids(),
        0,
        volume.material_ids().size(),
    )?;
    let ids: Vec<u32> = bytemuck::cast_slice(&bytes).to_vec();
    std::fs::write(out.join("initial-material-ids.bin"), &bytes)?;
    let grid = volume.inputs.snapshot().grid.clone();
    let h = f64::from(grid.cell_m);
    let cell = h.powi(3);
    let mut run = RunState::new(
        gpu.context.clone(),
        &volume,
        &p.model,
        p.run,
        p.electromagnetics,
        p.fluid,
    )?;
    let mut csv = String::from(
        "step,time_s,roi_volume_m3,roi_mean_Bz_T,roi_mean_applied_Bz_T,radial_pressure_proxy_Pa,roi_force_z_N,liquid_joule_power_W,solid_joule_power_W,em_iterations,true_relative_residual,accumulated_joule_J,external_work_J,coil_current_A,reaction_linkage_Wb,reaction_magnetic_J,reaction_BE_J\n",
    );
    for step in 0..=steps {
        if step > 0
            && let Err(error) = run.step(&p.model, &volume)
        {
            std::fs::write(
                out.join("run-status.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"status":"failed","accepted_steps":run.steps,"time_s":run.time_s,"error":error}),
                )?,
            )?;
            return Err(error.into());
        }
        let em = run.em.as_ref().ok_or("EM source required")?;
        let response = if run.em_config.wires.len() == 1
            && run.em_config.uniform.is_empty()
            && run.em_config.magnets.is_empty()
        {
            Some(em.coil_response(&volume)?)
        } else {
            None
        };
        let current = run
            .em_config
            .wires
            .first()
            .map_or(0., |wire| wire.current.value(run.time_s));
        gpu.context
            .ensure_memory(em.result_buffer().size(), "Hardware diagnostic readback")?;
        let fields = read_bytes(
            &gpu.context,
            em.result_buffer(),
            0,
            em.result_buffer().size(),
        )?;
        let fields: &[CellField] = bytemuck::cast_slice(&fields);
        let mut v = 0.;
        let mut bz = 0.;
        let mut applied = 0.;
        let mut moment = 0.;
        let mut fz = 0.;
        let mut liquid_heat = 0.;
        let mut solid_heat = 0.;
        for (i, (id, f)) in ids.iter().zip(fields).enumerate() {
            let heat = f64::from(f.force_heat[3]) * cell;
            if *id == 4 {
                liquid_heat += heat;
            } else if *id != 0 {
                solid_heat += heat;
            }
            if *id != 4 {
                continue;
            }
            let z = f64::from(grid.origin_m[2])
                + (i as u32 / (grid.dims[0] * grid.dims[1])) as f64 * h
                + 0.5 * h;
            if z < zlo || z > zhi {
                continue;
            }
            let x = f64::from(grid.origin_m[0]) + (i as u32 % grid.dims[0]) as f64 * h + 0.5 * h;
            let y = f64::from(grid.origin_m[1])
                + ((i as u32 / grid.dims[0]) % grid.dims[1]) as f64 * h
                + 0.5 * h;
            v += cell;
            bz += f64::from(f.b[2]) * cell;
            applied += f64::from(f.applied[2]) * cell;
            moment -= (f64::from(f.force_heat[0]) * x + f64::from(f.force_heat[1]) * y) * cell;
            fz += f64::from(f.force_heat[2]) * cell;
        }
        if v == 0. {
            return Err("No liquid cells in diagnostic ROI".into());
        }
        let linkage = response
            .as_ref()
            .and_then(|r| r.reaction_linkage_wb)
            .map_or(String::new(), |v| v.to_string());
        let reaction = response.as_ref().map_or(0., |r| r.reaction_magnetic_j);
        let reaction_be = response
            .as_ref()
            .map_or(0., |r| r.reaction_backward_euler_j);
        csv.push_str(&format!(
            "{step},{},{v},{},{},{},{fz},{liquid_heat},{solid_heat},{},{},{},{},{current},{linkage},{reaction},{reaction_be}\n",
            run.time_s,
            bz / v,
            applied / v,
            moment / v,
            em.diagnostics.iterations,
            em.diagnostics.relative_residual,
            run.energy.joule_j,
            run.energy.external_work_j
        ));
        std::fs::write(out.join("diagnostics.csv"), &csv)?;
        if step == 0 || step == 20 || step == steps {
            run.export_fields(&volume, &p.model, &out.join(format!("frame-{step:08}")))?;
        }
        eprintln!(
            "step {step}/{steps}: t={:.3} us, Bz={:.4} T, radial proxy={:.1} Pa, EM={} iterations",
            run.time_s * 1e6,
            bz / v,
            moment / v,
            em.diagnostics.iterations
        );
    }
    let mut performance = String::from(
        "step,time_s,accepted,stage,cpu_wall_ms,gpu_pass_ms,recorded_passes,total_passes\n",
    );
    for report in gpu.context.profiler.take_completed(true) {
        performance.push_str(&report.csv_rows());
    }
    std::fs::write(out.join("performance.csv"), performance)?;
    std::fs::write(
        out.join("run-status.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"complete","accepted_steps":run.steps,"time_s":run.time_s,"origin_m":grid.origin_m,"dims":grid.dims,"cell_m":grid.cell_m,"ROI_z_m":[zlo,zhi],"diagnostic":"radial force moment per ROI volume, not a pressure solution or ejection criterion","warnings":volume.warnings}),
        )?,
    )?;
    Ok(())
}
