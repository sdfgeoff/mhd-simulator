// Heights are occupied column lengths in cell units. Average each second
// difference across the orthogonal direction; quadratic surfaces remain exact.
fn height_hessian(h:array<f32,9>,cell:f32)->vec3<f32>{
 let xx=(h[3]+h[5]-2.0*h[4])+0.2*((h[0]+h[2]-2.0*h[1])+(h[6]+h[8]-2.0*h[7]));
 let yy=(h[1]+h[7]-2.0*h[4])+0.2*((h[0]+h[6]-2.0*h[3])+(h[2]+h[8]-2.0*h[5]));
 let xy=(h[8]-h[6]-h[2]+h[0])/(4.0*cell);
 return vec3<f32>(xx/(1.4*cell),yy/(1.4*cell),xy);
}
