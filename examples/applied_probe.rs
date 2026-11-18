//! Small observation-window export of the existing GPU applied-field evaluator.
//! Not a conduction, induction-reaction, thermal or fluid simulation.
use mhd_workbench::{
    domain::GpuDomain,
    em::{EmState, Mode},
    gpu::read_bytes,
    project,
};
use std::{error::Error, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let input = Path::new(args.get(1).ok_or("Expected PROJECT OUTPUT")?);
    let output = Path::new(args.get(2).ok_or("Missing output directory")?);
    let p = project::load(input)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir(output)?;
    let gpu = GpuDomain::new()?;
    let volume = gpu.build(&p.model, &p.settings, &p.materials)?;
    if volume.overlaps > 0 {
        return Err(format!("{} overlapping observation cells", volume.overlaps).into());
    }
    let em = EmState::new(
        gpu.context.clone(),
        &volume,
        &p.model,
        p.electromagnetics,
        Mode::Applied,
    )?;
    let grid = volume.inputs.snapshot().grid.clone();
    std::fs::write(
        output.join("material-ids.bin"),
        read_bytes(
            &gpu.context,
            volume.material_ids(),
            0,
            volume.material_ids().size(),
        )?,
    )?;
    std::fs::write(
        output.join("em.bin"),
        read_bytes(
            &gpu.context,
            em.result_buffer(),
            0,
            em.result_buffer().size(),
        )?,
    )?;
    let report = serde_json::json!({
        "status":"complete applied-field observation", "project":args[1],
        "dims":grid.dims, "origin_m":grid.origin_m, "cell_m":grid.cell_m,
        "mode":"Applied", "limits":[
            "Small field observation region, not a cropped conduction or induction-reaction domain.",
            "Global CAD collision checks must be performed separately.",
            "Fixed source magnetization; no jetting or force-performance conclusion."
        ]
    });
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
