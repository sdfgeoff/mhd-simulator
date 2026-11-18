// Mean-free CG residuals for a single connected Neumann pressure graph.
// The independent physical residual retains its original, unprojected norm.
@compute @workgroup_size(256)
fn pressure_residual_mean(@builtin(local_invocation_index) l:u32){
 var sum=vec4<f32>(0.);for(var i=l;i<count_groups();i+=256u){sum.x+=partials[i].w;}
 reduce[l]=sum;let total=sum_group(l);if l==0u{scalars[2].y=total.x*p.initial.w;}
}
fn project_pressure_residual(gid:vec3<u32>,l:u32,group:vec3<u32>,initial:bool){
 if group_id(group)>=count_groups(){return;}
 let i=flat_id(gid);var value=vec4<f32>(0.);
 if i<p.dims.w&&mask[i]!=0u&&(initial||scalars[1].z<0.5){
  let r=work[i].y-scalars[2].y;let d=cached_diagonal(i);work[i].y=r;
  if initial{work[i].z=r/d;}
  value=vec4<f32>(r*r/d,r*r,0.,0.);
 }
 reduce[l]=value;let total=sum_group(l);if l==0u{
  let old=partials[group_id(group)];
  partials[group_id(group)]=vec4<f32>(total.x,select(total.y,old.y,initial),old.z,0.);
 }
}
@compute @workgroup_size(256)
fn pressure_initial_subspace(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){project_pressure_residual(gid,l,group,true);}
@compute @workgroup_size(256)
fn pressure_residual_subspace(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){project_pressure_residual(gid,l,group,false);}
