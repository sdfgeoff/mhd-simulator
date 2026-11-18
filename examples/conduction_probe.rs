//! Experimental DC wall/current-sharing probe. Not an integrated electrode solver.
#[path = "conduction/gpu.rs"]
pub mod conduction;
#[cfg(test)]
#[path = "conduction/tests.rs"]
mod tests;
use mhd_workbench::{
    domain::GpuDomain,
    em::{EmState, Mode},
    gpu::read_bytes,
    project,
};
use std::{error::Error, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let a: Vec<_> = std::env::args().collect();
    let p = project::load(Path::new(
        a.get(1).ok_or("Expected PROJECT CURRENT_A OUTPUT")?,
    ))?;
    let current: f32 = a.get(2).ok_or("Missing current")?.parse()?;
    if !current.is_finite() || current <= 0. {
        return Err("Choose a positive current".into());
    }
    let output = Path::new(a.get(3).ok_or("Missing output")?);
    std::fs::create_dir_all(output.parent().ok_or("Output parent required")?)?;
    std::fs::create_dir(output)?;
    let gpu = GpuDomain::new()?;
    let volume = gpu.build(&p.model, &p.settings, &p.materials)?;
    if volume.overlaps > 0 {
        return Err(format!("{} overlapping cells", volume.overlaps).into());
    }
    let grid = &volume.inputs.snapshot().grid;
    let bytes = read_bytes(
        &gpu.context,
        volume.material_ids(),
        0,
        volume.material_ids().size(),
    )?;
    let ids = bytemuck::cast_slice::<u8, u32>(&bytes);
    let mut table = [0.; 256];
    for m in &p.materials {
        if m.id < 256 {
            table[m.id as usize] = m.conductivity_s_m.unwrap_or(0.) / 1e6;
        }
    }
    let cells: Vec<_> = ids
        .iter()
        .map(|&id| {
            [
                table[id as usize],
                match id {
                    16 => 0.5,
                    17 => -0.5,
                    0 | 4 => 3.,
                    _ => 2.,
                },
            ]
        })
        .collect();
    let em = EmState::new(
        gpu.context.clone(),
        &volume,
        &p.model,
        p.electromagnetics.clone(),
        Mode::Applied,
    )?;
    let contact: f32 = a.get(4).map(|s| s.parse()).transpose()?.unwrap_or(0.);
    let mut solver = conduction::Conduction::new(
        gpu.context.clone(),
        grid.dims,
        grid.cell_m,
        &cells,
        em.result_buffer(),
    )?;
    solver.set_contact_resistivity(contact)?;
    solver.solve()?;
    let unit = solver.fields(1.)?;
    if unit[0] <= 0. {
        return Err("No positive terminal current".into());
    }
    let voltage = current / unit[0];
    let stats = solver.fields(voltage)?;
    let fields = solver.read_fields()?;
    let h = f64::from(grid.cell_m);
    let cell = h.powi(3);
    let mut force = [0.; 3];
    let mut liquid_power = 0.;
    let mut liquid_current_integral = 0.;
    let mut wall_current_integral = 0.;
    let mut mean_by = 0.;
    let mut midplane_liquid = 0.;
    let mut midplane_wall = 0.;
    let mut count = 0usize;
    for (i, (&id, f)) in ids.iter().zip(&fields).enumerate() {
        let x = f64::from(grid.origin_m[0]) + (i as u32 % grid.dims[0]) as f64 * h + h / 2.;
        if i as u32 % grid.dims[0] == grid.dims[0] / 2 {
            if id == 4 {
                midplane_liquid += f64::from(f.j[0]) * h * h;
            }
            if id == 14 {
                midplane_wall += f64::from(f.j[0]) * h * h;
            }
        }
        if id == 4 {
            for (k, v) in force.iter_mut().enumerate() {
                *v += f64::from(f.force_heat[k]) * cell;
            }
            liquid_power += f64::from(f.force_heat[3]) * cell;
            mean_by += f64::from(f.b[1]);
            count += 1;
        }
        if x.abs() < 0.002 {
            if id == 4 {
                liquid_current_integral += f64::from(f.j[0]) * cell;
            }
            if id == 14 {
                wall_current_integral += f64::from(f.j[0]) * cell;
            }
        }
    }
    let report = serde_json::json!({"project":a[1],"contact_resistivity_ohm_m2":contact,"status":"cold DC conduction and applied permanent-magnet field only; no ejection simulation","iterations":solver.iterations,"true_relative_residual":solver.relative_residual,"current_A":stats[0],"return_current_A":stats[1],"terminal_voltage_V":voltage,"joule_power_W":stats[2],"source_power_W":voltage*stats[0],"power_relative_defect":stats[2]/(voltage*stats[0])-1.,"liquid_force_N":force,"liquid_joule_power_W":liquid_power,"liquid_mean_By_T":mean_by/count as f64,"midplane_adjacent_cell_liquid_current_A":midplane_liquid,"midplane_adjacent_cell_wall_current_A":midplane_wall,"mean_liquid_current_over_x_plus_minus_2mm_A":liquid_current_integral/0.004,"mean_wall_current_over_x_plus_minus_2mm_A":wall_current_integral/0.004,"limitations":["Midplane estimates integrate cell-centred J on the first nonnegative-x plane. The +/-2 mm mean includes the full interval even where a phase is absent; it is not a unique through-current.","Ideal electrical contact; material and oxide/contact resistance require bracketing.","Static liquid; no moving interface, motional EMF, self-field, driver, thermal evolution or droplet claim.","Applied magnetization and unit permeability approximate a specified magnet grade; check local field and temperature."]});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(output.join("em.bin"), bytemuck::cast_slice(&fields))?;
    std::fs::write(output.join("material-ids.bin"), &bytes)?;
    std::fs::write(
        output.join("grid.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"dims":grid.dims,"origin_m":grid.origin_m,"cell_m":grid.cell_m}),
        )?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
