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
// Limited primitive-velocity reconstruction. Mass flux and the WY source
// remain exactly those of the geometric fraction/density update.
fn mc_slope(left:f32,right:f32)->f32 {
 if (left>0.0 && right>0.0)||(left<0.0 && right<0.0) {
  return sign(left)*min(0.5*abs(left+right),min(2.0*abs(left),2.0*abs(right)));
 }
 return 0.0;
}
fn advector_courant(q:vec3<i32>,axis:u32)->f32 {
 let at=canonical_face(q);if !open_face(at,axis){return 0.0;}
 return velocity[face_index(at)][axis]*p.step.x/p.step.y;
}
fn dual_courant(q:vec3<i32>,component:u32,axis:u32)->f32 {
 var left=q;left[component]-=1;var upper=q;upper[axis]+=1;
 var left_upper=left;left_upper[axis]+=1;
 return 0.25*(advector_courant(q,axis)+advector_courant(left,axis)
   +advector_courant(upper,axis)+advector_courant(left_upper,axis));
}
// Limit higher-order reconstruction to a homogeneous phase stencil.
// Arithmetic MAC density does not supply an independent dual-phase geometry.
fn homogeneous_trace_stencil(q:vec3<i32>,component:u32,axis:u32)->bool {
 var low=2.0;var high=-1.0;
 for(var offset=-1;offset<=1;offset++){
  var at=q;at[axis]+=offset;var left=at;left[component]-=1;
  let a=input_field[index(bounded(at))].x;let b=input_field[index(bounded(left))].x;
  low=min(low,min(a,b));high=max(high,max(a,b));
 }
 return high<=1e-6||low>=1.0-1e-6;
}
fn momentum_trace(at:vec3<i32>,component:u32,axis:u32,flux:f32)->f32 {
 if flux==0.0 {return 0.0;}
 var donor=at;let side=select(-1.0,1.0,flux>0.0);
 if flux>0.0 {donor[axis]-=1;}
 donor=canonical_face(donor);
 if !open_face(donor,component){return 0.0;}
 let center=carried_velocity(donor,component);
 var lower=donor;lower[axis]-=1;var upper=donor;upper[axis]+=1;
 // Do not extrapolate transported velocity through a static/frozen wall.
 if !open_face(canonical_face(lower),component)||!open_face(canonical_face(upper),component){return center;}
 let left=carried_velocity(lower,component);let right=carried_velocity(upper,component);
 let slope=mc_slope(center-left,right-center);
 if slope==0.0 {return center;}
 if p.step.z!=p.step.w && !homogeneous_trace_stencil(donor,component,axis){return center;}
 // Hancock's half-step predictor uses the frozen volume-advection velocity.
 let predicted=center+0.5*(side-dual_courant(donor,component,axis))*slope;
 return clamp(predicted,min(center,min(left,right)),max(center,max(left,right)));
}
@compute @workgroup_size(256)
fn momentum(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat(g);let fd=p.dims.xyz+vec3<u32>(1);if i>=fd.x*fd.y*fd.z{return;}
 let q=canonical_face(face_point(i));let axis=p.choice.z;var upper=q;upper[axis]+=1;var out=vec4<f32>(0.0);
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
  let inflow=low*momentum_trace(q,component,axis,low);
  let outflow=high*momentum_trace(upper,component,axis,high);
  let source_left=compression_density(left)*(left_high.y-left_low.y);
  let source=0.5*(source_q+source_left)*velocity[face_index(q)][component];
  out[component]=(old_density*center+inflow-outflow+source)/new_density;
 }
 transported[i]=out;
}
