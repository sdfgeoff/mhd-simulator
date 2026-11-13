struct Params {origin:vec4<f32>,dims:vec4<u32>,edges:vec4<u32>,control:vec4<u32>,physical:vec4<f32>}
@group(0) @binding(0) var<uniform> scene:Params;
fn flat_id(id:vec3<u32>)->u32{return id.x+id.y*65535u*256u;}
fn edge_dims(axis:u32)->vec3<u32>{var d=scene.dims.xyz+vec3<u32>(1);d[axis]-=1u;return d;}
fn edge_offset(axis:u32)->u32{if axis==0u{return 0u;}if axis==1u{return scene.edges.x;}return scene.edges.x+scene.edges.y;}
fn edge_index(axis:u32,p:vec3<i32>)->u32 {let d=edge_dims(axis);return edge_offset(axis)+u32(p.x)+d.x*(u32(p.y)+d.y*u32(p.z));}
fn edge_decode(i:u32)->vec4<u32>{var axis=0u;var q=i;if q>=scene.edges.x{axis=1u;q-=scene.edges.x;if q>=scene.edges.y {axis=2u;q-=scene.edges.y;}}let d=edge_dims(axis);return vec4<u32>(q%d.x,(q/d.x)%d.y,q/(d.x*d.y),axis);}
fn edge_position(e:vec4<u32>)->vec3<f32>{var p=vec3<f32>(e.xyz);p[e.w]+=0.5;return scene.origin.xyz+p*scene.physical.z;}
fn boundary(e:vec4<u32>)->bool {let a=(e.w+1u)%3u;let b=(e.w+2u)%3u;return e[a]==0u||e[a]==scene.dims[a]||e[b]==0u||e[b]==scene.dims[b];}
fn face_dims(axis:u32)->vec3<u32>{var d=scene.dims.xyz;d[axis]+=1u;return d;}
fn valid(p:vec3<i32>,d:vec3<u32>)->bool{return all(p>=vec3<i32>(0))&&all(p<vec3<i32>(d));}
