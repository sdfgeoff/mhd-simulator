struct Slice { origin:vec4<f32>, u:vec4<f32>, v:vec4<f32>, minimum:vec4<f32>, dimensions:vec4<u32>, options:vec4<u32> }
@group(0) @binding(0) var<uniform> params:Slice;
@group(0) @binding(1) var<storage,read> masks:array<u32>;
@group(0) @binding(2) var<storage,read> materials:array<u32>;
@group(0) @binding(3) var<storage,read_write> cells:array<vec4<u32>>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>){
    let k=params.options.x;let a=(k+1u)%3u;let b=(k+2u)%3u;
    if id.x>=params.dimensions[a]||id.y>=params.dimensions[b]{return;}
    let base=(id.y*params.dimensions[a]+id.x)*4u;
    for(var slot=0u;slot<4u;slot++){cells[base+slot]=vec4<u32>(0u);}
    let h=params.minimum.w;let normal=normalize(cross(params.u.xyz,params.v.xyz));
    let ca=params.minimum[a]+(f32(id.x)+0.5)*h;let cb=params.minimum[b]+(f32(id.y)+0.5)*h;
    let centre=params.origin[k]-(normal[a]*(ca-params.origin[a])+normal[b]*(cb-params.origin[b]))/normal[k];
    let radius=h*0.5*(1.0+abs(normal[a]/normal[k])+abs(normal[b]/normal[k]));
    var first=i32(ceil((centre-radius-params.minimum[k])/h-0.5));
    var last=i32(floor((centre+radius-params.minimum[k])/h-0.5));
    // An axis-aligned plane exactly on a face belongs to the positive cell.
    if abs(normal[a])+abs(normal[b])<0.000001{first=i32(floor((params.origin[k]-params.minimum[k])/h));last=first;}
    for(var slot=0u;slot<4u;slot++){
        let z=first+i32(slot);if z>last||z<0||z>=i32(params.dimensions[k]){continue;}
        var p=vec3<u32>(0u);p[a]=id.x;p[b]=id.y;p[k]=u32(z);
        let i=(p.z*params.dimensions.y+p.y)*params.dimensions.x+p.x;let mask=masks[i];if mask==0u{continue;}
        var body=255u;if countOneBits(mask)==1u{body=firstTrailingBit(mask)+1u;}
        cells[base+slot]=vec4<u32>(p,body|(materials[i]<<8u));
    }
}
