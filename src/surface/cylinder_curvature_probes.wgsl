// Initial geometry only: analytic r(z) and exact surface-of-revolution curvature.
// Reduction writes only private storage; accepted fields/normals stay untouched.
@compute @workgroup_size(256)
fn cylinder_collect(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let gi=group.x+65535u*group.y;if gi>=(p.dims.w+255u)/256u{return;}
 let i=flat(g);var a=vec4<f32>(0.);var b=vec4<f32>(0.);
 if i<p.dims.w {
  let alpha=input_field[i].x;
  if alpha>1e-6&&alpha<1.-1e-6&&!wall(i){
   let q=point(i);let theta=6.283185307179586*(f32(q.z)+0.5)/f32(p.dims.z);
   let c=cos(theta);let r=CYLINDER_RADIUS*(1.+CYLINDER_EPSILON*c);
   let rz=-CYLINDER_RADIUS*CYLINDER_EPSILON*CYLINDER_K*sin(theta);
   let rzz=-CYLINDER_RADIUS*CYLINDER_EPSILON*CYLINDER_K*CYLINDER_K*c;
   let slope=1.+rz*rz;let reference=1./(r*sqrt(slope))-rzz/pow(slope,1.5);
   let kappa=input_field[i].y;let w=alpha*(1.-alpha);let error=kappa-reference;
   a=w*vec4<f32>(1.,c,c*c,kappa);b=w*vec4<f32>(kappa*c,reference,reference*c,error*error);
  }
 }
 sums[l]=a;bounds[l]=b;workgroupBarrier();
 for(var stride=128u;stride>0u;stride/=2u){if l<stride{sums[l]+=sums[l+stride];bounds[l]+=bounds[l+stride];}workgroupBarrier();}
 if l==0u{stats[2u*gi]=sums[0];stats[2u*gi+1u]=bounds[0];}
}
@compute @workgroup_size(256)
fn cylinder_reduce(@builtin(local_invocation_index) l:u32){
 let count=(p.dims.w+255u)/256u;var a=vec4<f32>(0.);var b=vec4<f32>(0.);
 for(var i=l;i<count;i+=256u){a+=stats[2u*i];b+=stats[2u*i+1u];}
 sums[l]=a;bounds[l]=b;workgroupBarrier();
 for(var stride=128u;stride>0u;stride/=2u){if l<stride{sums[l]+=sums[l+stride];bounds[l]+=bounds[l+stride];}workgroupBarrier();}
 if l==0u{stats[2u*count]=sums[0];stats[2u*count+1u]=bounds[0];}
}
