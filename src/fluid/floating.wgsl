// Mean-free residual CG on the physical Neumann operator, one connected region.
// Scalars[2].z=1/N, [2].y=residual mean; [0].w=search mean.
@compute @workgroup_size(256)
fn floating_collect(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 if group_id(group)>=count_groups(){return;}
 let i=flat_id(gid);var value=vec4<f32>(0.);
 if i<p.dims.w&&mask[i]!=0u{value=vec4<f32>(1.,0.,0.,0.);}
 reduce[l]=value;let total=sum_group(l);if l==0u{partials[group_id(group)]=total;}
}
@compute @workgroup_size(256)
fn floating_prepare(@builtin(local_invocation_index) l:u32){
 var sum=vec4<f32>(0.);for(var i=l;i<count_groups();i+=256u){sum+=partials[i];}
 reduce[l]=sum;let total=sum_group(l);if l==0u{scalars[2].z=1./max(total.x,1.);scalars[0].w=0.;}
}
// Orthogonalize the residual without changing the physical acceptance norm.
@compute @workgroup_size(256)
fn floating_residual_reduce(@builtin(local_invocation_index) l:u32){
 var sum=vec4<f32>(0.);for(var i=l;i<count_groups();i+=256u){sum.x+=partials[i].w;}
 reduce[l]=sum;let total=sum_group(l);if l==0u{scalars[2].y=total.x*scalars[2].z;}
}
fn floating_project(gid:vec3<u32>,l:u32,group:vec3<u32>,initial:bool){
 if group_id(group)>=count_groups(){return;}
 let i=flat_id(gid);var value=vec4<f32>(0.);
 if i<p.dims.w&&mask[i]!=0u&&(initial||scalars[1].z<0.5){
  let r=work[i].y-scalars[2].y;let d=cached_diagonal(i);work[i].y=r;
  if initial{work[i].z=r/d;}
  value=vec4<f32>(r*r/d,r*r,0.,r/d);
 }
 reduce[l]=value;let total=sum_group(l);if l==0u{
  let old=partials[group_id(group)];
  partials[group_id(group)]=vec4<f32>(total.x,select(total.y,old.y,initial),old.z,total.w);
 }
}
@compute @workgroup_size(256)
fn floating_initial_project(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){floating_project(gid,l,group,true);}
@compute @workgroup_size(256)
fn floating_residual_project(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){floating_project(gid,l,group,false);}
@compute @workgroup_size(256)
fn search(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 if group_id(group)>=count_groups(){return;}
 let i=flat_id(gid);var value=0.;
 if i<p.dims.w&&mask[i]!=0u&&scalars[1].z<0.5{let v=work[i];value=v.y/v.w+scalars[1].y*v.z;work[i].z=value;}
 reduce[l]=vec4<f32>(value,0.,0.,0.);let total=sum_group(l);if l==0u{partials[group_id(group)].w=total.x;}
}
@compute @workgroup_size(256)
fn floating_direction_collect(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 if group_id(group)>=count_groups(){return;}
 let i=flat_id(gid);var value=0.;if i<p.dims.w&&mask[i]!=0u&&scalars[1].z<0.5{value=work[i].z;}
 reduce[l]=vec4<f32>(value,0.,0.,0.);let total=sum_group(l);if l==0u{partials[group_id(group)].w=total.x;}
}
@compute @workgroup_size(256)
fn floating_search_reduce(@builtin(local_invocation_index) l:u32){
 var sum=vec4<f32>(0.);for(var i=l;i<count_groups();i+=256u){sum.x+=partials[i].w;}
 reduce[l]=sum;let total=sum_group(l);if l==0u{scalars[0].w=total.x*scalars[2].z;}
}
