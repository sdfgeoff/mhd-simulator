struct Params { minimum:vec4<f32>, dimensions:vec4<u32>, control:vec4<u32> }
@group(0) @binding(0) var<uniform> params:Params;
@group(0) @binding(1) var occupancy:texture_2d_array<f32>;
@group(0) @binding(2) var<storage,read_write> masks:array<u32>;
@group(0) @binding(3) var<storage,read_write> materials:array<u32>;
@group(0) @binding(4) var<storage,read> assignments:array<u32>;
@group(0) @binding(5) var<storage,read_write> totals:array<atomic<u32>>;
var<workgroup> counts:array<atomic<u32>,33>;
fn index(p:vec3<u32>)->u32{return (p.z*params.dimensions.y+p.y)*params.dimensions.x+p.x;}
@compute @workgroup_size(8,8,4)
fn merge(@builtin(global_invocation_id) p:vec3<u32>){
    if any(p>=params.dimensions.xyz){return;}
    if textureLoad(occupancy,vec2<i32>(p.xy),i32(p.z),0).r>0.5{masks[index(p)]|=params.dimensions.w;}
}
@compute @workgroup_size(8,8,4)
fn finalize(@builtin(global_invocation_id) p:vec3<u32>,@builtin(local_invocation_index) local:u32){
    if local<33u {atomicStore(&counts[local],0u);}workgroupBarrier();
    if all(p<params.dimensions.xyz){
        let i=index(p);let mask=masks[i];var material=0u;
        if mask!=0u {
            atomicAdd(&counts[32],1u);
            if countOneBits(mask)>1u{material=255u;atomicAdd(&counts[31],1u);}else{material=assignments[firstTrailingBit(mask)];}
            for(var body=0u;body<params.control.y;body++){if (mask&(1u<<body))!=0u{atomicAdd(&counts[body],1u);}}
        }
        materials[i]=material;
    }
    workgroupBarrier();if local<33u{atomicAdd(&totals[local],atomicLoad(&counts[local]));}
}

// Surface coverage is a diagnostic, not a proof that every subcell detail survives.
@group(0) @binding(6) var<storage,read> probes:array<vec4<f32>>;
@compute @workgroup_size(64)
fn coverage(@builtin(global_invocation_id) id:vec3<u32>){
    let i=id.x+id.y*65536u;if i>=params.control.z{return;}
    let probe=probes[i];let p=vec3<i32>(floor((probe.xyz-params.minimum.xyz)/params.minimum.w));
    if any(p<vec3<i32>(0))||any(p>=vec3<i32>(params.dimensions.xyz)){return;}
    let bit=1u<<u32(probe.w);
    for(var z=-1;z<=1;z++){for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){
        let q=p+vec3<i32>(x,y,z);if all(q>=vec3<i32>(0))&&all(q<vec3<i32>(params.dimensions.xyz)){
            if (masks[index(vec3<u32>(q))]&bit)!=0u{return;}
        }
    }}}
    atomicAdd(&totals[33u+u32(probe.w)],1u);
}
