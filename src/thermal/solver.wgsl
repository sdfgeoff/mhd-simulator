struct Scene {dims:vec4<u32>,control:vec4<u32>,step:vec4<f32>,boundary:array<vec4<f32>,6>,exposed:vec4<f32>}
struct Cell {state:vec4<f32>,flux:vec4<f32>,budget:vec4<f32>,valid:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Scene;
@group(0) @binding(1) var<storage,read> ids:array<u32>;
@group(0) @binding(2) var<storage,read> bodies:array<u32>;
@group(0) @binding(3) var<storage,read> catalog:array<vec4<f32>>;
@group(0) @binding(4) var<storage,read_write> cells_in:array<Cell>;
@group(0) @binding(5) var<storage,read_write> cells_out:array<Cell>;
@group(0) @binding(6) var<storage,read> em:array<vec4<f32>>;
@group(0) @binding(7) var<storage,read> velocity:array<vec4<f32>>;
@group(0) @binding(8) var<storage,read_write> stats:array<vec4<f32>>;
var<workgroup> reduction:array<vec4<f32>,256>;
var<workgroup> extremes:array<vec4<f32>,256>;
var<workgroup> corrections:array<vec4<f32>,256>;
fn flat(g:vec3<u32>)->u32{return g.x+g.y*65535u*256u;}
fn groups()->u32{return (p.dims.w+255u)/256u;}
fn group(g:vec3<u32>)->u32{return g.x+g.y*65535u;}
fn point(i:u32)->vec3<i32>{return vec3<i32>(i32(i%p.dims.x),i32((i/p.dims.x)%p.dims.y),i32(i/(p.dims.x*p.dims.y)));}
fn inside(q:vec3<i32>)->bool{return all(q>=vec3<i32>(0))&&all(q<vec3<i32>(p.dims.xyz));}
fn index(q:vec3<i32>)->u32{return u32(q.x)+p.dims.x*(u32(q.y)+p.dims.y*u32(q.z));}
fn row(i:u32)->vec4<f32>{return catalog[ids[i]*2u];}
fn transition(i:u32)->vec4<f32>{return catalog[ids[i]*2u+1u];}
fn body(i:u32)->u32{return p.control.y+2u*firstTrailingBit(bodies[i]);}
fn enthalpy(i:u32,t:f32)->f32 {
    let m=row(i);let phase=transition(i);var f=0.0;
    if phase.z>0.5 {if phase.x==phase.y {f=select(0.0,1.0,t>phase.x);}else{f=clamp((t-phase.x)/(phase.y-phase.x),0.0,1.0);}}
    return m.x*(m.y*(t-273.15)+m.w*f);
}
// Return T, liquid fraction, and an invalid/missing-phase-data indicator.
fn invert(i:u32,h:f32)->vec3<f32>{
    let m=row(i);let phase=transition(i);let cp=m.x*m.y;
    var t=h/cp+273.15;var f=0.0;var bad=0.0;
    if phase.z>0.5 {
        let hs=cp*(phase.x-273.15);let hl=cp*(phase.y-273.15)+m.x*m.w;
        if phase.z>1.5 {if t>phase.x {bad=1.0;}}
        else if h>hs && h<hl {
            if phase.x==phase.y {t=phase.x;f=(h-hs)/(m.x*m.w);}
            else {t=(h+cp*273.15+m.x*m.w*phase.x/(phase.y-phase.x))/(cp+m.x*m.w/(phase.y-phase.x));f=clamp((t-phase.x)/(phase.y-phase.x),0.0,1.0);}
        }else if h>=hl {t=(h-m.x*m.w)/cp+273.15;f=1.0;}
    }
    if !(t>=0.0) || !(t<1e8) || !(abs(h)<1e30) {bad=1.0;}
    return vec3<f32>(t,f,bad);
}
fn boundary_rate(b:vec4<f32>,k:f32,t:f32)->f32 {
    if b.x<0.5{return 0.0;}
    if b.x<1.5{return 2.0*k*(b.y-t)/(p.step.y*p.step.y);}
    if b.x<2.5{return b.y/p.step.y;}
    return b.y*(b.z-t)/p.step.y;
}
@compute @workgroup_size(256)
fn initialize(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat(gid);if i>=p.dims.w{return;}var c=Cell(vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0));
    if ids[i]!=0u && bodies[i]!=0u {
        let b=body(i);var t=catalog[b].x;if catalog[b+1u].x>0.5{t=catalog[b].y;}
        let h=enthalpy(i,t);let phase=invert(i,h);
        c.state=vec4<f32>(phase.xy,h,1.0);c.valid=vec4<f32>(1.0,0.0,0.0,0.0);c.budget.w=phase.z;
    }cells_out[i]=c;
}
@compute @workgroup_size(256)
fn reset_budget(@builtin(global_invocation_id) gid:vec3<u32>){let i=flat(gid);if i<p.dims.w{cells_in[i].budget=vec4<f32>(0.0);cells_in[i].valid.z=cells_in[i].state.z;cells_in[i].valid.w=cells_in[i].valid.y;}}
@compute @workgroup_size(256)
fn integrate(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat(gid);if i>=p.dims.w{return;}var c=cells_in[i];if c.state.w<0.5 {cells_out[i]=c;return;}
    let q=point(i);let m=row(i);let t=c.state.x;let dx=p.step.y;let dt=p.step.x;
    var conduction=0.0;var external_heat=0.0;var advection=0.0;
    for(var axis=0u;axis<3u;axis++){for(var side=0u;side<2u;side++){
        var neighbor=q;neighbor[axis]+=select(-1,1,side==1u);
        if !inside(neighbor) {external_heat+=boundary_rate(p.boundary[axis*2u+side],m.z,t);}
        else {let j=index(neighbor);
            if cells_in[j].state.w<0.5 {external_heat+=boundary_rate(p.exposed,m.z,t);}
            else {
                let k=row(j).z;conduction+=2.0*m.z*k/max(m.z+k,1e-30)*(cells_in[j].state.x-t)/(dx*dx);
                if p.control.w!=0u {
                    var face=q;if side==1u {face[axis]+=1;}
                    let fd=p.dims.xyz+vec3<u32>(1);let fi=u32(face.x)+fd.x*(u32(face.y)+fd.y*u32(face.z));let u=velocity[fi][axis];
                    let sign=select(-1.0,1.0,side==1u);let outward=u*sign;
                    let transported=select(cells_in[j].state.z,c.state.z,outward>=0.0);advection-=outward*transported/dx;
                }
            }
        }
    }}
    var joule=0.0;if p.control.z!=0u {joule=em[i*4u+2u].w;}
    let b=body(i);external_heat+=catalog[b].z;
    let increment=dt*(conduction+advection+external_heat+joule)-c.valid.y;
    var h=c.state.z+increment;var pinned=false;
    if catalog[b+1u].x>0.5 {
        let fixed=enthalpy(i,catalog[b].y);let transition_data=transition(i);
        let plateau=transition_data.z==1.0 && transition_data.x==transition_data.y && catalog[b].y==transition_data.x && h>=fixed && h<=fixed+m.x*m.w;
        if !plateau {
            let adjusted=select(fixed,clamp(h,fixed,fixed+m.x*m.w),transition_data.z==1.0 && transition_data.x==transition_data.y && catalog[b].y==transition_data.x);
            external_heat+=(adjusted-c.state.z+c.valid.y)/dt-(conduction+advection+external_heat+joule);h=adjusted;pinned=true;
        }
    }
    c.state.z=h;c.valid.y=increment;c.flux.w=select(0.0,1.0,pinned);
    c.budget+=vec4<f32>(joule*dt,external_heat*dt,0.0,0.0);
    if !(joule>=0.0) || !(joule<1e30) {c.budget.w=1.0;}
    cells_out[i]=c;
}
// A separate dispatch forces high-enthalpy rounding before recovering the low part.
// Relaxed GPU arithmetic can algebraically cancel an in-expression Kahan correction.
@compute @workgroup_size(256)
fn enthalpy_difference(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat(gid);if i>=p.dims.w || cells_out[i].state.w<0.5{return;}
    cells_out[i].flux.z=cells_out[i].state.z-cells_in[i].state.z;
    cells_out[i].budget.z=cells_out[i].state.z-cells_out[i].valid.z;
}
@compute @workgroup_size(256)
fn compensate(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat(gid);if i>=p.dims.w || cells_out[i].state.w<0.5{return;}
    var c=cells_out[i];var correction=c.flux.z-c.valid.y;
    if c.flux.w>0.5 {correction=0.0;}
    c.valid.y=correction;let phase=invert(i,c.state.z-correction);c.state.x=phase.x;c.state.y=phase.y;
    c.budget.z+=(c.valid.w-correction);c.budget.w+=phase.z;cells_out[i]=c;
}
@compute @workgroup_size(256)
fn flux(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat(gid);if i>=p.dims.w || cells_in[i].state.w<0.5{return;}let q=point(i);let t=cells_in[i].state.x;let k=row(i).z;var qflux=vec3<f32>(0.0);
    for(var axis=0u;axis<3u;axis++){for(var side=0u;side<2u;side++){
        var neighbor=q;neighbor[axis]+=select(-1,1,side==1u);var rate=0.0;
        if !inside(neighbor){rate=boundary_rate(p.boundary[axis*2u+side],k,t);}
        else {let j=index(neighbor);if cells_in[j].state.w<0.5{rate=boundary_rate(p.exposed,k,t);}else {let k2=row(j).z;rate=2.0*k*k2/max(k+k2,1e-30)*(cells_in[j].state.x-t)/(p.step.y*p.step.y);}}
        qflux[axis]+=select(1.0,-1.0,side==1u)*rate*p.step.y*0.5;
    }}cells_in[i].flux=vec4<f32>(qflux,length(qflux));
}
@compute @workgroup_size(256)
fn collect(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) g:vec3<u32>){
    let i=flat(gid);var sum=vec4<f32>(0.0);var bound=vec4<f32>(1e30,0.0,0.0,0.0);var extra=vec4<f32>(0.0);
    if i<p.dims.w && cells_in[i].state.w>0.5 {let c=cells_in[i];let volume=p.step.y*p.step.y*p.step.y;
        sum=vec4<f32>(c.state.z,c.budget.xy,c.state.y)*volume;extra=vec4<f32>(vec3<f32>(c.budget.z,c.valid.y,0.0)*volume,c.flux.w);bound=vec4<f32>(c.state.x,c.state.x,1.0,c.budget.w);
    }
    reduction[l]=sum;extremes[l]=bound;corrections[l]=extra;workgroupBarrier();
    for(var s=128u;s>0u;s/=2u){if l<s{reduction[l]+=reduction[l+s];corrections[l]=vec4<f32>(corrections[l].xyz+corrections[l+s].xyz,max(corrections[l].w,corrections[l+s].w));extremes[l]=vec4<f32>(min(extremes[l].x,extremes[l+s].x),max(extremes[l].y,extremes[l+s].y),extremes[l].zw+extremes[l+s].zw);}workgroupBarrier();}
    if l==0u && group(g)<groups(){stats[group(g)*3u]=reduction[0];stats[group(g)*3u+1u]=extremes[0];stats[group(g)*3u+2u]=corrections[0];}
}
@compute @workgroup_size(256)
fn reduce(@builtin(local_invocation_index) l:u32){
    var sum=vec4<f32>(0.0);var bound=vec4<f32>(1e30,0.0,0.0,0.0);var extra=vec4<f32>(0.0);
    for(var i=l;i<groups();i+=256u){sum+=stats[i*3u];let e=stats[i*3u+2u];extra=vec4<f32>(extra.xyz+e.xyz,max(extra.w,e.w));let b=stats[i*3u+1u];bound=vec4<f32>(min(bound.x,b.x),max(bound.y,b.y),bound.zw+b.zw);}
    reduction[l]=sum;extremes[l]=bound;corrections[l]=extra;workgroupBarrier();
    for(var s=128u;s>0u;s/=2u){if l<s{reduction[l]+=reduction[l+s];corrections[l]=vec4<f32>(corrections[l].xyz+corrections[l+s].xyz,max(corrections[l].w,corrections[l+s].w));extremes[l]=vec4<f32>(min(extremes[l].x,extremes[l+s].x),max(extremes[l].y,extremes[l+s].y),extremes[l].zw+extremes[l+s].zw);}workgroupBarrier();}
    if l==0u {stats[groups()*3u]=reduction[0];stats[groups()*3u+1u]=extremes[0];stats[groups()*3u+2u]=corrections[0];}
}
