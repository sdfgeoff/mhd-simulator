// Diagnostic reductions after reconstruct. Restore normal/collect/reduce later.
fn audit_face(q0:vec3<i32>,axis:u32)->f32 {
 var q=q0;for(var a=0u;a<3u;a++){if periodic(a){q[a]=wrap_periodic(q[a],p.dims[a]);}}
 let d=p.dims.xyz+vec3<u32>(1);let u=vec3<u32>(q);
 return velocity[u.x+d.x*(u.y+d.y*u.z)][axis];
}
fn patch_collect(g:vec3<u32>,l:u32,group:vec3<u32>,kinetic:bool){
 let gi=group.x+group.y*65535u;if gi>=(p.dims.w+255u)/256u{return;}
 let i=flat(g);var value=vec4<f32>(0.);var phase=vec4<f32>(0.);
 if i<p.dims.w {
  let a=input_field[i].x;
  if !wall(i) && a>1e-6 && a<1.-1e-6 {
   let plane=normals[i];let area=plic_patch_area(plane.xyz,plane.w);
   let bad=!(area>=0. && area<=2.);
   value=vec4<f32>(area*p.step.y*p.step.y,1.,select(0.,1.,dot(abs(plane.xyz),vec3<f32>(1.))<0.5),select(0.,1.,bad));
  }
  if kinetic {
   let q=point(i);var face_energy=0.;
   for(var axis=0u;axis<3u;axis++){var upper=q;upper[axis]+=1;let lo=audit_face(q,axis);let hi=audit_face(upper,axis);face_energy+=0.5*(lo*lo+hi*hi);}
   let weight=0.5*face_energy*p.step.y*p.step.y*p.step.y;
   phase=vec4<f32>(p.step.z*a*weight,p.step.w*(1.-a)*weight,0.,0.);
  }
 }
 sums[l]=value;bounds[l]=phase;workgroupBarrier();
 for(var stride=128u;stride>0u;stride/=2u){if l<stride{sums[l]+=sums[l+stride];bounds[l]+=bounds[l+stride];}workgroupBarrier();}
 if l==0u {stats[gi*2u]=sums[0];stats[gi*2u+1u]=bounds[0];}
}
@compute @workgroup_size(256)
fn patch_area_collect(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){patch_collect(g,l,group,false);}
@compute @workgroup_size(256)
fn patch_energy_collect(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){patch_collect(g,l,group,true);}
@compute @workgroup_size(256)
fn patch_energy_reduce(@builtin(local_invocation_index) l:u32){
 let n=(p.dims.w+255u)/256u;var a=vec4<f32>(0.);var b=vec4<f32>(0.);
 for(var i=l;i<n;i+=256u){a+=stats[2u*i];b+=stats[2u*i+1u];}
 sums[l]=a;bounds[l]=b;workgroupBarrier();
 for(var stride=128u;stride>0u;stride/=2u){if l<stride{sums[l]+=sums[l+stride];bounds[l]+=bounds[l+stride];}workgroupBarrier();}
 if l==0u {stats[2u*n]=sums[0];stats[2u*n+1u]=bounds[0];}
}
