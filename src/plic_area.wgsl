// Area of n·x=offset inside the unit cube centred at zero. This is |n| times
// the offset derivative of the factored cube_halfspace CDF (vof.wgsl).
// As in reconstruction, negligible normal components are discarded. Pure-cell
// boundary planes return zero: summing mixed-cell patches is an area estimate,
// not the complete boundary of a union of possibly discontinuous PLIC cells.
fn plic_patch_area(normal:vec3<f32>,offset:f32)->f32 {
 var weights=abs(normal);let largest=max(weights.x,max(weights.y,weights.z));
 if largest<=1e-30{return 0.;}
 weights=select(weights,vec3<f32>(0.),weights<vec3<f32>(largest*1e-8));
 let scale=weights.x+weights.y+weights.z;let coordinate=offset/scale+0.5;
 if coordinate<=0. || coordinate>=1. {return 0.;}
 let lower=min(coordinate,1.-coordinate);weights/=scale;
 let a=min(weights.x,min(weights.y,weights.z));
 let c=max(weights.x,max(weights.y,weights.z));
 let b=max(min(weights.x,weights.y),min(max(weights.x,weights.y),weights.z));
 var derivative=0.;
 if b==0. {derivative=1./c;}
 else if a==0. {
  derivative=select(1./c,lower/(b*c),lower<b);
 }else if lower<a {derivative=lower*lower/(2.*a*b*c);}
 else if c>=a+b && lower>=a+b {derivative=1./c;}
 else {
  let above_b=max(lower-b,0.);let above_c=max(lower-c,0.);
  derivative=(a*(2.*lower-a)-above_b*above_b-above_c*above_c)/(2.*a*b*c);
 }
 return length(weights)*max(derivative,0.);
}
