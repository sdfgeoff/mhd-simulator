// Read-only comparison with complete, monotone columns over radii 3..6.
// Constants HEIGHT_TARGET and HEIGHT_MARGIN are supplied by the diagnostic.
var<workgroup> height_extra:array<vec4<f32>,256>;
fn complete_range(q:vec3<i32>,axis:u32,radius:i32,reference:vec3<i32>)->vec3<f32>{
 var center=bounded(q);center[axis]=q[axis];var lo=center;var hi=center;lo[axis]-=radius;hi[axis]+=radius;
 if !inside(lo)||!inside(hi){return vec3<f32>(0.);}
 let a=alpha(lo,reference);let b=alpha(hi,reference);
 if !((a>=1.-1e-6 && b<=1e-6)||(b>=1.-1e-6 && a<=1e-6)){return vec3<f32>(0.);}
 return height_range(q,axis,radius,reference);
}
fn complete_height(q:vec3<i32>,axis:u32,reference:vec3<i32>)->vec3<f32>{
 let h3=complete_range(q,axis,3,reference);if h3.y>0.5{return h3;}
 let h4=complete_range(q,axis,4,reference);if h4.y>0.5{return h4;}
 let h5=complete_range(q,axis,5,reference);if h5.y>0.5{return h5;}
 return complete_range(q,axis,6,reference);
}
// x=strict curvature, y=current HF valid, z=strict HF valid, w=column difference.
fn height_comparison(q:vec3<i32>,i:u32)->vec4<f32>{
 let n=abs(normals[i].xyz);if !any(n>vec3<f32>(0.)){return vec4<f32>(0.);}
 var axis=0u;if n.y>n.x{axis=1u;}if n.z>n[axis]{axis=2u;}
 let a=(axis+1u)%3u;let b=(axis+2u)%3u;let orientation=sign(normals[i][axis]);
 var heights:array<f32,9>;var current=true;var strict=true;var difference=0.;
 for(var v=-1;v<=1;v++){for(var u=-1;u<=1;u++){
  var at=q;at[a]+=u;at[b]+=v;let hc=curvature_height(at,axis,q);let hs=complete_height(at,axis,q);
  let cv=hc.y>0.5&&hc.z==orientation;let sv=hs.y>0.5&&hs.z==orientation;
  current=current&&cv;strict=strict&&sv;heights[u32((u+1)+3*(v+1))]=hs.x;
  if cv&&sv{difference=max(difference,abs(hc.x-hs.x));}
 }}
 var k=0.;if strict {
  let hx=0.5*(heights[5]-heights[3]);let hy=0.5*(heights[7]-heights[1]);let h=height_hessian(heights,p.step.y);
  k=-(h.x*(1.+hy*hy)+h.y*(1.+hx*hx)-2.*h.z*hx*hy)/pow(1.+hx*hx+hy*hy,1.5);
  k=clamp(k,-2./p.step.y,2./p.step.y);
 }
 return vec4<f32>(k,select(0.,1.,current),select(0.,1.,strict),select(0.,difference,current&&strict));
}
@compute @workgroup_size(256)
fn height_probe_collect(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let gi=group.x+group.y*65535u;if gi>=(p.dims.w+255u)/256u{return;}
 let i=flat(g);var counts=vec4<f32>(0.);var errors=vec4<f32>(0.);var extra=vec4<f32>(0.);
 if i<p.dims.w && !wall(i) {
  let q=point(i);let fraction=input_field[i].x;
  if fraction>1e-6 && fraction<1.-1e-6 && all(q>=vec3<i32>(i32(HEIGHT_MARGIN))) && all(q<vec3<i32>(p.dims.xyz)-vec3<i32>(i32(HEIGHT_MARGIN))) {
   let h=height_comparison(q,i);let both=h.y>0.5&&h.z>0.5;let recovered=h.y<0.5&&h.z>0.5;
   let scale=max(abs(HEIGHT_TARGET),1.);let ce=(input_field[i].y-HEIGHT_TARGET)/scale;let se=(h.x-HEIGHT_TARGET)/scale;
   counts=vec4<f32>(1.,h.y,h.z,select(0.,1.,both));
   errors=vec4<f32>(ce*ce,select(0.,ce*ce,both),select(0.,se*se,both),select(0.,ce*ce,recovered));
   extra=vec4<f32>(select(0.,se*se,recovered),select(0.,input_field[i].y,both),select(0.,h.x,both),h.w);
  }
 }
 sums[l]=counts;bounds[l]=errors;height_extra[l]=extra;workgroupBarrier();
 for(var stride=128u;stride>0u;stride/=2u){if l<stride{sums[l]+=sums[l+stride];bounds[l]+=bounds[l+stride];height_extra[l]=vec4<f32>(height_extra[l].xyz+height_extra[l+stride].xyz,max(height_extra[l].w,height_extra[l+stride].w));}workgroupBarrier();}
 if l==0u {stats[3u*gi]=sums[0];stats[3u*gi+1u]=bounds[0];stats[3u*gi+2u]=height_extra[0];}
}
@compute @workgroup_size(256)
fn height_probe_reduce(@builtin(local_invocation_index) l:u32){
 let n=(p.dims.w+255u)/256u;var a=vec4<f32>(0.);var b=vec4<f32>(0.);var c=vec4<f32>(0.);
 for(var i=l;i<n;i+=256u){a+=stats[3u*i];b+=stats[3u*i+1u];let x=stats[3u*i+2u];c=vec4<f32>(c.xyz+x.xyz,max(c.w,x.w));}
 sums[l]=a;bounds[l]=b;height_extra[l]=c;workgroupBarrier();
 for(var stride=128u;stride>0u;stride/=2u){if l<stride{sums[l]+=sums[l+stride];bounds[l]+=bounds[l+stride];height_extra[l]=vec4<f32>(height_extra[l].xyz+height_extra[l+stride].xyz,max(height_extra[l].w,height_extra[l+stride].w));}workgroupBarrier();}
 if l==0u {stats[3u*n]=sums[0];stats[3u*n+1u]=bounds[0];stats[3u*n+2u]=height_extra[0];}
}

// Diagnostic only: try all axes in descending normal magnitude, as in the
// standard height-function selection strategy. No production field is written.
fn height_on_axis(q:vec3<i32>,i:u32,axis:u32)->vec2<f32>{
 let orientation=sign(normals[i][axis]);if orientation==0.{return vec2<f32>(0.);}
 let a=(axis+1u)%3u;let b=(axis+2u)%3u;var values:array<f32,9>;
 for(var v=-1;v<=1;v++){for(var u=-1;u<=1;u++){
  var at=q;at[a]+=u;at[b]+=v;let h=curvature_height(at,axis,q);
  if h.y<0.5||h.z!=orientation{return vec2<f32>(0.);}
  values[u32((u+1)+3*(v+1))]=h.x;
 }}
 let hx=.5*(values[5]-values[3]);let hy=.5*(values[7]-values[1]);let h=height_hessian(values,p.step.y);
 let k=-(h.x*(1.+hy*hy)+h.y*(1.+hx*hx)-2.*h.z*hx*hy)/pow(1.+hx*hx+hy*hy,1.5);
 return vec2<f32>(clamp(k,-2./p.step.y,2./p.step.y),1.);
}
@compute @workgroup_size(256)
fn height_axis_probe_collect(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let gi=group.x+group.y*65535u;if gi>=(p.dims.w+255u)/256u{return;}
 let i=flat(g);var counts=vec4<f32>(0.);var errors=vec4<f32>(0.);var extra=vec4<f32>(0.);
 if i<p.dims.w&&!wall(i) {
  let q=point(i);let fraction=input_field[i].x;
  if fraction>1e-6&&fraction<1.-1e-6 {
   var order=array<u32,3>(0u,1u,2u);let n=abs(normals[i].xyz);
   for(var a=0u;a<2u;a++){for(var b=a+1u;b<3u;b++){
    if n[order[b]]>n[order[a]]{let old=order[a];order[a]=order[b];order[b]=old;}
   }}
   let first=height_on_axis(q,i,order[0]);var selected=first;
   for(var a=1u;a<3u&&selected.y<.5;a++){selected=height_on_axis(q,i,order[a]);}
   let chosen=select(input_field[i].y,selected.x,selected.y>.5);
   let scale=max(abs(HEIGHT_TARGET),1.);let before=(input_field[i].y-HEIGHT_TARGET)/scale;let after=(chosen-HEIGHT_TARGET)/scale;
   counts=vec4<f32>(1.,first.y,selected.y,select(0.,1.,first.y<.5&&selected.y>.5));
   errors=vec4<f32>(before*before,after*after,0.,0.);
   extra=vec4<f32>(chosen,input_field[i].y,0.,abs(chosen-input_field[i].y));
  }
 }
 sums[l]=counts;bounds[l]=errors;height_extra[l]=extra;workgroupBarrier();
 for(var stride=128u;stride>0u;stride/=2u){if l<stride{sums[l]+=sums[l+stride];bounds[l]+=bounds[l+stride];height_extra[l]=vec4<f32>(height_extra[l].xyz+height_extra[l+stride].xyz,max(height_extra[l].w,height_extra[l+stride].w));}workgroupBarrier();}
 if l==0u{stats[3u*gi]=sums[0];stats[3u*gi+1u]=bounds[0];stats[3u*gi+2u]=height_extra[0];}
}
