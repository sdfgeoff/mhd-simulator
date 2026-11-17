// TEST ONLY: disjoint same-orientation principal blocks of the EM operator.
// Full CᵀνC + GᵀDG + mass remains the reference/acceptance equation.
struct EmTile {origin:vec3<u32>,axis:u32}
struct EmTileRow {diagonal:f32,lower:vec3<f32>,upper:vec3<f32>}
var<workgroup> em_tile_even:array<f32,256>;
var<workgroup> em_tile_odd:array<f32,256>;
fn em_tile_count()->u32 {
 var count=0u;
 for(var axis=0u;axis<3u;axis++){let d=(edge_dims(axis)+vec3<u32>(3u))/4u;count+=d.x*d.y*d.z;}
 return count;
}
fn em_tile_decode(group:u32)->EmTile {
 var id=group;
 for(var axis=0u;axis<3u;axis++){
  let d=(edge_dims(axis)+vec3<u32>(3u))/4u;let count=d.x*d.y*d.z;
  if id<count{return EmTile(4u*vec3<u32>(id%d.x,(id/d.x)%d.y,id/(d.x*d.y)),axis);}
  id-=count;
 }
 return EmTile(vec3<u32>(0u),0u);
}
// Caller supplies a valid edge. Constrained/invalid lanes have zero RHS and
// stay zero in shared memory, so links to them need no per-neighbor geometry
// checks. Free-edge transverse neighbors are valid; axial outer links have γ=0.
fn em_tile_row(tile:EmTile,local:vec3<u32>)->EmTileRow {
 var row=EmTileRow(1.,vec3<f32>(0.),vec3<f32>(0.));
 let q=vec3<i32>(tile.origin+local);let i=edge_index(tile.axis,q);
 if source[i+scene.edges.w].w>0.5 {
  // Strict scalar-air eligibility already proves six unit links, zero mass,
  // and two complete endpoint gauges. Its diagonal is exactly six.
  row.diagonal=6.;
  row.lower=select(vec3<f32>(0.),vec3<f32>(1.),local>vec3<u32>(0u));
  row.upper=select(vec3<f32>(0.),vec3<f32>(1.),local<vec3<u32>(3u));
  return row;
 }
 if boundary(vec4<u32>(vec3<u32>(q),tile.axis)){return row;}
 row.diagonal=diagonal(i);let flags=source[i+scene.edges.w].yz;
 for(var axis=0u;axis<3u;axis++){
  var lower=flags.x;var upper=flags.y;
  if axis!=tile.axis {var lo=q;lo[axis]-=1;let normal=3u-axis-tile.axis;lower=nu_face(normal,lo);upper=nu_face(normal,q);}
  if local[axis]>0u{row.lower[axis]=lower;}
  if local[axis]<3u{row.upper[axis]=upper;}
 }
 return row;
}
fn em_tile_neighbor(local:u32,offset:i32)->u32 {
 let base=64u*(local/64u);return u32(clamp(i32(local)+offset,i32(base),i32(base+63u)));
}
fn em_tile_sum_even(local:u32,row:EmTileRow)->f32{
 return row.lower.x*em_tile_even[em_tile_neighbor(local,-1)] + row.upper.x*em_tile_even[em_tile_neighbor(local,1)] + row.lower.y*em_tile_even[em_tile_neighbor(local,-4)] + row.upper.y*em_tile_even[em_tile_neighbor(local,4)] + row.lower.z*em_tile_even[em_tile_neighbor(local,-16)] + row.upper.z*em_tile_even[em_tile_neighbor(local,16)];
}
fn em_tile_sum_odd(local:u32,row:EmTileRow)->f32{
 return row.lower.x*em_tile_odd[em_tile_neighbor(local,-1)] + row.upper.x*em_tile_odd[em_tile_neighbor(local,1)] + row.lower.y*em_tile_odd[em_tile_neighbor(local,-4)] + row.upper.y*em_tile_odd[em_tile_neighbor(local,4)] + row.lower.z*em_tile_odd[em_tile_neighbor(local,-16)] + row.upper.z*em_tile_odd[em_tile_neighbor(local,16)];
}
fn em_tile_solve_jacobi(local:u32,row:EmTileRow,rhs:f32)->f32 {
 em_tile_even[local]=0.;em_tile_odd[local]=0.;workgroupBarrier();
 // Eight fixed weighted-Jacobi stages from zero. The two explicit phases
 // read one shared array and write the other; no eager select reads a writer.
 for(var pair=0u;pair<EM_TILE_PAIRS;pair++){
  em_tile_odd[local]=em_tile_even[local]/3.+(2./3.)*(rhs+em_tile_sum_even(local,row))/row.diagonal;
  workgroupBarrier();
  em_tile_even[local]=em_tile_odd[local]/3.+(2./3.)*(rhs+em_tile_sum_odd(local,row))/row.diagonal;
  workgroupBarrier();
 }
 return em_tile_even[local];
}
fn em_tile_solve_chebyshev(local:u32,row:EmTileRow,rhs:f32)->f32 {
 em_tile_even[local]=0.;em_tile_odd[local]=0.;workgroupBarrier();var previous=0.;
 for(var pair=0u;pair<EM_TILE_PAIRS;pair++){
  let first=em_tile_coeff(2u*pair);let even=em_tile_even[local];
  em_tile_odd[local]=even+first.x*((rhs+em_tile_sum_even(local,row))/row.diagonal-even)+first.y*(even-previous);
  previous=even;workgroupBarrier();
  let second=em_tile_coeff(2u*pair+1u);let odd=em_tile_odd[local];
  em_tile_even[local]=odd+second.x*((rhs+em_tile_sum_odd(local,row))/row.diagonal-odd)+second.y*(odd-previous);
  previous=odd;workgroupBarrier();
 }
 return em_tile_even[local];
}
fn em_tile_solve(local:u32,row:EmTileRow,rhs:f32)->f32 {
 if EM_TILE_CHEBYSHEV{return em_tile_solve_chebyshev(local,row,rhs);}
 return em_tile_solve_jacobi(local,row,rhs);
}
@compute @workgroup_size(4,4,4)
fn em_tile_precondition(@builtin(local_invocation_id) local:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let gi=group_id(group);if gi>=em_tile_count(){return;}
 let tile=em_tile_decode(gi);let q=vec3<i32>(tile.origin+local);let within=valid(q,edge_dims(tile.axis));
 var rhs=0.;var i=0u;var row=EmTileRow(1.,vec3<f32>(0.),vec3<f32>(0.));
 if within {i=edge_index(tile.axis,q);row=em_tile_row(tile,local);if !boundary(vec4<u32>(vec3<u32>(q),tile.axis)){rhs=work[i].y;}}
 let z=em_tile_solve(l,row,rhs);
 if within {work[i].w=z;}
}
@compute @workgroup_size(4,4,4)
fn em_tile_block_action(@builtin(local_invocation_id) local:vec3<u32>,@builtin(workgroup_id) group:vec3<u32>){
 let gi=group_id(group);if gi>=em_tile_count(){return;}
 let tile=em_tile_decode(gi);let q=vec3<i32>(tile.origin+local);if !valid(q,edge_dims(tile.axis)){return;}
 let i=edge_index(tile.axis,q);let row=em_tile_row(tile,local);var value=row.diagonal*work[i].z;
 for(var axis=0u;axis<3u;axis++){
  var lo=q;var hi=q;lo[axis]-=1;hi[axis]+=1;
  if row.lower[axis]!=0. {value-=row.lower[axis]*work[edge_index(tile.axis,lo)].z;}
  if row.upper[axis]!=0. {value-=row.upper[axis]*work[edge_index(tile.axis,hi)].z;}
 }
 work[i].w=value;
}

@compute @workgroup_size(256)
fn em_tile_search(@builtin(global_invocation_id) g:vec3<u32>){let i=flat_id(g);if i<scene.edges.w&&scalars[1].w==0.{work[i].z=work[i].w+scalars[1].y*work[i].z;}}


// Four independent 4³ tiles share one 256-thread group. A few groups handle
// another round when partial edge tiles exceed four times the linear group
// count. This retains the existing global reduction size and scratch buffer.
fn em_tile_cg(local:u32,gi:u32,initialize:bool) {
 let groups=(scene.edges.w+255u)/256u;let count=em_tile_count();
 let lane=local%64u;let at=vec3<u32>(lane%4u,(lane/4u)%4u,lane/16u);
 var partial=vec4<f32>(0.);
 for(var base=4u*gi;base<count;base+=4u*groups){
  let tile_id=base+local/64u;var within=false;var i=0u;var rhs=0.;
  var row=EmTileRow(1.,vec3<f32>(0.),vec3<f32>(0.));
  if tile_id<count {
   let tile=em_tile_decode(tile_id);let q=vec3<i32>(tile.origin+at);within=valid(q,edge_dims(tile.axis));
   if within {i=edge_index(tile.axis,q);row=em_tile_row(tile,at);if !boundary(vec4<u32>(vec3<u32>(q),tile.axis)){rhs=work[i].y;}}
  }
  let z=em_tile_solve(local,row,rhs);
  if within {work[i].w=z;if initialize {work[i].z=z;}}
  partial+=vec4<f32>(rhs*z,rhs*rhs,0.,0.);
 }
 reduction[local]=partial;let sum=sum_group(local);if local==0u{partials[gi]=sum;}
}
@compute @workgroup_size(256)
fn em_tile_cg_precondition(@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let gi=group_id(group);if gi>=(scene.edges.w+255u)/256u{return;}em_tile_cg(l,gi,false);
}
@compute @workgroup_size(256)
fn em_tile_cg_initial(@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 let gi=group_id(group);if gi>=(scene.edges.w+255u)/256u{return;}em_tile_cg(l,gi,true);
}
