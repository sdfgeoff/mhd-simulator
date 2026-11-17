// Test-only direct operator samples, independent of time-integration error.
@compute @workgroup_size(256)
fn stress_original_probe(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat_id(g);if i>=p.faces.w{return;}let q=decode(i,p.faces.xyz);
 var out=vec4<f32>(0.0);
 for(var axis=0u;axis<3u;axis++){if open_face(q,axis){out[axis]=stress_acceleration(q,axis,false);}}
 velocity[i]=out;
}
@compute @workgroup_size(256)
fn stress_cached_probe(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat_id(g);if i>=p.faces.w{return;}let q=decode(i,p.faces.xyz);
 var out=vec4<f32>(0.0);
 for(var axis=0u;axis<3u;axis++){if open_face(q,axis){out[axis]=stress_acceleration(q,axis,true);}}
 velocity[i]=out;
}
