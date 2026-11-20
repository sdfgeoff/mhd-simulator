//! Build the initial grid and report overlapping body masks. No time integration.
use mhd_workbench::{domain::GpuDomain, gpu::read_bytes, project};
use std::{error::Error, path::Path};
fn main() -> Result<(), Box<dyn Error>> {
    let a: Vec<_> = std::env::args().collect();
    let p = project::load(Path::new(a.get(1).ok_or("Expected PROJECT OUTPUT_JSON")?))?;
    let gpu = GpuDomain::new()?;
    let v = gpu.build(&p.model, &p.settings, &p.materials)?;
    let bytes = read_bytes(&gpu.context, v.body_masks(), 0, v.body_masks().size())?;
    if let Some(path)=a.get(3){std::fs::write(path,&bytes)?;}
    let masks = bytemuck::cast_slice::<u8, u32>(&bytes);
    let [nx, ny, _] = v.dims;
    let cells: Vec<_> = masks.iter().enumerate().filter(|(_,m)|m.count_ones()>1).map(|(i,&mask)| {
        let x=i as u32%nx; let y=i as u32/nx%ny; let z=i as u32/(nx*ny);
        let xyz=[x,y,z];
        let centre:Vec<_>=xyz.iter().enumerate().map(|(j,&c)|p.settings.min_mm[j]+(c as f32+0.5)*p.settings.cell_mm).collect();
        let bodies:Vec<_>=p.model.bodies.iter().enumerate().filter(|(j,_)|mask&(1u32<<j)!=0).map(|(j,b)|serde_json::json!({"index":j,"name":b.name})).collect();
        serde_json::json!({"index":i,"cell":xyz,"centre_mm":centre,"mask":mask,"bodies":bodies})
    }).collect();
    let report=serde_json::json!({"method":"Initial GPU voxelization only; no fluid/electrical timestep","adapter":gpu.name,"dims":v.dims,"overlaps":v.overlaps,"cells":cells});
    std::fs::write(a.get(2).ok_or("Missing output")?,serde_json::to_string_pretty(&report)?)?;
    println!("{} overlapping cells on {}",v.overlaps,gpu.name);
    Ok(())
}
