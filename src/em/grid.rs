use crate::simulation::GridSi;
use bytemuck::{Pod, Zeroable};
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Params {
    pub origin: [f32; 4],
    pub dims: [u32; 4],
    pub edges: [u32; 4],
    pub control: [u32; 4],
    pub physical: [f32; 4],
}
pub(crate) fn params(grid: &GridSi) -> Result<Params, String> {
    let [x, y, z] = grid.dims.map(|v| v as u64);
    let counts = [
        x * (y + 1) * (z + 1),
        (x + 1) * y * (z + 1),
        (x + 1) * (y + 1) * z,
    ];
    let total: u64 = counts.iter().sum();
    if total > u32::MAX as u64 || x * y * z > u32::MAX as u64 {
        return Err("EM grid indexing exceeds u32".into());
    }
    Ok(Params {
        origin: [grid.origin_m[0], grid.origin_m[1], grid.origin_m[2], 0.],
        dims: [x as u32, y as u32, z as u32, (x * y * z) as u32],
        edges: [
            counts[0] as u32,
            counts[1] as u32,
            counts[2] as u32,
            total as u32,
        ],
        control: [0; 4],
        physical: [1e-5, 4e-7 * std::f32::consts::PI, grid.cell_m, 1e-10],
    })
}
