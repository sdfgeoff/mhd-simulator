struct Slice {origin:vec4<f32>,u:vec4<f32>,v:vec4<f32>,minimum:vec4<f32>,dims:vec4<u32>,control:vec4<u32>}
struct Result {b:vec4<f32>,j:vec4<f32>,fq:vec4<f32>,applied:vec4<f32>}
@group(0) @binding(0) var<uniform> slice:Slice;
@group(0) @binding(1) var<storage,read> fields:array<Result>;
@group(0) @binding(2) var<storage,read_write> samples:array<vec4<f32>>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>){
    let n=slice.control.x;if id.x>=n||id.y>=n{return;}
    let xy=(vec2<f32>(id.xy)+vec2<f32>(0.5))/f32(n)*2.0-vec2<f32>(1.0);
    let world=slice.origin.xyz+xy.x*slice.u.xyz+xy.y*slice.v.xyz;
    let p=vec3<i32>(floor((world-slice.minimum.xyz)/slice.minimum.w));var out=vec4<f32>(0.0);
    if all(p>=vec3<i32>(0))&&all(p<vec3<i32>(slice.dims.xyz)){
        let f=fields[u32(p.x)+slice.dims.x*(u32(p.y)+slice.dims.y*u32(p.z))];out=f.b;
        if slice.control.y==2u{out=f.applied;}else if slice.control.y==3u{out=vec4<f32>(f.b.xyz-f.applied.xyz,0.0);}else if slice.control.y==4u{out=f.j;}else if slice.control.y==5u{out=vec4<f32>(f.fq.xyz,0.0);}
    }
    samples[id.x+n*id.y]=out;
}
