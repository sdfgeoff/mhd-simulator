// Test-only volume/Fourier moments. Only the private reduction buffer is written.
@compute @workgroup_size(256)
fn cylinder_collect(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let gi=group.x+256u*group.y;if gi>=(p.dims.w+255u)/256u{return;}
 let i=flat(g);var a=vec4<f32>(0.);var b=vec4<f32>(0.);
 if i<p.dims.w {
  let q=point(i);let alpha=input_field[i].x;let phase=6.283185307179586*(f32(q.z)+0.5)/f32(p.dims.z);
  a=alpha*vec4<f32>(1.,cos(phase),sin(phase),cos(2.*phase));
  let xy=vec2<f32>(q.xy)+vec2<f32>(0.5)-0.5*vec2<f32>(p.dims.xy);
  b=vec4<f32>(alpha*xy,alpha*sin(2.*phase),select(0.,1.,alpha>1e-6&&alpha<1.-1e-6));
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
