@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var linear_sampler:sampler;
struct Output{@builtin(position) p:vec4<f32>,@location(0) uv:vec2<f32>}
@vertex fn vertex(@builtin(vertex_index) i:u32)->Output{let x=f32((i<<1u)&2u);let y=f32(i&2u);var o:Output;o.p=vec4<f32>(x*2.0-1.0,y*2.0-1.0,0.5,1.0);o.uv=vec2<f32>(x,1.0-y);return o;}
@fragment fn fragment(v:Output)->@location(0) vec4<f32>{return textureSampleLevel(source,linear_sampler,v.uv,0.0);}
