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
fn alpha(q:vec3<i32>)->f32{return input_field[index(bounded(q))].x;}
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
