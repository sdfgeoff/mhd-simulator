struct Params {dims:vec4<u32>,choice:vec4<u32>,step:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read> original:array<u32>;
@group(0) @binding(2) var<storage,read> surface:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read_write> evolving:array<u32>;
@compute @workgroup_size(256)
fn update(@builtin(global_invocation_id) g:vec3<u32>){let i=g.x+g.y*65535u*256u;if i>=p.dims.w{return;}let id=original[i];
 if id!=0u && id!=p.choice.x {evolving[i]=id;return;}
 let a=surface[i].x;if a<=0.0{evolving[i]=0u;}else{evolving[i]=0x80000000u|(u32(round(clamp(a,0.0,1.0)*8388607.0))<<8u)|p.choice.x;}
}
