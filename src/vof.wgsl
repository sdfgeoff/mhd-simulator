// Independent geometric half-space integration for a unit cube centred at zero.
// Volume is the CDF of a weighted sum of three uniform coordinates. Reflect to
// the lower half and factor the cubic differences to avoid cancellation near
// axis-aligned planes; no polygon/tetrahedron sampling is involved.
fn cube_halfspace(normal:vec3<f32>,offset:f32)->f32 {
    var weights=abs(normal);let largest=max(weights.x,max(weights.y,weights.z));
    if largest<=1e-30 {return select(0.0,1.0,offset>=0.0);}
    weights=select(weights,vec3<f32>(0.0),weights<vec3<f32>(largest*1e-8));
    let scale=weights.x+weights.y+weights.z;
    let coordinate=offset/scale+0.5;
    if coordinate<=0.0{return 0.0;}if coordinate>=1.0{return 1.0;}
    let lower=min(coordinate,1.0-coordinate);
    weights/=scale;
    let a=min(weights.x,min(weights.y,weights.z));
    let c=max(weights.x,max(weights.y,weights.z));
    let b=max(min(weights.x,weights.y),min(max(weights.x,weights.y),weights.z));
    var fraction=0.0;
    if b==0.0 {
        fraction=lower/c;
    }else if a==0.0 {
        if lower<b {fraction=lower*lower/(2.0*b*c);}
        else {fraction=(lower-0.5*b)/c;}
    }else if lower<a {
        fraction=lower*lower*lower/(6.0*a*b*c);
    }else if c>=a+b && lower>=a+b {
        fraction=(lower-0.5*(a+b))/c;
    }else {
        let above_b=max(lower-b,0.0);let above_c=max(lower-c,0.0);
        let cubic=a*(3.0*lower*lower-3.0*lower*a+a*a)-above_b*above_b*above_b-above_c*above_c*above_c;
        fraction=cubic/(6.0*a*b*c);
    }
    fraction=clamp(fraction,0.0,0.5);
    return select(fraction,1.0-fraction,coordinate>0.5);
}
fn cube_intercept(normal:vec3<f32>,fraction:f32)->f32 {
    let extent=0.5*(abs(normal.x)+abs(normal.y)+abs(normal.z));
    var low=-extent;var high=extent;
    for(var iteration=0u;iteration<24u;iteration++) {
        let middle=0.5*(low+high);
        if cube_halfspace(normal,middle)<fraction {low=middle;}else{high=middle;}
    }
    return 0.5*(low+high);
}
fn slab_fraction(normal:vec3<f32>,offset:f32,axis:u32,width:f32,direction:f32)->f32 {
    var scaled=normal;scaled[axis]*=width;
    return cube_halfspace(scaled,offset-normal[axis]*direction*(0.5-0.5*width));
}
