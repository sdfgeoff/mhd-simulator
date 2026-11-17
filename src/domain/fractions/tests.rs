use super::*;
use crate::{
    model::Body,
    simulation::{DomainSettings, default_materials},
    surface::SurfaceState,
    thermal::{ThermalCell, ThermalConfig, ThermalState},
};
use glam::Vec3;

fn box_triangles(lo: Vec3, hi: Vec3) -> Vec<[Vec3; 3]> {
    let v = [
        lo,
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(hi.x, hi.y, lo.z),
        Vec3::new(lo.x, hi.y, lo.z),
        Vec3::new(lo.x, lo.y, hi.z),
        Vec3::new(hi.x, lo.y, hi.z),
        hi,
        Vec3::new(lo.x, hi.y, hi.z),
    ];
    [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [3, 7, 6],
        [3, 6, 2],
        [0, 4, 7],
        [0, 7, 3],
        [1, 2, 6],
        [1, 6, 5],
    ]
    .map(|f| f.map(|i| v[i]))
    .to_vec()
}
fn body(name: &str, triangles: Vec<[Vec3; 3]>, temperature: f32) -> Body {
    let mut b = Body::new(name.into(), triangles).unwrap();
    b.simulation.material_id = 3;
    b.simulation.temperature_c = Some(temperature);
    b
}
fn settings() -> DomainSettings {
    DomainSettings {
        min_mm: [0.; 3],
        max_mm: [1000.; 3],
        cell_mm: 125.,
    }
}
fn sample(gpu: &GpuDomain, model: &Model) -> (Volume, SurfaceState, Volume) {
    let v = gpu.build(model, &settings(), &default_materials()).unwrap();
    let (s, e) = SurfaceState::new(gpu.context.clone(), &v, 3).unwrap();
    (v, s, e)
}
fn fractions(gpu: &GpuDomain, s: &SurfaceState) -> Vec<[f32; 4]> {
    let bytes = read_bytes(&gpu.context, s.field(), 0, s.field().size()).unwrap();
    bytemuck::cast_slice(&bytes).to_vec()
}
#[test]
fn exact_slab_and_cavity_integrals_preserve_subcell_geometry_and_reverse_winding() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    // Both faces lie between the same two cell centres: binary occupancy misses the slab.
    let slab = box_triangles(Vec3::new(0., 0., 401.), Vec3::new(1000., 1000., 409.));
    let mut reference = Vec::new();
    for reverse in [false, true] {
        let mut triangles = slab.clone();
        if reverse {
            for t in &mut triangles {
                t.swap(1, 2);
            }
        }
        let model = Model {
            bodies: vec![body("Thin sheet", triangles, 250.)],
            ..Default::default()
        };
        let (v, mut s, _) = sample(&gpu, &model);
        s.initialize_cad(&v, &model, None).unwrap();
        let values = fractions(&gpu, &s);
        for (i, a) in values.iter().enumerate() {
            let expected = if i / 64 == 3 { 8. / 125. } else { 0. };
            assert!((a[0] - expected).abs() < 2e-6, "{i}: {a:?}");
        }
        assert!((s.diagnostics.volume_m3 - 0.008).abs() < 2e-7);
        if reverse {
            assert_eq!(values, reference);
        } else {
            reference = values;
        }
        assert!(s.diagnostics.initial_workspace_bytes < 1_000_000);
    }
    let mut hollow = box_triangles(Vec3::splat(125.), Vec3::splat(875.));
    let mut cavity = box_triangles(Vec3::splat(250.), Vec3::splat(750.));
    for t in &mut cavity {
        t.swap(1, 2);
    }
    hollow.extend(cavity);
    let model = Model {
        bodies: vec![body("Hollow solid", hollow, 250.)],
        ..Default::default()
    };
    let (v, mut s, _) = sample(&gpu, &model);
    s.initialize_cad(&v, &model, None).unwrap();
    assert!((s.diagnostics.volume_m3 - (0.75f64.powi(3) - 0.5f64.powi(3))).abs() < 1e-7);
    assert_eq!(fractions(&gpu, &s)[4 + 8 * (4 + 8 * 4)][0], 0.);
}
#[test]
fn inconsistent_face_orientation_is_rejected_for_fractional_initialization() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let mut t = box_triangles(Vec3::splat(125.), Vec3::splat(875.));
    t[0].swap(1, 2);
    let b = body("Broken winding", t, 250.);
    assert!(b.closed());
    assert!(!b.consistently_oriented());
    let model = Model {
        bodies: vec![b],
        ..Default::default()
    };
    let (v, mut s, _) = sample(&gpu, &model);
    assert!(
        s.initialize_cad(&v, &model, None)
            .unwrap_err()
            .contains("consistently oriented")
    );
}
#[test]
fn fractional_enthalpy_uses_each_body_temperature_and_preserves_static_solids() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    // The interface divides coarse cell x=3 into exactly 10/16 and 6/16.
    let cut = 453.125;
    let mut wall = body(
        "Static wall",
        box_triangles(Vec3::new(0., 0., 875.), Vec3::splat(1000.)),
        100.,
    );
    wall.simulation.material_id = 2;
    let model = Model {
        bodies: vec![
            body(
                "Left",
                box_triangles(Vec3::ZERO, Vec3::new(cut, 1000., 875.)),
                250.,
            ),
            body(
                "Right",
                box_triangles(Vec3::new(cut, 0., 0.), Vec3::new(1000., 1000., 875.)),
                300.,
            ),
            wall,
        ],
        ..Default::default()
    };
    let (v, mut s, e) = sample(&gpu, &model);
    let mut thermal = ThermalState::new(gpu.context.clone(), &v, ThermalConfig::default()).unwrap();
    s.initialize_cad(&v, &model, Some(thermal.initial_surface_buffers()))
        .unwrap();
    s.update_materials(&v, &e);
    thermal.recover_initial_surface(&e).unwrap();
    let bytes = read_bytes(
        &gpu.context,
        thermal.result_buffer(),
        0,
        thermal.result_buffer().size(),
    )
    .unwrap();
    let cells: &[ThermalCell] = bytemuck::cast_slice(&bytes);
    for (i, c) in cells.iter().enumerate() {
        let x = i % 8;
        let z = i / 64;
        let expected = if z == 7 {
            100.
        } else if x < 3 {
            250.
        } else if x == 3 {
            268.75
        } else {
            300.
        };
        assert!(
            (c.state[0] - 273.15 - expected).abs() < 0.002,
            "{i}: {:?}, expected {expected}",
            c.state
        );
        assert_eq!(c.state[3], 1.);
    }
    assert_eq!(thermal.diagnostics.delta_enthalpy_j, 0.);
    assert_eq!(thermal.diagnostics.energy_error_j, 0.);
    let initial = thermal.diagnostics.enthalpy_j;
    thermal.step(&e, 1e-5, None, None).unwrap();
    assert!((thermal.diagnostics.enthalpy_j - initial).abs() / initial.abs() < 2e-6);
}
#[test]
fn partial_cells_recover_uniform_temperature_without_diluting_latent_heat() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let model = Model {
        bodies: vec![body(
            "Liquid sheet",
            box_triangles(Vec3::new(0., 0., 401.), Vec3::new(1000., 1000., 409.)),
            250.,
        )],
        ..Default::default()
    };
    let (v, mut s, e) = sample(&gpu, &model);
    let mut t = ThermalState::new(gpu.context.clone(), &v, ThermalConfig::default()).unwrap();
    s.initialize_cad(&v, &model, Some(t.initial_surface_buffers()))
        .unwrap();
    s.update_materials(&v, &e);
    t.recover_initial_surface(&e).unwrap();
    let bytes = read_bytes(&gpu.context, t.result_buffer(), 0, t.result_buffer().size()).unwrap();
    for (i, c) in bytemuck::cast_slice::<u8, ThermalCell>(&bytes)
        .iter()
        .enumerate()
    {
        if i / 64 == 3 {
            assert!((c.state[0] - 523.15).abs() < 0.002, "{:?}", c.state);
            assert_eq!(c.state[1], 1.);
            assert!((c.state[3] - 0.064).abs() < 2e-6);
        } else {
            assert_eq!(c.state, [0.; 4]);
        }
    }
}

#[test]
fn unified_run_initializes_thin_liquid_before_projecting_velocity_and_advancing_heat() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let model = Model {
        bodies: vec![body(
            "Liquid sheet",
            box_triangles(Vec3::new(0., 0., 401.), Vec3::new(1000., 1000., 409.)),
            250.,
        )],
        ..Default::default()
    };
    let v = gpu
        .build(&model, &settings(), &default_materials())
        .unwrap();
    for thermal in [false, true] {
        let mut run = crate::run::RunState::new(
            gpu.context.clone(),
            &v,
            &model,
            crate::run::RunConfig {
                thermal_enabled: thermal,
                dt_s: 1e-4,
                ..Default::default()
            },
            crate::em::EmConfig::default(),
            crate::fluid::FluidConfig {
                material_id: 3,
                periodic: [!thermal; 3],
                initial_velocity_m_s: if thermal { [0.; 3] } else { [0.01, 0.02, 0.03] },
                acceleration_m_s2: [0.; 3],
                surface: crate::surface::SurfaceConfig { enabled: true },
                ..Default::default()
            },
        )
        .unwrap();
        let initial = run.surface.as_ref().unwrap().diagnostics.volume_m3;
        assert!((initial - 0.008).abs() < 2e-7);
        assert_eq!(run.energy.thermal_balance_error_j, 0.);
        let kinetic = run.energy.kinetic_j;
        if !thermal {
            assert!(kinetic > 0.);
        }
        run.step(&model, &v).unwrap();
        assert!((run.surface.as_ref().unwrap().diagnostics.volume_m3 / initial - 1.).abs() < 2e-6);
        if let Some(t) = run.thermal.as_ref() {
            let bytes =
                read_bytes(&gpu.context, t.result_buffer(), 0, t.result_buffer().size()).unwrap();
            for c in bytemuck::cast_slice::<u8, ThermalCell>(&bytes) {
                if c.state[3] > 1e-7 {
                    assert!((c.state[0] - 523.15).abs() < 0.01, "{:?}", c.state);
                }
            }
        } else {
            assert!((run.energy.kinetic_j / kinetic - 1.).abs() < 1e-4);
        }
    }
}
#[test]
fn oblique_wedge_integrates_linear_height_at_subcell_resolution() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let mut triangles = box_triangles(Vec3::ZERO, Vec3::splat(1000.));
    // Affine shear/scale maps the box to a solid with top z=200+0.6x.
    for t in &mut triangles {
        for v in t {
            v.z *= (200. + 0.6 * v.x) / 1000.;
        }
    }
    let model = Model {
        bodies: vec![body("Wedge", triangles, 250.)],
        ..Default::default()
    };
    let (v, mut s, _) = sample(&gpu, &model);
    s.initialize_cad(&v, &model, None).unwrap();
    let values = fractions(&gpu, &s);
    // Every XY column integrates to its mean height, regardless of Z clipping.
    for y in 0..8 {
        for x in 0..8 {
            let sum: f32 = (0..8).map(|z| values[x + 8 * (y + 8 * z)][0] * 125.).sum();
            assert!(
                (sum - (200. + 0.6 * (x as f32 + 0.5) * 125.)).abs() < 0.001,
                "{x},{y}: {sum}"
            );
        }
    }
    assert!((s.diagnostics.volume_m3 - 0.5).abs() < 1e-6);
}

#[test]
fn raster_tiles_preserve_fraction_at_partial_last_tile() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let model = Model {
        bodies: vec![body(
            "Wide box",
            box_triangles(Vec3::ZERO, Vec3::new(8093.75, 375., 200.)),
            250.,
        )],
        ..Default::default()
    };
    let v = gpu
        .build(
            &model,
            &DomainSettings {
                min_mm: [0.; 3],
                max_mm: [8125., 375., 250.],
                cell_mm: 125.,
            },
            &default_materials(),
        )
        .unwrap();
    assert_eq!(v.dims, [65, 3, 2]);
    let (mut s, _) = SurfaceState::new(gpu.context.clone(), &v, 3).unwrap();
    s.initialize_cad(&v, &model, None).unwrap();
    for (i, a) in fractions(&gpu, &s).iter().enumerate() {
        let x = i % 65;
        let z = i / (65 * 3);
        let expected = if x == 64 { 0.75 } else { 1. } * if z == 1 { 0.6 } else { 1. };
        assert!(
            (a[0] - expected).abs() < 2e-6,
            "{i}: {a:?} expected {expected}"
        );
    }
}

#[test]
fn wall_conditioned_projection_keeps_free_surface_fraction_and_initial_temperature() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let mut wall = body(
        "Wall",
        box_triangles(Vec3::ZERO, Vec3::new(410., 1000., 1000.)),
        100.,
    );
    wall.simulation.material_id = 2;
    let model = Model {
        bodies: vec![
            wall,
            body(
                "Liquid",
                box_triangles(Vec3::new(410., 0., 0.), Vec3::new(1000., 1000., 420.)),
                250.,
            ),
        ],
        ..Default::default()
    };
    let (volume, mut surface, evolving) = sample(&gpu, &model);
    let mut thermal =
        ThermalState::new(gpu.context.clone(), &volume, ThermalConfig::default()).unwrap();
    initialize(
        &gpu.context,
        &volume,
        &model,
        3,
        surface.field(),
        Some(thermal.initial_surface_buffers()),
    )
    .unwrap();
    surface.refresh(&volume).unwrap();
    surface.update_materials(&volume, &evolving);
    thermal.recover_initial_surface(&evolving).unwrap();
    let values = fractions(&gpu, &surface);
    let bytes = read_bytes(
        &gpu.context,
        thermal.result_buffer(),
        0,
        thermal.result_buffer().size(),
    )
    .unwrap();
    let temperatures = bytemuck::cast_slice::<u8, ThermalCell>(&bytes);
    for (i, (alpha, temperature)) in values.iter().zip(temperatures).enumerate() {
        let x = i % 8;
        let z = i / 64;
        let expected = if x < 3 || z > 3 {
            0.
        } else if z == 3 {
            45. / 125.
        } else {
            1.
        };
        assert!(
            (alpha[0] - expected).abs() < 2e-6,
            "cell {i}: {alpha:?}, expected {expected}"
        );
        if x < 3 {
            assert!((temperature.state[0] - 373.15).abs() < 0.002);
        } else if expected > 0. {
            assert!(
                (temperature.state[0] - 523.15).abs() < 0.002,
                "cell {i}: {:?}",
                temperature.state
            );
            assert!((temperature.state[3] - expected).abs() < 2e-6);
        }
    }
    // The wall's voxel boundary lies at 375 mm: this projected domain's liquid
    // volume is 0.625*1*0.420, rather than the source CAD volume 0.590*1*0.420.
    assert!((surface.diagnostics.volume_m3 - 0.2625).abs() < 2e-7);
}

#[test]
fn conditioned_projection_preserves_isolated_cad_fractions_byte_for_byte() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let mut distant = body(
        "Unrelated wall",
        box_triangles(Vec3::splat(875.), Vec3::splat(1000.)),
        100.,
    );
    distant.simulation.material_id = 2;
    let model = Model {
        bodies: vec![
            body(
                "Free liquid",
                box_triangles(Vec3::new(153., 201., 271.), Vec3::new(733., 866., 952.)),
                250.,
            ),
            distant,
        ],
        ..Default::default()
    };
    let (volume, mut surface, _) = sample(&gpu, &model);
    initialize_raw(&gpu.context, &volume, &model, 3, surface.field(), None).unwrap();
    surface.refresh(&volume).unwrap();
    let original = fractions(&gpu, &surface);
    initialize(&gpu.context, &volume, &model, 3, surface.field(), None).unwrap();
    surface.refresh(&volume).unwrap();
    assert_eq!(original, fractions(&gpu, &surface));
}
#[test]
fn conditioned_projection_rejects_subcell_liquid_solid_overlap() {
    let _lock = crate::gpu_test_lock();
    let gpu = GpuDomain::new().unwrap();
    let mut wall = body(
        "Wall",
        box_triangles(Vec3::ZERO, Vec3::new(410., 1000., 1000.)),
        100.,
    );
    wall.simulation.material_id = 2;
    let model = Model {
        bodies: vec![
            wall,
            body(
                "Intersecting liquid",
                box_triangles(Vec3::new(400., 0., 0.), Vec3::splat(1000.)),
                250.,
            ),
        ],
        ..Default::default()
    };
    let (volume, surface, _) = sample(&gpu, &model);
    assert_eq!(volume.overlaps, 0);
    let error = initialize(&gpu.context, &volume, &model, 3, surface.field(), None).unwrap_err();
    assert!(error.contains("overlapping liquid/solid"), "{error}");
}

#[test]
#[ignore = "Compare raw/conditioned initialization of MHD_CAD_PROJECT without editing it"]
fn project_wall_conditioning_diagnostic() {
    let _lock = crate::gpu_test_lock();
    let filename =
        std::env::var("MHD_CAD_PROJECT").expect("Set MHD_CAD_PROJECT to a project JSON path");
    let project = crate::project::load(std::path::Path::new(&filename)).unwrap();
    let gpu = GpuDomain::new().unwrap();
    let volume = gpu
        .build(&project.model, &project.settings, &project.materials)
        .unwrap();
    let material = project.fluid.material_id;
    let (mut surface, _) = SurfaceState::new(gpu.context.clone(), &volume, material).unwrap();
    initialize_raw(
        &gpu.context,
        &volume,
        &project.model,
        material,
        surface.field(),
        None,
    )
    .unwrap();
    surface.refresh(&volume).unwrap();
    let raw = surface.diagnostics.volume_m3;
    let start = std::time::Instant::now();
    let workspace = initialize(
        &gpu.context,
        &volume,
        &project.model,
        material,
        surface.field(),
        None,
    )
    .unwrap();
    surface.refresh(&volume).unwrap();
    eprintln!(
        "Project conditioned initialization {filename}: raw volume={raw:.9e}, conditioned={:.9e}, relative change={:.6e}, workspace={workspace} bytes, wall={:.3}s",
        surface.diagnostics.volume_m3,
        surface.diagnostics.volume_m3 / raw - 1.,
        start.elapsed().as_secs_f64()
    );
}
