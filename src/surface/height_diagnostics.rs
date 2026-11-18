//! Read-only height completeness/curvature experiments, separate from production.
use super::*;

fn install(surface: &mut SurfaceState, target: f32, margin: u32) {
    let source = format!(
        "const HEIGHT_TARGET:f32={target:?};\nconst HEIGHT_MARGIN:u32={margin}u;\n{}",
        include_str!("height_probes.wgsl")
    );
    for name in [
        "height_probe_collect",
        "height_axis_probe_collect",
        "height_probe_reduce",
    ] {
        install_test_entry(surface, name, &source);
    }
}
fn sample(surface: &SurfaceState, volume: &Volume) -> [f32; 12] {
    sample_entry(surface, volume, "height_probe_collect")
}
fn sample_entry(surface: &SurfaceState, volume: &Volume, entry: &str) -> [f32; 12] {
    let context = &surface.context;
    let device = &context.device;
    let count = surface.params.dims[3].div_ceil(256);
    let stats = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Height completeness reductions"),
        size: (count as u64 + 1) * 48,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let buffers = [
        &surface.uniform,
        volume.material_ids(),
        surface.field(),
        &surface.fields[1 - surface.current],
        &surface.zero,
        &surface.zero,
        &surface.zero_output,
        &surface.normals,
        &stats,
    ];
    let entries: Vec<_> = buffers
        .iter()
        .enumerate()
        .map(|(i, b)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: b.as_entire_binding(),
        })
        .collect();
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &surface.layout,
        entries: &entries,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = context.compute_pass(&mut encoder, None);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_pipeline(&surface.pipelines[entry]);
        let g = groups(surface.params.dims[3]);
        pass.dispatch_workgroups(g[0], g[1], g[2]);
        pass.set_pipeline(&surface.pipelines["height_probe_reduce"]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    context.queue.submit(Some(encoder.finish()));
    let bytes = read_bytes(context, &stats, count as u64 * 48, 48).unwrap();
    let values = bytemuck::cast_slice::<u8, f32>(&bytes);
    assert!(values.iter().all(|v| v.is_finite()));
    values.try_into().unwrap()
}
#[test]
fn complete_height_probe_keeps_oblique_plane_flat_and_preserves_fields() {
    let _lock = crate::gpu_test_lock();
    let n = 32u32;
    let (gpu, volume, _, _) = super::tests::box_fixture(
        crate::simulation::DomainSettings {
            min_mm: [0.; 3],
            max_mm: [1000.; 3],
            cell_mm: 1000. / n as f32,
        },
        &crate::simulation::default_materials(),
    );
    let (mut surface, _) = SurfaceState::new(gpu.context.clone(), &volume, 3).unwrap();
    install_test_entry(
        &mut surface,
        "height_plane",
        r#"
@compute @workgroup_size(256)
fn height_plane(@builtin(global_invocation_id) g:vec3<u32>){let i=flat(g);if i>=p.dims.w{return;}let n=vec3<f32>(1.,0.5,0.25);let center=vec3<f32>(p.dims.xyz)*0.5;let offset=dot(n,center-vec3<f32>(point(i))-vec3<f32>(0.5));output_field[i]=vec4<f32>(cube_halfspace(n,offset),0.,0.,0.);}
"#,
    );
    surface.dispatch(&volume, None, None, &["height_plane"]);
    surface.current = 1 - surface.current;
    surface.refresh(&volume).unwrap();
    install(&mut surface, 0., 7);
    let capture = |s: &SurfaceState| {
        [
            &s.fields[0],
            &s.fields[1],
            &s.normals,
            &s.checkpoint,
            &s.stats,
        ]
        .iter()
        .map(|b| read_bytes(&s.context, b, 0, b.size()).unwrap())
        .collect::<Vec<_>>()
    };
    let before = capture(&surface);
    let v = sample(&surface, &volume);
    assert_eq!(before, capture(&surface));
    assert!(
        v[0] > 100. && v[1] == v[0] && v[2] == v[0] && v[3] == v[0],
        "{v:?}"
    );
    assert!(
        (v[4] / v[0]).sqrt() < 1e-3 && (v[6] / v[3]).sqrt() < 1e-3 && v[11] < 1e-5,
        "{v:?}"
    );
    eprintln!("Complete-height plane: {v:?}");
}
#[test]
#[ignore = "Compare initial sphere height completeness and curvature"]
fn height_stencil_completeness() {
    let _lock = crate::gpu_test_lock();
    let ns = std::env::var("MHD_HEIGHT_GRIDS").unwrap_or_else(|_| "24,32,48,64,80,96".into());
    let mut csv = String::from(
        "grid,offset_cells,mixed,current_hf,complete_hf,both,current_rms_all,current_rms_common,complete_rms_common,current_rms_recovered,complete_rms_recovered,current_mean_common,complete_mean_common,max_height_difference_cells\n",
    );
    for n in ns.split(',').map(|s| s.parse::<u32>().unwrap()) {
        for offset in [0., 0.5] {
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
                0.,
                offset,
            );
            gpu.context
                .queue
                .write_buffer(surface.field(), 0, bytemuck::cast_slice(&initial));
            surface.refresh(&volume).unwrap();
            install(&mut surface, 10., 0);
            let v = sample(&surface, &volume);
            let recovered = v[2] - v[3];
            let rms = |s: f32, count: f32| {
                if count > 0. {
                    (s / count).sqrt() as f64
                } else {
                    f64::NAN
                }
            };
            let mean = |s: f32, count: f32| {
                if count > 0. {
                    (s / count) as f64
                } else {
                    f64::NAN
                }
            };
            csv.push_str(&format!(
                "{n},{offset},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                v[0],
                v[1],
                v[2],
                v[3],
                rms(v[4], v[0]),
                rms(v[5], v[3]),
                rms(v[6], v[3]),
                rms(v[7], recovered),
                rms(v[8], recovered),
                mean(v[9], v[3]),
                mean(v[10], v[3]),
                v[11]
            ));
            eprintln!(
                "Height n={n} offset={offset}: counts={:?}, current RMSall={:.6e}, common RMS {:.6e}->{:.6e}, maxΔH={:.6e}",
                &v[..4],
                rms(v[4], v[0]),
                rms(v[5], v[3]),
                rms(v[6], v[3]),
                v[11]
            );
        }
    }
    std::fs::create_dir_all("artifacts").unwrap();
    std::fs::write("artifacts/height-completeness.csv", csv).unwrap();
}

#[test]
fn extended_curvature_columns_preserve_origins_and_reject_extra_crossings() {
    let _lock = crate::gpu_test_lock();
    let n = 16u32;
    let (gpu, volume, mut surface, _) = super::tests::fixture(n);
    install_test_entry(
        &mut surface,
        "column_probe",
        r#"
@compute @workgroup_size(256)
fn column_probe(@builtin(global_invocation_id) g:vec3<u32>){
 if flat(g)!=0u{return;}
 let q=vec3<i32>(7,7,7);
 stats[0]=vec4<f32>(height(q,1u,q),0.);
 stats[1]=vec4<f32>(curvature_height(q,1u,q),0.);
}
"#,
    );
    for (name, profile, expected) in [
        (
            "falling",
            (0..n)
                .map(|y| (2.75 - y as f32).clamp(0., 1.))
                .collect::<Vec<_>>(),
            Some([-4.25, 1., 1.]),
        ),
        (
            "rising",
            (0..n)
                .map(|y| 1. - (2.75 - y as f32).clamp(0., 1.))
                .collect(),
            Some([5.25, 1., -1.]),
        ),
        (
            "multiple crossings",
            (0..n)
                .map(|y| match y {
                    0 | 1 | 5 | 6 => 1.,
                    2 => 0.75,
                    4 | 7 => 0.5,
                    _ => 0.,
                })
                .collect(),
            None,
        ),
        ("pure phase", vec![1.; n as usize], None),
    ] {
        let field: Vec<_> = (0..n.pow(3))
            .map(|i| [profile[((i / n) % n) as usize], 0., 0., 0.])
            .collect();
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&field));
        surface.dispatch(&volume, None, None, &["column_probe"]);
        let bytes = read_bytes(&gpu.context, &surface.stats, 0, 32).unwrap();
        let result = bytemuck::cast_slice::<u8, f32>(&bytes);
        assert!(result.iter().all(|v| v.is_finite()), "{name}: {result:?}");
        assert_eq!(result[1], 0., "Short column unexpectedly complete: {name}");
        if let Some(expected) = expected {
            for (actual, reference) in result[4..7].iter().zip(expected) {
                assert!((actual - reference).abs() < 1e-6, "{name}: {result:?}");
            }
        } else {
            assert_eq!(result[5], 0., "{name}: {result:?}");
        }
    }
}

#[test]
#[ignore = "Read-only height completeness on a saved native isolated-drop frame"]
fn native_export_height_completeness() {
    let _lock = crate::gpu_test_lock();
    let project_path = std::env::var("MHD_HEIGHT_PROJECT").expect("MHD_HEIGHT_PROJECT");
    let frame_path = std::env::var("MHD_HEIGHT_FRAME").expect("MHD_HEIGHT_FRAME");
    let output_path = std::env::var("MHD_HEIGHT_OUTPUT").expect("MHD_HEIGHT_OUTPUT");
    let project = crate::project::load(std::path::Path::new(&project_path)).unwrap();
    let frame = std::path::Path::new(&frame_path);
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(frame.join("metadata.json")).unwrap()).unwrap();
    assert_eq!(metadata["byte_order"], "little");
    let gpu = crate::domain::GpuDomain::new().unwrap();
    let volume = gpu
        .build(&project.model, &project.settings, &project.materials)
        .unwrap();
    let grid = &volume.inputs.snapshot().grid;
    assert_eq!(metadata["dims"], serde_json::json!(grid.dims));
    for axis in 0..3 {
        assert_eq!(
            metadata["origin_m"][axis].as_f64().unwrap() as f32,
            grid.origin_m[axis]
        );
    }
    assert_eq!(metadata["cell_m"].as_f64().unwrap() as f32, grid.cell_m);
    assert_eq!(
        metadata["fluid"]["periodic"],
        serde_json::json!(project.fluid.periodic)
    );
    let bytes = std::fs::read(frame.join("fluid.bin")).unwrap();
    let fields = bytemuck::cast_slice::<u8, [f32; 16]>(&bytes);
    assert_eq!(
        fields.len(),
        grid.dims.iter().map(|&n| n as usize).product::<usize>()
    );
    let values: Vec<[f32; 4]> = fields.iter().map(|v| [v[14], 0., 0., 0.]).collect();
    let (mut surface, _) = SurfaceState::with_periodic_boundaries(
        gpu.context.clone(),
        &volume,
        project.fluid.material_id,
        project.fluid.periodic,
    )
    .unwrap();
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
    surface.refresh(&volume).unwrap();
    let before = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
    let recomputed = bytemuck::cast_slice::<u8, [f32; 4]>(&before);
    assert!(recomputed.iter().zip(fields).all(|(a, b)| a[0] == b[14]));
    let max_curvature_difference = recomputed
        .iter()
        .zip(fields)
        .map(|(a, b)| (a[1] - b[15]).abs())
        .fold(0_f32, f32::max);
    let target = 2. / 0.0003;
    install(&mut surface, target, 0);
    let v = sample(&surface, &volume);
    let axis_probe = sample_entry(&surface, &volume, "height_axis_probe_collect");
    let after = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
    assert_eq!(before, after, "Read-only probe changed the physical field");
    let result = serde_json::json!({
        "project":project_path,"frame":frame_path,"step":metadata["step"],"time_s":metadata["time_s"],
        "cell_m":grid.cell_m,"mixed_cells":v[0],"axis_probe":{"mixed":axis_probe[0],"primary_valid":axis_probe[1],"any_axis_valid":axis_probe[2],"recovered":axis_probe[3],"current_RMS":(axis_probe[4]/axis_probe[0].max(1.)).sqrt(),"all_axes_RMS":(axis_probe[5]/axis_probe[0].max(1.)).sqrt()},
        "current_height_valid":v[1],
        "strict_height_valid":v[2],"both_valid":v[3],
        "curvature_relative_RMS_all":(v[4]/v[0].max(1.)).sqrt(),
        "current_curvature_relative_RMS_common":(v[5]/v[3].max(1.)).sqrt(),
        "strict_curvature_relative_RMS_common":(v[6]/v[3].max(1.)).sqrt(),
        "mean_current_curvature_common_m_inv":v[9]/v[3].max(1.),
        "mean_strict_curvature_common_m_inv":v[10]/v[3].max(1.),
        "maximum_column_difference_cells":v[11],
        "exported_vs_recomputed_curvature_max_abs_difference_m_inv":max_curvature_difference,
        "physical_field_preserved_bitwise":true,
        "reference_curvature_m_inv":target,
        "scope":"Read-only production GPU curvature/height probe on saved alpha. Sphere target is exact only initially; evolved deformation is not a curvature-error reference. No fluid advance or correction."
    });
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .unwrap();
    writeln!(file, "{}", serde_json::to_string_pretty(&result).unwrap()).unwrap();
    eprintln!("{result}");
}
