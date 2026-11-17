// Projection of CAD phase fractions into the binary voxel-wall domain.
// x=liquid CAD fraction, y=fixed-solid CAD fraction until the final body.
@compute @workgroup_size(8,8)
fn accumulate(@builtin(global_invocation_id) g:vec3<u32>){
 let samples=p.grid.w;let size=p.region.xy/samples;if any(g.xy>=size){return;}
 let i=g.x+p.tile.x+p.grid.x*(g.y+p.tile.y+p.grid.y*p.region.z);
 let wall=original[i]!=0u&&original[i]!=p.tile.z;
 let a=sampled_fraction(g.xy);
 var fractions=surface[i];var c:Cell;
 if p.flags.x!=0u{fractions=vec4<f32>(0.);}
 if p.flags.z!=0u{c=thermal[i];if p.flags.x!=0u&&!wall{c=Cell(vec4<f32>(0.),vec4<f32>(0.),vec4<f32>(0.),vec4<f32>(0.));}}
 if !wall {
  if p.flags.w!=0u{fractions.y+=a;}else{
   fractions.x+=a;
   if p.flags.z!=0u{
    let b=p.tile.w+2u+2u*p.region.w;let t=initial_body_temperature(catalog[b],catalog[b+1u]);
    c.state.z+=a*material_enthalpy(catalog[p.tile.z*2u],catalog[p.tile.z*2u+1u],t);
   }
  }
 }else{fractions=vec4<f32>(0.);}
 if p.flags.y!=0u {
  if fractions.x>1.00001{atomicOr(&invalid,2u);}
  let open=1.-fractions.y;var ratio=0.;
  if !wall&&fractions.x>0. {
   if open<=1e-6||fractions.x+fractions.y>1.00001{atomicOr(&invalid,4u);}
   ratio=fractions.x/max(open,1e-6);
   if ratio>1.00001{atomicOr(&invalid,4u);}
   if p.flags.z!=0u{c.state.z/=max(open,1e-6);if ratio>1.{c.state.z/=ratio;}}
  }
  fractions=vec4<f32>(clamp(ratio,0.,1.),0.,0.,0.);
  if p.flags.z!=0u&&!wall{c.state.w=fractions.x;c.valid.x=select(0.,1.,fractions.x>0.);}
 }
 surface[i]=fractions;if p.flags.z!=0u&&!wall{thermal[i]=c;}
}
