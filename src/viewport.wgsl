struct Uniforms { camera:mat4x4<f32>, origin:vec4<f32>, normal:vec4<f32>, settings:vec4<f32>,grid_minimum:vec4<f32>,dimensions:vec4<u32>,palette:array<vec4<f32>,8> }
@group(0) @binding(0) var<uniform> scene:Uniforms;
@group(0) @binding(1) var<storage,read> masks:array<u32>;
@group(0) @binding(2) var<storage,read> materials:array<u32>;
struct Input { @location(0) position:vec3<f32>, @location(1) color:vec4<f32>, @location(2) uv:vec2<f32>, @location(3) mode:f32 }
struct Output { @builtin(position) position:vec4<f32>, @location(0) world:vec3<f32>, @location(1) color:vec4<f32>, @location(2) uv:vec2<f32>, @location(3) @interpolate(flat) mode:f32 }
@vertex fn vertex(v:Input)->Output{var out:Output;out.position=scene.camera*vec4<f32>(v.position,1.0);out.world=v.position;out.color=v.color;out.uv=v.uv;out.mode=v.mode;return out;}
@fragment fn fragment(v:Output)->@location(0) vec4<f32>{
    let coordinate=(v.uv.x+v.uv.y)*0.70710678/scene.settings.y;
    let width=max(fwidth(coordinate),0.002);
    let grid_ink=voxel_grid_coverage(v.world,scene.grid_minimum.xyz,scene.settings.w);
    if v.mode>2.5 {
        let p=(v.world-scene.grid_minimum.xyz)/scene.grid_minimum.w;
        if any(p<vec3<f32>(0.0)) || any(p>=vec3<f32>(scene.dimensions.xyz)) {discard;}
        let q=vec3<u32>(floor(p));
        let index=q.x+scene.dimensions.x*(q.y+scene.dimensions.y*q.z);
        let mask=masks[index];
        if mask==0u {discard;}
        var id=firstTrailingBit(mask)+1u;
        if countOneBits(mask)>1u {id=255u;}
        if scene.dimensions.w!=0u {id=materials[index];}
        var c=scene.palette[(max(id,1u)-1u)%6u];
        if id==255u {c=vec4<f32>(1.0,0.032,0.102,1.0);}
        return apply_voxel_grid(c,grid_ink,scene.settings.z);
    }
    if scene.settings.x>0.5 && v.mode<0.5 && dot(v.world-scene.origin.xyz,scene.normal.xyz)>0.0 {discard;}
    if v.mode>0.5 && v.mode<1.5 {
        let distance=abs(fract(coordinate)-0.5);
        let ink=1.0-smoothstep(width*0.45,width*1.15,distance);
        return vec4<f32>(mix(v.color.rgb,vec3<f32>(0.13,0.16,0.20),ink*0.85),1.0);
    }
    return v.color;
}
