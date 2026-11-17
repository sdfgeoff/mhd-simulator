// RKC2 stores initial face values/operator in display lanes that are dead until
// fields() rebuilds the accepted packet. Pressure and interface lanes stay intact.
// Every independent open face has a canonical lower cell; closed outer faces are zero.
struct StressStage {blend:vec4<f32>,initial:vec4<f32>}
@group(0) @binding(9) var<uniform> stress_stage:StressStage;
@compute @workgroup_size(256)
fn stress_save_initial(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat_id(g);if i>=p.dims.w{return;}let q=decode(i,p.dims.xyz);
 var y=vec3<f32>(0.0);var op=vec3<f32>(0.0);
 for(var axis=0u;axis<3u;axis++){if open_face(q,axis){
  y[axis]=face(q,axis,true);op[axis]=stress_acceleration(q,axis);
 }}
 output_fields[i].velocity=vec4<f32>(y,0.0);
 output_fields[i].force=vec4<f32>(op,0.0);
}
@compute @workgroup_size(256)
fn stress_stabilized(@builtin(global_invocation_id) g:vec3<u32>){
 let i=flat_id(g);if i>=p.faces.w{return;}let q=decode(i,p.faces.xyz);
 let older=velocity[i];var out=vec4<f32>(0.0);
 for(var axis=0u;axis<3u;axis++){if open_face(q,axis){
  let cell=index(wrap_cell(q),p.dims.xyz);
  let previous=face(q,axis,true);
  let initial=output_fields[cell].velocity[axis];
  let initial_op=output_fields[cell].force[axis];
  out[axis]=stress_stage.blend.x*previous+stress_stage.blend.y*older[axis]
    +stress_stage.blend.z*initial+stress_stage.blend.w*stress_acceleration(q,axis)
    +stress_stage.initial.x*initial_op;
 }}
 velocity[i]=out;
}
