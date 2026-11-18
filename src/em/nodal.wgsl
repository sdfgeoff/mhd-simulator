// Additive gradient-space Jacobi correction. G=D^T is the exact node-to-edge
// incidence gradient. diag(G^T A G) includes both edge mass and the air gauge.
@group(0) @binding(9) var<storage,read_write> nodal_values:array<f32>;
fn nodal_dims()->vec3<u32>{return scene.dims.xyz+vec3<u32>(1u);}
fn nodal_index(p:vec3<u32>)->u32 {let d=nodal_dims();return p.x+d.x*(p.y+d.y*p.z);}
fn interior_node(p:vec3<i32>)->bool {return all(p>vec3<i32>(0)) && all(p<vec3<i32>(scene.dims.xyz));}
fn cached_air_node(p:vec3<i32>)->f32 {if !interior_node(p){return 0.;}return source[edge_index(0u,p)+scene.edges.w].y;}
fn nodal_value(i:u32)->f32 {
 let d=nodal_dims();let p=vec3<i32>(vec3<u32>(i%d.x,(i/d.x)%d.y,i/(d.x*d.y)));
 if !interior_node(p){return 0.;}
 var rhs=0.;var diag=36.*cached_air_node(p);
 for(var a=0u;a<3u;a++) {
  var lower=p;lower[a]-=1;var upper=p;upper[a]+=1;
  let out=edge_index(a,p);let inc=edge_index(a,lower);
  rhs+=work[out].y-work[inc].y;
  diag+=mass(out)+mass(inc)+cached_air_node(lower)+cached_air_node(upper);
 }
 return rhs/max(diag,1e-30);
}
@compute @workgroup_size(256)
fn nodal_build_initial(@builtin(global_invocation_id) gid:vec3<u32>){let i=flat_id(gid);let d=nodal_dims();if i<d.x*d.y*d.z {nodal_values[i]=nodal_value(i);}}
@compute @workgroup_size(256)
fn nodal_build(@builtin(global_invocation_id) gid:vec3<u32>){let i=flat_id(gid);let d=nodal_dims();if scalars[1].w==0. && i<d.x*d.y*d.z {nodal_values[i]=nodal_value(i);}}
fn nodal_z(i:u32)->f32 {
 let e=edge_decode(i);var upper=e.xyz;upper[e.w]+=1u;
 return work[i].y/diagonal(i)+nodal_values[nodal_index(e.xyz)]-nodal_values[nodal_index(upper)];
}
@compute @workgroup_size(256)
fn nodal_initial(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let i=flat_id(gid);var v=vec4<f32>(0.);
 if i<scene.edges.w {let r=work[i].y;let z=nodal_z(i);work[i].z=z;work[i].w=z;v=vec4<f32>(r*z,r*r,0.,0.);}
 reduction[l]=v;let sum=sum_group(l);let g=group_id(group);
 if l==0u && g<(scene.edges.w+255u)/256u {partials[g].x=sum.x;partials[g].y=sum.y;}
}
@compute @workgroup_size(256)
fn nodal_update(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let i=flat_id(gid);var v=vec4<f32>(0.);
 if i<scene.edges.w && scalars[1].w==0. {let r=work[i].y;let z=nodal_z(i);work[i].w=z;v=vec4<f32>(r*z,r*r,0.,0.);}
 reduction[l]=v;let sum=sum_group(l);let g=group_id(group);
 if l==0u && scalars[1].w==0. && g<(scene.edges.w+255u)/256u {partials[g].x=sum.x;partials[g].y=sum.y;}
}
