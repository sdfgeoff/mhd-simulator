struct Uniforms { camera:mat4x4<f32>, origin:vec4<f32>, normal:vec4<f32>, settings:vec4<f32>,grid_minimum:vec4<f32>,dimensions:vec4<u32>,palette:array<vec4<f32>,8>,field:vec4<f32>,inverse_camera:mat4x4<f32>,liquid:vec4<f32> }
@group(0) @binding(0) var<uniform> scene:Uniforms;
@group(0) @binding(1) var<storage,read> masks:array<u32>;
@group(0) @binding(2) var<storage,read> materials:array<u32>;
struct FieldResult {b:vec4<f32>,j:vec4<f32>,fq:vec4<f32>,applied:vec4<f32>}
@group(0) @binding(3) var<storage,read> fields:array<FieldResult>;
fn field_color(x:f32)->vec3<f32>{let t=clamp(x,0.0,1.0);return clamp(vec3<f32>(1.5-abs(4.0*t-3.0),1.5-abs(4.0*t-2.0),1.5-abs(4.0*t-1.0)),vec3<f32>(0.0),vec3<f32>(1.0));}
fn vector_value(index:u32)->vec3<f32>{let f=fields[index];if scene.field.x>11.5 && scene.field.x<12.5{return f.j.xyz;}if scene.field.x>6.5{return f.b.xyz;}if scene.field.x<1.5{return f.b.xyz;}if scene.field.x<2.5{return f.applied.xyz;}return f.b.xyz-f.applied.xyz;}
fn smooth_magnetic(p:vec3<f32>)->vec3<f32>{
    let q=p-vec3<f32>(0.5);let base=vec3<i32>(floor(q));let fraction=fract(q);var value=vec3<f32>(0.0);
    for(var z=0;z<2;z++){for(var y=0;y<2;y++){for(var x=0;x<2;x++){
        let o=vec3<i32>(x,y,z);let c=vec3<u32>(clamp(base+o,vec3<i32>(0),vec3<i32>(scene.dimensions.xyz)-vec3<i32>(1)));
        let w=select(vec3<f32>(1.0)-fraction,fraction,o>vec3<i32>(0));value+=vector_value(c.x+scene.dimensions.x*(c.y+scene.dimensions.y*c.z))*w.x*w.y*w.z;
    }}}return value;
}
fn linear_color(c:vec3<f32>)->vec3<f32>{return select(c/12.92,pow((c+vec3<f32>(0.055))/1.055,vec3<f32>(2.4)),c>vec3<f32>(0.04045));}
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
        if scene.field.x>0.5 {
            let f=fields[index];if scene.field.x>6.5 && f.applied.x<0.5 && !(scene.field.x>12.5 && f.applied.z>0.0) {discard;}var vector=f.b.xyz;var scalar=length(vector);
            if scene.field.x>1.5 && scene.field.x<2.5 {vector=f.applied.xyz;}
            else if scene.field.x>2.5 && scene.field.x<3.5 {vector=f.b.xyz-f.applied.xyz;}
            else if scene.field.x>3.5 && scene.field.x<4.5 {vector=f.j.xyz;}
            else if scene.field.x>4.5 && scene.field.x<5.5 {vector=f.fq.xyz;}
            if scene.field.x<3.5 || (scene.field.x>6.5 && scene.field.x<7.5) {vector=smooth_magnetic(p);}
            scalar=length(vector);
            if scene.field.y>0.5 {scalar=vector[u32(scene.field.y)-1u];}
            if scene.field.x>5.5 && scene.field.x<6.5 {scalar=f.fq.w;}
            if scene.field.x>7.5 && scene.field.x<8.5 {scalar=f.j.x;}
            if scene.field.x>8.5 && scene.field.x<9.5 {scalar=f.j.y;}
            if scene.field.x>9.5 && scene.field.x<10.5 {scalar=f.b.x-273.15;}
            if scene.field.x>10.5 && scene.field.x<11.5 {scalar=f.b.y;}
            if scene.field.x>11.5 && scene.field.x<12.5 {vector=f.j.xyz;scalar=length(vector);}
            if scene.field.x>12.5 {scalar=f.applied.z;}
            var color=field_color(scalar/scene.field.z);
            if (scene.field.y>0.5 && (scene.field.x<5.5 || (scene.field.x>6.5 && scene.field.x<7.5))) || (scene.field.x>7.5 && scene.field.x<10.5) {let x=clamp(scalar/scene.field.z,-1.0,1.0);color=mix(vec3<f32>(0.93),select(vec3<f32>(0.12,0.32,0.95),vec3<f32>(0.95,0.18,0.12),x>0.0),abs(x));}
            return vec4<f32>(linear_color(color),1.0);
        }
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

struct TrailInput { @location(0) position:vec4<f32>, @location(1) color:vec4<f32> }
@vertex fn vertex_trail(v:TrailInput)->Output {
    var out:Output;out.position=scene.camera*v.position;out.world=v.position.xyz;
    let dark=scene.field.y>0.5 || scene.field.x>7.5;
    let tint=select(vec3<f32>(1.0),vec3<f32>(0.03,0.05,0.08),dark);
    out.color=vec4<f32>(tint*v.color.a,v.color.a);out.uv=vec2<f32>(0.0);out.mode=2.0;return out;
}

@group(0) @binding(4) var<storage,read> liquid_fields:array<FieldResult>;
struct LiquidVertex {@builtin(position) position:vec4<f32>,@location(0) clip:vec2<f32>}
@vertex fn vertex_liquid(@builtin(vertex_index) i:u32)->LiquidVertex {
 let xy=vec2<f32>(f32((i<<1u)&2u),f32(i&2u))*2.0-vec2<f32>(1.0);return LiquidVertex(vec4<f32>(xy,0.0,1.0),xy);
}
fn liquid_alpha(world:vec3<f32>)->f32 {
 let p=(world-scene.grid_minimum.xyz)/scene.grid_minimum.w-vec3<f32>(0.5);let base=vec3<i32>(floor(p));let fraction=fract(p);var a=0.0;
 for(var z=0;z<2;z++){for(var y=0;y<2;y++){for(var x=0;x<2;x++){
 let o=vec3<i32>(x,y,z);let q=base+o;if any(q<vec3<i32>(0))||any(q>=vec3<i32>(scene.dimensions.xyz)){continue;}
 let c=vec3<u32>(q);let w=select(vec3<f32>(1.0)-fraction,fraction,o>vec3<i32>(0));a+=liquid_fields[c.x+scene.dimensions.x*(c.y+scene.dimensions.y*c.z)].applied.z*w.x*w.y*w.z;
 }}}return a;
}
struct LiquidFragment {@location(0) color:vec4<f32>,@builtin(frag_depth) depth:f32}
@fragment fn fragment_liquid(v:LiquidVertex)->LiquidFragment {
 let near=scene.inverse_camera*vec4<f32>(v.clip,0.0,1.0);let far=scene.inverse_camera*vec4<f32>(v.clip,1.0,1.0);let origin=near.xyz/near.w;let direction=normalize(far.xyz/far.w-origin);
 let denominator=dot(direction,scene.normal.xyz);let plane_t=clamp(dot(scene.origin.xyz-origin,scene.normal.xyz)/select(1e-20,denominator,abs(denominator)>1e-20),-1e7,1e7);
 let plane_world=origin+direction*plane_t;let plane_a=liquid_alpha(plane_world);let edge_width=max(fwidth(plane_a),1e-6);
 let u=normalize(cross(scene.normal.xyz,select(vec3<f32>(0.0,0.0,1.0),vec3<f32>(0.0,1.0,0.0),abs(scene.normal.z)>0.9)));let w=cross(scene.normal.xyz,u);
 let coordinate=(dot(plane_world,u)+dot(plane_world,w))*0.70710678/scene.settings.y;let width=max(fwidth(coordinate),0.002);let ink=1.0-smoothstep(width*0.45,width*1.15,abs(fract(coordinate)-0.5));
 let hatched_color=mix(vec3<f32>(0.7,0.76,0.8),vec3<f32>(0.13,0.16,0.2),ink*0.85);
 if scene.liquid.y>0.5 {
    if plane_a<0.5 || (scene.liquid.z<0.5 && plane_a>0.5+2.0*edge_width){discard;}
    let projected=scene.camera*vec4<f32>(plane_world,1.0);return LiquidFragment(vec4<f32>(linear_color(select(vec3<f32>(0.7,0.85,0.9),hatched_color,scene.liquid.z>0.5)),1.0),projected.z/projected.w);
 }
 let low=scene.grid_minimum.xyz;let high=low+vec3<f32>(scene.dimensions.xyz)*scene.grid_minimum.w;
 var first=0.0;var last=1e30;
 for(var axis=0u;axis<3u;axis++){if abs(direction[axis])<1e-10 {if origin[axis]<low[axis]||origin[axis]>high[axis]{discard;}}else{let a=(low[axis]-origin[axis])/direction[axis];let b=(high[axis]-origin[axis])/direction[axis];first=max(first,min(a,b));last=min(last,max(a,b));}}
 if first>last{discard;}
 let h=scene.grid_minimum.w;var t=first;var previous=first;var found=false;
 for(var step=0u;step<2048u;step++){
 if t>last{break;}let at=origin+t*direction;let kept=scene.settings.x<0.5||dot(at-scene.origin.xyz,scene.normal.xyz)<=0.0;
 if kept && liquid_alpha(at)>=0.5 {found=true;break;}previous=t;t+=0.5*h;
 }
 if !found{discard;}
 for(var j=0;j<6;j++){let mid=0.5*(previous+t);let at=origin+mid*direction;let kept=scene.settings.x<0.5||dot(at-scene.origin.xyz,scene.normal.xyz)<=0.0;if kept&&liquid_alpha(at)>=0.5{t=mid;}else{previous=mid;}}
 let world=origin+t*direction;var grad=vec3<f32>(0.0);for(var axis=0u;axis<3u;axis++){var d=vec3<f32>(0.0);d[axis]=0.5*h;grad[axis]=liquid_alpha(world+d)-liquid_alpha(world-d);}
 var normal=-grad/max(length(grad),1e-20);let cap=scene.settings.x>0.5 && abs(dot(world-scene.origin.xyz,scene.normal.xyz))<0.05*h;if cap{normal=scene.normal.xyz;}
 let shade=0.3+0.7*abs(dot(normal,normalize(vec3<f32>(0.3,-0.6,0.74))));let projected=scene.camera*vec4<f32>(world,1.0);
 var color=vec3<f32>(0.68,0.76,0.8)*shade;
 if cap && scene.liquid.w>0.5 {if scene.liquid.z>0.5 {color=hatched_color;}else{if plane_a>0.5+2.0*edge_width{discard;}color=vec3<f32>(0.7,0.85,0.9);}}
 return LiquidFragment(vec4<f32>(linear_color(color),1.0),projected.z/projected.w+select(0.0,1e-5,cap && scene.liquid.w<0.5));
}
