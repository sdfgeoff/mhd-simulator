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
fn main() -> Result<(), Box<dyn Error>> {
    let a: Vec<_> = std::env::args().collect();
    let mut p = project::load(Path::new(a.get(1).ok_or(
        "Expected PROJECT PEAK_A POSITIVE_US STEPS OUTPUT [CONTACT] [RETRACT_FRACTION] [DT_US] [REPEAT_HZ] [FIELD_EVERY] [TAIL_DT_US]",
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
    {
        return Err("Invalid pulse".into());
    }
    if frequency_hz > 0. && duration + 25e-6 + duration / 3. >= 1. / frequency_hz {
        return Err("Pulses overlap the requested period".into());
    }
    if frequency_hz > 0. && tail_dt_s != dt_s {
        return Err("A separate tail timestep currently requires a single pulse".into());
    }
    std::fs::create_dir_all(out.parent().ok_or("Output parent required")?)?;
    std::fs::create_dir(out)?;
    let gpu = GpuDomain::new()?;
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
    let bias = EmState::new(
        gpu.context.clone(),
        &volume,
        &p.model,
        p.electromagnetics.clone(),
        Mode::Applied,
    )?;
    // Keep only B before allocating the conduction workspace, reducing startup peak.
    let static_field = bias.result_buffer().clone();
    let released_static_workspace_bytes = bias.bytes - static_field.size();
    drop(bias);
    let mut current = conduction::Conduction::new(
        gpu.context.clone(),
        grid.dims,
        grid.cell_m,
        &cells,
        &static_field,
    )?;
    drop(static_field);
    current.set_contact_resistivity(contact)?;
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
        "step,time_s,command_A,current_A,voltage_V,joule_W,mechanical_W,field_power_defect_W,max_u_m_s,liquid_volume_m3,charge_iterations,charge_true_residual,pressure_iterations,timestep_s\n",
    );
    let mut joule = 0.;
    let mut electrical = 0.;
    let mut lagged_mechanical = 0.;
    let mut max_voltage = 0_f32;
    let mut error: Option<String> = None;
    let mut costs = String::from("step,charge_wall_ms,fluid_wall_ms\n");
    for step in 1..=steps {
        let next_dt = if run.time_s >= duration * 4. / 3. + 25e-6 {
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
        let pulse_time = if frequency_hz > 0. {
            t.rem_euclid(1. / frequency_hz)
        } else {
            t
        };
        let command = peak
            * (trapezoid(pulse_time, 0., duration, rise)
                - retract * trapezoid(pulse_time, duration + 25e-6, duration / 3., rise));
        let charge_start = std::time::Instant::now();
        let (voltage, s) = match current.solve_current(command) {
            Ok(v) => v,
            Err(e) => {
                error = Some(e);
                break;
            }
        };
        let charge_ms = charge_start.elapsed().as_secs_f64() * 1000.;
        let fluid_start = std::time::Instant::now();
        if let Err(e) = run.step(&p.model, &volume) {
            error = Some(e);
            break;
        }
        costs.push_str(&format!(
            "{step},{charge_ms},{}\n",
            fluid_start.elapsed().as_secs_f64() * 1000.
        ));
        joule += f64::from(s[2]) * p.run.dt_s;
        electrical += f64::from(voltage * s[0]) * p.run.dt_s;
        lagged_mechanical += f64::from(s[3]) * p.run.dt_s;
        max_voltage = max_voltage.max(voltage.abs());
        let fluid = run.fluid.as_ref().unwrap();
        let material_volume = run.surface.as_ref().unwrap().diagnostics.volume_m3;
        csv.push_str(&format!(
            "{step},{},{command},{},{voltage},{},{},{},{},{material_volume},{},{},{},{}\n",
            run.time_s,
            s[0],
            s[2],
            s[3],
            voltage * s[0] - s[2] - s[3],
            fluid.diagnostics.max_speed_m_s,
            current.iterations,
            current.relative_residual,
            fluid.diagnostics.iterations,
            p.run.dt_s
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
        if step % field_every == 0 || step == steps {
            run.export_fields(&volume, &p.model, &out.join(format!("frame-{step:08}")))?;
            std::fs::write(out.join("current-diagnostics.csv"), &csv)?;
            std::fs::write(out.join("iteration-costs.csv"), &costs)?;
        }
    }
    std::fs::write(out.join("current-diagnostics.csv"), &csv)?;
    std::fs::write(out.join("iteration-costs.csv"), &costs)?;
    let mut performance = String::from(
        "step,time_s,accepted,stage,cpu_wall_ms,gpu_pass_ms,recorded_passes,total_passes\n",
    );
    for report in gpu.context.profiler.take_completed(true) {
        performance.push_str(&report.csv_rows());
    }
    std::fs::write(out.join("performance.csv"), performance)?;
    let report = serde_json::json!({"project":a[1],"released_static_workspace_bytes":released_static_workspace_bytes,"status":if error.is_some(){"failed"}else{"complete"},"error":error,"accepted_steps":run.steps,"time_s":run.time_s,"peak_A":peak,"timestep_s":dt_s,"tail_timestep_s":tail_dt_s,"tail_transition_not_before_s":duration*4./3.+25e-6,"frequency_hz":frequency_hz,"field_every":field_every,"grid":{"dims":grid.dims,"cell_m":grid.cell_m},"positive_duration_s":duration,"rise_s":rise,"retract_fraction":retract,"contact_resistivity_ohm_m2":contact,"max_terminal_voltage_V":max_voltage,"joule_generated_J":joule,"lagged_electrical_work_J":electrical,"lagged_mechanical_work_J":lagged_mechanical,"fluid_mechanical_work_J":run.energy.external_work_j,"method":"GPU finite-volume div J=0, J=sigma(-grad phi + u cross B0), current-controlled terminals, conductivity updated from accepted alpha; static CAD permanent-magnet B0. Forces drive the normal RunState fluid/interface engine.","limitations":["Inductionless imposed-field approximation: applied-current self-field and magnetic diffusion/reaction omitted; assess Rm and self-field/B0 bounds.","Motion is lagged by one fluid step; compare timestep refinement and lagged electrical versus actual fluid work.","Thermal evolution, oxidation, contact chemistry, wire feeding and driver switching not modeled.","New research client; not a GUI electrode feature or frozen-baseline claim."]});
    std::fs::write(
        out.join("run-status.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if let Some(e) = error {
        return Err(e.into());
    }
    Ok(())
}
