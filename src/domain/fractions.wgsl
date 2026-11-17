// For an oriented closed mesh, occupied length in [z0,z1] is the sum
// sign(n_z)*clamp(z_intersection-z0,0,h). Integer sums are order independent.
struct Params {minimum:vec4<f32>,region:vec4<u32>,grid:vec4<u32>,tile:vec4<u32>,flags:vec4<u32>}
struct Coverage {sum:atomic<u32>,count:atomic<u32>}
struct Cell {state:vec4<f32>,flux:vec4<f32>,budget:vec4<f32>,valid:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read_write> coverage:array<Coverage>;
@group(0) @binding(2) var<storage,read_write> surface:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read_write> thermal:array<Cell>;
@group(0) @binding(4) var<storage,read> catalog:array<vec4<f32>>;
@group(0) @binding(5) var<storage,read> original:array<u32>;
@group(0) @binding(6) var<storage,read_write> invalid:atomic<u32>;
const SCALE:i32=1048576;
struct VertexOut {@builtin(position) position:vec4<f32>,@location(0) height:f32,@location(1) @interpolate(flat) direction:f32}
@vertex fn vertex(@location(0) v:vec4<f32>)->VertexOut{
 var o:VertexOut;
 let extent=vec2<f32>(p.region.xy)*p.minimum.w/f32(p.grid.w);
 let xy=(v.xy-p.minimum.xy)/extent;
 o.position=vec4<f32>(2.0*xy.x-1.0,1.0-2.0*xy.y,0.5,1.0);
 o.height=v.z-p.minimum.z;o.direction=v.w;return o;
}
@fragment fn fragment(v:VertexOut)->@location(0) vec4<f32>{
 let value=v.direction*clamp((v.height-f32(p.region.z)*p.minimum.w)/p.minimum.w,0.0,1.0);
 if value==0.0 {discard;}
 let q=vec2<u32>(v.position.xy);let i=q.x+p.region.x*q.y;
 let fixed=i32(round(value*f32(SCALE)));
 atomicAdd(&coverage[i].sum,bitcast<u32>(fixed));atomicAdd(&coverage[i].count,1u);
 return vec4<f32>(0.0);
}

fn sampled_fraction(g:vec2<u32>)->f32 {
 let samples=p.grid.w;
 var sum=0i;
 for(var y=0u;y<samples;y++){for(var x=0u;x<samples;x++){
  let at=(g.x*samples+x)+p.region.x*(g.y*samples+y);
  let value=bitcast<i32>(atomicLoad(&coverage[at].sum));
  // One unit for rounding and one for f32 interpolation/arithmetic per crossing.
  let error=2i*i32(atomicLoad(&coverage[at].count))+1i;
  if value < -error || value>SCALE+error {atomicOr(&invalid,1u);}
  sum+=clamp(value,0i,SCALE);
 }}
 return f32(sum)/(f32(samples*samples)*f32(SCALE));
}
