// Test-only unconditioned CAD-volume reference.
@compute @workgroup_size(8,8)
fn accumulate(@builtin(global_invocation_id) g:vec3<u32>){
 let samples=p.grid.w;let size=p.region.xy/samples;
 if any(g.xy>=size){return;}
 let i=g.x+p.tile.x+p.grid.x*(g.y+p.tile.y+p.grid.y*p.region.z);
 let wall=original[i]!=0u && original[i]!=p.tile.z;
 let a=sampled_fraction(g.xy);
 var total=surface[i].x;var c:Cell;
 if p.flags.x!=0u {total=0.0;}
 if p.flags.z!=0u {c=thermal[i];if p.flags.x!=0u && !wall{c=Cell(vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0));}}
 if !wall {
  total+=a;
  if p.flags.z!=0u {
   let b=p.tile.w+2u+2u*p.region.w;
   let t=initial_body_temperature(catalog[b],catalog[b+1u]);
   c.state.z+=a*material_enthalpy(catalog[p.tile.z*2u],catalog[p.tile.z*2u+1u],t);
   c.state.w=total;c.valid.x=select(0.0,1.0,total>0.0);
  }
 } else {total=0.0;}
 if p.flags.y!=0u {
  if total>1.00001 {atomicOr(&invalid[0],2u);}
  if total>1.0 && p.flags.z!=0u && !wall {c.state.z/=total;c.state.w=1.0;}
  total=clamp(total,0.0,1.0);
 }
 surface[i]=vec4<f32>(total,0.0,0.0,0.0);
 if p.flags.z!=0u && !wall {thermal[i]=c;}
}
