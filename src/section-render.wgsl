struct Params { spacing:vec4<f32> }
@group(0) @binding(0) var<uniform> params:Params;
struct Input { @location(0) p:vec2<f32>, @location(1) color:vec4<f32>, @location(2) uv:vec2<f32>, @location(3) mode:f32 }
struct Output { @builtin(position) p:vec4<f32>, @location(0) color:vec4<f32>, @location(1) uv:vec2<f32>, @location(2) @interpolate(flat) mode:f32 }
@vertex fn vertex(v:Input)->Output{var o:Output;o.p=vec4<f32>(v.p,0.5,1.0);o.color=v.color;o.uv=v.uv;o.mode=v.mode;return o;}
@fragment fn fragment(v:Output)->@location(0) vec4<f32>{
    let coordinate=(v.uv.x+v.uv.y)*0.70710678/params.spacing.x;
    let width=max(fwidth(coordinate),0.002);
    if v.mode>0.5{let distance=abs(fract(coordinate)-0.5);let ink=1.0-smoothstep(width*0.45,width*1.15,distance);return vec4<f32>(mix(v.color.rgb,vec3<f32>(0.04,0.055,0.07),ink*0.85),v.color.a);}
    return v.color;
}
