use super::*;
use crate::simulation::{self, Material};
/// Explicit illustrative setup for the bundled hotend, never inferred for arbitrary CAD.
pub fn hotend_example(model: &mut Model) -> Result<(Vec<Material>, EmConfig), String> {
    if model.bodies.len() != 5
        || !model.bodies[0].name.starts_with("Chamber")
        || !model.bodies[1].name.starts_with("Molten")
        || !model.bodies[2].name.starts_with("Coil")
    {
        return Err("The example inputs apply to the bundled hotend fixture; assign properties for this CAD explicitly".into());
    }
    let mut materials = simulation::default_materials();
    materials[1].name = "Nonconducting envelope (example)".into();
    materials[1].conductivity_s_m = Some(0.);
    materials[1].relative_permeability = Some(1.);
    materials[1].notes="Coil/magnet display envelopes are nonconducting in this example. Prescribed wire paths supply current. Assign measured properties to simulate conductive magnet bodies.".into();
    materials[3].conductivity_s_m = Some(4e6);
    materials[3].relative_permeability = Some(1.);
    materials[3].notes="Illustrative molten-solder conductivity 4e6 S/m, not calibrated operating-temperature data.".into();
    materials[6].relative_permeability = Some(1.);
    for (i, b) in model.bodies.iter_mut().enumerate() {
        b.simulation.material_id = match i {
            0 => 6,
            1 => 3,
            _ => 1,
        };
    }
    model.bodies[2].simulation.current_a = Some(100.);
    let (lo, hi) = model.bounds();
    let mut wire = helix(
        ((lo + hi) * 0.5).to_array(),
        5.,
        8.,
        4,
        Waveform::Pulse {
            peak: 1.,
            delay_s: 0.,
            rise_s: 50e-6,
            hold_s: 50e-6,
            fall_s: 50e-6,
        },
    );
    wire.body_current = Some(2);
    Ok((
        materials,
        EmConfig {
            wires: vec![wire],
            ..Default::default()
        },
    ))
}
