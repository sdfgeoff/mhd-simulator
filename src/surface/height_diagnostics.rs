//! Read-only height completeness/curvature experiments, separate from production.
use super::*;

fn install(surface: &mut SurfaceState, target: f32, margin: u32) {
    let source = format!(
        "const HEIGHT_TARGET:f32={target:?};\nconst HEIGHT_MARGIN:u32={margin}u;\n{}",
        include_str!("height_probes.wgsl")
    );
    for name in ["height_probe_collect", "height_probe_reduce"] {
        install_test_entry(surface, name, &source);
    }
}
fn sample(surface: &SurfaceState, volume: &Volume) -> [f32; 12] {
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
        pass.set_pipeline(&surface.pipelines["height_probe_collect"]);
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
