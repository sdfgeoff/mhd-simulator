struct Element {a:vec4<f32>,b:vec4<f32>,c:vec4<f32>}
@group(0) @binding(1) var<storage,read> elements:array<Element>;
@group(0) @binding(2) var<storage,read_write> source:array<vec4<f32>>;
fn ash(x:f32)->f32{return sign(x)*log(abs(x)+sqrt(x*x+1.0));}
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id:vec3<u32>){
    let i=flat_id(id);if i>=scene.edges.w{return;}
    let e=edge_decode(i);let p=edge_position(e);var a=vec3<f32>(0.0);
    let begin=scene.control.w;let end=min(scene.control.x,begin+32u);
    for(var j=begin;j<end;j++) {
        let s=elements[j];if s.c.z==0.0 {continue;}let r=p-s.a.xyz;
        if s.c.y<0.5 {let delta=s.b.xyz-s.a.xyz;let len=length(delta);let u=delta/len;let t=dot(r,u);let perp=r-u*t;let radius=sqrt(dot(perp,perp)+s.c.x*s.c.x);a+=u*(1e-7*s.c.z*(ash(t/radius)-ash((t-len)/radius)));}
        else if s.c.y<1.5 {a+=s.b.xyz*(1e-7*s.c.z*inverseSqrt(dot(r,r)+s.c.x*s.c.x));}
        else {a+=0.5*s.c.z*cross(s.b.xyz,p);}
    }
    if begin==0u {source[i].y=source[i].x;source[i].x=a[e.w];}else{source[i].x+=a[e.w];}
    if scene.control.y==1u && end==scene.control.x {source[i].y=source[i].x;}
}
