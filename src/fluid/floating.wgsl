// A_g=A+gamma*11^T/N. Only one connected region is supported.
// Scalars[2].z=1/N, [2].w=harmonic mean diagonal; [0].w=current vector mean.
@compute @workgroup_size(256)
fn floating_collect(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 if group_id(group)>=count_groups(){return;}
 let i=flat_id(gid);var value=vec4<f32>(0.);
 if i<p.dims.w&&mask[i]!=0u{value=vec4<f32>(1.,1./cached_diagonal(i),work[i].x,0.);}
 reduce[l]=value;let total=sum_group(l);if l==0u{partials[group_id(group)]=total;}
}
@compute @workgroup_size(256)
fn floating_prepare(@builtin(local_invocation_index) l:u32){
 var sum=vec4<f32>(0.);for(var i=l;i<count_groups();i+=256u){sum+=partials[i];}
 reduce[l]=sum;let total=sum_group(l);if l==0u{scalars[2].z=1./max(total.x,1.);scalars[2].w=total.x/max(total.y,1e-35);scalars[0].w=total.z*scalars[2].z;}
}
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
