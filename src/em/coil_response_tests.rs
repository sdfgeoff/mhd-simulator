use super::*;

#[test]
fn reciprocal_coil_work_matches_heat_and_reaction_energy_without_changing_fields() {
    let _lock = crate::gpu_test_lock();
    let (gpu, volume, mut model) = tests::setup(16);
    model.curves.push(crate::model::CurvePath {
        name: "Closed winding".into(),
        points_mm: vec![
            [-5., -5., 0.],
            [5., -5., 0.],
            [5., 5., 0.],
            [-5., 5., 0.],
            [-5., -5., 0.],
        ],
    });
    let mut config = EmConfig {
        dt_s: 1e-5,
        relative_tolerance: 1e-4,
        max_iterations: 4000,
        ..Default::default()
    };
    config.wires = cad_wires(&model);
    config.wires[0].current = Waveform::Pulse {
        peak: 25.,
        delay_s: 0.,
        rise_s: 3e-5,
        hold_s: 2e-5,
        fall_s: 3e-5,
    };
    let wave = config.wires[0].current.clone();
    let mut observed = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config.clone(),
        Mode::Induction,
    )
    .unwrap();
    let mut reference = EmState::new(
        gpu.context.clone(),
        &volume,
        &model,
        config,
        Mode::Induction,
    )
    .unwrap();
    let mut current = 0.;
    let mut source_work = 0.;
    let mut heat = 0.;
    let mut numerical_loss = 0.;
    let mut reaction = 0.;
    for step in 1..=12 {
        observed.step(&model, &volume).unwrap();
        reference.step(&model, &volume).unwrap();
        let response = observed.coil_response(&volume).unwrap();
        let new_current = f64::from(wave.value(observed.time_s));
        source_work -= response.reaction_linkage_wb.unwrap_or(0.) * (new_current - current);
        current = new_current;
        assert!(
            response.reaction_linkage_wb.is_some(),
            "Linkage unavailable after zero-current interval at step {step}"
        );
        heat += observed.energy_step().unwrap().joule_j;
        numerical_loss += response.reaction_backward_euler_j;
        reaction = response.reaction_magnetic_j;
        let a = read_bytes(
            &gpu.context,
            observed.result_buffer(),
            0,
            observed.result_buffer().size(),
        )
        .unwrap();
        let b = read_bytes(
            &gpu.context,
            reference.result_buffer(),
            0,
            reference.result_buffer().size(),
        )
        .unwrap();
        assert_eq!(a, b, "Observer changes fields at step {step}");
    }
    assert_eq!(current, 0.);
    assert!(heat > 0.);
    let error =
        (source_work - heat - numerical_loss - reaction).abs() / (heat + numerical_loss + reaction);
    eprintln!(
        "Reciprocal coil work={source_work:e} J, heat={heat:e}, reaction BE={numerical_loss:e}, final reaction={reaction:e}; relative defect={error:e}"
    );
    assert!(error < 5e-4);
}
