struct Slice { origin:vec4<f32>, u:vec4<f32>, v:vec4<f32>, minimum:vec4<f32>, dimensions:vec4<u32>, options:vec4<u32> }
struct Pixel { body:u32, material:u32, padding:f32, inside:u32 }
@group(0) @binding(0) var<uniform> params:Slice;
@group(0) @binding(1) var<storage,read> masks:array<u32>;
@group(0) @binding(2) var<storage,read> materials:array<u32>;
@group(0) @binding(3) var<storage,read_write> pixels:array<Pixel>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>){
    let n=params.options.x;if id.x>=n||id.y>=n{return;}
    let xy=(vec2<f32>(id.xy)+vec2<f32>(0.5))/f32(n)*2.0-vec2<f32>(1.0);
    let world=params.origin.xyz+xy.x*params.u.xyz+xy.y*params.v.xyz;
    let grid=(world-params.minimum.xyz)/params.minimum.w;let p=vec3<i32>(floor(grid));
    var result=Pixel(0u,0u,0.0,0u);
    if all(p>=vec3<i32>(0))&&all(p<vec3<i32>(params.dimensions.xyz)){
        let i=(u32(p.z)*params.dimensions.y+u32(p.y))*params.dimensions.x+u32(p.x);let mask=masks[i];
        result.material=materials[i];result.inside=1u;
        if countOneBits(mask)>1u{result.body=255u;}else if mask!=0u{result.body=firstTrailingBit(mask)+1u;}

    }
    pixels[id.y*n+id.x]=result;
}
