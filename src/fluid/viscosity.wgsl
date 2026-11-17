// Symmetric Newtonian stresses on the MAC grid. Normal stresses live at cells,
// shear stresses at edges shared by the two momentum control volumes.
fn dynamic_mu(q:vec3<i32>)->f32 {
 if !fluid(q){return 0.0;}
 return mix(p.surface.w,p.physical.z*p.physical.w,clamp(occupancy(q),0.0,1.0));
}
fn edge_mu(q:vec3<i32>,i:u32,j:u32)->f32 {
 return coefficient_edge_mu(q,i,j,false);
}
// Cell viscosity remains fixed during a diffusion macro-step. The pressure
// display's W lane is dead until fields() rebuilds the accepted packet.
fn coefficient_mu(q:vec3<i32>,cached:bool)->f32 {
 if !cached {return dynamic_mu(q);}
 let at=wrap_cell(q);if !valid_cell(at){return 0.0;}
 return output_fields[index(at,p.dims.xyz)].pressure.w;
}
fn coefficient_edge_mu(q:vec3<i32>,i:u32,j:u32,cached:bool)->f32 {
 var total=0.0;var count=0.0;
 for(var a=0;a<2;a++){for(var b=0;b<2;b++){
  var at=q;at[i]-=a;at[j]-=b;
  if fluid(at){total+=coefficient_mu(at,cached);count+=1.0;}
 }}
 return total/max(count,1.0);
}
@compute @workgroup_size(256)
fn stress_coefficients(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat_id(g);if i>=p.dims.w{return;}let q=decode(i,p.dims.xyz);
 output_fields[i].pressure.w=dynamic_mu(q);
}
fn normal_stress(q:vec3<i32>,i:u32,cached:bool)->f32 {
 if !fluid(q){return 0.0;}var upper=q;upper[i]+=1;
 return 2.0*coefficient_mu(q,cached)*(face(upper,i,true)-face(q,i,true))/p.physical.x;
}
fn shear_stress(q:vec3<i32>,i:u32,j:u32,cached:bool)->f32 {
 var lower_i=q;lower_i[j]-=1;var lower_j=q;lower_j[i]-=1;
 var ui1=face(q,i,true);var ui0=face(lower_i,i,true);
 var uj1=face(q,j,true);var uj0=face(lower_j,j,true);
 // Odd tangential ghosts place zero velocity at the wall, halfway between
 // the fluid sample and its ghost. Closed normal faces already have u=0.
 if !open_face(q,i)&&open_face(lower_i,i){ui1=-ui0;}
 if !open_face(lower_i,i)&&open_face(q,i){ui0=-ui1;}
 if !open_face(q,j)&&open_face(lower_j,j){uj1=-uj0;}
 if !open_face(lower_j,j)&&open_face(q,j){uj0=-uj1;}
 return coefficient_edge_mu(q,i,j,cached)*(ui1-ui0+uj1-uj0)/p.physical.x;
}
// Normal stresses and XY shear use the pressure work vector; XZ/YZ shear
// use display pressure Y/Z. Live warm-start pressure X is never overwritten.
@compute @workgroup_size(256)
fn stress_tensor(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat_id(g);if i>=p.dims.w{return;}let q=decode(i,p.dims.xyz);
 work[i]=vec4<f32>(normal_stress(q,0u,true),normal_stress(q,1u,true),
   normal_stress(q,2u,true),shear_stress(q,0u,1u,true));
 output_fields[i].pressure.y=shear_stress(q,0u,2u,true);
 output_fields[i].pressure.z=shear_stress(q,1u,2u,true);
}
fn tensor_normal(q:vec3<i32>,i:u32)->f32 {
 let at=wrap_cell(q);if !valid_cell(at){return 0.0;}
 return work[index(at,p.dims.xyz)][i];
}
fn tensor_shear(q:vec3<i32>,i:u32,j:u32)->f32 {
 let at=wrap_cell(q);
 if !valid_cell(at) {return shear_stress(q,i,j,true);}
 let cell=index(at,p.dims.xyz);
 let plane=i+j;
 if plane==1u {return work[cell].w;}
 return output_fields[cell].pressure[plane-1u];
}
fn stress_acceleration(q:vec3<i32>,i:u32,cached:bool)->f32 {
 var left=q;left[i]-=1;var divergence=0.0;
 if cached {
  divergence=tensor_normal(q,i)-tensor_normal(left,i);
  for(var j=0u;j<3u;j++){if j!=i{
   var upper=q;upper[j]+=1;divergence+=tensor_shear(upper,i,j)-tensor_shear(q,i,j);
  }}
 } else {
  divergence=normal_stress(q,i,false)-normal_stress(left,i,false);
  for(var j=0u;j<3u;j++){if j!=i{
   var upper=q;upper[j]+=1;divergence+=shear_stress(upper,i,j,false)-shear_stress(q,i,j,false);
  }}
 }
 return divergence/(p.physical.x*face_density(q,i));
}
@compute @workgroup_size(256)
fn viscosity_bound(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 if group_id(group)>=(p.faces.w+255u)/256u{return;}
 let i=flat_id(g);var bound=0.0;
 if i<p.faces.w {
  let q=decode(i,p.faces.xyz);
  for(var component=0u;component<3u;component++){if open_face(q,component){
   var left=q;left[component]-=1;var sum=dynamic_mu(q)+dynamic_mu(left);
   for(var j=0u;j<3u;j++){if j!=component{
    var upper=q;upper[j]+=1;sum+=edge_mu(q,component,j)+edge_mu(upper,component,j);
   }}
   // Absolute row-sum bound of rho^-1 div(tau). Symmetry gives real
   // nonpositive eigenvalues; dt*bound<=1.8 stays inside Euler's stability disk.
   bound=max(bound,4.0*sum/(face_density(q,component)*p.physical.x*p.physical.x));
  }}
 }
 reduce[l]=vec4<f32>(bound,0.0,0.0,0.0);workgroupBarrier();
 for(var d=128u;d>0u;d/=2u){if l<d{reduce[l].x=max(reduce[l].x,reduce[l+d].x);}workgroupBarrier();}
 if l==0u{partials[group_id(group)]=reduce[0];}
}
@compute @workgroup_size(256)
fn viscosity_bound_reduce(@builtin(local_invocation_index) l:u32){
 var bound=0.0;for(var i=l;i<(p.faces.w+255u)/256u;i+=256u){bound=max(bound,partials[i].x);}
 reduce[l]=vec4<f32>(bound,0.0,0.0,0.0);workgroupBarrier();
 for(var d=128u;d>0u;d/=2u){if l<d{reduce[l].x=max(reduce[l].x,reduce[l+d].x);}workgroupBarrier();}
 if l==0u{scalars[0]=reduce[0];}
}
@compute @workgroup_size(256)
fn stress_diffuse(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat_id(g);if i>=p.faces.w{return;}let q=decode(i,p.faces.xyz);var out=vec4<f32>(0.0);
 for(var axis=0u;axis<3u;axis++){if open_face(q,axis){
  out[axis]=face(q,axis,true)+p.physical.y*stress_acceleration(q,axis,false);
 }}
 velocity[i]=out;
}
