// Shared indexing and PLIC liquid flux for occupancy and momentum transport.
fn flat(g:vec3<u32>)->u32{return g.x+g.y*65535u*256u;}
fn index(q:vec3<i32>)->u32{return u32(q.x)+p.dims.x*(u32(q.y)+p.dims.y*u32(q.z));}
fn point(i:u32)->vec3<i32>{return vec3<i32>(i32(i%p.dims.x),i32((i/p.dims.x)%p.dims.y),i32(i/(p.dims.x*p.dims.y)));}
// Low bits select first/last split sweep; bits 2..4 are periodic axes.
fn periodic(axis:u32)->bool{return (p.choice.w & (4u<<axis))!=0u;}
fn inside(q:vec3<i32>)->bool{
 if (p.choice.w&28u)==0u {return all(q>=vec3<i32>(0))&&all(q<vec3<i32>(p.dims.xyz));}
 for(var axis=0u;axis<3u;axis++){if !periodic(axis)&&(q[axis]<0||q[axis]>=i32(p.dims[axis])){return false;}}
 return true;
}
fn bounded(q:vec3<i32>)->vec3<i32>{
 if (p.choice.w&28u)==0u {return clamp(q,vec3<i32>(0),vec3<i32>(p.dims.xyz)-vec3<i32>(1));}
 var at=q;
 for(var axis=0u;axis<3u;axis++){
  let length=i32(p.dims[axis]);
  if periodic(axis) {at[axis]=wrap_periodic(at[axis],p.dims[axis]);}
  else {at[axis]=clamp(at[axis],0,length-1);}
 }
 return at;
}
// Geometry-only mirror extension for neutral (90 degree) voxel CAD walls.
// The reference is the fluid cell whose interface is being reconstructed.
// Choosing its side prevents thin walls from sampling unrelated liquid/air on
// the opposite face. Ghost values never enter stored occupancy or face fluxes.
fn alpha(q:vec3<i32>,reference:vec3<i32>)->f32 {
 let at=bounded(q);let i=index(at);if !wall(i){return input_field[i].x;}
 var blocked=vec3<bool>(false);
 for(var distance=1;distance<=6;distance++){
  for(var axis=0u;axis<3u;axis++){
   if blocked[axis]{continue;}
   var delta=reference[axis]-at[axis];
   if periodic(axis){let length=i32(p.dims[axis]);if delta>length/2{delta-=length;}if delta< -length/2{delta+=length;}}
   let direction=sign(delta);if direction==0 {continue;}
   var probe=at;probe[axis]+=distance*direction;
   if inside(probe)&&!wall(index(bounded(probe))) {
    var mirror=at;mirror[axis]+=(2*distance-1)*direction;
    var clear=true;
    for(var step=distance;step<2*distance;step++){
     var sample=at;sample[axis]+=step*direction;
     if !inside(sample)||wall(index(bounded(sample))){clear=false;break;}
    }
    if clear{return input_field[index(bounded(mirror))].x;}
    // A second obstruction invalidates this mirror. Do not jump through it
    // to sample an unrelated channel farther along the same axis.
    blocked[axis]=true;
   }
  }
 }
 // Inaccessible corners/long obstructions have no resolved mirror stencil.
 return input_field[i].x;
}
fn wall(i:u32)->bool{return original[i]!=0u && original[i]!=p.choice.x;}

fn liquid_flux(at:vec3<i32>,axis:u32)->vec3<f32>{
 var previous=at;previous[axis]-=1;
 if !inside(at)||!inside(previous){return vec3<f32>(0.0);}
 let right=bounded(at);let left=bounded(previous);
 if wall(index(right))||wall(index(left)){return vec3<f32>(0.0);}
 let fd=p.dims.xyz+vec3<u32>(1);let fi=u32(right.x)+fd.x*(u32(right.y)+fd.y*u32(right.z));
 let displacement=velocity[fi][axis]*p.step.x/p.step.y;
 // Independently verify the actual face CFL, even if a caller supplied an
 // incorrect speed bound. Returning a flag avoids unsafe geometry arithmetic.
 if !(abs(displacement)<=0.150001) {return vec3<f32>(0.0,0.0,1.0);}
 if displacement==0.0{return vec3<f32>(0.0);}
 let donor=index(select(right,left,displacement>0.0));let a=input_field[donor].x;
 var fraction=a;let plane=normals[donor];
 if dot(abs(plane.xyz),vec3<f32>(1.0))>0.5 {
  fraction=slab_fraction(plane.xyz,plane.w,axis,abs(displacement),sign(displacement));
 }
 return vec3<f32>(displacement*fraction,displacement,0.0);
}
