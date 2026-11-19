// Research DC conduction: cell-centred finite volumes, insulated exterior,
// Dirichlet terminal cells, harmonic face conductivity. No CPU field solver.
struct Params {dims:vec4<u32>,physical:vec4<f32>,control:vec4<u32>,extra:vec4<f32>}
struct Field {b:vec4<f32>,j:vec4<f32>,force_heat:vec4<f32>,applied:vec4<f32>}
@group(0) @binding(0) var<uniform> cfg:Params;
// x = sigma / 1e6, y = terminal voltage or 2 for an unknown potential.
@group(0) @binding(1) var<storage,read_write> cells:array<vec2<f32>>;
@group(0) @binding(2) var<storage,read_write> work:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read_write> partials:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read_write> scalars:array<vec4<f32>>;
@group(0) @binding(5) var<storage,read> bias:array<Field>;
@group(0) @binding(6) var<storage,read_write> output:array<Field>;
@group(0) @binding(7) var<storage,read> flow:array<Field>;
// Unit/motional high words and their low corrections. The low words also
// accumulate updates to the active solution, without adding a storage binding.
@group(0) @binding(8) var<storage,read_write> unit_potential:array<vec4<f32>>;
var<workgroup> scratch:array<vec4<f32>,256>;
fn groups()->u32{return (cfg.dims.w+255u)/256u;}
fn xyz(i:u32)->vec3<i32>{return vec3<i32>(vec3<u32>(i%cfg.dims.x,(i/cfg.dims.x)%cfg.dims.y,i/(cfg.dims.x*cfg.dims.y)));}
fn index(q:vec3<i32>)->u32{return u32(q.x)+cfg.dims.x*(u32(q.y)+cfg.dims.y*u32(q.z));}
fn valid(q:vec3<i32>)->bool{return all(q>=vec3<i32>(0))&&all(q<vec3<i32>(cfg.dims.xyz));}
fn unknown(i:u32)->bool{return cells[i].x>0.0&&cells[i].y>1.0;}
fn contact(i:u32,j:u32)->f32{if (cells[i].y==3.0)!=(cells[j].y==3.0){return cfg.physical.w;}return 0.0;}
fn coefficient(i:u32,j:u32)->f32{let a=cells[i].x;let b=cells[j].x;if a<=0.0||b<=0.0{return 0.0;}return 2.0*a*b/(a+b+2.0*contact(i,j)*1e6/cfg.physical.x*a*b);}
fn terminal(i:u32)->f32{if cfg.control.x==0u{return cells[i].y;}return 0.0;}
fn emf(i:u32)->vec3<f32>{if cfg.control.y==0u||cfg.control.x==0u{return vec3<f32>(0.0);}return cross(flow[i].b.xyz,bias[i].b.xyz);}
fn correction(i:u32)->f32{return unit_potential[i][cfg.control.x+2u];}
fn raw_difference(i:u32,j:u32)->f32{return (work[i].x-work[j].x)+(correction(i)-correction(j));}
fn potential_difference(i:u32,j:u32)->f32{let d=raw_difference(i,j);if cfg.control.x==0u{return d*cfg.physical.y;}return d+cfg.physical.y*((unit_potential[i].x-unit_potential[j].x)+(unit_potential[i].z-unit_potential[j].z));}
@compute @workgroup_size(256)
fn update_phase(@builtin(global_invocation_id) g:vec3<u32>){let i=g.x;if i<cfg.dims.w&&cfg.control.y!=0u&&cells[i].y==3.0{cells[i].x=clamp(flow[i].applied.z,0.0,1.0)*cfg.extra.x;}}
@compute @workgroup_size(256)
fn initial_guess(@builtin(global_invocation_id) g:vec3<u32>){let i=g.x;if i<cfg.dims.w{var x=0.0;if unknown(i)&&cfg.control.z!=0u{x=unit_potential[i][cfg.control.x];}else{unit_potential[i][cfg.control.x+2u]=0.0;if cells[i].x>0.0&&!unknown(i){x=terminal(i);}}work[i]=vec4<f32>(x,0.0,0.0,0.0);}}
@compute @workgroup_size(256)
fn clear_solution(@builtin(global_invocation_id) g:vec3<u32>){let i=g.x;if i<cfg.dims.w{var x=0.0;if cells[i].x>0.0&&!unknown(i){x=terminal(i);}unit_potential[i][cfg.control.x+2u]=0.0;work[i]=vec4<f32>(x,0.0,0.0,0.0);}}
@compute @workgroup_size(256)
fn save_motion(@builtin(global_invocation_id) g:vec3<u32>){let i=g.x;if i<cfg.dims.w{unit_potential[i].y=work[i].x;}}
@compute @workgroup_size(256)
fn save_unit(@builtin(global_invocation_id) g:vec3<u32>){let i=g.x;if i<cfg.dims.w{unit_potential[i].x=work[i].x;}}
fn diagonal(i:u32)->f32{
 let q=xyz(i);var d=0.0;
 for(var a=0u;a<3u;a++){for(var sign=-1;sign<=1;sign+=2){var n=q;n[a]+=sign;if valid(n){d+=coefficient(i,index(n));}}}
 return max(d,1e-30);
}
fn rhs(i:u32)->f32{
 if !unknown(i){return 0.0;}let q=xyz(i);var b=0.0;
 for(var a=0u;a<3u;a++){for(var sign=-1;sign<=1;sign+=2){var n=q;n[a]+=sign;if valid(n){let j=index(n);if cells[j].y<=1.0{b+=coefficient(i,j)*terminal(j);}b-=cfg.physical.x*coefficient(i,j)*0.5*(emf(i)[a]+emf(j)[a])*f32(sign);}}}
 return b;
}
fn matrix_value(i:u32,component:u32)->f32{
 if !unknown(i){return 0.0;}let q=xyz(i);var result=0.0;let v=work[i][component];
 for(var a=0u;a<3u;a++){for(var sign=-1;sign<=1;sign+=2){var n=q;n[a]+=sign;if valid(n){let j=index(n);var w=0.0;if unknown(j){w=work[j][component];}result+=coefficient(i,j)*(v-w);}}}
 return result;
}
// Form the physical residual from face differences. In particular, subtract
// terminal and adjacent potentials before multiplying by a large conductance.
fn residual_value(i:u32)->f32{
 if !unknown(i){return 0.0;}let q=xyz(i);var result=0.0;
 for(var a=0u;a<3u;a++){for(var sign=-1;sign<=1;sign+=2){var n=q;n[a]+=sign;if valid(n){let j=index(n);let c=coefficient(i,j);result+=c*raw_difference(j,i);result-=cfg.physical.x*c*0.5*(emf(i)[a]+emf(j)[a])*f32(sign);}}}
 return result;
}
fn reduce(value:vec4<f32>,l:u32,group:u32){
 scratch[l]=value;workgroupBarrier();
 for(var s=128u;s>0u;s/=2u){if l<s{scratch[l]+=scratch[l+s];}workgroupBarrier();}
 if l==0u{partials[group]=scratch[0];}
}
fn sum_partials(l:u32)->vec4<f32>{var v=vec4<f32>(0.0);for(var i=l;i<groups();i+=256u){v+=partials[i];}scratch[l]=v;workgroupBarrier();for(var s=128u;s>0u;s/=2u){if l<s{scratch[l]+=scratch[l+s];}workgroupBarrier();}return scratch[0];}
fn cg_active()->bool{return scalars[0].y>cfg.extra.y*cfg.extra.y*scalars[0].z;}
@compute @workgroup_size(256)
fn initialize(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let i=g.x;var v=vec4<f32>(0.0);if i<cfg.dims.w{
  if unknown(i){let b=rhs(i);let r=residual_value(i);let z=r/diagonal(i);work[i].y=r;work[i].z=z;work[i].w=0.0;v=vec4<f32>(r*z,r*r,b*b,0.0);}else if cells[i].x>0.0{work[i].x=terminal(i);}
 }reduce(v,l,group.x);
}
@compute @workgroup_size(256)
fn initial_reduce(@builtin(local_invocation_index) l:u32){let v=sum_partials(l);if l==0u{scalars[0]=v;}}
@compute @workgroup_size(256)
fn apply_operator(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let i=g.x;var v=vec4<f32>(0.0);if i<cfg.dims.w&&unknown(i){let a=matrix_value(i,2u);work[i].w=a;v.x=work[i].z*a;}reduce(v,l,group.x);
}
@compute @workgroup_size(256)
fn alpha_reduce(@builtin(local_invocation_index) l:u32){let v=sum_partials(l);if l==0u{scalars[0].w=0.0;if cg_active()&&v.x>0.0{scalars[0].w=scalars[0].x/v.x;}}}
@compute @workgroup_size(256)
fn update_residual(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let i=g.x;var v=vec4<f32>(0.0);if i<cfg.dims.w&&unknown(i){
  // Keep twenty high-word mantissa bits. Clearing thirteen bits made the low
  // word too large: subtracting adjacent nearly-equal copper potentials then
  // lost the tiny face drop at high conductivity contrast. The explicit
  // three-bit split retains a small low word without requiring f64/f16.
  let old=work[i].x;let increment=scalars[0].w*work[i].z+correction(i);let high=bitcast<f32>(bitcast<u32>(old+increment)&0xfffffff8u);
  unit_potential[i][cfg.control.x+2u]=(old-high)+increment;work[i].x=high;
  work[i].y-=scalars[0].w*work[i].w;let r=work[i].y;v=vec4<f32>(r*r/diagonal(i),r*r,0.0,0.0);
 }reduce(v,l,group.x);
}
@compute @workgroup_size(256)
fn beta_reduce(@builtin(local_invocation_index) l:u32){let v=sum_partials(l);if l==0u{scalars[1].x=0.0;if scalars[0].x>0.0{scalars[1].x=v.x/scalars[0].x;}scalars[0].x=v.x;scalars[0].y=v.y;}}
@compute @workgroup_size(256)
fn update_search(@builtin(global_invocation_id) g:vec3<u32>){let i=g.x;if i<cfg.dims.w&&unknown(i){work[i].z=work[i].y/diagonal(i)+scalars[1].x*work[i].z;}}
@compute @workgroup_size(256)
fn true_residual(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let i=g.x;var v=vec4<f32>(0.0);if i<cfg.dims.w&&unknown(i){let b=rhs(i);let r=residual_value(i);let z=r/diagonal(i);work[i].y=r;work[i].z=z;v=vec4<f32>(r*z,r*r,b*b,0.0);}reduce(v,l,group.x);
}
@compute @workgroup_size(256)
fn terminal_flux(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let i=g.x;var v=vec4<f32>(0.0);if i<cfg.dims.w&&cells[i].x>0.0&&!unknown(i){let q=xyz(i);
  for(var a=0u;a<3u;a++){for(var sign=-1;sign<=1;sign+=2){var n=q;n[a]+=sign;if valid(n){let j=index(n);let f=coefficient(i,j)*(raw_difference(i,j)+cfg.physical.x*0.5*(emf(i)[a]+emf(j)[a])*f32(sign));v.x+=f;v.y+=abs(f);}}}
 }reduce(v,l,group.x);
}
@compute @workgroup_size(256)
fn terminal_flux_reduce(@builtin(local_invocation_index) l:u32){let v=sum_partials(l);if l==0u{scalars[3]=v;}}
@compute @workgroup_size(256)
fn collect_fields(@builtin(global_invocation_id) g:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let i=g.x;var stats=vec4<f32>(0.0);if i<cfg.dims.w{
  let h=cfg.physical.x;let scale=cfg.physical.y;var jvec=vec3<f32>(0.0);var power=0.0;var divergence=0.0;let q=xyz(i);
  if cells[i].x>0.0{
   for(var a=0u;a<3u;a++){for(var sign=-1;sign<=1;sign+=2){var n=q;n[a]+=sign;if valid(n){let k=index(n);let sigma=coefficient(i,k)*1e6;let diff=potential_difference(i,k);let electric=diff/h+0.5*(emf(i)[a]+emf(k)[a])*f32(sign);let outward=sigma*electric;jvec[a]+=0.5*f32(sign)*outward;divergence+=outward/h;power+=0.5*outward*outward*(h*h*h/(cells[i].x*1e6)+h*h*contact(i,k));if cells[i].y==0.5{stats.x+=outward*h*h;}if cells[i].y== -0.5{stats.y+=outward*h*h;}}}}
  }
  let b=bias[i].b;output[i]=Field(b,vec4<f32>(jvec,divergence),vec4<f32>(cross(jvec,b.xyz),power/(h*h*h)),bias[i].applied);stats.z=power;stats.w= -dot(jvec,emf(i))*h*h*h;
 }reduce(stats,l,group.x);
}
@compute @workgroup_size(256)
fn fields_reduce(@builtin(local_invocation_index) l:u32){let v=sum_partials(l);if l==0u{scalars[2]=v;}}
