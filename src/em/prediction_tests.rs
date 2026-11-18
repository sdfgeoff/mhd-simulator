//! Test-only extrapolated-increment controls.
use super::*;
#[test]
#[ignore = "Headless coupled-project EM increment research; writes final fields"]
fn predicted_em_coupled_project_probe() {
    let _lock = crate::gpu_test_lock();
    let path = std::env::var("MHD_RESEARCH_PROJECT")
        .unwrap_or_else(|_| "assets/hotend-free-surface.mhd.json".into());
    let p = crate::project::load(std::path::Path::new(&path)).unwrap();
    let gpu = crate::domain::GpuDomain::new().unwrap();
    gpu.context
        .profiler
        .set_enabled(std::env::var_os("MHD_RESEARCH_PROFILE").is_some());
    let volume = gpu.build(&p.model, &p.settings, &p.materials).unwrap();
    let mut run = crate::run::RunState::new(
        gpu.context.clone(),
        &volume,
        &p.model,
        p.run,
        p.electromagnetics,
        p.fluid,
    )
    .unwrap();
    run.em.as_mut().unwrap().use_nodal_preconditioner = std::env::var_os("MHD_EM_NODAL").is_some();
    run.em.as_mut().unwrap().nodal_switch_after = std::env::var("MHD_EM_NODAL_AFTER")
        .unwrap_or_else(|_| "1024".into())
        .parse()
        .unwrap();
    let predict = std::env::var_os("MHD_EM_PREDICT").is_some();
    run.em
        .as_mut()
        .expect("Probe requires electromagnetics")
        .predict_increments = predict;
    eprintln!("Coupled EM prediction: {predict}");
    let ticks: u32 = std::env::var("MHD_RESEARCH_TICKS")
        .unwrap_or_else(|_| "3".into())
        .parse()
        .unwrap();
    assert!((1..=10000).contains(&ticks));
    let output = std::env::var("MHD_RESEARCH_OUTPUT")
        .unwrap_or_else(|_| format!("artifacts/em-prediction/combined-{predict}"));
    let out = std::path::Path::new(&output);
    std::fs::create_dir_all(out).unwrap();
    let mut records = Vec::new();
    for step in 0..=ticks {
        if step > 0 {
            run.step(&p.model, &volume)
                .unwrap_or_else(|e| panic!("Combined tick {step}: {e}"));
        }
        let fluid = run.fluid.as_ref().unwrap();
        let record = serde_json::json!({"step":step,"time_s":run.time_s,"max_u_m_s":fluid.diagnostics.max_speed_m_s,"kinetic_j":fluid.diagnostics.kinetic_energy_j,"pressure_iterations":fluid.diagnostics.iterations,"pressure_relative_residual":fluid.diagnostics.relative_residual,"rms_divergence_s_inv":fluid.diagnostics.rms_divergence_s_inv,"max_b_t":run.em.as_ref().map(|e|e.diagnostics.maxima[0]),"em_iterations":run.em.as_ref().map(|e|e.diagnostics.iterations),"em_relative_residual":run.em.as_ref().map(|e|e.diagnostics.relative_residual),"prediction":run.em.as_ref().map(|e|e.prediction_diagnostic),"thermal_j":run.energy.thermal_j,"surface_j":run.energy.surface_j,"working_bytes":run.working_memory(&volume).total(),"surface_volume_m3":run.surface.as_ref().map(|s|s.diagnostics.volume_m3)});
        eprintln!("{record}");
        records.push(record);
        std::fs::write(
            out.join("diagnostics.json"),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
    }
    let fluid = run.fluid.as_ref().unwrap();
    let bytes = read_bytes(
        &gpu.context,
        fluid.result_buffer(),
        0,
        fluid.result_buffer().size(),
    )
    .unwrap();
    std::fs::write(out.join("fluid-final.bin"), bytes).unwrap();
    if let Some(surface) = &run.surface {
        let bytes = read_bytes(&gpu.context, surface.field(), 0, surface.field().size()).unwrap();
        std::fs::write(out.join("surface-final.bin"), bytes).unwrap();
    }
    if let Some(em) = &run.em {
        let bytes = read_bytes(
            &gpu.context,
            em.result_buffer(),
            0,
            em.result_buffer().size(),
        )
        .unwrap();
        std::fs::write(out.join("em-final.bin"), bytes).unwrap();
    }
    let reports = gpu.context.profiler.take_completed(true);
    let mut csv = String::from(
        "step,time_s,accepted,stage,cpu_wall_ms,gpu_pass_ms,recorded_passes,total_passes\n",
    );
    for report in reports {
        csv.push_str(&report.csv_rows());
    }
    std::fs::write(out.join("performance.csv"), csv).unwrap();
}

fn pair() -> (crate::domain::GpuDomain, Volume, Model, EmState, EmState) {
    let (gpu, volume, model) = super::tests::setup(16);
    let mut config = EmConfig {
        max_iterations: 2000,
        ..Default::default()
    };
    config.uniform.push(UniformField {
        tesla: [0., 0., 0.001],
        scale: Waveform::Sine {
            amplitude: 1.,
            frequency_hz: 2000.,
            phase_rad: 0.,
        },
    });
    let mut a = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config.clone(),
        Mode::Induction,
    )
    .unwrap();
    let mut b = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config,
        Mode::Induction,
    )
    .unwrap();
    a.predict_increments = false;
    b.predict_increments = false;
    a.use_nodal_preconditioner = false;
    b.use_nodal_preconditioner = false;
    (gpu, volume, model, a, b)
}
fn fields(gpu: &crate::domain::GpuDomain, state: &EmState) -> Vec<u8> {
    read_bytes(
        &gpu.context,
        state.result_buffer(),
        0,
        state.result_buffer().size(),
    )
    .unwrap()
}
fn compare_fields(gpu: &crate::domain::GpuDomain, a: &EmState, b: &EmState) {
    let aa = fields(gpu, a);
    let bb = fields(gpu, b);
    let a = bytemuck::cast_slice::<u8, CellField>(&aa);
    let b = bytemuck::cast_slice::<u8, CellField>(&bb);
    for channel in 0..4 {
        let mut error = 0.;
        let mut norm = 0.;
        for (a, b) in a.iter().zip(b) {
            let x = match channel {
                0 => a.b,
                1 => a.j,
                2 => [a.force_heat[0], a.force_heat[1], a.force_heat[2], 0.],
                _ => [a.force_heat[3], 0., 0., 0.],
            };
            let y = match channel {
                0 => b.b,
                1 => b.j,
                2 => [b.force_heat[0], b.force_heat[1], b.force_heat[2], 0.],
                _ => [b.force_heat[3], 0., 0., 0.],
            };
            for (&x, &y) in x.iter().zip(&y) {
                error += (x as f64 - y as f64).powi(2);
                norm += (x as f64).powi(2);
            }
        }
        let relative = (error / norm.max(1e-38)).sqrt();
        eprintln!("Prediction field channel {channel}: relative L2={relative:e}");
        assert!(relative < 2e-4);
    }
}
#[test]
fn predicted_em_bad_history_falls_back_to_identical_zero_guess_fields() {
    let _lock = crate::gpu_test_lock();
    for poison in [1e-3f32, 1e20f32] {
        let (gpu, volume, model, mut original, mut predicted) = pair();
        for _ in 0..2 {
            original.step(&model, &volume).unwrap();
            predicted.step(&model, &volume).unwrap();
        }
        assert_eq!(fields(&gpu, &original), fields(&gpu, &predicted));
        predicted.predict_increments = true;
        let history = vec![poison; predicted.params.edges[3] as usize];
        gpu.context
            .queue
            .write_buffer(&predicted._previous, 0, bytemuck::cast_slice(&history));
        original.step(&model, &volume).unwrap();
        predicted.step(&model, &volume).unwrap();
        assert_eq!(predicted.prediction_diagnostic[2], 1.);
        assert_eq!(fields(&gpu, &original), fields(&gpu, &predicted));
    }
}
#[test]
fn predicted_em_tracks_timestep_changes_with_the_original_rhs_tolerance() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model, mut original, mut predicted) = pair();
    predicted.predict_increments = true;
    let mut previous_dt = 0.;
    for dt in [1e-5, 1e-5, 0.5e-5, 0.5e-5, 2e-5, 1e-5] {
        original.config.dt_s = dt;
        predicted.config.dt_s = dt;
        original.step(&model, &volume).unwrap();
        predicted.step(&model, &volume).unwrap();
        let expected = if previous_dt > 0. {
            dt / previous_dt
        } else {
            0.
        };
        assert!((predicted.prediction_diagnostic[0] as f64 - expected).abs() < 1e-6);
        assert!(predicted.diagnostics.relative_residual <= predicted.config.relative_tolerance);
        compare_fields(&gpu, &original, &predicted);
        previous_dt = dt;
    }
}
#[test]
fn predicted_em_failure_restores_fields_and_retry_uses_valid_history() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model, mut original, mut predicted) = pair();
    for _ in 0..2 {
        original.step(&model, &volume).unwrap();
        predicted.step(&model, &volume).unwrap();
    }
    predicted.predict_increments = true;
    predicted.config.max_iterations = 1;
    let accepted = fields(&gpu, &predicted);
    let time = predicted.time_s;
    let dt = predicted.last_step_dt_s;
    let error = predicted.step(&model, &volume).unwrap_err();
    assert!(error.contains("did not converge"));
    assert_eq!(predicted.time_s, time);
    assert_eq!(predicted.last_step_dt_s, dt);
    assert_eq!(fields(&gpu, &predicted), accepted);
    predicted.config.max_iterations = 2000;
    original.step(&model, &volume).unwrap();
    predicted.step(&model, &volume).unwrap();
    assert_eq!(original.time_s, predicted.time_s);
    compare_fields(&gpu, &original, &predicted);
}

#[test]
fn em_weak_drive_preserves_linear_fields_and_quadratic_force_heat() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model) = super::tests::setup(16);
    let mut states = Vec::new();
    for amplitude in [1., 1e-8] {
        let config = EmConfig {
            dt_s: 1e-5,
            relative_tolerance: 1e-4,
            max_iterations: 4000,
            uniform: vec![UniformField {
                tesla: [0., 0., 0.001],
                scale: Waveform::Sine {
                    amplitude,
                    frequency_hz: 2000.,
                    phase_rad: 0.,
                },
            }],
            ..Default::default()
        };
        let mut state = EmState::new(
            gpu.context.clone(),
            &volume,
            &model,
            config,
            Mode::Induction,
        )
        .unwrap();
        for tick in 1..=4 {
            state
                .step(&model, &volume)
                .unwrap_or_else(|error| panic!("amplitude {amplitude}, tick {tick}: {error}"));
            let status = state.read_f32(&state.scalars, 0, 4).unwrap();
            let scale = state.read_f32(&state.scalars, 96, 1).unwrap()[0] as f64;
            let r2 = status[1] as f64 * scale.powi(2);
            let b2 = status[2] as f64 * scale.powi(2);
            let threshold = b2 * (state.config.relative_tolerance as f64).powi(2) + 1e-38;
            assert!(
                r2 <= threshold * 1.00001,
                "physical r²={r2}, b²={b2}, bound={threshold}"
            );
            eprintln!(
                "amplitude={amplitude}, tick={tick}, relative={}, physical r²={r2}, bound={threshold}",
                state.diagnostics.relative_residual
            );
        }
        states.push(fields(&gpu, &state));
    }
    let reference = bytemuck::cast_slice::<u8, CellField>(&states[0]);
    let weak = bytemuck::cast_slice::<u8, CellField>(&states[1]);
    for (channel, name) in ["B", "J", "force", "heat", "applied B"]
        .into_iter()
        .enumerate()
    {
        let amplitude_scale = if channel == 2 || channel == 3 {
            1e-16
        } else {
            1e-8
        };
        let mut squared_error = 0.;
        let mut squared_norm = 0.;
        for (a, b) in reference.iter().zip(weak) {
            let (a, b) = match channel {
                0 => (a.b, b.b),
                1 => (a.j, b.j),
                2 | 3 => (a.force_heat, b.force_heat),
                _ => (a.applied, b.applied),
            };
            // Keep force density and heating power density in separate norms.
            let components = if channel == 3 { 3..4 } else { 0..3 };
            for k in components {
                let x = a[k] as f64;
                let y = b[k] as f64 / amplitude_scale;
                squared_error += (x - y).powi(2);
                squared_norm += x * x;
            }
        }
        let relative = (squared_error / squared_norm.max(1e-300)).sqrt();
        eprintln!("weak-drive {name}: relative L2={relative}");
        assert!(relative < 2e-4, "{name}: relative error {relative}");
    }
}

#[test]
fn held_sources_match_recomputation_across_plateaus_changes_and_failed_retry() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut model) = super::tests::setup(16);
    for z in [-2., 2.] {
        model.curves.push(crate::model::CurvePath {
            name: format!("Loop {z}"),
            points_mm: vec![
                [-5., -5., z],
                [5., -5., z],
                [5., 5., z],
                [-5., 5., z],
                [-5., -5., z],
            ],
        });
    }
    let mut config = EmConfig {
        max_iterations: 4000,
        ..Default::default()
    };
    config.wires = cad_wires(&model);
    config.wires[0].current = Waveform::Table {
        samples: vec![[0., 0.], [1e-5, 10.], [3e-5, 10.], [4e-5, 0.]],
    };
    config.wires[1].current = Waveform::Constant { value: -4. };
    config.uniform.push(UniformField {
        tesla: [0., 0., 0.001],
        scale: Waveform::Constant { value: 1. },
    });
    let mut reference = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config.clone(),
        Mode::Induction,
    )
    .unwrap();
    let mut cached = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config,
        Mode::Induction,
    )
    .unwrap();
    reference.cache_source_fields = false;
    reference.predict_increments = false;
    cached.predict_increments = false;
    for tick in 1..=5 {
        reference.step(&model, &volume).unwrap();
        if tick == 4 {
            let before = fields(&gpu, &cached);
            let time = cached.time_s;
            cached.config.max_iterations = 1;
            assert!(cached.step(&model, &volume).is_err());
            assert_eq!(cached.time_s, time);
            assert!(!cached.source_valid);
            assert_eq!(fields(&gpu, &cached), before);
            cached.config.max_iterations = 4000;
        }
        cached.step(&model, &volume).unwrap();
        assert_eq!(
            cached.source_reused,
            matches!(tick, 2 | 3 | 5),
            "tick {tick}"
        );
        assert_eq!(cached.time_s, reference.time_s);
        assert_eq!(
            fields(&gpu, &cached),
            fields(&gpu, &reference),
            "fields at tick {tick}"
        );
        let a = read_bytes(&gpu.context, &cached._source, 0, cached._source.size()).unwrap();
        let b = read_bytes(
            &gpu.context,
            &reference._source,
            0,
            reference._source.size(),
        )
        .unwrap();
        assert_eq!(
            a, b,
            "source/history/coefficient/motion packet at tick {tick}"
        );
        let a = cached.energy_step().unwrap();
        let b = reference.energy_step().unwrap();
        assert_eq!(
            [
                a.magnetic_j,
                a.delta_magnetic_j,
                a.joule_j,
                a.external_j,
                a.backward_euler_j,
                a.balance_error_j
            ],
            [
                b.magnetic_j,
                b.delta_magnetic_j,
                b.joule_j,
                b.external_j,
                b.backward_euler_j,
                b.balance_error_j
            ]
        );
    }
}

#[test]
fn nodal_gradient_correction_matches_original_em_fields() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model, mut reference, mut corrected) = pair();
    corrected.use_nodal_preconditioner = true;
    corrected.nodal_switch_after = 0;
    for _ in 0..4 {
        reference.step(&model, &volume).unwrap();
        corrected.step(&model, &volume).unwrap();
        compare_fields(&gpu, &reference, &corrected);
    }
}

#[test]
fn nodal_gradient_correction_is_symmetric_positive() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model, _, mut state) = pair();
    state.use_nodal_preconditioner = true;
    state.nodal_switch_after = 0;
    state.step(&model, &volume).unwrap();
    let n = state.params.edges[3] as usize;
    let r: Vec<f32> = (0..n).map(|i| ((i * 17 % 31) as f32 - 15.) / 16.).collect();
    let s: Vec<f32> = (0..n).map(|i| ((i * 29 % 37) as f32 - 18.) / 19.).collect();
    let apply = |v: &[f32]| {
        let data: Vec<[f32; 4]> = v.iter().map(|r| [0., *r, 0., 0.]).collect();
        gpu.context
            .queue
            .write_buffer(&state._work, 0, bytemuck::cast_slice(&data));
        let mut commands = Vec::new();
        state.append_nodal(&mut commands, true);
        state.dispatch(&commands);
        let result = state.read_f32(&state._work, 0, n * 4).unwrap();
        (0..n).map(|i| result[i * 4 + 3] as f64).collect::<Vec<_>>()
    };
    let br = apply(&r);
    let bs = apply(&s);
    let dot = |a: &[f32], b: &[f64]| a.iter().zip(b).map(|(a, b)| *a as f64 * b).sum::<f64>();
    let rbr = dot(&r, &br);
    let sbs = dot(&s, &bs);
    assert!(rbr > 0. && sbs > 0.);
    let symmetry = (dot(&r, &bs) - dot(&s, &br)).abs() / (rbr * sbs).sqrt();
    eprintln!("nodal preconditioner: rBr={rbr}, sBs={sbs}, normalized symmetry error={symmetry}");
    assert!(symmetry < 2e-6);
}

#[test]
fn nodal_correction_preserves_failed_state_and_timestep_changes() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model, mut reference, mut corrected) = pair();
    corrected.use_nodal_preconditioner = true;
    corrected.nodal_switch_after = 1;
    corrected.predict_increments = true;
    // Reject workspace indexing after a real Jacobi iteration without making
    // an enormous allocation; rollback must also preserve source history.
    let accepted = fields(&gpu, &corrected);
    let dims = corrected.grid.dims;
    corrected.grid.dims = [100_000; 3];
    let error = corrected
        .advance_to(&model, corrected.config.dt_s, false)
        .unwrap_err();
    corrected.grid.dims = dims;
    assert!(error.contains("Nodal indexing"));
    assert_eq!(corrected.time_s, 0.);
    assert!(!corrected.source_valid);
    assert_eq!(fields(&gpu, &corrected), accepted);
    for dt in [1e-5, 0.5e-5, 2e-5] {
        reference.config.dt_s = dt;
        corrected.config.dt_s = dt;
        reference.step(&model, &volume).unwrap();
        corrected.step(&model, &volume).unwrap();
        assert_eq!(corrected.params.control[3], 1);
        assert!(corrected.diagnostics.iterations <= corrected.config.max_iterations);
        compare_fields(&gpu, &reference, &corrected);
    }
    let accepted = fields(&gpu, &corrected);
    let time = corrected.time_s;
    corrected.config.max_iterations = 1;
    assert!(corrected.step(&model, &volume).is_err());
    assert_eq!(corrected.time_s, time);
    assert_eq!(fields(&gpu, &corrected), accepted);
    corrected.config.max_iterations = 2000;
    reference.step(&model, &volume).unwrap();
    corrected.step(&model, &volume).unwrap();
    compare_fields(&gpu, &reference, &corrected);
}

#[test]
fn nodal_fallback_keeps_short_solves_byte_identical_without_allocation() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, model, mut reference, mut corrected) = pair();
    corrected.use_nodal_preconditioner = true;
    for _ in 0..4 {
        reference.step(&model, &volume).unwrap();
        corrected.step(&model, &volume).unwrap();
        assert!(corrected.nodal.is_none());
        assert_eq!(reference.bytes, corrected.bytes);
        assert_eq!(fields(&gpu, &reference), fields(&gpu, &corrected));
        assert_eq!(
            reference.diagnostics.iterations,
            corrected.diagnostics.iterations
        );
    }
}
