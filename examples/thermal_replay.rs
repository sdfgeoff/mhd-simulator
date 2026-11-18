//! Native GPU heat transport driven by a fixed accepted cold Joule field.
//! This is not temperature-dependent electrical feedback or a nozzle safety proof.
use bytemuck::{Pod, Zeroable};
use mhd_workbench::{
    domain::GpuDomain,
    em::CellField,
    gpu::read_bytes,
    project,
    thermal::{Source, ThermalCell, ThermalConfig, ThermalState},
};
use std::{error::Error, path::Path};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Scale {
    cells: u32,
    factor: f32,
    pad: [u32; 2],
}

fn lobe_dose(lo: f64, hi: f64, start: f64, duration: f64, rise: f64) -> f64 {
    let mut total = 0.;
    for (a, b, va, vb) in [
        (start, start + rise, 0., 1.),
        (start + rise, start + duration - rise, 1., 1.),
        (start + duration - rise, start + duration, 1., 0.),
    ] {
        let left = lo.max(a);
        let right = hi.min(b);
        if right > left {
            let u = va + (vb - va) * (left - a) / (b - a);
            let v = va + (vb - va) * (right - a) / (b - a);
            total += (right - left) * (u * u + u * v + v * v) / 3.;
        }
    }
    total
}
fn dose(lo: f64, hi: f64, peak: f64, duration: f64) -> f64 {
    let rise = 20e-6_f64.min(duration / 6.);
    peak * peak
        * (lobe_dose(lo, hi, 0., duration, rise)
            + 0.25_f64.powi(2) * lobe_dose(lo, hi, duration + 25e-6, duration / 3., rise))
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mut p = project::load(Path::new(args.get(1).ok_or(
        "PROJECT COLD_DIR PEAK_A POSITIVE_US STEPS DT_US OUTPUT [ADIABATIC_CONTROL] [TIP_ROI]",
    )?))?;
    let cold = Path::new(args.get(2).ok_or("Missing cold directory")?);
    let peak: f64 = args.get(3).ok_or("Missing peak")?.parse()?;
    let duration = args.get(4).ok_or("Missing duration")?.parse::<f64>()? * 1e-6;
    let steps: u32 = args.get(5).ok_or("Missing steps")?.parse()?;
    let dt = args.get(6).ok_or("Missing timestep")?.parse::<f64>()? * 1e-6;
    let out = Path::new(args.get(7).ok_or("Missing output")?);
    let adiabatic = args
        .get(8)
        .map(|v| v.parse::<bool>())
        .transpose()?
        .unwrap_or(false);
    let tip_roi = args
        .get(9)
        .map(|v| v.parse::<bool>())
        .transpose()?
        .unwrap_or(false);
    if !peak.is_finite()
        || peak < 0.
        || !duration.is_finite()
        || duration <= 0.
        || !dt.is_finite()
        || dt <= 0.
        || steps == 0
    {
        return Err("Invalid replay controls".into());
    }
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cold.join("report.json"))?)?;
    if std::fs::canonicalize(&args[1])?
        != std::fs::canonicalize(report["project"].as_str().ok_or("Missing cold project")?)?
    {
        return Err("Cold source belongs to a different project".into());
    }
    let base_current = report["current_A"].as_f64().ok_or("Missing cold current")?;
    if !base_current.is_finite() || base_current == 0. {
        return Err("Invalid cold current".into());
    }
    let grid_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cold.join("grid.json"))?)?;
    let old_ids = std::fs::read(cold.join("material-ids.bin"))?;
    let old_ids: Vec<u32> = bytemuck::cast_slice::<u8, u32>(&old_ids).to_vec();
    let fields = std::fs::read(cold.join("em.bin"))?;
    if fields.len() != old_ids.len() * 64 {
        return Err("Cold field size mismatch".into());
    }
    let base_q: Vec<f32> = bytemuck::cast_slice::<u8, CellField>(&fields)
        .iter()
        .map(|f| f.force_heat[3])
        .collect();
    for body in &mut p.model.bodies {
        if !body.simulation.included {
            continue;
        }
        let material = p
            .materials
            .iter()
            .find(|m| m.id == body.simulation.material_id)
            .ok_or("Missing material")?;
        if material.id == 18 || material.conductivity_s_m.unwrap_or(0.) <= 0. {
            body.simulation.included = false;
            continue;
        }
        if body.simulation.temperature_c != Some(250.)
            || body.simulation.fixed_temperature_c.is_some()
            || body.simulation.heat_source_w_m3 != 0.
        {
            return Err(
                "Replay requires unpinned250C conductor bodies and no other heat source".into(),
            );
        }
    }
    if adiabatic {
        for m in &mut p.materials {
            if let Some(k) = &mut m.thermal.conductivity_w_m_k {
                *k *= 1e-12;
            }
        }
    }
    if tip_roi {
        p.settings.min_mm = [-1.5, -1.0, 0.0];
        p.settings.max_mm = [1.5, 1.0, 1.2];
    }
    let gpu = GpuDomain::new()?;
    let volume = gpu.build(&p.model, &p.settings, &p.materials)?;
    let grid = volume.inputs.snapshot().grid.clone();
    let expected_dims: [u32; 3] = serde_json::from_value(grid_json["dims"].clone())?;
    let expected_origin: [f32; 3] = serde_json::from_value(grid_json["origin_m"].clone())?;
    if grid.cell_m != grid_json["cell_m"].as_f64().ok_or("Missing cell size")? as f32 {
        return Err("Cold/replay cell size differs".into());
    }
    let mut offset = [0u32; 3];
    for axis in 0..3 {
        let shift =
            (grid.origin_m[axis] as f64 - expected_origin[axis] as f64) / grid.cell_m as f64;
        let integer = shift.round();
        if (shift - integer).abs() > 1e-4
            || integer < 0.
            || integer as u32 + grid.dims[axis] > expected_dims[axis]
        {
            return Err("Thermal ROI is not an aligned native-cell subset".into());
        }
        offset[axis] = integer as u32;
    }
    if !tip_roi && (grid.dims != expected_dims || grid.origin_m != expected_origin) {
        return Err("Cold/replay grids differ".into());
    }
    let mut selected_ids = Vec::new();
    let mut selected_q = Vec::new();
    for z in 0..grid.dims[2] {
        for y in 0..grid.dims[1] {
            for x in 0..grid.dims[0] {
                let index = ((x + offset[0])
                    + expected_dims[0] * ((y + offset[1]) + expected_dims[1] * (z + offset[2])))
                    as usize;
                selected_ids.push(old_ids[index]);
                selected_q.push(base_q[index]);
            }
        }
    }
    let old_ids = selected_ids;
    let base_q = selected_q;
    let field_bytes = base_q.len() as u64 * 64;
    let ids_bytes = read_bytes(
        &gpu.context,
        volume.material_ids(),
        0,
        volume.material_ids().size(),
    )?;
    let ids: Vec<u32> = bytemuck::cast_slice::<u8, u32>(&ids_bytes).to_vec();
    if ids.len() != old_ids.len() {
        return Err("ID size mismatch".into());
    }
    for (i, (&now, &old)) in ids.iter().zip(&old_ids).enumerate() {
        let q = base_q[i];
        if !q.is_finite()
            || q < 0.
            || (now != old
                && !(now == 0
                    && q == 0.
                    && p.materials
                        .iter()
                        .find(|m| m.id == old)
                        .is_some_and(|m| m.id == 18 || m.conductivity_s_m.unwrap_or(0.) <= 0.)))
        {
            return Err("Material/source alignment changed or invalid Joule field".into());
        }
    }
    std::fs::create_dir_all(out.parent().ok_or("Output parent required")?)?;
    std::fs::create_dir(out)?;
    let mut thermal = ThermalState::new(gpu.context.clone(), &volume, ThermalConfig::default())?;
    let source = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Accepted cold field"),
            contents: bytemuck::cast_slice(&base_q),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let heat = gpu.context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Replay heat"),
        size: field_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let peak_field = gpu.context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Peak temperature field"),
        size: field_bytes,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let params = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Replay scale"),
            contents: bytemuck::bytes_of(&Scale {
                cells: ids.len() as u32,
                factor: 0.,
                pad: [0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
    let shader = gpu
        .context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cold heat scaling"),
            source: wgpu::ShaderSource::Wgsl(include_str!("thermal_replay.wgsl").into()),
        });
    let pipe = gpu
        .context
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Cold heat scaling"),
            layout: None,
            module: &shader,
            entry_point: Some("scale_heat"),
            compilation_options: Default::default(),
            cache: None,
        });
    let bind = gpu
        .context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Replay sources"),
            layout: &pipe.get_bind_group_layout(0),
            entries: &[&params, &source, &heat]
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect::<Vec<_>>(),
        });
    let mut csv = String::from(
        "step,time_s,interval_I2dt_A2s,maximum_temperature_C,joule_J,energy_error_J,substeps\n",
    );
    let mut initial_copy = gpu
        .context
        .device
        .create_command_encoder(&Default::default());
    initial_copy.copy_buffer_to_buffer(
        thermal.result_buffer(),
        0,
        &peak_field,
        0,
        peak_field.size(),
    );
    gpu.context.queue.submit(Some(initial_copy.finish()));
    let mut generated = 0.;
    let mut energy_error = 0.;
    let mut max_temperature = 250_f32 + 273.15;
    let mut peak_time = 0.;
    for step in 1..=steps {
        let lo = (step - 1) as f64 * dt;
        let hi = step as f64 * dt;
        let i2dt = dose(lo, hi, peak, duration);
        gpu.context.queue.write_buffer(
            &params,
            0,
            bytemuck::bytes_of(&Scale {
                cells: ids.len() as u32,
                factor: (i2dt / dt / base_current.powi(2)) as f32,
                pad: [0; 2],
            }),
        );
        let mut enc = gpu
            .context
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&pipe);
            pass.set_bind_group(0, &bind, &[]);
            let groups = (ids.len() as u32).div_ceil(256);
            pass.dispatch_workgroups(groups.min(65535), groups.div_ceil(65535), 1);
        }
        gpu.context.queue.submit(Some(enc.finish()));
        if let Err(error) = thermal.step(
            &volume,
            dt,
            Some(Source {
                buffer: &heat,
                grid: &grid,
            }),
            None,
        ) {
            std::fs::write(
                out.join("run-status.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"status":"failed","accepted_steps":step-1,"error":error}),
                )?,
            )?;
            return Err(error.into());
        }
        let diag = &thermal.diagnostics;
        generated += diag.joule_j;
        energy_error += diag.energy_error_j;
        if diag.max_temperature_k >= max_temperature {
            max_temperature = diag.max_temperature_k;
            peak_time = thermal.time_s;
            let mut enc = gpu
                .context
                .device
                .create_command_encoder(&Default::default());
            enc.copy_buffer_to_buffer(
                thermal.result_buffer(),
                0,
                &peak_field,
                0,
                peak_field.size(),
            );
            gpu.context.queue.submit(Some(enc.finish()));
        }
        csv.push_str(&format!(
            "{step},{},{i2dt},{},{},{},{}\n",
            thermal.time_s,
            diag.max_temperature_k - 273.15,
            diag.joule_j,
            diag.energy_error_j,
            diag.substeps
        ));
        if step % 20 == 0 {
            println!(
                "{step}/{steps}: {:.1}us Tmax={:.4}C",
                thermal.time_s * 1e6,
                diag.max_temperature_k - 273.15
            );
        }
    }
    let final_bytes = read_bytes(
        &gpu.context,
        thermal.result_buffer(),
        0,
        thermal.result_buffer().size(),
    )?;
    let peak_bytes = read_bytes(&gpu.context, &peak_field, 0, peak_field.size())?;
    let peak_cells: &[ThermalCell] = bytemuck::cast_slice(&peak_bytes);
    let mut per_material = Vec::new();
    for material in &p.materials {
        let selected: Vec<_> = ids
            .iter()
            .enumerate()
            .filter(|(_, id)| **id == material.id && **id != 0)
            .map(|(i, _)| i)
            .collect();
        if selected.is_empty() {
            continue;
        }
        let (index, temperature) = selected
            .iter()
            .map(|&i| (i, peak_cells[i].state[0]))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .ok_or("No peak")?;
        per_material.push(serde_json::json!({"id":material.id,"name":material.name,"maximum_C_at_global_peak_frame":temperature-273.15,"index":index}));
    }
    let total_dose = dose(0., steps as f64 * dt, peak, duration);
    let expected =
        base_q.iter().map(|&q| q as f64).sum::<f64>() * (grid.cell_m as f64).powi(3) * total_dose
            / base_current.powi(2);
    std::fs::write(out.join("thermal-final.bin"), final_bytes)?;
    std::fs::write(out.join("thermal-peak.bin"), peak_bytes)?;
    std::fs::write(out.join("material-ids.bin"), ids_bytes)?;
    std::fs::write(out.join("diagnostics.csv"), csv)?;
    let report = serde_json::json!({"status":"complete","project":args[1],"cold":args[2],"peak_A":peak,"positive_us":duration*1e6,"dt_us":dt*1e6,"steps":steps,"time_s":thermal.time_s,"adiabatic_control":adiabatic,"tip_roi":tip_roi,"source_offset_cells":offset,"initial_temperature_C":250.,"peak_temperature_C":max_temperature-273.15,"peak_time_s":peak_time,"per_material_at_global_peak":per_material,"joule_J":generated,"expected_joule_J":expected,"source_energy_relative_error":(generated-expected)/expected.abs().max(1e-30),"summed_step_energy_error_J":energy_error,"I2dt_A2s":total_dose,"grid":{"origin_m":grid.origin_m,"cell_m":grid.cell_m,"dims":grid.dims},"method":"Native ThermalState diffusion/enthalpy with exact per-interval I²dt scaling of accepted cold Joule field. Isolated zero-Joule magnet bodies omitted from thermal domain; all retained material IDs/grid match.","limits":["Fixed current path/material properties; no temperature-dependent conductivity, moving interface, external copper/steel joint resistance or heater/clamp cooling. Steel/liquid interface resistance is only whatever is in the cold source.","Closed insulated retained conductors initially250C; magnet/air heat transfer omitted, no repetition equilibrium or qualified melting margin.","TIP_ROI when enabled uses insulated cuts and exact native-cell source mapping from the full-head electrical solution; validate thermal cut sensitivity before use, and do not extrapolate it to steady heating.","Sharp contact geometry and finite grid do not resolve real microscopic contact areas or establish peak convergence."]});
    std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    std::fs::write(
        out.join("run-status.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"complete","accepted_steps":steps,"time_s":thermal.time_s}),
        )?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_pulse_dose_and_partition() {
        let whole = dose(0., 0.001, 200., 250e-6);
        assert!((whole - 9.075).abs() < 1e-12);
        let pieces = (0..500)
            .map(|i| dose(i as f64 * 2e-6, (i + 1) as f64 * 2e-6, 200., 250e-6))
            .sum::<f64>();
        assert!((whole - pieces).abs() < 1e-12);
        assert_eq!(dose(0., 0.001, 0., 250e-6), 0.);
    }
}
