//! Steady heater-to-pool or copper-lead cooling using the existing GPU FV operator.
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
) -> Result<([f32; 4], Vec<f32>, f32, u32), String> {
    solve_with_pair(context, dims, h, cells, tol, None)
}
fn solve_with_pair(
    context: GpuContext,
    dims: [u32; 3],
    h: f32,
    cells: &[[f32; 2]],
    tol: f32,
    pair: Option<(&[u32], u32, u32, f32)>,
) -> Result<([f32; 4], Vec<f32>, f32, u32), String> {
    let zero = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("No magnetic or flow field in thermal conduction analogy"),
        size: cells.len() as u64 * 64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let mut solver =
        conduction::Conduction::new_with_groups(context, dims, h, cells, &zero, pair.map(|v| v.0))?;
    if let Some((_, a, b, r)) = pair {
        solver.set_pair_resistivity(a, b, r)?;
    }
    solver.set_relative_tolerance(tol)?;
    solver.solve()?;
    let stats = solver.fields(1.)?;
    Ok((
        stats,
        solver.read_scalar_solution()?,
        solver.relative_residual,
        solver.iterations,
    ))
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
            let (stats, potential, _, _) =
                solve(gpu.context.clone(), [16, 1, 1], 0.001, &cells, 1e-6)?;
            for (i, v) in potential.iter().enumerate() {
                assert!((v - (0.5 - i as f32 / 15.)).abs() < 2e-5);
            }
            let reference = k * 1e-6 / 0.015;
            assert!((stats[0] / reference - 1.).abs() < 2e-5, "{stats:?}");
            assert!((stats[2] / stats[0] - 1.).abs() < 2e-5);
            println!(
                "k={k} W/mK: GPU conductance={} W/K; analytic={reference}",
                stats[0]
            );
        }
        // A one-cell mica layer between metals exercises both half-cell
        // resistances. Analytic series resistors are a verification identity,
        // not a duplicate field solver.
        let k: Vec<f32> = (0..16)
            .map(|i| {
                if i < 8 {
                    167.
                } else if i == 8 {
                    0.528
                } else {
                    394.
                }
            })
            .collect();
        let cells: Vec<_> = k
            .iter()
            .enumerate()
            .map(|(i, k)| {
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
        let resistances: Vec<_> = k
            .windows(2)
            .map(|w| 0.001 / (2. * w[0]) + 0.001 / (2. * w[1]))
            .collect();
        let total: f32 = resistances.iter().sum();
        let (stats, potential, _, _) = solve(gpu.context.clone(), [16, 1, 1], 0.001, &cells, 1e-6)?;
        println!(
            "Layer check stats={stats:?}, analytic={}, relative flux error={}",
            1e-6 / total,
            stats[0] / (1e-6 / total) - 1.
        );
        // Boundary flux subtracts nearly equal f32 potentials in the high-k metal.
        // The dissipation identity avoids that cancellation and gets a tighter check.
        assert!((stats[0] / (1e-6 / total) - 1.).abs() < 5e-5);
        assert!((stats[2] / (1e-6 / total) - 1.).abs() < 2e-5);
        let mut used = 0.;
        for (i, v) in potential.iter().enumerate() {
            if i > 0 {
                used += resistances[i - 1];
            }
            assert!((v - (0.5 - used / total)).abs() < 2e-5);
        }
        println!(
            "Metal/mica/metal GPU conductance={} W/K; analytic={}",
            stats[0],
            1e-6 / total
        );
        return Ok(());
    }
    if args
        .get(1)
        .is_some_and(|x| x == "--integral-pool" || x == "--integral-pool-config")
    {
        return integral_pool(&args, gpu);
    }
    let p = project::load(Path::new(args.get(1).ok_or(
        "PROJECT HEATER_START_MM OUTPUT [STEEL_K] [TOLERANCE] [pool|leads|anchors] [MICA_K]",
    )?))?;
    let heater_start: f32 = args.get(2).ok_or("Heater start")?.parse()?;
    let output = Path::new(args.get(3).ok_or("Output")?);
    let steel_k: f32 = args.get(4).map(|x| x.parse()).transpose()?.unwrap_or(15.);
    let tolerance: f32 = args.get(5).map(|x| x.parse()).transpose()?.unwrap_or(1e-6);
    if !heater_start.is_finite() || !steel_k.is_finite() || steel_k <= 0. {
        return Err("Invalid thermal controls".into());
    }
    let mode = args.get(6).map(String::as_str).unwrap_or("pool");
    if !["pool", "leads", "anchors"].contains(&mode) {
        return Err("Mode must be pool, leads or anchors".into());
    }
    let mica_k: f32 = args.get(7).map(|x| x.parse()).transpose()?.unwrap_or(0.528);
    if !mica_k.is_finite() || mica_k < 0. {
        return Err("Invalid mica k".into());
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
    let mut copper_outer_x = [u32::MAX, 0];
    for (i, &id) in ids.iter().enumerate() {
        if id == 16 || id == 17 {
            let x = i as u32 % grid.dims[0];
            copper_outer_x[0] = copper_outer_x[0].min(x);
            copper_outer_x[1] = copper_outer_x[1].max(x);
        }
    }
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
                    let heated = mode != "anchors"
                        && z >= heater_start * 0.001
                        && z <= (heater_start + 5.4) * 0.001
                        && (x * x + y * y).sqrt() >= (3e-3 - grid.cell_m);
                    if heated {
                        hot += 1;
                    }
                    [steel_k / 1e6, if heated { 0.5 } else { 2. }]
                }
                4 => {
                    if mode == "pool" {
                        cold += 1;
                    }
                    [liquid_k / 1e6, if mode == "pool" { -0.5 } else { 2. }]
                }
                16 | 17 if mode != "pool" => {
                    let x = i as u32 % grid.dims[0];
                    let terminal = x == copper_outer_x[0] || x == copper_outer_x[1];
                    if terminal {
                        cold += 1;
                    }
                    [394. / 1e6, if terminal { -0.5 } else { 2. }]
                }
                19 if mode == "anchors" => [mica_k / 1e6, 2.],
                8 if mode == "anchors" => {
                    hot += 1;
                    [167. / 1e6, 0.5]
                }
                _ => [0., 2.],
            }
        })
        .collect();
    if hot == 0 || cold == 0 {
        return Err("Missing heater or pool boundary".into());
    }
    let (stats, potential, true_relative_residual, iterations) = solve(
        gpu.context.clone(),
        grid.dims,
        grid.cell_m,
        &cells,
        tolerance,
    )?;
    let liquid_min = ids
        .iter()
        .zip(&potential)
        .filter(|(id, _)| **id == 4)
        .map(|(_, v)| *v)
        .fold(f32::INFINITY, f32::min);
    let liquid_min_index = ids
        .iter()
        .zip(&potential)
        .enumerate()
        .filter(|(_, (id, _))| **id == 4)
        .min_by(|(_, (_, a)), (_, (_, b))| a.total_cmp(b))
        .map(|(i, _)| i)
        .ok_or("No liquid cells")?;
    let q = liquid_min_index as u32;
    let liquid_min_xyz_m = [
        q % grid.dims[0],
        (q / grid.dims[0]) % grid.dims[1],
        q / (grid.dims[0] * grid.dims[1]),
    ]
    .map(|v| v as f32);
    let liquid_min_xyz_m: [f32; 3] =
        std::array::from_fn(|i| grid.origin_m[i] + (liquid_min_xyz_m[i] + 0.5) * grid.cell_m);
    let temperature_cases: Vec<_> = [25_f32, 100., 150., 200., 220., 230., 240.]
        .into_iter()
        .map(|cold_c| {
            serde_json::json!({"heater_C":250.,"cold_boundary_C":cold_c,
            "liquid_min_C":cold_c+(250.-cold_c)*(liquid_min+0.5),
            "heat_draw_W":(250.-cold_c)*stats[0]})
        })
        .collect();
    let record = serde_json::json!({"true_relative_residual":true_relative_residual,"iterations":iterations,"mica_effective_k_W_m_K":mica_k,"liquid_min_location_m":liquid_min_xyz_m,"mode":mode,"cold_boundary_cells":cold,"liquid_min_unit_potential":liquid_min,"temperature_cases":temperature_cases,"copper_k_W_m_K_if_used":394.,"project":args[1],"heater_start_mm":heater_start,"steel_k_W_m_K":steel_k,"liquid_k_W_m_K":liquid_k,"hot_boundary_cells":hot,"isothermal_pool_cells":if mode=="pool" {cold} else {0},"temperature_difference_K":1.,"conductance_W_K":stats[0],"thermal_resistance_K_W":1./stats[0],"cold_return_W":stats[1],"unit_energy_identity_value":stats[2],"power_identity_relative_error":stats[2]/stats[0]-1.,"heat_return_relative_error":(stats[0]+stats[1]).abs()/stats[0],"requested_relative_tolerance":tolerance,"grid":{"dims":grid.dims,"origin_m":grid.origin_m,"cell_m":grid.cell_m},"method":if mode=="anchors" {"Existing native GPU scalar finite-volume operator with conductivity k. Entire aluminium heater (ID8) fixed at +0.5 K; outer copper planes fixed at -0.5 K; mica/steel/liquid/copper unknown. No new PDE solver."} else {"Existing native GPU scalar finite-volume operator with conductivity k. Outer cylindrical heated barrel at +0.5 K; selected pool or outer copper-plane boundary at -0.5 K. No new PDE solver."},"limits":["Steady conduction screen; fixed k, ideal interfaces. Pool/leads modes fix outer M6 skin; anchors mode fixes whole aluminium heater. Thread helix/root/contact resistance unresolved.","Air and magnets thermally omitted. Copper omitted in pool mode; included in leads/anchors modes. Anchors mode adds fixed167W/mK heater and requested effective-k mica with ideal interfaces; reducing k can represent extra sheet/interface resistance. No feed advection, wire melting, latent heat, transient heat capacity, Joule heating, external convection or radiation.","Pool mode assumes isothermal liquid. Leads mode instead solves stationary liquid/steel/copper temperature with fixed cold outer copper planes; it omits Joule heat, external losses and feeding, and uses fixed liquid k even if temperatures indicate freezing. Neither is a complete operation model."]});
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

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct BoreBoundary {
    axis: usize,
    centre_mm: [f64; 3],
    void_radius_mm: f64,
    void_span_mm: [f64; 2],
}
impl BoreBoundary {
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        if self.axis > 2
            || !self.centre_mm.iter().all(|v| v.is_finite())
            || !self.void_radius_mm.is_finite()
            || self.void_radius_mm <= 0.
            || !self.void_span_mm.iter().all(|v| v.is_finite())
            || self.void_span_mm[0] >= self.void_span_mm[1]
        {
            return Err("Invalid bore boundary geometry".into());
        }
        Ok(())
    }
    fn radial(&self, xyz: &[f64]) -> f64 {
        (0..3)
            .filter(|&j| j != self.axis)
            .map(|j| (xyz[j] - self.centre_mm[j]).powi(2))
            .sum::<f64>()
            .sqrt()
    }
    fn excludes(&self, xyz: &[f64]) -> bool {
        self.radial(xyz) < self.void_radius_mm
            && (self.void_span_mm[0]..=self.void_span_mm[1]).contains(&xyz[self.axis])
    }
}
#[derive(serde::Deserialize, serde::Serialize)]
struct PairContact {
    group_a: u32,
    group_b: u32,
    resistance_m2_k_w: f32,
    material_groups: std::collections::BTreeMap<u32, u32>,
}
#[derive(serde::Deserialize, serde::Serialize)]
struct IntegralBoundary {
    heater: BoreBoundary,
    sensor: BoreBoundary,
    hot_radius_band_mm: [f64; 2],
    hot_span_mm: [f64; 2],
    label: String,
    #[serde(default)]
    conductivity_by_material_w_m_k: Option<std::collections::BTreeMap<u32, f32>>,
    #[serde(default)]
    pair_contact: Option<PairContact>,
}

// Optimistic bulk-metal heat-delivery screen. The actual heater interface and
// exterior losses are deliberately excluded and stated in the output.
fn integral_pool(args: &[String], gpu: GpuDomain) -> Result<(), Box<dyn Error>> {
    let path = args
        .get(2)
        .ok_or("--integral-pool PROJECT HX_MM HZ_MM OUTPUT_DIR [SX_MM SZ_MM]")?;
    let configured = args[1] == "--integral-pool-config";
    let (boundary, output_index, tolerance_index) = if configured {
        let cfg: IntegralBoundary =
            serde_json::from_slice(&std::fs::read(args.get(3).ok_or("Missing boundary JSON")?)?)?;
        (cfg, 4, 5)
    } else {
        let hx: f64 = args.get(3).ok_or("Missing heater x")?.parse()?;
        let hz: f64 = args.get(4).ok_or("Missing heater z")?.parse()?;
        let sx: f64 = args
            .get(6)
            .map(|v| v.parse())
            .transpose()?
            .unwrap_or(hx + 2.5);
        let sz: f64 = args.get(7).map(|v| v.parse()).transpose()?.unwrap_or(hz);
        (
            IntegralBoundary {
                heater: BoreBoundary {
                    axis: 1,
                    centre_mm: [hx, 0., hz],
                    void_radius_mm: 1.6,
                    void_span_mm: [-1e6, 1e6],
                },
                sensor: BoreBoundary {
                    axis: 1,
                    centre_mm: [sx, 0., sz],
                    void_radius_mm: 0.6,
                    void_span_mm: [-1e6, 1e6],
                },
                hot_radius_band_mm: [1.6, 1.75],
                hot_span_mm: [-5.8, 5.8],
                label: "Legacy ideal transverse bore wall".into(),
                conductivity_by_material_w_m_k: None,
                pair_contact: None,
            },
            5,
            8,
        )
    };
    boundary.heater.validate()?;
    boundary.sensor.validate()?;
    if !boundary
        .hot_radius_band_mm
        .iter()
        .chain(&boundary.hot_span_mm)
        .all(|v| v.is_finite())
        || boundary.hot_radius_band_mm[0] < boundary.heater.void_radius_mm
        || boundary.hot_radius_band_mm[1] <= boundary.hot_radius_band_mm[0]
        || boundary.hot_span_mm[0] >= boundary.hot_span_mm[1]
        || boundary.hot_span_mm[0] < boundary.heater.void_span_mm[0]
        || boundary.hot_span_mm[1] > boundary.heater.void_span_mm[1]
    {
        return Err("Invalid hot wall span or radius band".into());
    }
    if let Some(table) = &boundary.conductivity_by_material_w_m_k {
        if !table.values().all(|k| k.is_finite() && *k >= 0.) {
            return Err("Invalid material conductivity table".into());
        }
    }
    let tolerance: f32 = args
        .get(tolerance_index)
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(1e-6);
    let output = Path::new(args.get(output_index).ok_or("Missing output directory")?);
    std::fs::create_dir(output)?;
    let p = project::load(Path::new(path))?;
    let volume = gpu.build(&p.model, &p.settings, &p.materials)?;
    if volume.overlaps != 0 {
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
        .and_then(|m| m.thermal.conductivity_w_m_k)
        .ok_or("Missing liquid k")?;
    let mut hot = 0usize;
    let mut cold = 0usize;
    let cells: Vec<[f32; 2]> = ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            if id == 4 {
                cold += 1;
                return [liquid_k / 1e6, -0.5];
            }
            let k = if let Some(table) = &boundary.conductivity_by_material_w_m_k {
                table.get(&id).copied().unwrap_or(0.)
            } else if matches!(id, 14 | 16 | 17) {
                15.
            } else {
                0.
            };
            if k == 0. {
                return [0., 2.];
            }
            let i = i as u32;
            let q = [
                i % grid.dims[0],
                (i / grid.dims[0]) % grid.dims[1],
                i / (grid.dims[0] * grid.dims[1]),
            ];
            let xyz: Vec<f64> = q
                .iter()
                .enumerate()
                .map(|(j, &v)| {
                    (f64::from(grid.origin_m[j]) + (f64::from(v) + 0.5) * f64::from(grid.cell_m))
                        * 1000.
                })
                .collect();
            let r = boundary.heater.radial(&xyz);
            // Omit floating sheaths only within their actual axial void spans.
            if boundary.heater.excludes(&xyz) || boundary.sensor.excludes(&xyz) {
                return [0., 2.];
            }
            let flag = if (boundary.hot_span_mm[0]..=boundary.hot_span_mm[1])
                .contains(&xyz[boundary.heater.axis])
                && (boundary.hot_radius_band_mm[0]..=boundary.hot_radius_band_mm[1]).contains(&r)
            {
                hot += 1;
                0.5
            } else {
                2.
            };
            [k / 1e6, flag]
        })
        .collect();
    if hot == 0 || cold == 0 {
        return Err("Missing heater-wall or pool boundary".into());
    }
    let groups: Vec<_> = ids
        .iter()
        .map(|id| {
            boundary
                .pair_contact
                .as_ref()
                .and_then(|p| p.material_groups.get(id).copied())
                .unwrap_or(*id)
        })
        .collect();
    let (stats, potential, residual, iterations) = solve_with_pair(
        gpu.context.clone(),
        grid.dims,
        grid.cell_m,
        &cells,
        tolerance,
        boundary
            .pair_contact
            .as_ref()
            .map(|p| (groups.as_slice(), p.group_a, p.group_b, p.resistance_m2_k_w)),
    )?;
    let return_error = (stats[0] + stats[1]).abs() / stats[0];
    let energy_error = stats[2] / stats[0] - 1.;
    if stats[0] <= 0. || return_error > 1e-5 || energy_error.abs() > 1e-4 {
        return Err(format!("Thermal conductance checks failed: {stats:?}, return={return_error}, energy={energy_error}, true_residual={residual}, iterations={iterations}").into());
    }
    let assumed_loads =
        [1_f32, 5., 10.].map(|w| serde_json::json!({"pool_W":w,"delta_K":w/stats[0]}));
    let report = serde_json::json!({
        "project":path,"adapter":gpu.name,"method":"Existing GPU finite-volume scalar operator with k; selected heater-bore wall cells +0.5K, entire liquid pool -0.5K, other exterior adiabatic. Optional explicit material k table retained in boundary_geometry.",
        "boundary_geometry":boundary,
        "heater_centre_xz_mm":[boundary.heater.centre_mm[0],boundary.heater.centre_mm[2]],
        "sensor_centre_xz_mm":[boundary.sensor.centre_mm[0],boundary.sensor.centre_mm[2]],
        "hot_cells":hot,"pool_boundary_cells":cold,"legacy_steel_k_W_m_K":15.,"liquid_k_W_m_K":liquid_k,
        "conductance_W_K":stats[0],"thermal_resistance_K_W":1./stats[0],"return_relative_error":return_error,
        "energy_identity_relative_error":energy_error,"true_relative_residual":residual,"iterations":iterations,
        "requested_relative_tolerance":tolerance,
        "required_delta_K_at_assumed_pool_loads_W": assumed_loads,
        "limits":["Ideal prescribed-temperature bore-wall layer and isothermal pool; optimistic bulk-metal delivery only, not operating temperatures or finite heater power.","No heater contact/interface resistance, external losses, cable/mount cooling, wire-feed melting, phase change, convection, transient heat capacity or Joule heat.","Fixed k and one0.1mm grid. Assumed pool loads illustrate thermal resistance; no actual feed demand has been established.","Air, magnets and floating heater/sensor sheaths are thermally omitted. Temperature is prescribed directly in the steel bore-wall layer; no sheath/interface contact is inferred from sub-cell gaps."]
    });
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(
        output.join("potential.bin"),
        bytemuck::cast_slice(&potential),
    )?;
    std::fs::write(output.join("material-ids.bin"), &bytes)?;
    std::fs::write(
        output.join("grid.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"dims":grid.dims,"origin_m":grid.origin_m,"cell_m":grid.cell_m}),
        )?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn bore_selection_is_invariant_under_axis_permutation() {
        let transverse = BoreBoundary {
            axis: 1,
            centre_mm: [-4., 0., 8.5],
            void_radius_mm: 1.6,
            void_span_mm: [-6., 6.],
        };
        let vertical = BoreBoundary {
            axis: 2,
            centre_mm: [-4., 8.5, 0.],
            void_radius_mm: 1.6,
            void_span_mm: [-6., 6.],
        };
        for x in [-6., -4., -2.] {
            for y in [-7., 0., 5.] {
                for z in [7., 8.5, 10.] {
                    let a = [x, y, z];
                    let b = [x, z, y];
                    assert_eq!(transverse.radial(&a), vertical.radial(&b));
                    assert_eq!(transverse.excludes(&a), vertical.excludes(&b));
                }
            }
        }
    }

    #[test]
    fn finite_bore_keeps_drill_floor_and_omits_sheath() {
        let bore = BoreBoundary {
            axis: 2,
            centre_mm: [-4., 0., 0.],
            void_radius_mm: 1.6,
            void_span_mm: [2.4, 28.1],
        };
        bore.validate().unwrap();
        assert!(!bore.excludes(&[-4., 0., 1.0]));
        assert!(bore.excludes(&[-4., 0., 3.0]));
        assert!(!bore.excludes(&[-2., 0., 3.0]));
        let invalid = BoreBoundary { axis: 3, ..bore };
        assert!(invalid.validate().is_err());
    }
}
