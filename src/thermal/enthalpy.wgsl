// Shared constitutive initialization: enthalpy per occupied material volume.
fn material_enthalpy(m:vec4<f32>,phase:vec4<f32>,t:f32)->f32 {
 var f=0.0;
 if phase.z>0.5 {if phase.x==phase.y {f=select(0.0,1.0,t>phase.x);}
 else {f=clamp((t-phase.x)/(phase.y-phase.x),0.0,1.0);}}
 return m.x*(m.y*(t-273.15)+m.w*f);
}
fn initial_body_temperature(data:vec4<f32>,kind:vec4<f32>)->f32 {
 return select(data.x,data.y,kind.x>0.5);
}
