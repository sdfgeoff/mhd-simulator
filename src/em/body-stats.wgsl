struct Result {b:vec4<f32>,j:vec4<f32>,fq:vec4<f32>,applied:vec4<f32>}
@group(0) @binding(1) var<storage,read> masks:array<u32>;
@group(0) @binding(2) var<storage,read> fields:array<Result>;
@group(0) @binding(3) var<storage,read_write> partials:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read_write> totals:array<vec4<f32>>;
var<workgroup> sum:array<vec4<f32>,256>;
@compute @workgroup_size(256)
fn collect(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
    let i=flat_id(gid);var v=vec4<f32>(0.0);
    if i<scene.dims.w && (masks[i]&(1u<<group.z))!=0u {v=fields[i].fq*scene.physical.z*scene.physical.z*scene.physical.z;}
    sum[l]=v;workgroupBarrier();for(var s=128u;s>0u;s/=2u){if l<s {sum[l]+=sum[l+s];}workgroupBarrier();}
    if l==0u {let groups=(scene.dims.w+255u)/256u;let g=group.x+group.y*65535u;if g<groups {partials[group.z*groups+g]=sum[0];}}
}
@compute @workgroup_size(256)
fn reduce(@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
    let n=(scene.dims.w+255u)/256u;var v=vec4<f32>(0.0);for(var i=l;i<n;i+=256u){v+=partials[group.z*n+i];}
    sum[l]=v;workgroupBarrier();for(var s=128u;s>0u;s/=2u){if l<s {sum[l]+=sum[l+s];}workgroupBarrier();}if l==0u{totals[group.z]=sum[0];}
}
