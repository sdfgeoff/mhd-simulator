struct Panel {origin:vec4<f32>,u:vec4<f32>,v:vec4<f32>,minimum:vec4<f32>,options:vec4<f32>}
@group(0) @binding(0) var<uniform> panel:Panel;
@group(0) @binding(1) var fill:texture_2d<f32>;
@group(0) @binding(2) var linear_sampler:sampler;
struct Out{@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>}
@vertex fn vertex(@builtin(vertex_index) i:u32)->Out{let x=f32((i<<1u)&2u);let y=f32(i&2u);var out:Out;out.position=vec4<f32>(x*2.0-1.0,y*2.0-1.0,0.5,1.0);out.uv=vec2<f32>(x,1.0-y);return out;}
@fragment fn fragment(v:Out)->@location(0) vec4<f32>{
    let world=panel.origin.xyz+(v.uv.x*2.0-1.0)*panel.u.xyz+(1.0-v.uv.y*2.0)*panel.v.xyz;
    let ink=voxel_grid_coverage(world,panel.minimum.xyz,panel.minimum.w);
    let color=textureSample(fill,linear_sampler,v.uv);
    return apply_voxel_grid(color,ink,panel.options.x);
}
