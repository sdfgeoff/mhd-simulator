// Diagnostic-only entry points.
@compute @workgroup_size(256)
fn clear(@builtin(global_invocation_id) g:vec3<u32>){if scalars[1].z>0.5{return;}let i=flat(g);if i>=p.dims.w{return;}if p.location.z==0u {work[i].w=0.;}else{value[p.location.x+i].y=0.;}}
@compute @workgroup_size(256)
fn smooth_from_temp(@builtin(global_invocation_id) g:vec3<u32>){jacobi_step(flat(g),true);}
@compute @workgroup_size(256)
fn probe_action(@builtin(global_invocation_id) g:vec3<u32>){let i=flat(g);if i>=p.dims.w{return;}let a=action(decode(i,p.dims.xyz),p.dims.xyz,p.location.x,p.location.z==0u,false);if p.location.z==0u{predicted[i].w=a;}else{value[p.location.x+i].z=a;}}
