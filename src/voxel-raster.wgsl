struct Params { minimum:vec4<f32>, dimensions:vec4<u32>, control:vec4<u32> }
@group(0) @binding(0) var<uniform> params:Params;
struct Out { @builtin(position) position:vec4<f32>, @location(0) height:f32 }
@vertex fn surface(@location(0) p:vec3<f32>)->Out{
    var o:Out;
    let xy=(p.xy-params.minimum.xy)/(vec2<f32>(params.dimensions.xy)*params.minimum.w);
    o.position=vec4<f32>(xy.x*2.0-1.0,1.0-xy.y*2.0,0.5,1.0);o.height=p.z;return o;
}
@fragment fn crossing(v:Out)->@location(0) vec4<f32>{
    let slice=params.minimum.z+(f32(params.control.x)+0.5)*params.minimum.w;
    if v.height<slice {discard;}
    return vec4<f32>(0.0);
}
@vertex fn quad(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{
    let x=f32((i<<1u)&2u);let y=f32(i&2u);return vec4<f32>(x*2.0-1.0,y*2.0-1.0,0.5,1.0);
}
@fragment fn filled()->@location(0) vec4<f32>{return vec4<f32>(1.0);}
