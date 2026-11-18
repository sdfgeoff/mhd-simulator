//! Steady heater-to-isothermal-pool conductance using the existing GPU FV operator.
//! No transient, latent heat, air loss, thread-contact resistance or CPU PDE solver.
#[path = "conduction/gpu.rs"]
pub mod conduction;
use mhd_workbench::{
    domain::GpuDomain,
    gpu::{GpuContext, read_bytes},
    project,
};
use std::{error::Error, path::Path};

fn solve(
    context: GpuContext,
    dims: [u32; 3],
    h: f32,
    cells: &[[f32; 2]],
    tol: f32,
) -> Result<[f32; 4], String> {
    let zero = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("No magnetic or flow field in thermal conduction analogy"),
        size: cells.len() as u64 * 64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let mut solver = conduction::Conduction::new(context, dims, h, cells, &zero)?;
    solver.set_relative_tolerance(tol)?;
    solver.solve()?;
    solver.fields(1.)
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let gpu = GpuDomain::new()?;
    if args.get(1).is_some_and(|x| x == "--check") {
        for k in [15_f32, 30.] {
            let cells: Vec<_> = (0..16)
                .map(|i| {
                    [
                        k / 1e6,
                        if i == 0 {
                            0.5
                        } else if i == 15 {
                            -0.5
                        } else {
                            2.
                        },
                    ]
                })
                .collect();
            let stats = solve(gpu.context.clone(), [16, 1, 1], 0.001, &cells, 1e-6)?;
            let reference = k * 1e-6 / 0.015;
            assert!((stats[0] / reference - 1.).abs() < 2e-5, "{stats:?}");
            assert!((stats[2] / stats[0] - 1.).abs() < 2e-5);
            println!(
                "k={k} W/mK: GPU conductance={} W/K; analytic={reference}",
                stats[0]
            );
        }
        return Ok(());
    }
    let p = project::load(Path::new(
        args.get(1)
            .ok_or("PROJECT HEATER_START_MM OUTPUT [STEEL_K] [TOLERANCE]")?,
    ))?;
    let heater_start: f32 = args.get(2).ok_or("Heater start")?.parse()?;
    let output = Path::new(args.get(3).ok_or("Output")?);
    let steel_k: f32 = args.get(4).map(|x| x.parse()).transpose()?.unwrap_or(15.);
    let tolerance: f32 = args.get(5).map(|x| x.parse()).transpose()?.unwrap_or(1e-6);
    if !heater_start.is_finite() || !steel_k.is_finite() || steel_k <= 0. {
        return Err("Invalid thermal controls".into());
    }
    let volume = gpu.build(&p.model, &p.settings, &p.materials)?;
    if volume.overlaps > 0 {
        return Err("Overlapping CAD".into());
    }
    let grid = volume.inputs.snapshot().grid.clone();
    let bytes = read_bytes(
        &gpu.context,
        volume.material_ids(),
        0,
        volume.material_ids().size(),
    )?;
    let ids = bytemuck::cast_slice::<u8, u32>(&bytes);
    let liquid_k = p
        .materials
        .iter()
        .find(|m| m.id == 4)
        .ok_or("No liquid")?
        .thermal
        .conductivity_w_m_k
        .ok_or("Liquid k missing")?;
    let mut hot = 0;
    let mut cold = 0;
    let cells: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let x = grid.origin_m[0]
                + (i as u32 % grid.dims[0]) as f32 * grid.cell_m
                + 0.5 * grid.cell_m;
            let y = grid.origin_m[1]
                + ((i as u32 / grid.dims[0]) % grid.dims[1]) as f32 * grid.cell_m
                + 0.5 * grid.cell_m;
            let z = grid.origin_m[2]
                + (i as u32 / (grid.dims[0] * grid.dims[1])) as f32 * grid.cell_m
                + 0.5 * grid.cell_m;
            match id {
                14 => {
                    let heated = z >= heater_start * 0.001
                        && z <= (heater_start + 5.4) * 0.001
                        && (x * x + y * y).sqrt() >= (3e-3 - grid.cell_m);
                    if heated {
                        hot += 1;
                    }
                    [steel_k / 1e6, if heated { 0.5 } else { 2. }]
                }
                4 => {
                    cold += 1;
                    [liquid_k / 1e6, -0.5]
                }
                _ => [0., 2.],
            }
        })
        .collect();
    if hot == 0 || cold == 0 {
        return Err("Missing heater or pool boundary".into());
    }
    let stats = solve(
        gpu.context.clone(),
        grid.dims,
        grid.cell_m,
        &cells,
        tolerance,
    )?;
    let record = serde_json::json!({"project":args[1],"heater_start_mm":heater_start,"steel_k_W_m_K":steel_k,"liquid_k_W_m_K":liquid_k,"hot_boundary_cells":hot,"isothermal_pool_cells":cold,"temperature_difference_K":1.,"conductance_W_K":stats[0],"thermal_resistance_K_W":1./stats[0],"cold_return_W":stats[1],"unit_energy_identity_value":stats[2],"power_identity_relative_error":stats[2]/stats[0]-1.,"heat_return_relative_error":(stats[0]+stats[1]).abs()/stats[0],"requested_relative_tolerance":tolerance,"grid":{"dims":grid.dims,"origin_m":grid.origin_m,"cell_m":grid.cell_m},"method":"Existing native GPU scalar finite-volume operator with conductivity k. Outer cylindrical heated barrel at +0.5 K; whole pool at -0.5 K. No new CPU or GPU PDE implementation.","limits":["Steady metal-to-isothermal-pool conduction screen; fixed k, perfect contact to outer M6 cylindrical envelope. Thread helix/root/contact resistance unresolved.","Air, copper contacts and magnets thermally omitted to isolate the nozzle path. No feed advection, wire melting, latent heat, transient heat capacity, cooling or radiation.","An isothermal pool is a mixing idealization, not a prediction of local liquid temperature."]});
    use std::io::Write;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    writeln!(file, "{}", serde_json::to_string_pretty(&record)?)?;
    println!("{}", serde_json::to_string_pretty(&record)?);
    Ok(())
}
