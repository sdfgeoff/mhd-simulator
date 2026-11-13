mod app;
mod caps;
mod domain;
mod gizmo;
mod model;
mod plane;
mod project;
mod render;
use glam::Vec3;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--inspect") {
        let path = args.get(2).ok_or("Usage: --inspect FILE")?;
        let m = model::Model::load(std::path::Path::new(path))?;
        for b in &m.bodies {
            println!(
                "{}: {} triangles; closed={}",
                b.name,
                b.triangles.len(),
                b.closed()
            );
        }
        if !m.bodies.is_empty() {
            println!("Bounds (mm): {:?}", m.bounds());
        }
        for n in &m.notices {
            println!("{n}");
        }
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--gpu-check") {
        let path = args
            .get(2)
            .map(String::as_str)
            .unwrap_or("assets/hotend.gltf");
        let m = model::Model::load(std::path::Path::new(path))?;
        let gpu = domain::GpuDomain::new()?;
        let mut settings = project::DomainSettings::fit(&m);
        if let Some(h) = args.get(3) {
            settings.cell_mm = h.parse()?;
        }
        let volume = gpu.build(&m, &settings, &project::default_materials())?;
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
    let path = args.get(1).cloned().unwrap_or("assets/hotend.gltf".into());
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1100., 660.]),
        ..Default::default()
    };
    eframe::run_native(
        "MHD Workbench",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(path, cc.wgpu_render_state.clone())))),
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
