struct Physical {values:vec4<f32>,flags:vec4<u32>}
struct Result {b:vec4<f32>,j:vec4<f32>,fq:vec4<f32>,applied:vec4<f32>}
@group(0) @binding(1) var<storage,read> materials:array<u32>;
@group(0) @binding(2) var<storage,read> properties:array<Physical>;
@group(0) @binding(3) var<storage,read_write> source:array<vec4<f32>>;
// x=reaction A, y=residual, z=PCG search direction.
// w=operator/search product until update_residual, then the diagonal for update_search.
@group(0) @binding(4) var<storage,read_write> work:array<vec4<f32>>;
@group(0) @binding(5) var<storage,read_write> previous:array<f32>;
@group(0) @binding(6) var<storage,read_write> partials:array<vec4<f32>>;
@group(0) @binding(7) var<storage,read_write> scalars:array<vec4<f32>>;
@group(0) @binding(8) var<storage,read_write> results:array<Result>;
var<workgroup> reduction:array<vec4<f32>,256>;
var<workgroup> reduction_b:array<vec4<f32>,256>;
var<workgroup> reduction_c:array<vec4<f32>,256>;
var<workgroup> reduction_d:array<vec4<f32>,256>;
fn cell_properties(p:vec3<i32>)->vec2<f32> {
    if !valid(p,scene.dims.xyz)||scene.control.z==1u {return vec2<f32>(0.0,1.0);}
    let i=u32(p.x)+scene.dims.x*(u32(p.y)+scene.dims.y*u32(p.z));let id=materials[i];
    if id==0u{return properties[0].values.xy;}
    if (id&0x80000000u)!=0u {
        let alpha=f32((id>>8u)&0x7fffffu)/8388607.0;let value=properties[id&255u].values.xy;
        return vec2<f32>(alpha*value.x,mix(properties[0].values.y,value.y,alpha));
    }
    return properties[id].values.xy;
}
fn nu_face(axis:u32,p:vec3<i32>)->f32 {
    if !valid(p,face_dims(axis)){return 0.0;}
    var q=p;q[axis]-=1;
    return 0.5/cell_properties(p).y+0.5/cell_properties(q).y;
}
fn a_edge(axis:u32,p:vec3<i32>,kind:u32)->f32 {
    if !valid(p,edge_dims(axis)){return 0.0;}
    let i=edge_index(axis,p);
    if kind==0u{return work[i].x;}if kind==1u{return work[i].z;}
    if kind==2u{return source[i].x;}if kind==4u{return previous[i];}return previous[i]+work[i].x+source[i].x;
}
// C is the oriented edge-to-face incidence matrix, with h factored out.
fn curl_face(axis:u32,p:vec3<i32>,kind:u32)->f32 {
    if !valid(p,face_dims(axis)){return 0.0;}
    let a=(axis+1u)%3u;let b=(axis+2u)%3u;
    var qa=p;qa[a]+=1;var qb=p;qb[b]+=1;
    return a_edge(b,qa,kind)-a_edge(b,p,kind)-a_edge(a,qb,kind)+a_edge(a,p,kind);
}
fn face_term(axis:u32,p:vec3<i32>,kind:u32,contrast:bool)->f32 {
    if !valid(p,face_dims(axis)){return 0.0;}
    var nu=nu_face(axis,p);if contrast {nu=1.0-nu;}
    return nu*curl_face(axis,p,kind);
}
// Exact transpose of C: divergence(C^T v)=0, including material interfaces.
fn adjoint(e:vec4<u32>,kind:u32,contrast:bool)->f32 {
    let p=vec3<i32>(e.xyz);let a=(e.w+1u)%3u;let b=(e.w+2u)%3u;
    var qa=p;qa[a]-=1;var qb=p;qb[b]-=1;
    return face_term(b,p,kind,contrast)-face_term(b,qa,kind,contrast)-face_term(a,p,kind,contrast)+face_term(a,qb,kind,contrast);
}
// Remove only the pure-gauge nullspace supported on strictly nonconducting nodes.
// Each selected node touches zero-conductivity edges, so this cannot change J or curl A.
fn air_node(p:vec3<i32>)->bool {
    if any(p<=vec3<i32>(0)) || any(p>=vec3<i32>(scene.dims.xyz)){return false;}
    for(var x=0;x<2;x++){for(var y=0;y<2;y++){for(var z=0;z<2;z++){
        if cell_properties(p-vec3<i32>(x,y,z)).x>0.0{return false;}
    }}}return true;
}
fn node_div(p:vec3<i32>,kind:u32)->f32 {
    var value=0.0;for(var a=0u;a<3u;a++){var q=p;q[a]-=1;value+=a_edge(a,p,kind)-a_edge(a,q,kind);}return value;
}
fn gauge(e:vec4<u32>,kind:u32)->f32 {
    let flags=source[edge_index(e.w,vec3<i32>(e.xyz))+scene.edges.w].yz;
    let p=vec3<i32>(e.xyz);var upper=p;upper[e.w]+=1;
    var value=0.0;if flags.x>0.5 {value+=node_div(p,kind);}if flags.y>0.5 {value-=node_div(upper,kind);}return value;
}
fn mass(i:u32)->f32 {if scene.control.y==1u{return 0.0;}return scene.physical.y*source[i].z*scene.physical.z*scene.physical.z/scene.physical.x;}
fn diagonal(i:u32)->f32 {return max(source[i].w+mass(i),1e-30);}
fn group_id(w:vec3<u32>)->u32{return w.x+w.y*65535u;}
fn sum_group(local:u32)->vec4<f32>{
    workgroupBarrier();for(var s=128u;s>0u;s/=2u){if local<s {reduction[local]+=reduction[local+s];}workgroupBarrier();}return reduction[0];
}
@compute @workgroup_size(256)
fn coefficients(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>) {
    let i=flat_id(gid);var v=vec4<f32>(0.0,1e30,0.0,0.0);
    if i<scene.edges.w {
        let e=edge_decode(i);let p=vec3<i32>(e.xyz);let a=(e.w+1u)%3u;let b=(e.w+2u)%3u;
        var sigma=0.0;for(var u=0;u<2;u++){for(var v=0;v<2;v++){var q=p;q[a]-=u;q[b]-=v;sigma+=0.25*cell_properties(q).x;}}
        var qa=p;qa[a]-=1;var qb=p;qb[b]-=1;
        var diag=nu_face(b,p)+nu_face(b,qa)+nu_face(a,p)+nu_face(a,qb);
        if boundary(e){diag=1.0;if sigma>0.0{v.x=1.0;}}
        var upper=p;upper[e.w]+=1;
        let lower_air=select(0.0,1.0,air_node(p));let upper_air=select(0.0,1.0,air_node(upper));
        // In unit-reluctivity air, CᵀC + the complete nodal gauge is the
        // componentwise seven-point negative Laplacian. Mark only edges whose
        // two endpoints have the full gauge and whose four incident faces
        // have unit reluctivity. Material/boundary edges retain the full stencil.
        let scalar_air=lower_air>0.5 && upper_air>0.5 &&
            nu_face(b,p)==1.0 && nu_face(b,qa)==1.0 &&
            nu_face(a,p)==1.0 && nu_face(a,qb)==1.0;
        source[i+scene.edges.w]=vec4<f32>(0.0,lower_air,upper_air,select(0.0,1.0,scalar_air));
        source[i].z=sigma;source[i].w=diag+lower_air+upper_air;
        v.y=select(1e30,sigma,sigma>0.0);v.z=sigma;v.w=select(0.0,1.0,sigma>0.0);
    }
    reduction[l]=v;workgroupBarrier();for(var stride=128u;stride>0u;stride/=2u){if l<stride{let a=reduction[l];let b=reduction[l+stride];reduction[l]=vec4<f32>(a.x+b.x,min(a.y,b.y),max(a.z,b.z),a.w+b.w);}workgroupBarrier();}let sum=reduction[0];if l==0u && group_id(group)<(scene.edges.w+255u)/256u{partials[group_id(group)]=sum;}
}
@compute @workgroup_size(256)
fn coefficient_check(@builtin(local_invocation_index) l:u32){
    var sum=vec4<f32>(0.0,1e30,0.0,0.0);let n=(scene.edges.w+255u)/256u;for(var i=l;i<n;i+=256u){let b=partials[i];sum=vec4<f32>(sum.x+b.x,min(sum.y,b.y),max(sum.z,b.z),sum.w+b.w);}
    reduction[l]=sum;workgroupBarrier();for(var stride=128u;stride>0u;stride/=2u){if l<stride{let a=reduction[l];let b=reduction[l+stride];reduction[l]=vec4<f32>(a.x+b.x,min(a.y,b.y),max(a.z,b.z),a.w+b.w);}workgroupBarrier();}if l==0u{scalars[0]=reduction[0];}
}
@compute @workgroup_size(256)
fn prepare(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>) {
    let i=flat_id(gid);var v=vec4<f32>(0.0);
    if i<scene.edges.w {
        let e=edge_decode(i);
        var r=mass(i)*scene.physical.x*source[i+scene.edges.w].x-mass(i)*(source[i].x-source[i].y)+adjoint(e,2u,true)-adjoint(e,4u,false)-gauge(e,4u);
        if boundary(e){r=0.0;}
        let z=r/diagonal(i);work[i].y=r;work[i].z=z;work[i].w=0.0;v=vec4<f32>(r*z,r*r,0.0,0.0);
    }
    reduction[l]=v;let sum=sum_group(l);if l==0u && group_id(group)<(scene.edges.w+255u)/256u{partials[group_id(group)]=sum;}
}
@compute @workgroup_size(256)
fn initial_reduce(@builtin(local_invocation_index) l:u32){
    var v=vec4<f32>(0.0);let n=(scene.edges.w+255u)/256u;for(var i=l;i<n;i+=256u){v+=partials[i];}reduction[l]=v;let sum=sum_group(l);
    if l==0u {let threshold=sum.y*scene.physical.w+1e-38;scalars[0]=vec4<f32>(sum.x,sum.y,sum.y,threshold);scalars[1]=vec4<f32>(0.0,0.0,0.0,select(0.0,1.0,sum.y<=threshold));}
}
fn air_laplacian(e:vec4<u32>)->f32 {
    let p=vec3<i32>(e.xyz);let center=work[edge_index(e.w,p)].z;
    var result=0.0;
    for(var a=0u;a<3u;a++) {
        var lower=p;lower[a]-=1;var upper=p;upper[a]+=1;
        result+=(center-a_edge(e.w,lower,1u))+(center-a_edge(e.w,upper,1u));
    }
    return result;
}
@compute @workgroup_size(256)
fn apply_operator(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
    let i=flat_id(gid);var dot=0.0;
    if i<scene.edges.w && scalars[1].w==0.0 {
        let e=edge_decode(i);var q=0.0;
        if source[i+scene.edges.w].w>0.5 {q=air_laplacian(e);}else{q=adjoint(e,1u,false)+gauge(e,1u)+mass(i)*work[i].z;}
        if boundary(e){q=work[i].z;}work[i].w=q;dot=work[i].z*q;
    }
    reduction[l]=vec4<f32>(dot,0.0,0.0,0.0);let sum=sum_group(l);if l==0u && group_id(group)<(scene.edges.w+255u)/256u{partials[group_id(group)]=sum;}
}
@compute @workgroup_size(256)
fn alpha_reduce(@builtin(local_invocation_index) l:u32){
    var v=vec4<f32>(0.0);let n=(scene.edges.w+255u)/256u;for(var i=l;i<n;i+=256u){v+=partials[i];}reduction[l]=v;let sum=sum_group(l);
    if l==0u && scalars[1].w==0.0 {if sum.x<=0.0 || abs(sum.x)>3e38 {scalars[1].w=2.0;}else{scalars[1].x=scalars[0].x/sum.x;}}
}
@compute @workgroup_size(256)
fn update_residual(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
    let i=flat_id(gid);var v=vec4<f32>(0.0);
    if i<scene.edges.w && scalars[1].w==0.0 {
        let old=work[i];let r=old.y-scalars[1].x*old.w;let d=diagonal(i);
        // The operator product is dead after this update. Carry the diagonal
        // to search in the same record, avoiding another full coefficient read.
        work[i]=vec4<f32>(old.x+scalars[1].x*old.z,r,old.z,d);
        v=vec4<f32>(r*r/d,r*r,0.0,0.0);
    }
    reduction[l]=v;let sum=sum_group(l);if l==0u && group_id(group)<(scene.edges.w+255u)/256u{partials[group_id(group)]=sum;}
}
@compute @workgroup_size(256)
fn beta_reduce(@builtin(local_invocation_index) l:u32){
    var v=vec4<f32>(0.0);let n=(scene.edges.w+255u)/256u;for(var i=l;i<n;i+=256u){v+=partials[i];}reduction[l]=v;let sum=sum_group(l);
    if l==0u && scalars[1].w==0.0 {scalars[1].y=sum.x/scalars[0].x;scalars[0].x=sum.x;scalars[0].y=sum.y;scalars[1].z+=1.0;if sum.y<=scalars[0].w {scalars[1].w=1.0;}if abs(sum.x)>3e38||abs(sum.y)>3e38{scalars[1].w=2.0;}}
}
@compute @workgroup_size(256)
fn update_search(@builtin(global_invocation_id) gid:vec3<u32>){let i=flat_id(gid);if i<scene.edges.w && scalars[1].w==0.0 {let v=work[i];work[i].z=v.y/v.w+scalars[1].y*v.z;}}
@compute @workgroup_size(256)
fn rollback(@builtin(global_invocation_id) gid:vec3<u32>){let i=flat_id(gid);if i<scene.edges.w {work[i].x=previous[i];source[i].x=source[i].y;}}
fn electric_edge(axis:u32,p:vec3<i32>)->f32 {
    if scene.control.y==1u || !valid(p,edge_dims(axis)){return 0.0;}
    let i=edge_index(axis,p);return -(work[i].x+(source[i].x-source[i].y))/scene.physical.x+source[i+scene.edges.w].x;
}
fn current_edge(axis:u32,p:vec3<i32>)->f32 {if !valid(p,edge_dims(axis)){return 0.0;}return source[edge_index(axis,p)].z*electric_edge(axis,p);}
@compute @workgroup_size(256)
fn fields(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
    let i=flat_id(gid);var a=vec4<f32>(0.0);var b=vec4<f32>(0.0);var c=vec4<f32>(0.0);var d=vec4<f32>(0.0);
    if i<scene.dims.w {
        let p=vec3<i32>(vec3<u32>(i%scene.dims.x,(i/scene.dims.x)%scene.dims.y,i/(scene.dims.x*scene.dims.y)));let physical=cell_properties(p);
        var magnetic=vec3<f32>(0.0);var applied=vec3<f32>(0.0);var j=vec3<f32>(0.0);var heat=0.0;var div_b=0.0;var div_j=0.0;var max_edge=0.0;var magnetic_square=0.0;
        for(var axis=0u;axis<3u;axis++) {
            var upper=p;upper[axis]+=1;
            let b0=curl_face(axis,p,3u)/scene.physical.z;let b1=curl_face(axis,upper,3u)/scene.physical.z;
            magnetic_square+=0.5*(b0*b0+b1*b1);
            magnetic[axis]=0.5*(b0+b1);applied[axis]=0.5*(curl_face(axis,p,2u)+curl_face(axis,upper,2u))/scene.physical.z;div_b+=b1-b0;
            let aa=(axis+1u)%3u;let bb=(axis+2u)%3u;var es=0.0;
            for(var u=0;u<2;u++){for(var v=0;v<2;v++){var q=p;q[aa]+=u;q[bb]+=v;let ee=electric_edge(axis,q);es+=0.25*ee;heat+=physical.x*0.25*ee*ee;}}
            j[axis]=physical.x*es;
            var prev=p;prev[axis]-=1;let jp=current_edge(axis,p);let jm=current_edge(axis,prev);div_j+=jp-jm;max_edge=max(max_edge,max(abs(jp),abs(jm)));
        }
        let force=cross(j,magnetic);let bn=length(magnetic);let jn=length(j);let fnorm=length(force);let volume=scene.physical.z*scene.physical.z*scene.physical.z;
        results[i]=Result(vec4<f32>(magnetic,bn),vec4<f32>(j,jn),vec4<f32>(force,heat),vec4<f32>(applied,length(applied)));
        a=vec4<f32>(bn,jn,fnorm,heat);b=vec4<f32>(force*volume,heat*volume);c=vec4<f32>(abs(div_j),max_edge,abs(div_b),magnetic_square*volume/(2.0*scene.physical.y*physical.y));d=vec4<f32>(length(applied),length(magnetic-applied),0.0,0.0);
    }
    reduction[l]=a;reduction_b[l]=b;reduction_c[l]=c;reduction_d[l]=d;workgroupBarrier();
    for(var s=128u;s>0u;s/=2u){if l<s {reduction[l]=max(reduction[l],reduction[l+s]);reduction_b[l]+=reduction_b[l+s];reduction_c[l]=vec4<f32>(max(reduction_c[l].xyz,reduction_c[l+s].xyz),reduction_c[l].w+reduction_c[l+s].w);reduction_d[l]=max(reduction_d[l],reduction_d[l+s]);}workgroupBarrier();}
    if l==0u && group_id(group)<(scene.dims.w+255u)/256u {let group=group_id(group)*4u;partials[group]=reduction[0];partials[group+1u]=reduction_b[0];partials[group+2u]=reduction_c[0];partials[group+3u]=reduction_d[0];}
}
@compute @workgroup_size(256)
fn field_reduce(@builtin(local_invocation_index) l:u32){
    var a=vec4<f32>(0.0);var b=vec4<f32>(0.0);var c=vec4<f32>(0.0);var d=vec4<f32>(0.0);let n=(scene.dims.w+255u)/256u;
    for(var i=l;i<n;i+=256u){let v=partials[i*4u+2u];a=max(a,partials[i*4u]);b+=partials[i*4u+1u];c=vec4<f32>(max(c.xyz,v.xyz),c.w+v.w);d=max(d,partials[i*4u+3u]);}
    reduction[l]=a;reduction_b[l]=b;reduction_c[l]=c;reduction_d[l]=d;workgroupBarrier();
    for(var s=128u;s>0u;s/=2u){if l<s {reduction[l]=max(reduction[l],reduction[l+s]);reduction_b[l]+=reduction_b[l+s];reduction_c[l]=vec4<f32>(max(reduction_c[l].xyz,reduction_c[l+s].xyz),reduction_c[l].w+reduction_c[l+s].w);reduction_d[l]=max(reduction_d[l],reduction_d[l+s]);}workgroupBarrier();}
    if l==0u {scalars[2]=reduction[0];scalars[3]=reduction_b[0];scalars[4]=reduction_c[0];scalars[5]=reduction_d[0];}
}
@compute @workgroup_size(256)
fn true_residual(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>) {
    let i=flat_id(gid);var v=vec4<f32>(0.0);
    if i<scene.edges.w {
        let e=edge_decode(i);
        var r=mass(i)*scene.physical.x*source[i+scene.edges.w].x-mass(i)*(work[i].x+(source[i].x-source[i].y))+adjoint(e,2u,true)-adjoint(e,4u,false)-gauge(e,4u)-adjoint(e,0u,false)-gauge(e,0u);
        if boundary(e){r=0.0;}
        work[i].y=r;work[i].z=r/diagonal(i);v=vec4<f32>(r*r/diagonal(i),r*r,0.0,0.0);
    }
    reduction[l]=v;let sum=sum_group(l);if l==0u && group_id(group)<(scene.edges.w+255u)/256u{partials[group_id(group)]=sum;}
}
@compute @workgroup_size(256)
fn restart_reduce(@builtin(local_invocation_index) l:u32){
    var v=vec4<f32>(0.0);let n=(scene.edges.w+255u)/256u;for(var i=l;i<n;i+=256u){v+=partials[i];}reduction[l]=v;let sum=sum_group(l);
    if l==0u {scalars[0].x=sum.x;scalars[0].y=sum.y;scalars[1].w=select(0.0,1.0,sum.y<=scalars[0].w);}
}
// Prescribed reaction boundary data used by the analytic TEAM verification case.
@compute @workgroup_size(256)
fn set_boundary(@builtin(global_invocation_id) gid:vec3<u32>){let i=flat_id(gid);if i<scene.edges.w && boundary(edge_decode(i)){work[i].x=previous[i];}}

@compute @workgroup_size(256)
fn begin_step(@builtin(global_invocation_id) gid:vec3<u32>){let i=flat_id(gid);if i<scene.edges.w {previous[i]=work[i].x;work[i].x=0.0;}}
@compute @workgroup_size(256)
fn commit_step(@builtin(global_invocation_id) gid:vec3<u32>){let i=flat_id(gid);if i<scene.edges.w {work[i].x+=previous[i];}}
