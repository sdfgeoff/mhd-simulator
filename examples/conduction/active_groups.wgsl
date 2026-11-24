// Rebuild the work list after accepted fractions set the cell conductivities.
// Slots may be assigned in any order. Each CG group still reduces to its
// original dense index, preserving the reduction tree and neighbour indices.
struct Config {dims:vec4<u32>, physical:vec4<f32>, control:vec4<u32>, extra:vec4<f32>, pair_contact:vec4<f32>}
@group(0) @binding(0) var<uniform> cfg:Config;
@group(0) @binding(1) var<storage,read_write> cells:array<vec2<f32>>;
@group(0) @binding(3) var<storage,read_write> partials:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read_write> scalar_words:array<atomic<u32>>;
var<workgroup> occupied:atomic<u32>;
@compute @workgroup_size(1)
fn reset_count(){atomicStore(&scalar_words[15],0u);}
@compute @workgroup_size(256)
fn list_groups(@builtin(workgroup_id) group:vec3<u32>,@builtin(num_workgroups) grid:vec3<u32>,@builtin(local_invocation_index) lane:u32){
 let dense=group.x+group.y*grid.x;
 let groups=(cfg.dims.w+255u)/256u;
 if dense>=groups{return;}
 let i=256u*dense+lane;
 if i<cfg.dims.w && cells[i].x>0. && bitcast<f32>(bitcast<u32>(cells[i].y)&0xffff0000u)>1. {
  atomicStore(&occupied,1u);
 }
 workgroupBarrier();
 if lane==0u && atomicLoad(&occupied)!=0u {
  let offset=groups+(groups+255u)/256u+(cfg.dims.w+3u)/4u;
  let slot=atomicAdd(&scalar_words[15],1u);
  // A u32 cell index has at most 24 group-index bits at 256 lanes per group.
  partials[offset+slot].x=f32(dense);
 }
}
