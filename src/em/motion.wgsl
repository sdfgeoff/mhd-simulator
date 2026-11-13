// Conductivity-weighted cell u×B projected onto the surrounding EM edges.
struct Packet {v:vec4<f32>,p:vec4<f32>,f:vec4<f32>,mask:vec4<f32>}
struct Field {b:vec4<f32>,j:vec4<f32>,f:vec4<f32>,applied:vec4<f32>}
struct Physical {values:vec4<f32>,flags:vec4<u32>}
@group(0) @binding(1) var<storage,read> velocity:array<Packet>;
@group(0) @binding(2) var<storage,read> magnetic:array<Field>;
@group(0) @binding(3) var<storage,read_write> source:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read> materials:array<u32>;
@group(0) @binding(5) var<storage,read> properties:array<Physical>;
@compute @workgroup_size(256)
fn motion(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat_id(gid);if i>=scene.edges.w{return;}
    let e=edge_decode(i);let a=(e.w+1u)%3u;let b=(e.w+2u)%3u;
    var weighted=0.0;var sigma=0.0;
    for(var u=0;u<2;u++){for(var v=0;v<2;v++){
        var p=vec3<i32>(e.xyz);p[a]-=u;p[b]-=v;
        if valid(p,scene.dims.xyz){
            let c=u32(p.x)+scene.dims.x*(u32(p.y)+scene.dims.y*u32(p.z));
            let id=materials[c];let base=select(id,id&255u,(id&0x80000000u)!=0u);var s=properties[base].values.x;
            if (id&0x80000000u)!=0u{s=properties[id&255u].values.x*f32((id>>8u)&0x7fffffu)/8388607.0;}
            sigma+=s;
            if velocity[c].mask.x>0.5{weighted+=s*cross(velocity[c].v.xyz,magnetic[c].b.xyz)[e.w];}
        }
    }}
    source[i+scene.edges.w].x=weighted/max(sigma,1e-30);
}
