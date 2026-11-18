struct Params { cells:u32,factor:f32,pad:vec2<u32> }
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read> cold:array<f32>;
@group(0) @binding(2) var<storage,read_write> heat:array<vec4<f32>>;
@compute @workgroup_size(256)
fn scale_heat(@builtin(global_invocation_id) gid:vec3<u32>) {
 let i=gid.x+gid.y*65535u*256u;
 if i>=p.cells{return;}
 for(var j=0u;j<4u;j++){heat[i*4u+j]=vec4<f32>(0.0);}
 heat[i*4u+2u].w=cold[i]*p.factor;
}
