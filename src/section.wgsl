struct Params { origin: vec4<f32>, u: vec4<f32>, v: vec4<f32>, shape: vec4<u32> }
struct Triangle { a:vec4<f32>, b:vec4<f32>, c:vec4<f32> }
@group(0) @binding(0) var<storage,read> tris: array<Triangle>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage,read_write> pixels: array<u32>;
fn hit(p:vec3<f32>, t:Triangle)->bool {
    let d=vec3<f32>(0.743391,0.421719,0.519173);
    let e1=t.b.xyz-t.a.xyz;let e2=t.c.xyz-t.a.xyz;let h=cross(d,e2);let det=dot(e1,h);
    if abs(det)<1e-10 {return false;}
    let s=p-t.a.xyz;let a=dot(s,h)/det;
    if a<0.0 || a>1.0 {return false;}
    let q=cross(s,e1);let b=dot(d,q)/det;
    if b<0.0 || a+b>1.0 {return false;}
    return dot(e2,q)/det>1e-7;
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>){
    let n=params.shape.x; if id.x>=n || id.y>=n{return;}
    let xy=(vec2<f32>(id.xy)+vec2<f32>(0.5))/f32(n)*2.0-vec2<f32>(1.0);
    let p=params.origin.xyz+xy.x*params.u.xyz+xy.y*params.v.xyz;
    var mask=0u;
    for(var i=0u;i<params.shape.y;i++){if hit(p,tris[i]){mask=mask^(1u<<u32(tris[i].a.w));}}
    var value=0u;
    if countOneBits(mask)>1u {value=255u;}else if mask!=0u {value=firstTrailingBit(mask)+1u;}
    pixels[id.y*n+id.x]=value;
}
