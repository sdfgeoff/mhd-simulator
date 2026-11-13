struct Grid { dims:vec4<u32> }
@group(0) @binding(0) var<uniform> grid:Grid;
@group(0) @binding(1) var<storage,read> masks:array<u32>;
@group(0) @binding(2) var<storage,read> assignments:array<u32>;
@group(0) @binding(3) var<storage,read_write> materials:array<u32>;
@compute @workgroup_size(8,8,4)
fn main(@builtin(global_invocation_id) p:vec3<u32>) {
    if any(p>=grid.dims.xyz) {return;}
    let i=p.x+grid.dims.x*(p.y+grid.dims.y*p.z);
    let mask=masks[i];
    var material=0u;
    if countOneBits(mask)>1u {material=255u;} else if mask!=0u {material=assignments[firstTrailingBit(mask)];}
    materials[i]=material;
}
