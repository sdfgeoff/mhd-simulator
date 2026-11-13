struct Params { origin:vec4<f32>, u:vec4<f32>, v:vec4<f32>, grid:vec4<f32>, dims:vec4<u32>, source:vec4<u32>, validity:vec4<u32>, motion:vec4<f32> }
struct Vertex {position:vec4<f32>,color:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read> field:array<vec4<f32>>;
@group(0) @binding(2) var<storage,read_write> vertices:array<Vertex>;
fn hash(n:u32)->f32 {var x=n*747796405u+2891336453u;x=((x>>((x>>28u)+4u))^x)*277803737u;return f32((x>>22u)^x)/4294967296.0;}
fn cell(c:vec3<u32>)->vec3<f32> {
    let i=(c.x+p.dims.x*(c.y+p.dims.y*c.z))*p.dims.w;
    var v=field[i+p.source.x].xyz;
    if p.source.y!=0xffffffffu {v-=field[i+p.source.y].xyz;}
    return v;
}
fn inside(world:vec3<f32>)->bool {let q=(world-p.grid.xyz)/p.grid.w;return all(q>=vec3<f32>(0.0))&&all(q<vec3<f32>(p.dims.xyz));}
fn vector(world:vec3<f32>)->vec3<f32> {
    if !inside(world) {return vec3<f32>(0.0);}
    let q=(world-p.grid.xyz)/p.grid.w;
    if p.validity.x!=0xffffffffu {
        let cell=vec3<u32>(floor(q));let i=(cell.x+p.dims.x*(cell.y+p.dims.y*cell.z))*p.dims.w;
        if !(field[i+p.validity.x][p.validity.y]>=0.5) {return vec3<f32>(0.0);}
    }
    if p.source.w==0u {return cell(vec3<u32>(floor(q)));}
    let f=q-vec3<f32>(0.5);let base=vec3<i32>(floor(f));let fraction=fract(f);var value=vec3<f32>(0.0);
    for(var z=0;z<2;z++){for(var y=0;y<2;y++){for(var x=0;x<2;x++){
        let o=vec3<i32>(x,y,z);let c=vec3<u32>(clamp(base+o,vec3<i32>(0),vec3<i32>(p.dims.xyz)-vec3<i32>(1)));
        let w=select(vec3<f32>(1.0)-fraction,fraction,o>vec3<i32>(0));value+=cell(c)*w.x*w.y*w.z;
    }}}return value;
}
fn direction(world:vec3<f32>)->vec3<f32> {
    let v=vector(world);let projected=dot(v,p.u.xyz)*p.u.xyz+dot(v,p.v.xyz)*p.v.xyz;
    let magnitude=length(projected);
    if !(magnitude>max(p.motion.w,1e-20) && magnitude<1e30) {return vec3<f32>(0.0);}
    return projected/magnitude;
}
fn alpha(u:f32,head:f32)->f32 {
    let distance=head-u;
    if distance<0.0 || distance>p.motion.z {return 0.0;}
    return (1.0-distance/p.motion.z)*smoothstep(0.0,0.06,head)*smoothstep(0.0,0.06,1.0-head);
}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    let n=p.source.z;let seed=id.x;if seed>=n*n{return;}
    let jitter=vec2<f32>(hash(seed*2u),hash(seed*2u+1u))*0.7+vec2<f32>(0.15);
    let uv=(vec2<f32>(f32(seed%n),f32(seed/n))+jitter)/f32(n)*2.0-vec2<f32>(1.0);
    var position=p.origin.xyz+p.origin.w*(uv.x*p.u.xyz+uv.y*p.v.xyz);
    let head=fract(p.motion.x+hash(seed+9187u));var alive=true;
    for(var i=0u;i<64u;i++) {
        let a=position;let d=direction(a);let mid=a+d*p.motion.y*0.5;let d2=direction(mid);
        let b=a+d2*p.motion.y;
        let end_direction=direction(b);
        alive=alive && dot(d,d)>0.5 && dot(d2,d2)>0.5 && dot(end_direction,end_direction)>0.5 && inside(b);
        let u0=f32(i)/64.0;let u1=f32(i+1u)/64.0;
        let alpha0=select(0.0,alpha(u0,head),alive);let alpha1=select(0.0,alpha(u1,head),alive);
        let index=(seed*64u+i)*2u;
        vertices[index]=Vertex(vec4<f32>(a,1.0),vec4<f32>(alpha0));
        vertices[index+1u]=Vertex(vec4<f32>(b,1.0),vec4<f32>(alpha1));
        position=b;
    }
}

@compute @workgroup_size(64)
fn gather(@builtin(global_invocation_id) id:vec3<u32>) {
    let n=p.source.z;let i=id.x;if i>=n*n{return;}
    let uv=(vec2<f32>(f32(i%n),f32(i/n))+vec2<f32>(0.5))/f32(n)*2.0-vec2<f32>(1.0);
    let world=p.origin.xyz+p.origin.w*(uv.x*p.u.xyz+uv.y*p.v.xyz);
    vertices[i]=Vertex(vec4<f32>(vector(world),0.0),vec4<f32>(0.0));
}
