struct Uniforms { camera:mat4x4<f32>, origin:vec4<f32>, normal:vec4<f32>, settings:vec4<f32>,grid_minimum:vec4<f32> }
@group(0) @binding(0) var<uniform> scene:Uniforms;
@group(0) @binding(1) var section:texture_2d<f32>;
@group(0) @binding(2) var section_sampler:sampler;
struct Input { @location(0) position:vec3<f32>, @location(1) color:vec4<f32>, @location(2) uv:vec2<f32>, @location(3) mode:f32 }
struct Output { @builtin(position) position:vec4<f32>, @location(0) world:vec3<f32>, @location(1) color:vec4<f32>, @location(2) uv:vec2<f32>, @location(3) @interpolate(flat) mode:f32 }
@vertex fn vertex(v:Input)->Output{var out:Output;out.position=scene.camera*vec4<f32>(v.position,1.0);out.world=v.position;out.color=v.color;out.uv=v.uv;out.mode=v.mode;return out;}
@fragment fn fragment(v:Output)->@location(0) vec4<f32>{
    let coordinate=(v.uv.x+v.uv.y)*0.70710678/scene.settings.y;
    let width=max(fwidth(coordinate),0.002);
    let uv_dx=dpdx(v.uv);let uv_dy=dpdy(v.uv);
    let grid_ink=voxel_grid_coverage(v.world,scene.grid_minimum.xyz,scene.settings.w);
    if v.mode>2.5{let c=textureSampleGrad(section,section_sampler,v.uv,uv_dx,uv_dy);if c.a<0.001{discard;}return apply_voxel_grid(c,grid_ink,scene.settings.z);}
    if scene.settings.x>0.5 && v.mode<0.5 && dot(v.world-scene.origin.xyz,scene.normal.xyz)>0.0 {discard;}
    if v.mode>0.5 && v.mode<1.5 {
        let distance=abs(fract(coordinate)-0.5);
        let ink=1.0-smoothstep(width*0.45,width*1.15,distance);
        return vec4<f32>(mix(v.color.rgb,vec3<f32>(0.13,0.16,0.20),ink*0.85),1.0);
    }
    return v.color;
}
