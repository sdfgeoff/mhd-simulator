struct Params {dims:vec4<u32>,choice:vec4<u32>,step:vec4<f32>}
struct Thermal {state:vec4<f32>,flux:vec4<f32>,budget:vec4<f32>,valid:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read> original:array<u32>;
@group(0) @binding(2) var<storage,read> input_field:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read_write> output_field:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read> velocity:array<vec4<f32>>;
@group(0) @binding(5) var<storage,read> thermal:array<Thermal>;
@group(0) @binding(6) var<storage,read_write> transported:array<Thermal>;
@group(0) @binding(7) var<storage,read_write> normals:array<vec4<f32>>;
@group(0) @binding(8) var<storage,read_write> stats:array<vec4<f32>>;
var<workgroup> sums:array<vec4<f32>,256>;
var<workgroup> bounds:array<vec4<f32>,256>;
fn flat(g:vec3<u32>)->u32{return g.x+g.y*65535u*256u;}
fn index(q:vec3<i32>)->u32{return u32(q.x)+p.dims.x*(u32(q.y)+p.dims.y*u32(q.z));}
fn point(i:u32)->vec3<i32>{return vec3<i32>(i32(i%p.dims.x),i32((i/p.dims.x)%p.dims.y),i32(i/(p.dims.x*p.dims.y)));}
fn inside(q:vec3<i32>)->bool{return all(q>=vec3<i32>(0))&&all(q<vec3<i32>(p.dims.xyz));}
fn bounded(q:vec3<i32>)->vec3<i32>{return clamp(q,vec3<i32>(0),vec3<i32>(p.dims.xyz)-vec3<i32>(1));}
fn alpha(q:vec3<i32>)->f32{return input_field[index(bounded(q))].x;}
fn wall(i:u32)->bool{return original[i]!=0u && original[i]!=p.choice.x;}
@compute @workgroup_size(256)
fn initialize(@builtin(global_invocation_id) g:vec3<u32>){let i=flat(g);if i<p.dims.w{output_field[i]=vec4<f32>(select(0.0,1.0,original[i]==p.choice.x),0.0,0.0,0.0);}}
@compute @workgroup_size(256)
fn normal(@builtin(global_invocation_id) g:vec3<u32>){let i=flat(g);if i>=p.dims.w{return;}let q=point(i);var grad=vec3<f32>(0.0);
 for(var z=-1;z<=1;z++){for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){
 let o=vec3<i32>(x,y,z);let w=select(vec3<f32>(1.0),vec3<f32>(2.0),o==vec3<i32>(0));let a=alpha(q+o);
 grad+=vec3<f32>(f32(x)*w.y*w.z,f32(y)*w.x*w.z,f32(z)*w.x*w.y)*a;
 }}}let magnitude=length(grad);normals[i]=vec4<f32>(-grad/max(magnitude,1e-20),magnitude);
}
fn height(q:vec3<i32>,axis:u32)->vec2<f32>{var h=0.0;var low=q;low[axis]-=3;var high=q;high[axis]+=3;
 if !inside(low)||!inside(high){return vec2<f32>(0.0);}
 let a=alpha(low);let b=alpha(high);let valid=(a>0.99&&b<0.01)||(b>0.99&&a<0.01);
 for(var k=-3;k<=3;k++){var at=q;at[axis]+=k;h+=alpha(at);}
 return vec2<f32>(h,select(0.0,1.0,valid));
}
@compute @workgroup_size(256)
fn curvature(@builtin(global_invocation_id) g:vec3<u32>){let i=flat(g);if i>=p.dims.w{return;}let q=point(i);var k=0.0;
 for(var axis=0u;axis<3u;axis++){var l=q;var r=q;l[axis]-=1;r[axis]+=1;k+=(normals[index(bounded(r))][axis]-normals[index(bounded(l))][axis])/(2.0*p.step.y);}
 let n=abs(normals[i].xyz);var axis=0u;if n.y>n.x{axis=1u;}if n.z>n[axis]{axis=2u;}
 let a=(axis+1u)%3u;let b=(axis+2u)%3u;var heights:array<f32,9>;var valid=true;
 for(var v=-1;v<=1;v++){for(var u=-1;u<=1;u++){var at=q;at[a]+=u;at[b]+=v;let h=height(at,axis);heights[u32((u+1)+3*(v+1))]=h.x;valid=valid&&h.y>0.5;}}
 if valid {
 let hx=0.5*(heights[5]-heights[3]);let hy=0.5*(heights[7]-heights[1]);
 let hxx=(heights[5]+heights[3]-2.0*heights[4])/p.step.y;let hyy=(heights[7]+heights[1]-2.0*heights[4])/p.step.y;
 let hxy=(heights[8]-heights[6]-heights[2]+heights[0])/(4.0*p.step.y);
 k=-(hxx*(1.0+hy*hy)+hyy*(1.0+hx*hx)-2.0*hxy*hx*hy)/pow(1.0+hx*hx+hy*hy,1.5);
 }
 output_field[i]=vec4<f32>(input_field[i].x,clamp(k,-2.0/p.step.y,2.0/p.step.y),input_field[i].zw);
}
// During transport normals stores the PLIC plane (outward L1 normal, intercept).
// It is rebuilt from each intermediate fraction field and restored to curvature
// normals by refresh after all sweeps. Pure/degenerate cells use donor fluxes.
@compute @workgroup_size(256)
fn reconstruct(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat(g);if i>=p.dims.w{return;}let a=input_field[i].x;
 if a<=1e-6 || a>=1.0-1e-6 || wall(i){normals[i]=vec4<f32>(0.0);return;}
 let q=point(i);var grad=vec3<f32>(0.0);
 for(var z=-1;z<=1;z++){for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){
  let o=vec3<i32>(x,y,z);let w=select(vec3<f32>(1.0),vec3<f32>(2.0),o==vec3<i32>(0));
  grad-=vec3<f32>(f32(x)*w.y*w.z,f32(y)*w.x*w.z,f32(z)*w.x*w.y)*alpha(q+o);
 }}}
 let magnitude=abs(grad.x)+abs(grad.y)+abs(grad.z);
 if magnitude<=1e-12 {normals[i]=vec4<f32>(0.0);return;}
 let n=grad/magnitude;normals[i]=vec4<f32>(n,cube_intercept(n,a));
}
// Signed volume and enthalpy fluxes through one face, scaled by dt/h.
// Each neighboring cell evaluates this identical expression: no atomics or
// independently limited per-cell fluxes can break flux cancellation.
fn geometric_flux(at:vec3<i32>,axis:u32)->vec4<f32>{
 var left=at;left[axis]-=1;
 if !inside(at)||!inside(left){return vec4<f32>(0.0);}
 if wall(index(at))||wall(index(left)){return vec4<f32>(0.0);}
 let fd=p.dims.xyz+vec3<u32>(1);let fi=u32(at.x)+fd.x*(u32(at.y)+fd.y*u32(at.z));
 let displacement=velocity[fi][axis]*p.step.x/p.step.y;
 // Independently verify the actual face CFL, even if a caller supplied an
 // incorrect speed bound. Returning a flag avoids unsafe geometry arithmetic.
 if !(abs(displacement)<=0.150001) {return vec4<f32>(0.0,0.0,0.0,1.0);}
 if displacement==0.0{return vec4<f32>(0.0);}
 let donor=index(select(at,left,displacement>0.0));let a=input_field[donor].x;
 var fraction=a;let plane=normals[donor];
 if dot(abs(plane.xyz),vec3<f32>(1.0))>0.5 {
  fraction=slab_fraction(plane.xyz,plane.w,axis,abs(displacement),sign(displacement));
 }
 let flux=displacement*fraction;var heat=0.0;
 if p.choice.y!=0u && a>1e-12 {heat=flux*(thermal[donor].state.z-thermal[donor].valid.y)/a;}
 return vec4<f32>(flux,heat,displacement,0.0);
}
@compute @workgroup_size(256)
fn sweep(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat(g);if i>=p.dims.w{return;}let q=point(i);let axis=p.choice.z;
 let first=(p.choice.w&1u)!=0u;let last=(p.choice.w&2u)!=0u;
 var invalid=!first && input_field[i].z<0.0;
 let old_alpha=input_field[i].x;var a=old_alpha;
 var cc=max(input_field[i].z,0.0);var correction=input_field[i].w;
 if first {cc=select(0.0,1.0,a>0.5);correction=0.0;}
 var c:Thermal;var h=0.0;var hc=0.0;
 if p.choice.y!=0u {
  c=thermal[i];h=c.state.z-c.valid.y;hc=c.valid.z;
  if first {hc=0.0;if a>0.5 {hc=h/a;}}
 }
 if !wall(i){
  var upper=q;upper[axis]+=1;
  let low_flux=geometric_flux(q,axis);let high_flux=geometric_flux(upper,axis);
  invalid=invalid || low_flux.w>0.5 || high_flux.w>0.5;
  let divergence=high_flux.z-low_flux.z;
  a+=low_flux.x-high_flux.x+cc*divergence;correction+=cc*divergence;
  if p.choice.y!=0u {h+=low_flux.y-high_flux.y+hc*divergence;}
 }
 // Flush only unresolved tails, with their mass/enthalpy drift still reported.
 if abs(a)<1e-12 {a=0.0;h=0.0;}
 output_field[i]=vec4<f32>(a,0.0,select(cc,-1.0,invalid),correction);
 if p.choice.y!=0u {
  if !wall(i){c.state.z=h;c.state.w=a;c.valid=vec4<f32>(select(0.0,1.0,a>0.0),0.0,select(hc,0.0,last),0.0);c.budget=vec4<f32>(0.0);}
  transported[i]=c;
 }
}
@compute @workgroup_size(256)
fn collect(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){let i=flat(g);var s=vec4<f32>(0.0);var b=vec4<f32>(1e30,-1e30,0.0,0.0);
 if i<p.dims.w{let f=input_field[i];s.x=f.x*p.step.y*p.step.y*p.step.y;s.y=normals[i].w*p.step.y*p.step.y/32.0;s.z=f.w*p.step.y*p.step.y*p.step.y;s.w=select(0.0,1.0,!(abs(f.x)<1e10)||!(abs(f.y)<1e10)||f.z<0.0||!(abs(f.w)<1e10));b=vec4<f32>(f.x,f.x,select(0.0,abs(f.y),f.x>1e-6 && f.x<0.999999),0.0);}
 sums[l]=s;bounds[l]=b;workgroupBarrier();for(var d=128u;d>0u;d/=2u){if l<d{sums[l]+=sums[l+d];bounds[l]=vec4<f32>(min(bounds[l].x,bounds[l+d].x),max(bounds[l].yz,bounds[l+d].yz),0.0);}workgroupBarrier();}
 let gi=group.x+group.y*65535u;if l==0u{stats[gi*2u]=sums[0];stats[gi*2u+1u]=bounds[0];}
}
@compute @workgroup_size(256)
fn reduce(@builtin(local_invocation_index) l:u32){let n=(p.dims.w+255u)/256u;var s=vec4<f32>(0.0);var b=vec4<f32>(1e30,-1e30,0.0,0.0);
 for(var i=l;i<n;i+=256u){s+=stats[i*2u];let v=stats[i*2u+1u];b=vec4<f32>(min(b.x,v.x),max(b.yz,v.yz),0.0);}
 sums[l]=s;bounds[l]=b;workgroupBarrier();for(var d=128u;d>0u;d/=2u){if l<d{sums[l]+=sums[l+d];bounds[l]=vec4<f32>(min(bounds[l].x,bounds[l+d].x),max(bounds[l].yz,bounds[l+d].yz),0.0);}workgroupBarrier();}if l==0u{stats[n*2u]=sums[0];stats[n*2u+1u]=bounds[0];}
}
