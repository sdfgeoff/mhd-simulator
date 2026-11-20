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

fn first_nonnegative_cell(origin: f64, h: f64, n: u32) -> Option<u32> {
    (0..n).find(|&i| origin + (f64::from(i) + 0.5) * h >= 0.)
}

fn main() -> Result<(), Box<dyn Error>> {
    let a: Vec<_> = std::env::args().collect();
    let p = project::load(Path::new(a.get(1).ok_or(
        "Expected PROJECT CURRENT_A OUTPUT [CONTACT_RESISTIVITY] [body|x-ends] [RELATIVE_TOLERANCE] [CU_SS_CONTACT_RHO_OHM_M2]",
    )?))?;
    let current: f32 = a.get(2).ok_or("Missing current")?.parse()?;
    if !current.is_finite() || current <= 0. {
        return Err("Choose a positive current".into());
    }
    let terminal_boundary = a.get(5).map(String::as_str).unwrap_or("body");
    if !matches!(terminal_boundary, "body" | "x-ends") {
        return Err("Choose body or x-ends terminal boundary".into());
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
    let terminal_x = [
        ids.iter()
            .enumerate()
            .filter(|(_, id)| **id == 16)
            .map(|(i, _)| i % grid.dims[0] as usize)
            .min()
            .ok_or("No positive copper pad")?,
        ids.iter()
            .enumerate()
            .filter(|(_, id)| **id == 17)
            .map(|(i, _)| i % grid.dims[0] as usize)
            .max()
            .ok_or("No negative copper pad")?,
    ];
    let cells: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            [
                table[id as usize],
                match id {
                    16 if terminal_boundary == "body"
                        || i % grid.dims[0] as usize == terminal_x[0] =>
                    {
                        0.5
                    }
                    17 if terminal_boundary == "body"
                        || i % grid.dims[0] as usize == terminal_x[1] =>
                    {
                        -0.5
                    }
                    0 | 4 => 3.,
                    _ => 2.,
                },
            ]
        })
        .collect();
    let contact: f32 = a.get(4).map(|s| s.parse()).transpose()?.unwrap_or(0.);
    // Static J and Joule heat do not require a magnetic solve when B=0.
    let (static_field, released_static_workspace_bytes) = if p.electromagnetics.has_sources() {
        let em = EmState::new(
            gpu.context.clone(),
            &volume,
            &p.model,
            p.electromagnetics.clone(),
            Mode::Applied,
        )?;
        let field = em.result_buffer().clone();
        let released = em.bytes - field.size();
        (field, released)
    } else {
        (
            gpu.context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Zero applied field for cold electrical heat screen"),
                size: ids.len() as u64 * std::mem::size_of::<mhd_workbench::em::CellField>() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            0,
        )
    };
    let cu_ss_contact: Option<f32> = a.get(7).map(|s| s.parse()).transpose()?;
    let groups: Vec<u32> = ids
        .iter()
        .map(|id| if matches!(*id, 16 | 17 | 20) { 20 } else { *id })
        .collect();
    let mut solver = conduction::Conduction::new_with_groups(
        gpu.context.clone(),
        grid.dims,
        grid.cell_m,
        &cells,
        &static_field,
        cu_ss_contact.map(|_| groups.as_slice()),
    )?;
    if let Some(rho) = cu_ss_contact {
        solver.set_pair_resistivity(14, 20, rho)?;
    }
    drop(static_field);
    let relative_tolerance: f32 = a.get(6).map(|s| s.parse()).transpose()?.unwrap_or(1e-5);
    solver.set_relative_tolerance(relative_tolerance)?;
    solver.set_contact_resistivity(contact)?;
    gpu.context.profiler.set_enabled(true);
    gpu.context.profiler.begin_step(0, 0.);
    let timing = gpu
        .context
        .profiler
        .stage(mhd_workbench::profile::Stage::EmSolve);
    solver.solve()?;
    let unit = solver.fields(1.)?;
    if unit[0] <= 0. {
        return Err("No positive terminal current".into());
    }
    let voltage = current / unit[0];
    let stats = solver.fields(voltage)?;
    drop(timing);
    gpu.context.profiler.end_step(true);
    let mut timing_csv = String::from(
        "step,time_s,accepted,stage,cpu_wall_ms,gpu_pass_ms,recorded_passes,total_passes\n",
    );
    for record in gpu.context.profiler.take_completed(true) {
        timing_csv.push_str(&record.csv_rows());
    }
    std::fs::write(output.join("conduction-performance.csv"), timing_csv)?;
    let fields = solver.read_fields()?;
    let h = f64::from(grid.cell_m);
    let cell = h.powi(3);
    let mut force = [0.; 3];
    let mut liquid_power = 0.;
    let mut liquid_current_integral = 0.;
    let mut wall_current_integral = 0.;
    let mut mean_by = 0.;
    let zero_plane = first_nonnegative_cell(f64::from(grid.origin_m[0]), h, grid.dims[0]);
    let mut midplane_liquid = 0.;
    let mut midplane_wall = 0.;
    let mut count = 0usize;
    for (i, (&id, f)) in ids.iter().zip(&fields).enumerate() {
        let x = f64::from(grid.origin_m[0]) + (i as u32 % grid.dims[0]) as f64 * h + h / 2.;
        if Some(i as u32 % grid.dims[0]) == zero_plane {
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
    let report = serde_json::json!({"project":a[1],"terminal_boundary":terminal_boundary,"requested_relative_tolerance":relative_tolerance,"terminal_x_cell_indices":terminal_x,"released_static_workspace_bytes":released_static_workspace_bytes,"contact_resistivity_ohm_m2":contact,"cu_ss_contact_resistivity_ohm_m2":cu_ss_contact,"solver_version":conduction::SOLVER_VERSION,"status":"cold DC conduction and applied permanent-magnet field only; no ejection simulation","iterations":solver.iterations,"true_relative_residual":solver.relative_residual,"boundary_current_defect":solver.boundary_current_defect,"potential_representation":"split f32 high/low","current_A":stats[0],"return_current_A":stats[1],"terminal_voltage_V":voltage,"joule_power_W":stats[2],"source_power_W":voltage*0.5*(stats[0]-stats[1]),"power_relative_defect":stats[2]/(voltage*0.5*(stats[0]-stats[1]))-1.,"power_definition":"sum terminal phi*I on both ports","liquid_force_N":force,"liquid_joule_power_W":liquid_power,"liquid_mean_By_T":mean_by/count as f64,"midplane_x_cell_index":zero_plane,"midplane_x_m":zero_plane.map(|i|f64::from(grid.origin_m[0])+(f64::from(i)+0.5)*h),"midplane_adjacent_cell_liquid_current_A":midplane_liquid,"midplane_adjacent_cell_wall_current_A":midplane_wall,"mean_liquid_current_over_x_plus_minus_2mm_A":liquid_current_integral/0.004,"mean_wall_current_over_x_plus_minus_2mm_A":wall_current_integral/0.004,"limitations":["Midplane estimates integrate cell-centred J on the first nonnegative-x plane. The +/-2 mm mean includes the full interval even where a phase is absent; it is not a unique through-current.","Ideal electrical contact; material and oxide/contact resistance require bracketing.","body mode makes whole terminal bodies equipotential. x-ends supplies the outermost modeled x-plane of each copper pad and resolves copper resistance only inside the electrical domain; out-of-domain leads remain separate.","Static liquid; no moving interface, motional EMF, self-field, driver, thermal evolution or droplet claim.","Applied magnetization and unit permeability approximate a specified magnet grade; check local field and temperature."]});
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

#[cfg(test)]
mod plane_tests {
    use super::first_nonnegative_cell;
    #[test]
    fn physical_zero_plane_is_independent_of_domain_centre() {
        assert_eq!(first_nonnegative_cell(-11.2, 0.1, 214), Some(112));
        assert_eq!(first_nonnegative_cell(-10.2, 0.1, 204), Some(102));
        assert_eq!(first_nonnegative_cell(-10., 0.1, 20), None);
    }
}
