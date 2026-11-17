//! Analytic geometry/spectral checks for the transverse height-Hessian filter.
use crate::gpu::{GpuContext, read_bytes};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Case {
    h0: [f32; 4],
    h1: [f32; 4],
    h2: [f32; 4],
    info: [f32; 4],
}
#[test]
fn height_hessian_filter_preserves_polynomials_and_extrusions() {
    let _lock = crate::gpu_test_lock();
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for cell in [0.05f32, 0.1, 0.25, 1.] {
        for profile in 0..4 {
            let mut values = [0f32; 12];
            for y in -1i32..=1 {
                for x in -1i32..=1 {
                    let (x, y) = (x as f64, y as f64);
                    let h = match profile {
                        0 => {
                            3. + 0.1 * x - 0.05 * y
                                + cell as f64 * (x * x + 0.3 * x * y - 0.5 * y * y)
                        }
                        1 => 3. + (std::f64::consts::PI * x / 3.).cos(),
                        2 => 3. + 0.001 * (std::f64::consts::PI * (x + y)).cos(),
                        _ => 3.5,
                    };
                    values[((x as i32 + 1) + 3 * (y as i32 + 1)) as usize] = h as f32;
                }
            }
            cases.push(Case {
                h0: values[0..4].try_into().unwrap(),
                h1: values[4..8].try_into().unwrap(),
                h2: values[8..12].try_into().unwrap(),
                info: [cell, 0., 0., 0.],
            });
            expected.push(match profile {
                0 => [2., -1., 0.3],
                // An extruded sinusoid retains its unfiltered 1D second difference.
                1 => [-1. / cell, 0., 0.],
                // The diagonal Nyquist mode is reduced by 3/7 in both directions.
                2 => [-(12. / 7.) * 0.001 / cell, -(12. / 7.) * 0.001 / cell, 0.],
                _ => [0.; 3],
            });
        }
    }
    let context = GpuContext::headless().unwrap();
    let input = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Analytic height stencils"),
            contents: bytemuck::cast_slice(&cases),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let output = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Height Hessians"),
        size: cases.len() as u64 * 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let shader = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Height geometry probe"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}",
                    include_str!("../height_hessian.wgsl"),
                    r#"struct Case {h0:vec4<f32>,h1:vec4<f32>,h2:vec4<f32>,info:vec4<f32>}
            @group(0) @binding(0) var<storage,read> cases:array<Case>;
            @group(0) @binding(1) var<storage,read_write> result:array<vec4<f32>>;
            @compute @workgroup_size(64)
            fn probe(@builtin(global_invocation_id) g:vec3<u32>){
                let i=g.x;if i>=arrayLength(&cases){return;}
                let c=cases[i];
                let h=array<f32,9>(c.h0.x,c.h0.y,c.h0.z,c.h0.w,c.h1.x,c.h1.y,c.h1.z,c.h1.w,c.h2.x);
                result[i]=vec4<f32>(height_hessian(h,c.info.x),0.0);
            }"#
                )
                .into(),
            ),
        });
    let pipeline = context
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("probe"),
            compilation_options: Default::default(),
            cache: None,
        });
    let bind = context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
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
    let mut encoder = context.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups((cases.len() as u32).div_ceil(64), 1, 1);
    }
    context.queue.submit(Some(encoder.finish()));
    let bytes = read_bytes(&context, &output, 0, output.size()).unwrap();
    for (i, (value, expected)) in bytemuck::cast_slice::<u8, [f32; 4]>(&bytes)
        .iter()
        .zip(expected)
        .enumerate()
    {
        for axis in 0..3 {
            assert!(
                (value[axis] - expected[axis]).abs() < 5e-5 * (1. + expected[axis].abs()),
                "stencil {i} axis {axis}: {} vs {}",
                value[axis],
                expected[axis]
            );
        }
    }
}

#[test]
#[ignore = "Inspect height-function coverage of analytic or saved drop geometry"]
fn drop_height_function_coverage() {
    let _lock = crate::gpu_test_lock();
    let n: u32 = std::env::var("MHD_HEIGHT_GRID")
        .unwrap_or("64".into())
        .parse()
        .unwrap();
    let offset: f64 = std::env::var("MHD_HEIGHT_OFFSET")
        .unwrap_or("0".into())
        .parse()
        .unwrap();
    let (gpu, volume, mut surface, _) = super::tests::fixture(n);
    let saved_input = std::env::var("MHD_HEIGHT_INPUT").ok();
    let values = if let Some(path) = &saved_input {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(bytes.len(), n.pow(3) as usize * 16);
        bytes
            .chunks_exact(16)
            .map(bytemuck::pod_read_unaligned::<[f32; 4]>)
            .collect::<Vec<_>>()
    } else {
        super::drop_tests::ellipsoid(n, 1. / n as f64, 0.2, 0.03, offset)
    };
    gpu.context
        .queue
        .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
    if saved_input.is_some() {
        // Retain the curvature from that run; only rebuild the normal/height guide.
        surface.dispatch(&volume, None, None, &["normal"]);
    } else {
        surface.refresh(&volume).unwrap();
    }
    super::install_test_entry(
        &mut surface,
        "coverage",
        r#"
        @compute @workgroup_size(256)
        fn coverage(@builtin(global_invocation_id) g:vec3<u32>){
          let i=flat(g);if i>=p.dims.w{return;}let q=point(i);var bits=0u;var wider_bits=0u;
          for(var axis=0u;axis<3u;axis++){
            let a=(axis+1u)%3u;let b=(axis+2u)%3u;var valid=true;var wider_valid=true;
            for(var v=-1;v<=1;v++){for(var u=-1;u<=1;u++){
              var at=q;at[a]+=u;at[b]+=v;let h=height(at,axis,q);
              valid=valid&&h.y>0.5&&h.z==sign(normals[i][axis]);
              // Diagnostic comparison only; production height() uses seven/nine cells.
              let extended=height_range(at,axis,5,q);
              let w=select(extended,h,h.y>0.5);
              wider_valid=wider_valid&&w.y>0.5&&w.z==sign(normals[i][axis]);
            }}
            if valid{bits|=1u<<axis;}
            if wider_valid{wider_bits|=1u<<axis;}
          }
          let normal=abs(normals[i].xyz);var best=0u;
          if normal.y>normal.x{best=1u;}if normal.z>normal[best]{best=2u;}
          output_field[i]=vec4<f32>(f32(bits|(wider_bits<<3u)),f32(best),input_field[i].xy);
        }"#,
    );
    surface.dispatch(&volume, None, None, &["coverage"]);
    let output = &surface.fields[1 - surface.current];
    let bytes = read_bytes(&gpu.context, output, 0, output.size()).unwrap();
    let mut counts = [0usize; 3];
    let mut squared = [0f64; 3];
    let mut maximum = [0f64; 3];
    let mut wider_rescued = 0;
    let mut csv =
        String::from("index,alpha,curvature,valid_axes,dominant_axis,extended_valid_axes\n");
    for (i, p) in bytemuck::cast_slice::<u8, [f32; 4]>(&bytes)
        .iter()
        .enumerate()
    {
        if p[2] <= 0.01 || p[2] >= 0.99 {
            continue;
        }
        let bits = p[0] as u32 & 7;
        let wider_bits = (p[0] as u32) >> 3;
        if bits == 0 && wider_bits != 0 {
            wider_rescued += 1;
        }
        let best = p[1] as u32;
        let category = if bits & (1 << best) != 0 {
            0
        } else if bits != 0 {
            1
        } else {
            2
        };
        counts[category] += 1;
        squared[category] += (p[3] as f64).powi(2);
        maximum[category] = maximum[category].max(p[3].abs() as f64);
        csv.push_str(&format!(
            "{i},{},{},{bits},{best},{wider_bits}\n",
            p[2], p[3]
        ));
    }
    for i in 0..3 {
        eprintln!(
            "height category {i} count={} rms_kappa={} max_abs_kappa={}",
            counts[i],
            (squared[i] / counts[i].max(1) as f64).sqrt(),
            maximum[i]
        );
    }
    eprintln!("Additional cells with complete eleven-cell columns: {wider_rescued}");
    std::fs::write(format!("artifacts/height-coverage-{n}.csv"), csv).unwrap();
    assert!(counts.iter().sum::<usize>() > 0);
}

#[test]
fn extended_height_columns_keep_common_origin_in_both_orientations() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut surface, _) = super::tests::fixture(16);
    super::install_test_entry(
        &mut surface,
        "height_origin",
        r#"
        @compute @workgroup_size(256)
        fn height_origin(@builtin(global_invocation_id) g:vec3<u32>){
          let i=flat(g);if i>=9u{return;}
          let u=i32(i%3u)-1;let v=i32(i/3u)-1;
          let q=vec3<i32>(8,8+u,8+v);
          stats[i]=vec4<f32>(height(q,0u,q),height_range(q,0u,3,q).y);
        }"#,
    );
    for inverse in [false, true] {
        let values: Vec<[f32; 4]> = (0..16u32.pow(3))
            .map(|i| {
                let sum = (i % 16 + (i / 16) % 16 + i / 256) as f64;
                let t = 26.3 - sum;
                // Exact CDF for the sum of three unit uniform coordinates.
                let positive_cube = |v: f64| v.max(0.).powi(3);
                let a = ((positive_cube(t) - 3. * positive_cube(t - 1.)
                    + 3. * positive_cube(t - 2.)
                    - positive_cube(t - 3.))
                    / 6.)
                    .clamp(0., 1.);
                [if inverse { 1. - a as f32 } else { a as f32 }, 0., 0., 0.]
            })
            .collect();
        gpu.context
            .queue
            .write_buffer(surface.field(), 0, bytemuck::cast_slice(&values));
        surface.dispatch(&volume, None, None, &["height_origin"]);
        let bytes = read_bytes(&gpu.context, &surface.stats, 0, 9 * 16).unwrap();
        let mut short_valid = 0;
        for (i, h) in bytemuck::cast_slice::<u8, [f32; 4]>(&bytes)
            .iter()
            .enumerate()
        {
            let u = (i % 3) as f32 - 1.;
            let v = (i / 3) as f32 - 1.;
            let expected = 1.3 - u - v;
            let expected = if inverse { 1. - expected } else { expected };
            assert!(h[1] > 0.5, "Incomplete column {i}: {h:?}");
            assert_eq!(h[2], if inverse { -1. } else { 1. });
            assert!(
                (h[0] - expected).abs() < 2e-5,
                "Column {i}: {h:?} expected {expected}"
            );
            short_valid += usize::from(h[3] > 0.5);
        }
        assert!(
            short_valid > 0 && short_valid < 9,
            "Need a mixture of original and extended columns"
        );
    }
}
