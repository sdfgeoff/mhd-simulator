// Prescribed force density, not a computed Lorentz force. Only liquid occupancy
// in the rear pool is driven; no force is applied to the emerging jet.
struct Params {dims:vec4<u32>,origin:vec4<f32>,pulse:vec4<f32>,radial:vec4<f32>,axial:vec4<f32>}
struct Field {b:vec4<f32>,j:vec4<f32>,force_heat:vec4<f32>,applied:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read> alpha:array<vec4<f32>>;
@group(0) @binding(2) var<storage,read_write> force:array<Field>;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) g:vec3<u32>){
 let i=g.x+g.y*256u*256u;if i>=p.dims.w{return;}
 let q=vec3<u32>(i%p.dims.x,(i/p.dims.x)%p.dims.y,i/(p.dims.x*p.dims.y));
 let x=p.origin.xyz+(vec3<f32>(q)+0.5)*p.pulse.x;
 let window=(1.-smoothstep(p.radial.x,p.radial.y,length(x.xy)))*smoothstep(p.axial.x,p.axial.y,x.z)*(1.-smoothstep(p.axial.z,p.axial.w,x.z));
 let f=p.pulse.y*alpha[i].x*window;
 force[i]=Field(vec4<f32>(0.),vec4<f32>(0.),vec4<f32>(0.,0.,f,0.),vec4<f32>(0.));
}
