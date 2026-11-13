mod app;
mod gpu;
mod model;
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
        let gpu = gpu::Gpu::new()?;
        let (lo, hi) = m.bounds();
        let center = (lo + hi) * 0.5;
        let span = (hi - lo).max_element() * 0.65;
        let pixels = gpu.section(&m, center, Vec3::Y * span, Vec3::Z * span, 128)?;
        let mut counts = std::collections::BTreeMap::new();
        for &p in &pixels {
            *counts.entry(p).or_insert(0usize) += 1;
        }
        println!("Adapter: {}\nSection labels: {:?}", gpu.name, counts);
        std::fs::create_dir_all("artifacts")?;
        let mut ppm = b"P6\n128 128\n255\n".to_vec();
        for row in pixels.chunks(128).rev() {
            for &p in row {
                ppm.extend(app::color(p));
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
        Box::new(move |_| Ok(Box::new(app::App::new(path)))),
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
    #[test]
    fn open_surface_cannot_enter_gpu_classification() {
        let m = model::Model::load(std::path::Path::new("assets/open-triangle.gltf")).unwrap();
        assert!(!m.bodies[0].closed());
        let gpu = gpu::Gpu::new().unwrap();
        assert!(
            gpu.section(&m, Vec3::ZERO, Vec3::X, Vec3::Y, 32)
                .unwrap_err()
                .contains("not closed")
        );
    }
    #[test]
    fn gpu_annulus_matches_analytic_sections_and_converges() {
        let gpu = gpu::Gpu::new().expect("GPU numerical tests require a working wgpu adapter");
        eprintln!("GPU numerical test adapter: {}", gpu.name);
        let m = model::Model::load(std::path::Path::new("assets/annulus.gltf")).unwrap();
        let span = 5.;
        let mut area_errors = Vec::new();
        for n in [64, 128, 256] {
            let labels = gpu
                .section(&m, Vec3::ZERO, Vec3::X * span, Vec3::Y * span, n)
                .unwrap();
            let area =
                labels.iter().filter(|&&v| v == 1).count() as f32 * (2. * span / n as f32).powi(2);
            let exact = std::f32::consts::PI * (9. - 2.25);
            let error = (area - exact).abs() / exact;
            assert!(error < 0.025, "n={n} area={area} error={error}");
            area_errors.push(error);
            assert_eq!(
                labels[(n * n / 2 + n / 2) as usize],
                0,
                "Cavity must remain empty"
            );
            for y in 0..n {
                for x in 0..n {
                    let p = Vec3::new(
                        ((x as f32 + 0.5) / n as f32 * 2. - 1.) * span,
                        ((y as f32 + 0.5) / n as f32 * 2. - 1.) * span,
                        0.,
                    );
                    let r = p.length();
                    if (r - 1.5).abs() > 0.03 && (r - 3.).abs() > 0.03 {
                        assert_eq!(labels[(y * n + x) as usize], u32::from(r > 1.5 && r < 3.));
                    }
                }
            }
        }
        assert!(
            area_errors[2] < area_errors[0],
            "Refinement must reduce area error: {area_errors:?}"
        );
        let u = Vec3::new(1., 0., 1.).normalize() * span;
        let v = Vec3::Y * span;
        let n = 128;
        let labels = gpu.section(&m, Vec3::new(0., 0., 0.37), u, v, n).unwrap();
        for y in 0..n {
            for x in 0..n {
                let p = Vec3::new(0., 0., 0.37)
                    + ((x as f32 + 0.5) / n as f32 * 2. - 1.) * u
                    + ((y as f32 + 0.5) / n as f32 * 2. - 1.) * v;
                let r = p.truncate().length();
                if (r - 1.5).abs() > 0.03 && (r - 3.).abs() > 0.03 && (p.z.abs() - 4.).abs() > 0.03
                {
                    assert_eq!(
                        labels[(y * n + x) as usize],
                        u32::from(r > 1.5 && r < 3. && p.z.abs() < 4.)
                    );
                }
            }
        }
    }
}
