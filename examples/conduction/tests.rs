use super::conduction::Conduction;
use mhd_workbench::{em::CellField, gpu::GpuContext};
use wgpu::util::DeviceExt;

#[test]
fn potential_predictors_reuse_only_solutions_that_pass_the_current_equation() {
    let context = GpuContext::headless().unwrap();
    let dims = [24, 6, 4];
    let n = dims.iter().product::<u32>() as usize;
    let bias = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Predictor bias"),
            contents: bytemuck::cast_slice(&vec![
                CellField {
                    b: [0., -0.5, 0., 0.],
                    ..Default::default()
                };
                n
            ]),
            usage: wgpu::BufferUsages::STORAGE,
        });
    let mut phase = vec![
        CellField {
            applied: [0., 0., 1., 0.],
            ..Default::default()
        };
        n
    ];
    let flow = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Changing phase and velocity"),
            contents: bytemuck::cast_slice(&phase),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
    let mut cells = vec![[1., 3.]; n];
    for (i, c) in cells.iter_mut().enumerate() {
        let x = i as u32 % dims[0];
        if x == 0 {
            c[1] = 0.5;
        }
        if x == dims[0] - 1 {
            c[1] = -0.5;
        }
    }
    let mut warm = Conduction::new(context.clone(), dims, 1e-4, &cells, &bias).unwrap();
    warm.attach_flow(&flow).unwrap();
    warm.solve_current(40.).unwrap();
    warm.solve_current(40.).unwrap();
    assert_eq!(
        warm.iterations, 0,
        "Unchanged accepted potentials should need no new PCG iterations"
    );
    for (i, f) in phase.iter_mut().enumerate() {
        let x = (i as u32 % dims[0]) as f32;
        let y = (i as u32 / dims[0] % dims[1]) as f32;
        f.b[2] = 0.7 * (x * 0.27).sin() * (y * 0.6).cos();
        f.applied[2] = 0.6 + 0.3 * (x * 0.19 + y * 0.23).cos();
    }
    context
        .queue
        .write_buffer(&flow, 0, bytemuck::cast_slice(&phase));
    let (warm_v, _) = warm.solve_current(0.).unwrap();
    let warm_fields = warm.read_fields().unwrap();
    let mut cold = Conduction::new(context.clone(), dims, 1e-4, &cells, &bias).unwrap();
    cold.attach_flow(&flow).unwrap();
    let (cold_v, _) = cold.solve_current(0.).unwrap();
    let cold_fields = cold.read_fields().unwrap();
    let norm: f64 = cold_fields
        .iter()
        .flat_map(|f| f.j[..3].iter())
        .map(|&v| (v as f64).powi(2))
        .sum();
    let delta: f64 = warm_fields
        .iter()
        .zip(cold_fields)
        .flat_map(|(a, b)| (0..3).map(move |k| (a.j[k] as f64 - b.j[k] as f64).powi(2)))
        .sum();
    assert!(
        (delta / norm).sqrt() < 1e-3,
        "Predictor altered the physical current: {}",
        (delta / norm).sqrt()
    );
    assert!((warm_v - cold_v).abs() < 1e-7);
    assert!(warm.relative_residual <= 1e-5 && cold.relative_residual <= 1e-5);
}
#[test]
fn uniform_and_parallel_conductors_obey_ohms_law_and_power_balance() {
    let context = GpuContext::headless().unwrap();
    let dims = [32, 4, 4];
    let h = 1e-4;
    let n = dims.iter().product::<u32>() as usize;
    let bias = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Test static field"),
            contents: bytemuck::cast_slice(&vec![
                CellField {
                    b: [0., -0.5, 0., 0.],
                    ..Default::default()
                };
                n
            ]),
            usage: wgpu::BufferUsages::STORAGE,
        });
    for parallel in [false, true] {
        let mut cells = vec![[1., 2.]; n];
        for (i, c) in cells.iter_mut().enumerate() {
            let x = i as u32 % dims[0];
            let y = (i as u32 / dims[0]) % dims[1];
            if parallel && y >= 2 {
                c[0] = 2.;
            }
            if x == 0 {
                c[1] = 0.5;
            }
            if x == 31 {
                c[1] = -0.5;
            }
        }
        let mut solver = Conduction::new(context.clone(), dims, h, &cells, &bias).unwrap();
        solver.solve().unwrap();
        let s = solver.fields(1.).unwrap();
        let mean_sigma = if parallel { 1.5e6 } else { 1e6 };
        let exact = mean_sigma * (dims[1] * dims[2]) as f32 * h / (dims[0] - 1) as f32;
        assert!(
            (s[0] / exact - 1.).abs() < 5e-5,
            "current {} exact {}",
            s[0],
            exact
        );
        assert!((s[1] / s[0] + 1.).abs() < 5e-5);
        assert!(
            (s[2] / s[0] - 1.).abs() < 5e-5,
            "power {} current {}",
            s[2],
            s[0]
        );
        let fields = solver.read_fields().unwrap();
        if parallel {
            let low = fields[16].j[0];
            let high = fields[16 + 32 * 2].j[0];
            assert!((high / low - 2.).abs() < 1e-4);
        }
        assert!(fields[16].force_heat[2] < 0.);
    }
}
#[test]
fn contact_resistance_and_motional_emf_preserve_power_balance() {
    let context = GpuContext::headless().unwrap();
    let dims = [32, 4, 4];
    let h = 1e-4;
    let n = dims.iter().product::<u32>() as usize;
    let make_buffer = |label, values: &[CellField]| {
        context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(values),
                usage: wgpu::BufferUsages::STORAGE,
            })
    };
    let bias = make_buffer(
        "Contact/motion test bias",
        &vec![
            CellField {
                b: [0., -0.5, 0., 0.],
                ..Default::default()
            };
            n
        ],
    );
    let mut cells = vec![[1., 3.]; n];
    for (i, c) in cells.iter_mut().enumerate() {
        let x = i as u32 % dims[0];
        if x == 0 {
            c[1] = 0.5;
        }
        if x == 31 {
            c[1] = -0.5;
        }
    }
    let mut solver = Conduction::new(context.clone(), dims, h, &cells, &bias).unwrap();
    let contact = 1e-9;
    solver.set_contact_resistivity(contact).unwrap();
    solver.solve().unwrap();
    let area = (dims[1] * dims[2]) as f32 * h * h;
    let length = (dims[0] - 1) as f32 * h;
    let resistance = length / (1e6 * area) + 2. * contact / area;
    let s = solver.fields(1.).unwrap();
    assert!((s[0] * resistance - 1.).abs() < 5e-5);
    assert!((s[2] / s[0] - 1.).abs() < 5e-5);
    let flow = make_buffer(
        "Uniform translating conductor",
        &vec![
            CellField {
                b: [0., 0., -1., 0.],
                applied: [0., 0., 1., 0.],
                ..Default::default()
            };
            n
        ],
    );
    solver.attach_flow(&flow).unwrap();
    let (voltage, s) = solver.solve_current(0.).unwrap();
    assert!(
        (voltage / (0.5 * length) - 1.).abs() < 2e-4,
        "open-circuit motional voltage {voltage}"
    );
    assert!(
        s[2] < 1e-8,
        "Rigid translation at open circuit must not generate Ohmic heat: {}",
        s[2]
    );
    let (voltage, s) = solver.solve_current(100.).unwrap();
    let expected = 100. * resistance + 0.5 * length;
    assert!((voltage / expected - 1.).abs() < 1e-4);
    assert!((s[0] / 100. - 1.).abs() < 5e-5);
    assert!(
        ((voltage * s[0] - s[2] - s[3]) / (voltage * s[0])).abs() < 5e-5,
        "DC source/Joule/mechanical balance {s:?}"
    );
}
