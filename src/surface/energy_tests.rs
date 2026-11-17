//! Independent PLIC patch-area diagnostics; no production field updates.
use super::*;
use glam::DVec3;
use wgpu::util::DeviceExt;

pub(super) struct PatchArea {
    pub area_m2: f64,
    pub mixed_cells: u64,
    pub degenerate_cells: u64,
    pub liquid_kinetic_j: f64,
    pub ambient_kinetic_j: f64,
}
pub(super) fn install(surface: &mut SurfaceState) {
    let source = format!(
        "{}\n{}",
        include_str!("../plic_area.wgsl"),
        include_str!("area_probes.wgsl")
    );
    for entry in [
        "patch_area_collect",
        "patch_energy_collect",
        "patch_energy_reduce",
    ] {
        install_test_entry(surface, entry, &source);
    }
}
/// Call only on an accepted, refreshed surface. All normal/reduction/uniform
/// storage is restored; occupancy, curvature and CPU diagnostics stay live.
pub(super) fn patch_area(surface: &mut SurfaceState, volume: &Volume) -> PatchArea {
    sample(surface, volume, None, None)
}
pub(super) fn patch_energy(
    surface: &mut SurfaceState,
    volume: &Volume,
    velocity: &wgpu::Buffer,
    density: [f32; 2],
) -> PatchArea {
    sample(surface, volume, Some(velocity), Some(density))
}
fn sample(
    surface: &mut SurfaceState,
    volume: &Volume,
    velocity: Option<&wgpu::Buffer>,
    density: Option<[f32; 2]>,
) -> PatchArea {
    let saved = surface.params;
    if let Some(density) = density {
        assert!(density.iter().all(|r| r.is_finite() && *r > 0.));
        let count = surface
            .grid
            .dims
            .iter()
            .map(|n| *n as u64 + 1)
            .product::<u64>();
        assert!(velocity.unwrap().size() >= count * 16);
        surface.params.step[2..4].copy_from_slice(&density);
        surface.context.queue.write_buffer(
            &surface.uniform,
            0,
            bytemuck::bytes_of(&surface.params),
        );
    }
    let collect = if density.is_some() {
        "patch_energy_collect"
    } else {
        "patch_area_collect"
    };
    surface.dispatch(
        volume,
        velocity,
        None,
        &["reconstruct", collect, "patch_energy_reduce"],
    );
    let result = read_bytes(
        &surface.context,
        &surface.stats,
        surface.params.dims[3].div_ceil(256) as u64 * 32,
        32,
    );
    surface.params = saved;
    if density.is_some() {
        surface.context.queue.write_buffer(
            &surface.uniform,
            0,
            bytemuck::bytes_of(&surface.params),
        );
    }
    surface.dispatch(volume, None, None, &["normal", "collect", "reduce"]);
    let bytes = result.unwrap();
    let v = bytemuck::cast_slice::<u8, f32>(&bytes);
    assert!(v.iter().all(|v| v.is_finite()) && v[3] == 0.);
    PatchArea {
        area_m2: v[0] as f64,
        mixed_cells: v[1] as u64,
        degenerate_cells: v[2] as u64,
        liquid_kinetic_j: v[4] as f64,
        ambient_kinetic_j: v[5] as f64,
    }
}

// These two restoration tests use a readable replacement of the otherwise
// write-only uniform. Production buffer usage and diagnostic calls are unchanged.
fn enable_uniform_readback(surface: &mut SurfaceState) {
    surface.uniform =
        surface
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Readable diagnostic test uniform"),
                contents: bytemuck::bytes_of(&surface.params),
                usage: wgpu::BufferUsages::UNIFORM
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
            });
}

// Independent scalar geometry oracle: intersect the cube's 12 edges, sort the
// polygon in its plane and integrate its signed triangle area. No grid solver.
fn polygon_area(n: DVec3, d: f64) -> f64 {
    if n.length_squared() == 0. {
        return 0.;
    }
    let mut vertices = Vec::<DVec3>::new();
    let corner = |i: u32| {
        DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) - DVec3::splat(0.5)
    };
    let mut add = |p: DVec3| {
        if !vertices.iter().any(|q| p.distance_squared(*q) < 1e-22) {
            vertices.push(p);
        }
    };
    for i in 0..8u32 {
        for axis in 0..3 {
            if i & (1 << axis) != 0 {
                continue;
            }
            let a = corner(i);
            let b = corner(i | (1 << axis));
            let da = n.dot(a) - d;
            let db = n.dot(b) - d;
            if da.abs() < 1e-12 {
                add(a);
            }
            if db.abs() < 1e-12 {
                add(b);
            }
            if da * db < 0. {
                add(a + (b - a) * (da / (da - db)));
            }
        }
    }
    if vertices.len() < 3 {
        return 0.;
    }
    let center = vertices.iter().copied().sum::<DVec3>() / vertices.len() as f64;
    let unit = n.normalize();
    let u = unit
        .cross(if unit.x.abs() < 0.8 {
            DVec3::X
        } else {
            DVec3::Y
        })
        .normalize();
    let v = unit.cross(u);
    let angle = |p: DVec3| {
        let p = p - center;
        p.dot(v).atan2(p.dot(u))
    };
    vertices.sort_by(|a, b| angle(*a).partial_cmp(&angle(*b)).unwrap());
    let mut sum = DVec3::ZERO;
    for i in 0..vertices.len() {
        sum += (vertices[i] - center).cross(vertices[(i + 1) % vertices.len()] - center);
    }
    0.5 * sum.dot(unit).abs()
}
#[test]
fn gpu_plic_patch_area_matches_independent_cube_polygons() {
    let _lock = crate::gpu_test_lock();
    let mut cases = vec![[0f32; 4]];
    for base in [
        [1., 0., 0.],
        [1., 1., 0.],
        [1., 1., 1.],
        [1., 0.2, 0.001],
        [1., 1e-6, 0.],
        [1., 0.9, 0.8],
        [1., 1e-8, 1e-10],
    ] {
        for rotate in 0..3 {
            for signs in 0..8 {
                for scale in [1e-4, 1., 1e4] {
                    for position in [-0.6, -0.49, -0.25, 0., 0.25, 0.49, 0.6] {
                        let n = std::array::from_fn::<_, 3, _>(|a| {
                            base[(a + rotate) % 3]
                                * scale
                                * if signs & (1 << a) == 0 { 1. } else { -1. }
                        });
                        cases.push([
                            n[0],
                            n[1],
                            n[2],
                            position * n.iter().map(|v| v.abs()).sum::<f32>(),
                        ]);
                    }
                }
            }
        }
    }
    let gpu = GpuContext::headless().unwrap();
    let device = &gpu.device;
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Plane area cases"),
        contents: bytemuck::cast_slice(&cases),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: cases.len() as u64 * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let source = format!(
        "{}\n@group(0) @binding(0) var<storage,read> cases:array<vec4<f32>>;\n@group(0) @binding(1) var<storage,read_write> areas:array<f32>;\n@compute @workgroup_size(256) fn evaluate(@builtin(global_invocation_id) g:vec3<u32>){{if g.x<arrayLength(&cases){{let c=cases[g.x];areas[g.x]=plic_patch_area(c.xyz,c.w);}}}}",
        include_str!("../plic_area.wgsl")
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("PLIC area geometry"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("evaluate"),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = gpu.compute_pass(&mut encoder, None);
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups((cases.len() as u32).div_ceil(256), 1, 1);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let bytes = read_bytes(&gpu, &output, 0, output.size()).unwrap();
    let areas = bytemuck::cast_slice::<u8, f32>(&bytes);
    let mut max_relative = 0f64;
    for (case, area) in cases.iter().zip(areas) {
        let expected = polygon_area(
            DVec3::new(case[0] as f64, case[1] as f64, case[2] as f64),
            case[3] as f64,
        );
        let error = (*area as f64 - expected).abs();
        max_relative = max_relative.max(error / expected.max(1e-4));
        assert!(
            area.is_finite() && error < 5e-5 * expected.max(1e-4),
            "{case:?}: {area} vs {expected}"
        );
    }
    eprintln!(
        "PLIC patch area: {} independent polygons, maximum scaled error {max_relative:.3e}",
        cases.len()
    );
}
#[test]
fn patch_area_readback_restores_accepted_surface_storage() {
    let _lock = crate::gpu_test_lock();
    let n = 16;
    let (gpu, volume, _, _) = super::tests::box_fixture(
        crate::simulation::DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &crate::simulation::default_materials(),
    );
    let (mut surface, _) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
    let initial = super::drop_tests::ellipsoid(n, 1. / n as f64, 0.2, 0.03, 0.);
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
    surface.refresh(&volume).unwrap();
    install(&mut surface);
    enable_uniform_readback(&mut surface);
    let capture = |s: &SurfaceState| {
        [
            &s.fields[0],
            &s.fields[1],
            &s.normals,
            &s.checkpoint,
            &s.stats,
            &s.uniform,
        ]
        .iter()
        .map(|b| read_bytes(&s.context, b, 0, b.size()).unwrap())
        .collect::<Vec<_>>()
    };
    let before = capture(&surface);
    let old_area = surface.diagnostics.area_m2;
    let result = patch_area(&mut surface, &volume);
    assert_eq!(before, capture(&surface));
    assert_eq!(old_area, surface.diagnostics.area_m2);
    assert!(
        result.area_m2 > 0.45
            && result.area_m2 < 0.55
            && result.mixed_cells > 0
            && result.degenerate_cells == 0
    );
    eprintln!(
        "PLIC patch area n={n}: {} m² vs gradient {} m²; {} mixed cells",
        result.area_m2, old_area, result.mixed_cells
    );
}

#[test]
fn phase_kinetic_readback_matches_uniform_translation_and_restores_uniforms() {
    let _lock = crate::gpu_test_lock();
    let n = 16u32;
    let (gpu, volume, _, _) = super::tests::box_fixture(
        crate::simulation::DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &crate::simulation::default_materials(),
    );
    let (mut surface, _) =
        SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, [true; 3]).unwrap();
    let initial = super::drop_tests::ellipsoid(n, 1. / n as f64, 0.2, 0.03, 0.);
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
    surface.refresh(&volume).unwrap();
    install(&mut surface);
    enable_uniform_readback(&mut surface);
    let speed = [0.2f32, -0.1, 0.05, 0.];
    let faces = vec![speed; (n + 1).pow(3) as usize];
    let velocity = gpu
        .context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&faces),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let capture = |s: &SurfaceState| {
        [&s.fields[0], &s.fields[1], &s.normals, &s.stats, &s.uniform]
            .iter()
            .map(|b| read_bytes(&s.context, b, 0, b.size()).unwrap())
            .collect::<Vec<_>>()
    };
    let before = capture(&surface);
    let volume_liquid = surface.diagnostics.volume_m3;
    let result = patch_energy(&mut surface, &volume, &velocity, [7000., 1.]);
    assert_eq!(before, capture(&surface));
    let squared = speed[..3].iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
    let expected = [
        0.5 * 7000. * volume_liquid * squared,
        0.5 * (1. - volume_liquid) * squared,
    ];
    for (actual, expected) in [result.liquid_kinetic_j, result.ambient_kinetic_j]
        .into_iter()
        .zip(expected)
    {
        assert!(
            (actual / expected - 1.).abs() < 2e-6,
            "{actual} vs {expected}"
        );
    }
}

pub(super) fn ellipsoid_area(r: f64, epsilon: f64) -> f64 {
    if epsilon.abs() < 1e-8 {
        return 4. * std::f64::consts::PI * r * r;
    }
    let a = r / (1. + epsilon).sqrt();
    let c = r * (1. + epsilon);
    let e = (1. - a * a / (c * c)).sqrt();
    2. * std::f64::consts::PI * a * a * (1. + c * e.asin() / (a * e))
}
#[test]
#[ignore = "Initial interface area refinement; no time integration"]
fn reconstructed_area_refinement() {
    let _lock = crate::gpu_test_lock();
    let ns = std::env::var("MHD_AREA_GRIDS").unwrap_or_else(|_| "16,24,32,48,64,96".into());
    let mut csv = String::from(
        "grid,deformation,volume_m3,analytic_area_m2,gradient_area_m2,plic_patch_area_m2,mixed_cells,degenerate_cells\n",
    );
    for n in ns.split(',').map(|s| s.parse::<u32>().unwrap()) {
        for epsilon in [0., 0.03] {
            let (gpu, volume, _, _) = super::tests::box_fixture(
                crate::simulation::DomainSettings {
                    min_mm: [0.; 3],
                    max_mm: [1000.; 3],
                    cell_mm: 1000. / n as f32,
                },
                &crate::simulation::default_materials(),
            );
            let (mut surface, _) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
            let initial = super::drop_tests::ellipsoid(
                n,
                volume.inputs.snapshot().grid.cell_m as f64,
                0.2,
                epsilon,
                0.,
            );
            gpu.context
                .queue
                .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
            surface.refresh(&volume).unwrap();
            install(&mut surface);
            let area = patch_area(&mut surface, &volume);
            let exact = ellipsoid_area(0.2, epsilon);
            eprintln!(
                "Area n={n} epsilon={epsilon}: gradient/exact-1={:.6e}, PLIC/exact-1={:.6e}",
                surface.diagnostics.area_m2 / exact - 1.,
                area.area_m2 / exact - 1.
            );
            csv.push_str(&format!(
                "{n},{epsilon},{},{exact},{},{},{},{}\n",
                surface.diagnostics.volume_m3,
                surface.diagnostics.area_m2,
                area.area_m2,
                area.mixed_cells,
                area.degenerate_cells
            ));
            assert!(area.area_m2.is_finite() && area.area_m2 > 0.);
        }
    }
    std::fs::create_dir_all("artifacts").unwrap();
    std::fs::write("artifacts/interface-area-refinement.csv", csv).unwrap();
}

#[test]
#[ignore = "Rigid-translation control for interface area estimators"]
fn rigid_translation_area_control() {
    let _lock = crate::gpu_test_lock();
    let ns = std::env::var("MHD_AREA_GRIDS").unwrap_or_else(|_| "32,64".into());
    let mut csv = String::from(
        "grid,step,time_s,sample,analytic_area_m2,gradient_area_m2,plic_patch_area_m2,mixed_cells,degenerate_cells,volume_m3\n",
    );
    for n in ns.split(',').map(|s| s.parse::<u32>().unwrap()) {
        let (gpu, volume, _, _) = super::tests::box_fixture(
            crate::simulation::DomainSettings {
                min_mm: [0.; 3],
                max_mm: [1000.; 3],
                cell_mm: 1000. / n as f32,
            },
            &crate::simulation::default_materials(),
        );
        let (mut surface, _) =
            SurfaceState::with_periodic_boundaries(gpu.context.clone(), &volume, 3, [true; 3])
                .unwrap();
        let h = volume.inputs.snapshot().grid.cell_m as f64;
        let epsilon = 0.03;
        let exact = ellipsoid_area(0.2, epsilon);
        let initial = super::drop_tests::ellipsoid(n, h, 0.2, epsilon, 0.);
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
        surface.refresh(&volume).unwrap();
        install(&mut surface);
        let initial_volume = surface.diagnostics.volume_m3;
        let faces = vec![[0.1f32, 0.1, 0.1, 0.]; (n + 1).pow(3) as usize];
        let velocity = gpu
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Rigid translation area control"),
                contents: bytemuck::cast_slice(&faces),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let steps = 2 * n;
        let dt = 1. / steps as f64;
        for step in 0..=steps {
            let area = patch_area(&mut surface, &volume);
            csv.push_str(&format!(
                "{n},{step},{},advected,{exact},{},{},{},{},{}\n",
                step as f64 * dt,
                surface.diagnostics.area_m2,
                area.area_m2,
                area.mixed_cells,
                area.degenerate_cells,
                surface.diagnostics.volume_m3
            ));
            if step == 0 || step == steps {
                eprintln!(
                    "Rigid area n={n} step={step}: gradient={} PLIC={} exact={exact}",
                    surface.diagnostics.area_m2, area.area_m2
                );
            }
            if step < steps {
                surface.advance(&volume, &velocity, None, dt, 0.1).unwrap();
            }
        }
        assert!((surface.diagnostics.volume_m3 / initial_volume - 1.).abs() < 2e-6);
        let reference = super::drop_tests::ellipsoid(n, h, 0.2, epsilon, 0.1f32 as f64 / h);
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&reference));
        surface.refresh(&volume).unwrap();
        let area = patch_area(&mut surface, &volume);
        csv.push_str(&format!(
            "{n},{steps},1,analytic_translated_fractions,{exact},{},{},{},{},{}\n",
            surface.diagnostics.area_m2,
            area.area_m2,
            area.mixed_cells,
            area.degenerate_cells,
            surface.diagnostics.volume_m3
        ));
        eprintln!(
            "Rigid reference n={n}: gradient={} PLIC={}",
            surface.diagnostics.area_m2, area.area_m2
        );
    }
    std::fs::create_dir_all("artifacts").unwrap();
    std::fs::write("artifacts/rigid-translation-area.csv", csv).unwrap();
}
