//! Research imposed-field, inductionless MHD using the normal fluid/interface RunState.
//! CLI: PROJECT PEAK_A POSITIVE_US STEPS OUTPUT [CONTACT_OHM_M2] [RETRACT_FRACTION]
#[path = "conduction/gpu.rs"]
pub mod conduction;
use mhd_workbench::{
    domain::GpuDomain,
    em::{EmState, Mode},
    fluid::ForceField,
    gpu::read_bytes,
    project,
    run::RunState,
};
use std::{error::Error, path::Path};

fn trapezoid(t: f64, start: f64, duration: f64, rise: f64) -> f32 {
    ((t - start) / rise)
        .min((start + duration - t) / rise)
        .clamp(0., 1.) as f32
}
fn pulse_current(t: f64, duration: f64, rise: f64, peak: f32, retract: f32, delay: f64) -> f32 {
    peak * (trapezoid(t, 0., duration, rise)
        - retract * trapezoid(t, duration + delay, duration / 3., rise))
}
fn scheduled_current(
    t: f64,
    start: f64,
    frequency: f64,
    duration: f64,
    rise: f64,
    peak: f32,
    retract: f32,
    delay: f64,
) -> f32 {
    if t < start {
        return 0.;
    }
    let elapsed = t - start;
    let phase = if frequency > 0. {
        elapsed.rem_euclid(1. / frequency)
    } else {
        elapsed
    };
    pulse_current(phase, duration, rise, peak, retract, delay)
}

pub(crate) fn main() -> Result<(), Box<dyn Error>> {
    let a: Vec<_> = std::env::args().collect();
    let mut p = project::load(Path::new(a.get(1).ok_or(
        "Expected PROJECT PEAK_A POSITIVE_US STEPS OUTPUT [CONTACT] [RETRACT_FRACTION] [DT_US] [REPEAT_HZ] [FIELD_EVERY] [TAIL_DT_US] [RETRACT_DELAY_US] [body|x-ends] [PULSE_START_US] [MOTIONAL_BOUNDARY_ABS_TOL_A] [CU_SS_RHO] [CHARGE_RELATIVE_TOL] [CHARGE_MAX_ITERATIONS] [SOURCE_INTERACTION_WORK_BUDGET]",
    )?))?;
    let peak: f32 = a.get(2).ok_or("Missing current")?.parse()?;
    let duration: f64 = a.get(3).ok_or("Missing duration")?.parse::<f64>()? * 1e-6;
    let steps: u32 = a.get(4).ok_or("Missing steps")?.parse()?;
    let out = Path::new(a.get(5).ok_or("Missing output")?);
    let contact: f32 = a.get(6).map(|s| s.parse()).transpose()?.unwrap_or(0.);
    let retract: f32 = a.get(7).map(|s| s.parse()).transpose()?.unwrap_or(0.);
    let dt_s: f64 = a
        .get(8)
        .map(|s| s.parse::<f64>())
        .transpose()?
        .unwrap_or(3.)
        * 1e-6;
    let frequency_hz: f64 = a
        .get(9)
        .map(|s| s.parse::<f64>())
        .transpose()?
        .unwrap_or(0.);
    let field_every: u32 = a.get(10).map(|s| s.parse()).transpose()?.unwrap_or(100);
    let tail_dt_s = a
        .get(11)
        .map(|s| s.parse::<f64>().map(|v| v * 1e-6))
        .transpose()?
        .unwrap_or(dt_s);
    let retract_delay_s = a
        .get(12)
        .map(|s| s.parse::<f64>().map(|v| v * 1e-6))
        .transpose()?
        .unwrap_or(25e-6);
    let terminal_boundary = a.get(13).map(String::as_str).unwrap_or("body");
    let motion_absolute_tolerance_a: f32 = a.get(15).map(|s| s.parse()).transpose()?.unwrap_or(0.);
    let cu_ss_contact: Option<f32> = a.get(16).map(|s| s.parse()).transpose()?;
    let charge_tolerance: f32 = a.get(17).map(|s| s.parse()).transpose()?.unwrap_or(1e-5);
    let charge_max_iterations: u32 = a.get(18).map(|s| s.parse()).transpose()?.unwrap_or(4000);
    let source_work_budget: u64 = a.get(19).map(|s| s.parse()).transpose()?.unwrap_or(50_000_000_000);
    let pulse_start_s = a
        .get(14)
        .map(|s| s.parse::<f64>().map(|v| v * 1e-6))
        .transpose()?
        .unwrap_or(0.);
    if !matches!(terminal_boundary, "body" | "x-ends") {
        return Err("Choose body or x-ends terminal boundary".into());
    }
    if !peak.is_finite()
        || peak < 0.
        || !duration.is_finite()
        || duration <= 0.
        || steps == 0
        || !retract.is_finite()
        || retract < 0.
        || !dt_s.is_finite()
        || dt_s <= 0.
        || !frequency_hz.is_finite()
        || frequency_hz < 0.
        || field_every == 0
        || !tail_dt_s.is_finite()
        || tail_dt_s <= 0.
        || !retract_delay_s.is_finite()
        || retract_delay_s < 0.
        || !pulse_start_s.is_finite()
        || pulse_start_s < 0.
    {
        return Err("Invalid pulse".into());
    }
    if frequency_hz > 0. && duration + retract_delay_s + duration / 3. >= 1. / frequency_hz {
        return Err("Pulses overlap the requested period".into());
    }
    if frequency_hz > 0. && tail_dt_s != dt_s {
        return Err("A separate tail timestep currently requires a single pulse".into());
    }
    std::fs::create_dir_all(out.parent().ok_or("Output parent required")?)?;
    std::fs::create_dir(out)?;
    let gpu = GpuDomain::new()?;
    if let Ok(expected) = std::env::var("MHD_EXPECT_ADAPTER")
        && gpu.name != expected
    {
        return Err("Native adapter differs from the scheduler preflight".into());
    }
    gpu.context.profiler.set_enabled(true);
    let volume = gpu.build(&p.model, &p.settings, &p.materials)?;
    if volume.overlaps > 0 {
        return Err(format!("{} overlapping cells", volume.overlaps).into());
    }
    let grid = volume.inputs.snapshot().grid.clone();
    let bytes = read_bytes(
        &gpu.context,
        volume.material_ids(),
        0,
        volume.material_ids().size(),
    )?;
    let ids = bytemuck::cast_slice::<u8, u32>(&bytes);
    let terminal_x = [
        ids.iter()
            .enumerate()
            .filter(|(_, id)| **id == 16)
            .map(|(i, _)| i % grid.dims[0] as usize)
            .min()
            .ok_or("No positive electrode")?,
        ids.iter()
            .enumerate()
            .filter(|(_, id)| **id == 17)
            .map(|(i, _)| i % grid.dims[0] as usize)
            .max()
            .ok_or("No negative electrode")?,
    ];
    let mut table = [0.; 256];
    for m in &p.materials {
        if m.id < 256 {
            table[m.id as usize] = m.conductivity_s_m.unwrap_or(0.) / 1e6;
        }
    }
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
    let bias = EmState::new_with_source_work_budget(
        gpu.context.clone(),
        &volume,
        &p.model,
        p.electromagnetics.clone(),
        Mode::Applied,
        source_work_budget,
    )?;
    // Keep only B before allocating the conduction workspace, reducing startup peak.
    let static_field = bias.result_buffer().clone();
    let released_static_workspace_bytes = bias.bytes - static_field.size();
    drop(bias);
    let groups: Vec<u32> = ids
        .iter()
        .map(|id| if matches!(*id, 16 | 17 | 20) { 20 } else { *id })
        .collect();
    let mut current = conduction::Conduction::new_with_groups(
        gpu.context.clone(),
        grid.dims,
        grid.cell_m,
        &cells,
        &static_field,
        cu_ss_contact.map(|_| groups.as_slice()),
    )?;
    if let Some(rho) = cu_ss_contact {
        current.set_pair_resistivity(14, 20, rho)?;
    }
    current.set_relative_tolerance(charge_tolerance)?;
    current.set_max_iterations(charge_max_iterations)?;
    drop(static_field);
    current.set_contact_resistivity(contact)?;
    current.set_motional_boundary_absolute_tolerance_a(motion_absolute_tolerance_a)?;
    p.run.fluid_enabled = true;
    p.run.thermal_enabled = false;
    p.run.prescribed_jet = None;
    p.run.dt_s = dt_s;
    p.fluid.surface.enabled = true;
    p.fluid.material_id = 4;
    let mut run = RunState::new_with_impressed_force(
        gpu.context.clone(),
        &volume,
        &p.model,
        p.run.clone(),
        p.fluid.clone(),
        ForceField {
            buffer: &current.result,
            grid: &grid,
        },
    )?;
    current.attach_flow(
        run.fluid
            .as_ref()
            .ok_or("No liquid fluid state")?
            .result_buffer(),
    )?;
    run.export_fields(&volume, &p.model, &out.join("frame-00000000"))?;
    let rise = 20e-6_f64.min(duration / 4.);
    let mut csv = String::from(
        "step,time_s,command_A,current_A,voltage_V,joule_W,mechanical_W,field_power_defect_W,max_u_m_s,liquid_volume_m3,charge_iterations,charge_true_residual,pressure_iterations,timestep_s,charge_boundary_current_defect,charge_unit_boundary_current_defect,charge_motional_boundary_imbalance_A,return_current_A\n",
    );
    let mut fluid_energy_csv = String::from(
        "step,time_s,kinetic_J,surface_energy_estimate_J,fluid_EM_work_J,body_work_J,fluid_external_work_J,capillary_work_J,area_work_mismatch_J,discrete_fluid_loss_J,electrical_source_work_J,electrical_joule_J,lagged_electrical_mechanical_J\n",
    );
    let mut performance = String::from(
        "step,time_s,accepted,stage,cpu_wall_ms,gpu_pass_ms,recorded_passes,total_passes\n",
    );
    let mut joule = 0.;
    let mut electrical = 0.;
    let mut lagged_mechanical = 0.;
    let mut max_voltage = 0_f32;
    let mut max_boundary_defect = 0_f32;
    let mut max_unit_boundary_defect = 0_f32;
    let mut max_motion_imbalance_a = 0_f32;
    let mut max_port_imbalance_a = 0_f32;
    let mut error: Option<String> = None;
    let mut costs = String::from("step,charge_wall_ms,fluid_wall_ms\n");
    for step in 1..=steps {
        let next_dt = if run.time_s >= pulse_start_s + duration * 4. / 3. + retract_delay_s {
            tail_dt_s
        } else {
            dt_s
        };
        if next_dt != p.run.dt_s {
            if let Err(e) = run.set_timestep(&volume, next_dt) {
                error = Some(e);
                break;
            }
            p.run.dt_s = next_dt;
        }
        let t = run.time_s + 0.5 * p.run.dt_s;
        let command = scheduled_current(
            t,
            pulse_start_s,
            frequency_hz,
            duration,
            rise,
            peak,
            retract,
            retract_delay_s,
        );
        if step == 1 { eprintln!("First timestep: starting charge solve"); }
        let charge_start = std::time::Instant::now();
        let (voltage, s) = match current.solve_current(command) {
            Ok(v) => v,
            Err(e) => {
                error = Some(e);
                break;
            }
        };
        let charge_ms = charge_start.elapsed().as_secs_f64() * 1000.;
        if step == 1 { eprintln!("First timestep: charge accepted in {charge_ms:.1} ms; starting fluid step"); }
        let fluid_start = std::time::Instant::now();
        if let Err(e) = run.step(&p.model, &volume) {
            error = Some(e);
            break;
        }
        if step == 1 { eprintln!("First timestep: fluid accepted in {:.1} ms",fluid_start.elapsed().as_secs_f64()*1000.); }
        costs.push_str(&format!(
            "{step},{charge_ms},{}\n",
            fluid_start.elapsed().as_secs_f64() * 1000.
        ));
        // Drain the profiler's bounded history while advancing, as the main
        // CLI does. This only collects diagnostics already completed on GPU.
        for report in gpu.context.profiler.take_completed(false) {
            performance.push_str(&report.csv_rows());
        }
        joule += f64::from(s[2]) * p.run.dt_s;
        let terminal_power = voltage * 0.5 * (s[0] - s[1]);
        electrical += f64::from(terminal_power) * p.run.dt_s;
        lagged_mechanical += f64::from(s[3]) * p.run.dt_s;
        let energy = &run.energy;
        fluid_energy_csv.push_str(&format!(
            "{step},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            run.time_s,
            energy.kinetic_j,
            energy.surface_j,
            energy.fluid_em_work_j,
            energy.fluid_acceleration_work_j,
            energy.external_work_j,
            energy.surface_work_j,
            energy.surface_balance_error_j,
            energy.fluid_loss_j,
            electrical,
            joule,
            lagged_mechanical
        ));
        max_voltage = max_voltage.max(voltage.abs());
        max_boundary_defect = max_boundary_defect.max(current.boundary_current_defect);
        max_unit_boundary_defect =
            max_unit_boundary_defect.max(current.unit_boundary_current_defect);
        max_motion_imbalance_a =
            max_motion_imbalance_a.max(current.motional_boundary_current_imbalance_a);
        max_port_imbalance_a = max_port_imbalance_a.max((s[0] + s[1]).abs());
        let fluid = run.fluid.as_ref().unwrap();
        let material_volume = run.surface.as_ref().unwrap().diagnostics.volume_m3;
        csv.push_str(&format!(
            "{step},{},{command},{},{voltage},{},{},{},{},{material_volume},{},{},{},{},{},{},{},{}\n",
            run.time_s,
            s[0],
            s[2],
            s[3],
            terminal_power - s[2] - s[3],
            fluid.diagnostics.max_speed_m_s,
            current.iterations,
            current.relative_residual,
            fluid.diagnostics.iterations,
            p.run.dt_s,
            current.boundary_current_defect,
            current.unit_boundary_current_defect,
            current.motional_boundary_current_imbalance_a,
            s[1]
        ));
        if step % 10 == 0 {
            println!(
                "step {step}/{steps} t={:.1} us I={:.3} A V={:.5} V Umax={:.4} m/s pressure={} charge={}",
                run.time_s * 1e6,
                s[0],
                voltage,
                fluid.diagnostics.max_speed_m_s,
                fluid.diagnostics.iterations,
                current.iterations
            );
        }
        let onset_checkpoint =
            pulse_start_s > 0. && (run.time_s - pulse_start_s).abs() <= 0.5 * p.run.dt_s;
        if step % field_every == 0 || step == steps || onset_checkpoint {
            if let Err(e) = run.export_fields(&volume, &p.model, &out.join(format!("frame-{step:08}"))) {
                error = Some(format!("Field export failed: {e}"));
                break;
            }
            std::fs::write(out.join("current-diagnostics.csv"), &csv)?;
            std::fs::write(out.join("iteration-costs.csv"), &costs)?;
        }
    }
    std::fs::write(out.join("current-diagnostics.csv"), &csv)?;
    std::fs::write(out.join("iteration-costs.csv"), &costs)?;
    std::fs::write(out.join("fluid-energy-diagnostics.csv"), fluid_energy_csv)?;
    for report in gpu.context.profiler.take_completed(true) {
        performance.push_str(&report.csv_rows());
    }
    std::fs::write(out.join("performance.csv"), performance)?;
    let mut report = serde_json::json!({"project":a[1],"terminal_boundary":terminal_boundary,"terminal_x_cell_indices":terminal_x,"uniform_acceleration_m_s2":p.fluid.acceleration_m_s2,"periodic_fluid_axes":p.fluid.periodic,"released_static_workspace_bytes":released_static_workspace_bytes,"status":if error.is_some(){"failed"}else{"complete"},"error":error,"accepted_steps":run.steps,"requested_steps":steps,"charge_solver":conduction::SOLVER_VERSION,"charge_requested_relative_tolerance":current.relative_tolerance(),"time_s":run.time_s,"peak_A":peak,"timestep_s":dt_s,"tail_timestep_s":tail_dt_s,"tail_transition_not_before_s":pulse_start_s+duration*4./3.+retract_delay_s,"pulse_start_s":pulse_start_s,"frequency_hz":frequency_hz,"field_every":field_every,"grid":{"dims":grid.dims,"cell_m":grid.cell_m},"positive_duration_s":duration,"rise_s":rise,"retract_fraction":retract,"retract_delay_s":retract_delay_s,"contact_resistivity_ohm_m2":contact,"max_terminal_voltage_V":max_voltage,"max_charge_boundary_current_defect":max_boundary_defect,"charge_potential_representation":"split f32 high/low","electrical_work_definition":"sum terminal phi*I on both ports","joule_generated_J":joule,"lagged_electrical_work_J":electrical,"lagged_mechanical_work_J":lagged_mechanical,"fluid_em_work_J":run.energy.fluid_em_work_j,"body_acceleration_work_J":run.energy.fluid_acceleration_work_j,"fluid_external_work_J":run.energy.external_work_j,"fluid_work_definition":"EM/impressed-force work only; excludes body acceleration","method":"GPU finite-volume div J=0, J=sigma(-grad phi + u cross B0), current-controlled terminals, conductivity updated from accepted alpha; static CAD permanent-magnet B0. Forces drive the normal RunState fluid/interface engine.","limitations":["Inductionless imposed-field approximation: applied-current self-field and magnetic diffusion/reaction omitted; assess Rm and self-field/B0 bounds.","Motion is lagged by one fluid step; compare timestep refinement and lagged electrical versus actual fluid work.","Thermal evolution, oxidation, contact chemistry, wire feeding and driver switching not modeled.","New research client; not a GUI electrode feature or frozen-baseline claim."]});
    report["cu_ss_contact_resistivity_ohm_m2"]=serde_json::json!(cu_ss_contact);
    report["charge_max_iterations"] = charge_max_iterations.into();
    report["source_interaction_work_budget"] = source_work_budget.into();
    report["charge_boundary_criteria"] = serde_json::json!({
        "unit_relative_tolerance": 1e-5,
        "motional_absolute_tolerance_A": current.motional_boundary_absolute_tolerance_a(),
        "max_unit_relative_imbalance": max_unit_boundary_defect,
        "max_motional_imbalance_A": max_motion_imbalance_a,
        "max_combined_port_imbalance_A": max_port_imbalance_a,
    });
    report["fluid_energy_diagnostics"] = serde_json::json!({
        "surface_energy_estimate_J": run.energy.surface_j,
        "capillary_work_J": run.energy.surface_work_j,
        "area_work_mismatch_J": run.energy.surface_balance_error_j,
        "kinetic_J": run.energy.kinetic_j,
        "discrete_fluid_loss_J": run.energy.fluid_loss_j,
        "scope": "Existing RunState ledger only; no additional GPU solve. Area-work mismatch compares the interface-area estimator with capillary-force work; it is not a certified discretization-error bound. Fluid loss includes physical and numerical contributions. Electrical Joule heat is accounted separately by the conduction client; thermal evolution is disabled."
    });
    std::fs::write(
        out.join("run-status.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if let Some(e) = error {
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conditioning_interval_has_no_early_single_or_repeated_pulse() {
        for frequency in [0., 200.] {
            for t in [0., 10e-6, 1e-3, 4.999e-3] {
                assert_eq!(
                    scheduled_current(t, 5e-3, frequency, 1e-3, 20e-6, 40., 0.25, 25e-6),
                    0.
                );
            }
            assert_eq!(
                scheduled_current(5.1e-3, 5e-3, frequency, 1e-3, 20e-6, 40., 0.25, 25e-6),
                40.
            );
        }
        for i in 0..1200 {
            let t = (i as f64 + 0.5) * 3e-6;
            assert_eq!(
                scheduled_current(t, 0., 0., 250e-6, 20e-6, 110., 0.25, 25e-6),
                pulse_current(t, 250e-6, 20e-6, 110., 0.25, 25e-6)
            );
        }
    }

    #[test]
    fn default_delay_preserves_original_commands() {
        let duration = 250e-6;
        let rise = 20e-6;
        for i in 0..1200 {
            let t = (i as f64 + 0.5) * 3e-6;
            let original = 150.
                * (trapezoid(t, 0., duration, rise)
                    - 0.25 * trapezoid(t, duration + 25e-6, duration / 3., rise));
            assert_eq!(
                pulse_current(t, duration, rise, 150., 0.25, 25e-6),
                original
            );
        }
    }

    #[test]
    fn delayed_retract_changes_only_negative_lobe_time() {
        let command = |t, delay| pulse_current(t, 250e-6, 20e-6, 150., 0.25, delay);
        for i in 0..250 {
            let t = (i as f64 + 0.5) * 1e-6;
            assert_eq!(command(t, 25e-6), command(t, 500e-6));
        }
        assert_eq!(command(300e-6, 500e-6), 0.);
        assert_eq!(command(780e-6, 500e-6), -37.5);
        assert_eq!(command(850e-6, 500e-6), 0.);
        let integral = |delay| {
            (0..1000)
                .map(|i| {
                    let current = command((i as f64 + 0.5) * 1e-6, delay) as f64;
                    current * current * 1e-6
                })
                .sum::<f64>()
        };
        assert!((integral(25e-6) - integral(500e-6)).abs() < 1e-6);
    }
}
