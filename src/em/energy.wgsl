// Exact energy of the edge/face induction operator, including source/boundary work.
fn en_a(axis:u32,p:vec3<i32>,kind:u32)->f32 {
    if !valid(p,edge_dims(axis)){return 0.0;}let i=edge_index(axis,p);
    if kind==0u{return work[i].x+source[i].x;}
    if kind==1u{return previous[i]+source[i].y;}
    if kind==3u{return (work[i].x-previous[i])+(source[i].x-source[i].y);}
    return source[i].x;
}
fn en_curl(axis:u32,p:vec3<i32>,kind:u32)->f32 {
    if !valid(p,face_dims(axis)){return 0.0;}let a=(axis+1u)%3u;let b=(axis+2u)%3u;
    var qa=p;qa[a]+=1;var qb=p;qb[b]+=1;
    return en_a(b,qa,kind)-en_a(b,p,kind)-en_a(a,qb,kind)+en_a(a,p,kind);
}
fn en_face(axis:u32,p:vec3<i32>,kind:u32,weighted:bool)->f32 {
    if !valid(p,face_dims(axis)){return 0.0;}
    return en_curl(axis,p,kind)*select(1.0,nu_face(axis,p),weighted);
}
fn en_adjoint(e:vec4<u32>,kind:u32,weighted:bool)->f32 {
    let p=vec3<i32>(e.xyz);let a=(e.w+1u)%3u;let b=(e.w+2u)%3u;
    var qa=p;qa[a]-=1;var qb=p;qb[b]-=1;
    return en_face(b,p,kind,weighted)-en_face(b,qa,kind,weighted)-en_face(a,p,kind,weighted)+en_face(a,qb,kind,weighted);
}
@compute @workgroup_size(256)
fn energy_collect(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
    let i=flat_id(gid);var a=vec4<f32>(0.0);var b=vec4<f32>(0.0);
    let h=scene.physical.z;let dt=scene.physical.x;let mu0=scene.physical.y;let volume=h*h*h;
    if i<scene.edges.w {
        let e=edge_decode(i);let delta=(work[i].x-previous[i])+(source[i].x-source[i].y);
        let motion=source[i+scene.edges.w].x;let ee=-delta/dt+motion;let j=source[i].z*ee;
        a.w=dt*volume*j*ee;b.x=-dt*volume*j*motion;
        var drive=en_adjoint(e,2u,false);
        if boundary(e){drive=en_adjoint(e,0u,true);}
        b.y=h/mu0*delta*drive;
    }
    let fx=(scene.dims.x+1u)*scene.dims.y*scene.dims.z;
    let fy=scene.dims.x*(scene.dims.y+1u)*scene.dims.z;
    let fz=scene.dims.x*scene.dims.y*(scene.dims.z+1u);
    if i<fx+fy+fz {
        var axis=0u;var q=i;if q>=fx {q-=fx;axis=1u;if q>=fy {q-=fy;axis=2u;}}
        let dims=face_dims(axis);let p=vec3<i32>(i32(q%dims.x),i32((q/dims.x)%dims.y),i32(q/(dims.x*dims.y)));
        let now=en_curl(axis,p,0u);let old=en_curl(axis,p,1u);let delta=en_curl(axis,p,3u);
        let scale=h/(2.0*mu0)*nu_face(axis,p);
        a.x=scale*now*now;a.y=scale*delta*(now+old);a.z=scale*delta*delta;
    }
    reduction[l]=a;reduction_b[l]=b;workgroupBarrier();
    for(var s=128u;s>0u;s/=2u){if l<s{reduction[l]+=reduction[l+s];reduction_b[l]+=reduction_b[l+s];}workgroupBarrier();}
    let n=(scene.edges.w+255u)/256u;if l==0u && group_id(group)<n {partials[group_id(group)]=reduction[0];partials[n+group_id(group)]=reduction_b[0];}
}
@compute @workgroup_size(256)
fn energy_reduce(@builtin(local_invocation_index) l:u32){
    let n=(scene.edges.w+255u)/256u;var a=vec4<f32>(0.0);var b=vec4<f32>(0.0);
    for(var i=l;i<n;i+=256u){a+=partials[i];b+=partials[n+i];}
    reduction[l]=a;reduction_b[l]=b;workgroupBarrier();
    for(var s=128u;s>0u;s/=2u){if l<s{reduction[l]+=reduction[l+s];reduction_b[l]+=reduction_b[l+s];}workgroupBarrier();}
    if l==0u {scalars[0]=reduction[0];scalars[1]=reduction_b[0];}
}
