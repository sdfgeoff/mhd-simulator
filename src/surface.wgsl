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
 output_field[i]=vec4<f32>(input_field[i].x,clamp(k,-2.0/p.step.y,2.0/p.step.y),0.0,0.0);
}
@compute @workgroup_size(256)
fn advect(@builtin(global_invocation_id) g:vec3<u32>){let i=flat(g);if i>=p.dims.w{return;}let q=point(i);var a=input_field[i].x;var c:Thermal;var h=0.0;
 if p.choice.y!=0u {c=thermal[i];h=c.state.z-c.valid.y;}
 if !wall(i){for(var axis=0u;axis<3u;axis++){for(var side=0u;side<2u;side++){
 var at=q;at[axis]+=select(-1,1,side==1u);if !inside(at)||wall(index(at)){continue;}
 var face=q;if side==1u{face[axis]+=1;}let fd=p.dims.xyz+vec3<u32>(1);let fi=u32(face.x)+fd.x*(u32(face.y)+fd.y*u32(face.z));let outward=velocity[fi][axis]*select(-1.0,1.0,side==1u);let donor=select(index(at),i,outward>=0.0);
 a-=p.step.x/p.step.y*outward*input_field[donor].x;
 if p.choice.y!=0u{h-=p.step.x/p.step.y*outward*(thermal[donor].state.z-thermal[donor].valid.y);}
 }}}
 // Flush negligible tails before f32 denormals corrupt specific enthalpy.
 // Volume/energy changes remain visible in the independent transport balances.
 if abs(a)<1e-12 {a=0.0;h=0.0;}
 output_field[i]=vec4<f32>(a,0.0,0.0,0.0);
 if p.choice.y!=0u {
  if !wall(i){c.state.z=h;c.state.w=a;c.valid=vec4<f32>(select(0.0,1.0,a>0.0),0.0,0.0,0.0);c.budget=vec4<f32>(0.0);}
  transported[i]=c;
 }
}
@compute @workgroup_size(256)
fn collect(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){let i=flat(g);var s=vec4<f32>(0.0);var b=vec4<f32>(1e30,-1e30,0.0,0.0);
 if i<p.dims.w{let f=input_field[i];s.x=f.x*p.step.y*p.step.y*p.step.y;s.y=normals[i].w*p.step.y*p.step.y/32.0;s.w=select(0.0,1.0,!(abs(f.x)<1e10)||!(abs(f.y)<1e10));b=vec4<f32>(f.x,f.x,select(0.0,abs(f.y),f.x>1e-6 && f.x<0.999999),0.0);}
 sums[l]=s;bounds[l]=b;workgroupBarrier();for(var d=128u;d>0u;d/=2u){if l<d{sums[l]+=sums[l+d];bounds[l]=vec4<f32>(min(bounds[l].x,bounds[l+d].x),max(bounds[l].yz,bounds[l+d].yz),0.0);}workgroupBarrier();}
 let gi=group.x+group.y*65535u;if l==0u{stats[gi*2u]=sums[0];stats[gi*2u+1u]=bounds[0];}
}
@compute @workgroup_size(256)
fn reduce(@builtin(local_invocation_index) l:u32){let n=(p.dims.w+255u)/256u;var s=vec4<f32>(0.0);var b=vec4<f32>(1e30,-1e30,0.0,0.0);
 for(var i=l;i<n;i+=256u){s+=stats[i*2u];let v=stats[i*2u+1u];b=vec4<f32>(min(b.x,v.x),max(b.yz,v.yz),0.0);}
 sums[l]=s;bounds[l]=b;workgroupBarrier();for(var d=128u;d>0u;d/=2u){if l<d{sums[l]+=sums[l+d];bounds[l]=vec4<f32>(min(bounds[l].x,bounds[l+d].x),max(bounds[l].yz,bounds[l+d].yz),0.0);}workgroupBarrier();}if l==0u{stats[n*2u]=sums[0];stats[n*2u+1u]=bounds[0];}
}
