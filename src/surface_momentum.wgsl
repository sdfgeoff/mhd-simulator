struct Params {dims:vec4<u32>,choice:vec4<u32>,step:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read> original:array<u32>;
@group(0) @binding(2) var<storage,read> input_field:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read> next_field:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read> velocity:array<vec4<f32>>; // frozen advector / compression velocity
@group(0) @binding(5) var<storage,read> carried:array<vec4<f32>>;
@group(0) @binding(6) var<storage,read_write> transported:array<vec4<f32>>;
@group(0) @binding(7) var<storage,read> normals:array<vec4<f32>>;
@group(0) @binding(8) var<storage,read> mask:array<u32>;
fn face_point(i:u32)->vec3<i32>{let d=p.dims.xyz+vec3<u32>(1);return vec3<i32>(i32(i%d.x),i32((i/d.x)%d.y),i32(i/(d.x*d.y)));}
fn face_index(q:vec3<i32>)->u32{let d=p.dims.xyz+vec3<u32>(1);return u32(q.x)+d.x*(u32(q.y)+d.y*u32(q.z));}
fn canonical_face(q:vec3<i32>)->vec3<i32>{var r=q;for(var a=0u;a<3u;a++){if periodic(a){r[a]=wrap_periodic(r[a],p.dims[a]);}}return r;}
fn open_face(q:vec3<i32>,component:u32)->bool{var left=q;left[component]-=1;if !inside(left)||!inside(q){return false;}return mask[index(bounded(left))]!=0u&&mask[index(bounded(q))]!=0u;}
fn carried_velocity(q:vec3<i32>,component:u32)->f32{let at=canonical_face(q);if !open_face(at,component){return 0.0;}return carried[face_index(at)][component];}
fn rho(q:vec3<i32>,next:bool)->f32{let i=index(bounded(q));var a=input_field[i].x;if next{a=next_field[i].x;}return p.step.w+(p.step.z-p.step.w)*a;}
fn mass_flux(f:vec3<f32>)->f32{return p.step.w*f.y+(p.step.z-p.step.w)*f.x;}
fn compression_density(q:vec3<i32>)->f32{
 let i=index(bounded(q));return p.step.w+(p.step.z-p.step.w)*max(next_field[i].z,0.0);
}
@compute @workgroup_size(256)
fn momentum(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat(g);let fd=p.dims.xyz+vec3<u32>(1);if i>=fd.x*fd.y*fd.z{return;}
 let q=canonical_face(face_point(i));let axis=p.choice.z;var upper=q;upper[axis]+=1;var lower=q;lower[axis]-=1;var out=vec4<f32>(0.0);
 let base_low=liquid_flux(q,axis);let base_high=liquid_flux(upper,axis);
 let source_q=compression_density(q)*(base_high.y-base_low.y);
 for(var component=0u;component<3u;component++){
  if !open_face(q,component){continue;}var left=q;left[component]-=1;
  let old_density=0.5*(rho(q,false)+rho(left,false));let new_density=0.5*(rho(q,true)+rho(left,true));
  var left_upper=left;left_upper[axis]+=1;
  let left_low=liquid_flux(left,axis);let left_high=liquid_flux(left_upper,axis);
  // Shared primal fluxes give the exact mass balance for arithmetic dual density.
  let center=carried_velocity(q,component);
  let low=0.5*(mass_flux(base_low)+mass_flux(left_low));
  let high=0.5*(mass_flux(base_high)+mass_flux(left_high));
  let inflow=low*select(center,carried_velocity(lower,component),low>0.0);
  let outflow=high*select(carried_velocity(upper,component),center,high>0.0);
  let source_left=compression_density(left)*(left_high.y-left_low.y);
  let source=0.5*(source_q+source_left)*velocity[face_index(q)][component];
  out[component]=(old_density*center+inflow-outflow+source)/new_density;
 }
 transported[i]=out;
}
