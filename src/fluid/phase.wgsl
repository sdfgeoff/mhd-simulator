struct Params {dims:vec4<u32>,choice:vec4<u32>}
struct Packet {state:vec4<f32>,flux:vec4<f32>,budget:vec4<f32>,valid:vec4<f32>}
struct Fluid {velocity:vec4<f32>,pressure:vec4<f32>,force:vec4<f32>,reserved:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read> thermal:array<Packet>;
@group(0) @binding(2) var<storage,read> materials:array<u32>;
@group(0) @binding(3) var<storage,read_write> mask:array<u32>;
@group(0) @binding(4) var<storage,read_write> fluid:array<Fluid>;
@group(0) @binding(5) var<storage,read_write> changed:array<atomic<u32>>;
@compute @workgroup_size(256)
fn phase(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=gid.x+gid.y*65535u*256u;if i>=p.dims.w{return;}
    var fraction=0.0;if materials[i]==p.choice.x{fraction=thermal[i].state.y;}
    let mobile=fraction>1e-6;let old=mask[i]!=0u;
    if mobile!=old {mask[i]=select(0u,1u,mobile);atomicOr(&changed[0],1u);}
    fluid[i].reserved.y=fraction;
}
