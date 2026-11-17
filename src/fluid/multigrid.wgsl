// Fixed symmetric aggregation V-cycle. Fine coefficients retain the original
// MAC indexing; coarse edges are the exact constant-prolongation Galerkin graph.
struct Level {dims:vec4<u32>,child:vec4<u32>,faces:vec4<u32>,location:vec4<u32>}
@group(0) @binding(0) var<uniform> p:Level;
@group(0) @binding(1) var<storage,read> mask:array<u32>;
@group(0) @binding(2) var<storage,read> cache:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read_write> work:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read_write> predicted:array<vec4<f32>>;
@group(0) @binding(5) var<storage,read_write> coeff:array<vec4<f32>>;
@group(0) @binding(6) var<storage,read_write> value:array<vec4<f32>>;
@group(0) @binding(7) var<storage,read_write> partials:array<vec4<f32>>;
@group(0) @binding(8) var<storage,read> scalars:array<vec4<f32>>;
var<workgroup> reduction:array<f32,256>;
// location: current offset, child offset, level index, periodic bits.
fn index(q:vec3<i32>,d:vec3<u32>)->u32 {let u=vec3<u32>(q);return u.x+d.x*(u.y+d.y*u.z);}
fn decode(i:u32,d:vec3<u32>)->vec3<i32> {return vec3<i32>(vec3<u32>(i%d.x,(i/d.x)%d.y,i/(d.x*d.y)));}
fn flat(g:vec3<u32>)->u32 {return g.x+g.y*65535u*256u;}
fn canonical(q0:vec3<i32>,d:vec3<u32>)->vec3<i32> {
 var q=q0;for(var a=0u;a<3u;a++){if (p.location.w&(1u<<a))!=0u {q[a]=wrap_periodic(q[a],d[a]);}}return q;
}
fn valid(q:vec3<i32>,d:vec3<u32>)->bool {return all(q>=vec3<i32>(0))&&all(q<vec3<i32>(d));}
fn fine_active(i:u32)->bool {return (mask[i]&3u)==1u;}
fn edge(q:vec3<i32>,axis:u32,sign:i32,d:vec3<u32>,offset:u32,fine:bool)->f32 {
 var face=q;if sign>0 {face[axis]+=1;}face=canonical(face,d);
 if fine {return cache[index(face,p.faces.xyz)][axis];}
 return coeff[offset+index(face,d)][axis];
}
fn solution(i:u32,offset:u32,fine:bool,temp:bool)->f32 {
 if fine {if temp{return predicted[i].w;}return work[i].w;}
 let v=value[offset+i];return select(v.y,v.z,temp);
}
fn action(q:vec3<i32>,d:vec3<u32>,offset:u32,fine:bool,temp:bool)->f32 {
 let i=index(q,d);let center=solution(i,offset,fine,temp);var out=0.;
 if fine {
  // A nonzero cached face certifies a valid neighbor. Every cycle producer
  // writes zero correction at walls/anchors, so neighbor masks are redundant.
  for(var a=0u;a<3u;a++){for(var s=-1;s<=1;s+=2){
   let beta=edge(q,a,s,d,offset,true);if beta>0. {
    var n=q;n[a]+=s;n=canonical(n,d);out+=beta*(center-solution(index(n,d),offset,true,temp));
   }
  }}return out;
 }
 out=coeff[offset+i].w*center;
 for(var a=0u;a<3u;a++){for(var s=-1;s<=1;s+=2){
  var n=q;n[a]+=s;n=canonical(n,d);if !valid(n,d){continue;}
  let beta=edge(q,a,s,d,offset,false);if beta>0. {out+=beta*(center-solution(index(n,d),offset,false,temp));}
 }}return out;
}
@compute @workgroup_size(256)
fn build(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat(g);if i>=p.dims.w{return;}let q=decode(i,p.dims.xyz);var c=vec4<f32>(0.);var diag=0.;
 let fine=p.location.z==1u;
 for(var z=0;z<2;z++){for(var y=0;y<2;y++){for(var x=0;x<2;x++){
  let child=2*q+vec3<i32>(x,y,z);if !valid(child,p.child.xyz){continue;}let ci=index(child,p.child.xyz);
  if fine {if !fine_active(ci){continue;}}else{let gamma=coeff[p.location.y+ci].w;c.w+=gamma;diag+=gamma;}
  for(var a=0u;a<3u;a++){for(var s=-1;s<=1;s+=2){
   var n=child;n[a]+=s;n=canonical(n,p.child.xyz);if !valid(n,p.child.xyz){continue;}
   let beta=edge(child,a,s,p.child.xyz,p.location.y,fine);if beta==0.{continue;}
   if fine && (mask[index(n,p.child.xyz)]&3u)==2u {c.w+=beta;diag+=beta;continue;}
   let parent=n/2;if all(parent==q){continue;}diag+=beta;if s<0{c[a]+=beta;}
  }}
 }}}
 coeff[p.location.x+i]=c;value[p.location.x+i]=vec4<f32>(0.,0.,0.,diag);
}
fn jacobi_step(i:u32,temp:bool){if scalars[1].z>0.5{return;}
 if i>=p.dims.w{return;}let fine=p.location.z==0u;let q=decode(i,p.dims.xyz);var d=0.;var rhs=0.;
 if fine {d=cache[i].w;rhs=work[i].y;if !fine_active(i){d=0.;}}else{d=value[p.location.x+i].w;rhs=value[p.location.x+i].x;}
 var next=0.;if d>0.{next=solution(i,p.location.x,fine,temp)+(2./3.)*(rhs-action(q,p.dims.xyz,p.location.x,fine,temp))/d;}
 if fine {if temp{work[i].w=next;}else{predicted[i].w=next;}}else{if temp{value[p.location.x+i].y=next;}else{value[p.location.x+i].z=next;}}
}
@compute @workgroup_size(256)
fn smooth_to_temp(@builtin(global_invocation_id) g:vec3<u32>){jacobi_step(flat(g),false);}
@compute @workgroup_size(256)
fn restrict_residual(@builtin(global_invocation_id) g:vec3<u32>){if scalars[1].z>0.5{return;}
 let i=flat(g);if i>=p.dims.w{return;}let q=decode(i,p.dims.xyz);let fine=p.location.z==1u;var rhs=0.;
 for(var z=0;z<2;z++){for(var y=0;y<2;y++){for(var x=0;x<2;x++){
  let child=2*q+vec3<i32>(x,y,z);if !valid(child,p.child.xyz){continue;}let ci=index(child,p.child.xyz);
  var r=0.;if fine {if !fine_active(ci){continue;}r=work[ci].y;}else{r=value[p.location.y+ci].x;}
  rhs+=r-action(child,p.child.xyz,p.location.y,fine,false);
 }}}
 value[p.location.x+i]=vec4<f32>(rhs,0.,0.,value[p.location.x+i].w);
}
@compute @workgroup_size(256)
fn prolong(@builtin(global_invocation_id) g:vec3<u32>){if scalars[1].z>0.5{return;}
 let i=flat(g);if i>=p.child.w{return;}let q=decode(i,p.child.xyz);let correction=value[p.location.x+index(q/2,p.dims.xyz)].y;
 if p.location.z==1u {if fine_active(i){work[i].w+=correction;}}else{value[p.location.y+i].y+=correction;}
}

fn dot_partial(g:vec3<u32>,local:u32,group:vec3<u32>,initial:bool){
 let groups=(p.dims.w+255u)/256u;let gi=group.x+group.y*65535u;if gi>=groups{return;}
 let i=flat(g);var v=0.;if i<p.dims.w && fine_active(i) && scalars[1].z<0.5 {v=work[i].y*work[i].w;if initial{work[i].z=work[i].w;}}
 reduction[local]=v;workgroupBarrier();
 for(var stride=128u;stride>0u;stride/=2u){if local<stride{reduction[local]+=reduction[local+stride];}workgroupBarrier();}
 if local==0u && scalars[1].z<0.5 {partials[gi].x=reduction[0];}
}
@compute @workgroup_size(256)
fn initial_direction(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){dot_partial(g,l,group,true);}
@compute @workgroup_size(256)
fn preconditioned_dot(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){dot_partial(g,l,group,false);}
@compute @workgroup_size(256)
fn preconditioned_search(@builtin(global_invocation_id) g:vec3<u32>){let i=flat(g);if i>=p.dims.w || !fine_active(i) || scalars[1].z>0.5{return;}work[i].z=work[i].w+scalars[1].y*work[i].z;}

// The coarsest grid has at most 4³ cells. Keep its fixed Jacobi sequence
// within one workgroup instead of launching many nearly empty dispatches.
var<workgroup> coarse_solution:array<f32,256>;
@compute @workgroup_size(256)
fn coarse_solve(@builtin(local_invocation_index) i:u32){
 if scalars[1].z>0.5{return;}
 let fine=p.location.z==0u;let in_grid=i<p.dims.w;
 var rhs=0.;var diag=0.;var reaction=0.;var q=vec3<i32>(0);
 if in_grid {
  q=decode(i,p.dims.xyz);
  if fine {if fine_active(i){rhs=work[i].y;diag=cache[i].w;}}else{rhs=value[p.location.x+i].x;diag=value[p.location.x+i].w;reaction=coeff[p.location.x+i].w;}
 }
 coarse_solution[i]=0.;workgroupBarrier();
 for(var step=0u;step<COARSE_SWEEPS;step++){
  let center=coarse_solution[i];var next=0.;
  if in_grid && diag>0. {
   var a=reaction*center;
   for(var axis=0u;axis<3u;axis++){for(var s=-1;s<=1;s+=2){
    var n=q;n[axis]+=s;n=canonical(n,p.dims.xyz);if !valid(n,p.dims.xyz){continue;}
    let ni=index(n,p.dims.xyz);var neighbor=coarse_solution[ni];if fine && !fine_active(ni){neighbor=0.;}
    a+=edge(q,axis,s,p.dims.xyz,p.location.x,fine)*(center-neighbor);
   }}
   next=center+(2./3.)*(rhs-a)/diag;
  }
  workgroupBarrier();coarse_solution[i]=next;workgroupBarrier();
 }
 if in_grid {if fine {work[i].w=coarse_solution[i];}else{value[p.location.x+i].y=coarse_solution[i];}}
}

// Every level starts from zero, so the first Jacobi sweep is diagonal-only.
@compute @workgroup_size(256)
fn diagonal_start(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat(g);if i>=p.dims.w || scalars[1].z>0.5{return;}var z=0.;
 if p.location.z==0u {if fine_active(i){z=(2./3.)*work[i].y/cache[i].w;}work[i].w=z;}
 else {let v=value[p.location.x+i];if v.w>0.{z=(2./3.)*v.x/v.w;}value[p.location.x+i].y=z;}
}
@compute @workgroup_size(256)
fn commit_smooth(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat(g);if i>=p.dims.w || scalars[1].z>0.5{return;}
 if p.location.z==0u {work[i].w=predicted[i].w;}else{value[p.location.x+i].y=value[p.location.x+i].z;}
}
