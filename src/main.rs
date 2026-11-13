pub use mhd_workbench::{
    caps, domain, em, flow, fluid, gpu, inputs, model, plane, project, simulation,
};
mod app;
mod em_ui;
mod fluid_ui;
mod gizmo;
mod render;
mod section;
mod timeline;
mod vector_ui;
use glam::Vec3;
// GPU tests share one physical adapter; serialize context lifetime and teardown.
#[cfg(test)]
pub(crate) fn gpu_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--inspect") {
        let path = args.get(2).ok_or("Usage: --inspect FILE")?;
        let m = model::Model::load(std::path::Path::new(path))?;
        for b in &m.bodies {
            println!(
                "{}: {} triangles; closed={}",
                b.name,
                b.triangles().len(),
                b.closed()
            );
        }
        for curve in &m.curves {
            println!(
                "{}: {} centreline points",
                curve.name,
                curve.points_mm.len()
            );
        }
        if !m.bodies.is_empty() || !m.curves.is_empty() {
            println!("Bounds (mm): {:?}", m.bounds());
        }
        for n in &m.notices {
            println!("{n}");
        }
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--fluid-check") {
        let path = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("assets/hotend-cad.mhd.json");
        let steps = args
            .get(3)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(10);
        let mut project = project::load(std::path::Path::new(path))?;
        let gpu = domain::GpuDomain::new()?;
        let volume = gpu.build(&project.model, &project.settings, &project.materials)?;
        let em_steps = args.get(4).and_then(|s| s.parse::<u32>().ok());
        if em_steps.is_some() {
            project.fluid.use_em_force = true;
        }
        let force = if project.fluid.use_em_force {
            let mut state = em::EmState::new(
                gpu.context.clone(),
                &volume,
                &project.model,
                project.electromagnetics,
                em::Mode::Induction,
            )?;
            for _ in 0..em_steps.unwrap_or(5) {
                state.step(&project.model, &volume)?;
            }
            println!("Frozen EM forcing at {:.3} µs", state.time_s * 1e6);
            Some(state.snapshot())
        } else {
            None
        };
        let mut state = fluid::FluidState::new(
            gpu.context.clone(),
            &volume,
            project.fluid,
            force.as_ref().map(|f| fluid::ForceField {
                buffer: f.result_buffer(),
                grid: f.grid(),
            }),
        )?;
        println!("Fluid reference probe: {:?} mm", state.reference_probe_mm);
        let mut csv = String::from(
            "time_s,ux_m_s,uy_m_s,uz_m_s,pressure_pa,divergence_s_inv,max_speed_m_s,max_divergence_s_inv,rms_divergence_s_inv,net_flux_m3_s,kinetic_energy_j,relative_pressure_residual,iterations\n",
        );
        for i in 0..=steps {
            if i > 0 {
                state.step(&volume)?;
            }
            let sample = state.probe(state.reference_probe_mm)?.unwrap_or_default();
            let d = &state.diagnostics;
            println!(
                "t={:.3} us Umax={:.3e} m/s Pmax={:.3e} Pa div_rms={:.3e} s^-1 flux={:.3e} m3/s ({} iterations, {:.1} ms)",
                state.time_s * 1e6,
                d.max_speed_m_s,
                d.max_pressure_pa,
                d.rms_divergence_s_inv,
                d.net_flux_m3_s,
                d.iterations,
                d.elapsed_ms
            );
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                state.time_s,
                sample.velocity[0],
                sample.velocity[1],
                sample.velocity[2],
                sample.pressure[0],
                sample.pressure[1],
                d.max_speed_m_s,
                d.max_divergence_s_inv,
                d.rms_divergence_s_inv,
                d.net_flux_m3_s,
                d.kinetic_energy_j,
                d.relative_residual,
                d.iterations
            ));
        }
        std::fs::create_dir_all("artifacts")?;
        std::fs::write("artifacts/fluid-probe.csv", csv)?;
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--em-check") {
        let path = args
            .get(2)
            .map(String::as_str)
            .unwrap_or("assets/hotend-cad.mhd.json");
        let steps = args
            .get(3)
            .map(|s| s.parse::<u32>())
            .transpose()?
            .unwrap_or(20);
        let project = project::load(std::path::Path::new(path))?;
        let gpu = domain::GpuDomain::new()?;
        let volume = gpu.build(&project.model, &project.settings, &project.materials)?;
        let mut state = em::EmState::new(
            gpu.context.clone(),
            &volume,
            &project.model,
            project.electromagnetics,
            em::Mode::Induction,
        )?;
        let mut csv = String::from(
            "time_s,bx_t,by_t,bz_t,jx_a_m2,jy_a_m2,jz_a_m2,fx_n_m3,fy_n_m3,fz_n_m3,heat_w_m3,net_fx_n,net_fy_n,net_fz_n,power_w,residual,iterations\n",
        );
        let mut body_csv = String::from("time_s,body,force_x_n,force_y_n,force_z_n,power_w\n");
        for step in 0..=steps {
            if step > 0 {
                state.step(&project.model, &volume)?;
            }
            let p = state.probe([1., 0., 4.])?.unwrap_or_default();
            let d = &state.diagnostics;
            for body in &state.bodies {
                body_csv.push_str(&format!(
                    "{},\"{}\",{},{},{},{}\n",
                    state.time_s,
                    body.name.replace('"', "\"\""),
                    body.force_n[0],
                    body.force_n[1],
                    body.force_n[2],
                    body.power_w
                ));
            }
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                state.time_s,
                p.b[0],
                p.b[1],
                p.b[2],
                p.j[0],
                p.j[1],
                p.j[2],
                p.force_heat[0],
                p.force_heat[1],
                p.force_heat[2],
                p.force_heat[3],
                d.force_n[0],
                d.force_n[1],
                d.force_n[2],
                d.power_w,
                d.relative_residual,
                d.iterations
            ));
            println!(
                "t={:.1} us Bmax={:.4e} T Jmax={:.4e} A/m2 P={:.4e} W residual={:.2e} iter={} ({:.1} ms)",
                state.time_s * 1e6,
                d.maxima[0],
                d.maxima[1],
                d.power_w,
                d.relative_residual,
                d.iterations,
                d.elapsed_ms
            );
        }
        std::fs::create_dir_all("artifacts")?;
        std::fs::write("artifacts/em-probe.csv", csv)?;
        std::fs::write("artifacts/em-body.csv", body_csv)?;
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--gpu-check") {
        let path = args
            .get(2)
            .map(String::as_str)
            .unwrap_or("assets/hotend.gltf");
        let m = model::Model::load(std::path::Path::new(path))?;
        let gpu = domain::GpuDomain::new()?;
        let mut settings = simulation::DomainSettings::fit(&m);
        if let Some(h) = args.get(3) {
            settings.cell_mm = h.parse()?;
        }
        let volume = gpu.build(&m, &settings, &crate::simulation::default_materials())?;
        let (lo, hi) = m.bounds();
        let plane = plane::SectionPlane::new((lo + hi) * 0.5);
        let pixels = gpu.section(
            &volume,
            plane,
            (volume.max() - Vec3::from(settings.min_mm)).max_element() * 0.65,
            256,
        )?;
        let mut counts = std::collections::BTreeMap::new();
        for p in &pixels {
            *counts.entry(p.body).or_insert(0usize) += 1;
        }
        println!(
            "Adapter: {}\nDomain: {:?} at {} mm, {:.1} ms build, {:.1} MiB persistent\nOccupied: {}, overlaps: {}\nSection labels: {:?}",
            gpu.name,
            volume.dims,
            settings.cell_mm,
            volume.build_ms,
            volume.persistent_bytes as f64 / 1048576.,
            volume.occupied,
            volume.overlaps,
            counts
        );
        for warning in &volume.warnings {
            println!("Warning: {warning}");
        }
        std::fs::create_dir_all("artifacts")?;
        let mut ppm = b"P6\n256 256\n255\n".to_vec();
        for row in pixels.chunks(256).rev() {
            for &p in row {
                ppm.extend(app::color(p.body));
            }
        }
        std::fs::write("artifacts/section.ppm", ppm)?;
        return Ok(());
    }
    if args
        .get(1)
        .is_some_and(|a| a.starts_with("--") && a != "--em-preview")
    {
        return Err("Unknown command. Supported: --inspect, --gpu-check, --em-check, --fluid-check, --em-preview".into());
    }
    let preview = args.get(1).is_some_and(|a| a == "--em-preview");
    let path = args
        .get(if preview { 2 } else { 1 })
        .cloned()
        .unwrap_or_else(|| "assets/hotend-cad.mhd.json".into());
    let preview_steps = if preview {
        Some(
            args.get(3)
                .map(|s| s.parse::<u32>())
                .transpose()?
                .unwrap_or(5),
        )
    } else {
        None
    };
    let mut setup = egui_wgpu::WgpuSetupCreateNew::default();
    setup.device_descriptor = std::sync::Arc::new(crate::gpu::device_descriptor);
    let options = eframe::NativeOptions {
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        renderer: eframe::Renderer::Wgpu,
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1100., 660.]),
        ..Default::default()
    };
    eframe::run_native(
        "MHD Workbench",
        options,
        Box::new(move |cc| {
            let mut app = app::App::new(path, cc.wgpu_render_state.clone());
            if let Some(n) = preview_steps {
                app.start_em_preview(n);
            }
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_assembly_units_placement_and_cavity() {
        let m = model::Model::load(std::path::Path::new("assets/hotend.gltf")).unwrap();
        let glb = model::Model::load(std::path::Path::new("assets/hotend.glb")).unwrap();
        assert_eq!(glb.bodies.len(), 5);
        assert_eq!(glb.bounds(), m.bounds());
        let stl = model::Model::load(std::path::Path::new("assets/annulus.stl")).unwrap();
        assert!(stl.bodies[0].closed());
        assert!((stl.bounds().1 - Vec3::new(3., 3., 4.)).length() < 1e-5);
        assert_eq!(m.bodies.len(), 5);
        assert!(m.bodies.iter().all(|b| b.closed()));
        let (lo, hi) = m.bounds();
        assert!((lo - Vec3::new(-7., -4.2, -2.)).length() < 1e-4);
        assert!((hi - Vec3::new(7., 4.2, 10.)).length() < 1e-4);
    }
}
