struct Params {dims:vec4<u32>,faces:vec4<u32>,flags:vec4<u32>,physical:vec4<f32>,acceleration:vec4<f32>,initial:vec4<f32>,solve:vec4<f32>,surface:vec4<f32>}
struct Result {velocity:vec4<f32>,pressure:vec4<f32>,force:vec4<f32>,reserved:vec4<f32>}
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var<storage,read> mask:array<u32>;
@group(0) @binding(2) var<storage,read_write> velocity:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read_write> predicted:array<vec4<f32>>;
// x=pressure potential, y=residual, z=search, w=operator/search product
@group(0) @binding(4) var<storage,read_write> work:array<vec4<f32>>;
@group(0) @binding(5) var<storage,read_write> partials:array<vec4<f32>>;
@group(0) @binding(6) var<storage,read_write> scalars:array<vec4<f32>>;
@group(0) @binding(7) var<storage,read_write> output_fields:array<Result>;
@group(0) @binding(8) var<storage,read> em:array<Result>;
var<workgroup> reduce:array<vec4<f32>,256>;
var<workgroup> reduce_b:array<vec4<f32>,256>;
var<workgroup> reduce_c:array<vec4<f32>,256>;
fn index(p0:vec3<i32>,dims:vec3<u32>)->u32 {let q=vec3<u32>(p0);return q.x+dims.x*(q.y+dims.y*q.z);}
fn decode(i:u32,dims:vec3<u32>)->vec3<i32> {return vec3<i32>(vec3<u32>(i%dims.x,(i/dims.x)%dims.y,i/(dims.x*dims.y)));}
fn flat_id(id:vec3<u32>)->u32 {return id.x+id.y*65535u*256u;}
fn group_id(id:vec3<u32>)->u32 {return id.x+id.y*65535u;}
fn count_groups()->u32 {return (p.dims.w+255u)/256u;}
fn periodic(axis:u32)->bool {return (p.flags.x&(1u<<axis))!=0u;}
fn wrap_cell(pos:vec3<i32>)->vec3<i32> {
    var q=pos;
    for(var axis=0u;axis<3u;axis++) {if periodic(axis) {let n=i32(p.dims[axis]);q[axis]=((q[axis]%n)+n)%n;}}
    return q;
}
fn valid_cell(pos:vec3<i32>)->bool {return all(pos>=vec3<i32>(0))&&all(pos<vec3<i32>(p.dims.xyz));}
fn fluid(pos:vec3<i32>)->bool {let q=wrap_cell(pos);if !valid_cell(q){return false;}return mask[index(q,p.dims.xyz)]!=0u;}
fn canonical_face(pos:vec3<i32>)->vec3<i32> {return wrap_cell(pos);}
fn valid_face(pos:vec3<i32>,axis:u32)->bool {var dims=p.dims.xyz;dims[axis]+=1u;return all(pos>=vec3<i32>(0))&&all(pos<vec3<i32>(dims));}
fn open_face(pos:vec3<i32>,axis:u32)->bool {let q=canonical_face(pos);if !valid_face(q,axis) {return false;}var left=q;left[axis]-=1;return fluid(left)&&fluid(q);}
fn face(pos:vec3<i32>,axis:u32,star:bool)->f32 {
    let q=canonical_face(pos);if !open_face(q,axis) {return 0.0;}
    let i=index(q,p.faces.xyz);if star {return predicted[i][axis];}return velocity[i][axis];
}
fn potential(pos:vec3<i32>,kind:u32)->f32 {
    let q=wrap_cell(pos);if !valid_cell(q)||!fluid(q){return 0.0;}let v=work[index(q,p.dims.xyz)];if kind==0u{return v.x;}return v.z;
}
fn occupancy(pos:vec3<i32>)->f32 {let q=wrap_cell(pos);if !valid_cell(q){return 0.0;}return output_fields[index(q,p.dims.xyz)].reserved.z;}
fn density(pos:vec3<i32>)->f32 {return mix(p.surface.y,p.physical.w,occupancy(pos));}
fn face_density(pos:vec3<i32>,axis:u32)->f32 {var left=pos;left[axis]-=1;return 0.5*(density(left)+density(pos));}
fn mobility(pos:vec3<i32>,axis:u32)->f32 {
    var beta=1.0;var left=pos;left[axis]-=1;
    if p.surface.x>0.5 {beta=p.physical.w/face_density(pos,axis);}
    if p.flags.w==0u{return beta;}
    if !fluid(pos)||!fluid(left){return 0.0;}
    let f=min(output_fields[index(wrap_cell(pos),p.dims.xyz)].reserved.y,output_fields[index(wrap_cell(left),p.dims.xyz)].reserved.y);
    let drag=p.solve.w*(1.0-f)*(1.0-f)/(f*f*f+1e-3);
    return beta/(1.0+p.physical.y*drag);
}
fn capillary_force(pos:vec3<i32>,axis:u32)->f32 {
 if p.surface.x<0.5 || !open_face(pos,axis){return 0.0;}var left=pos;left[axis]-=1;
 let al=occupancy(left);let ar=occupancy(pos);let kl=output_fields[index(wrap_cell(left),p.dims.xyz)].reserved.w;let kr=output_fields[index(wrap_cell(pos),p.dims.xyz)].reserved.w;
 let wl=al*(1.0-al);let wr=ar*(1.0-ar);let k=select(0.5*(kl+kr),(wl*kl+wr*kr)/max(wl+wr,1e-20),wl+wr>1e-6);
 return p.surface.z*k*(ar-al)/p.physical.x;
}
fn diagonal(pos:vec3<i32>)->f32 {
    var d=0.0;
    for(var axis=0u;axis<3u;axis++) {for(var sign=-1;sign<=1;sign+=2) {var q=pos;q[axis]+=sign;if fluid(q) {var fp=pos;if sign>0 {fp[axis]+=1;}d+=mobility(fp,axis);}}}
    if (mask[index(pos,p.dims.xyz)]&3u)==2u{return 1.0;}return max(d,1e-30);
}
fn laplacian(pos:vec3<i32>,kind:u32)->f32 {
    let center=potential(pos,kind);var out=0.0;
    for(var axis=0u;axis<3u;axis++) {for(var sign=-1;sign<=1;sign+=2) {var q=pos;q[axis]+=sign;if fluid(q) {var fp=pos;if sign>0 {fp[axis]+=1;}out+=mobility(fp,axis)*(center-potential(q,kind));}}}
    return out;
}
fn apply(pos:vec3<i32>,kind:u32)->f32 {
    if (mask[index(pos,p.dims.xyz)]&3u)==2u {return potential(pos,kind);}
    return laplacian(pos,kind);
}
fn divergence(pos:vec3<i32>,star:bool)->f32 {
    var sum=0.0;for(var axis=0u;axis<3u;axis++) {var q=pos;q[axis]+=1;sum+=face(q,axis,star)-face(pos,axis,star);}return sum/p.physical.x;
}
fn rhs(pos:vec3<i32>)->f32 {if (mask[index(pos,p.dims.xyz)]&3u)==2u{return 0.0;}return -p.physical.x*p.physical.x*divergence(pos,true);}
fn em_force(pos:vec3<i32>)->vec3<f32> {let q=wrap_cell(pos);if p.flags.z==0u || !valid_cell(q) {return vec3<f32>(0.0);}return em[index(q,p.dims.xyz)].force.xyz;}
fn sample_component(position:vec3<f32>,axis:u32)->f32 {
    var offset=vec3<f32>(0.5);offset[axis]=0.0;let q=position-offset;
    let base=vec3<i32>(floor(q));let fraction=fract(q);var value=0.0;
    for(var z=0;z<2;z++) {for(var y=0;y<2;y++) {for(var x=0;x<2;x++) {
        let o=vec3<i32>(x,y,z);let w=select(vec3<f32>(1.0)-fraction,fraction,o>vec3<i32>(0));value+=face(base+o,axis,false)*w.x*w.y*w.z;
    }}}
    return value;
}
fn sample_velocity(position:vec3<f32>)->vec3<f32> {return vec3<f32>(sample_component(position,0u),sample_component(position,1u),sample_component(position,2u));}
fn sum_group(local:u32)->vec4<f32> {
    workgroupBarrier();for(var stride=128u;stride>0u;stride/=2u) {if local<stride {reduce[local]+=reduce[local+stride];}workgroupBarrier();}return reduce[0];
}
@compute @workgroup_size(256)
fn initialize(@builtin(global_invocation_id) gid:vec3<u32>) {
    let i=flat_id(gid);if i>=p.faces.w {return;}let pos=decode(i,p.faces.xyz);var out=vec4<f32>(0.0);
    for(var axis=0u;axis<3u;axis++){if open_face(pos,axis){out[axis]=p.initial[axis];}}
    velocity[i]=out;
}
@compute @workgroup_size(256)
fn predict(@builtin(global_invocation_id) gid:vec3<u32>) {
    let i=flat_id(gid);if i>=p.faces.w {return;}let pos=decode(i,p.faces.xyz);var out=vec4<f32>(0.0);
    for(var axis=0u;axis<3u;axis++) {
        if !open_face(pos,axis){continue;}let center=face(pos,axis,false);
        var advected=center;
        if p.flags.y!=0u {
            var at=vec3<f32>(pos)+vec3<f32>(0.5);at[axis]-=0.5;
            let v=sample_velocity(at);let mid=at-v*(0.5*p.physical.y/p.physical.x);
            advected=sample_component(at-sample_velocity(mid)*(p.physical.y/p.physical.x),axis);
        }
        out[axis]=advected;
    }
    predicted[i]=out;
}
// Operator split: diffuse the advected velocity, then add forcing.
@compute @workgroup_size(256)
fn viscous(@builtin(global_invocation_id) gid:vec3<u32>) {
    let i=flat_id(gid);if i>=p.faces.w {return;}let pos=decode(i,p.faces.xyz);var out=vec4<f32>(0.0);
    for(var axis=0u;axis<3u;axis++) {
        if !open_face(pos,axis) {continue;}let center=face(pos,axis,true);var laplace=0.0;
        for(var d=0u;d<3u;d++) {for(var sign=-1;sign<=1;sign+=2) {
            var q=pos;q[d]+=sign;var neighbor=face(q,axis,true);
            if !open_face(q,axis)&&d!=axis {neighbor=-center;}laplace+=neighbor-center;
        }}
        var left=pos;left[axis]-=1;
        var rho=p.physical.w;var capillary=0.0;var viscosity=p.physical.z;
        if p.surface.x>0.5 {
            rho=face_density(pos,axis);let al=occupancy(left);let ar=occupancy(pos);
            capillary=capillary_force(pos,axis)/rho;
            viscosity*=0.5*(al+ar);
        }
        let acceleration=p.acceleration[axis]+(em_force(left)[axis]+em_force(pos)[axis])/(2.0*rho)+capillary;
        var beta=mobility(pos,axis);if p.surface.x>0.5{beta*=rho/p.physical.w;}
        out[axis]=(center+viscosity*p.physical.y/(p.physical.x*p.physical.x)*laplace+p.physical.y*acceleration)*beta;
    }
    velocity[i]=out;
}
fn old_potential(pos:vec3<i32>)->f32 {
    let q=wrap_cell(pos);if !fluid(q){return 0.0;}
    let i=index(q,p.dims.xyz);let anchor=mask[i]>>2u;return (output_fields[i].pressure.x-output_fields[anchor].pressure.x)*p.physical.y/p.physical.w;
}
// Apply the previous pressure before solving a small, zero-based correction.
// Each invocation reads/writes only its own face record (including periodic duplicates).
@compute @workgroup_size(256)
fn pressure_warm(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat_id(gid);if i>=p.faces.w{return;}let pos=decode(i,p.faces.xyz);var out=predicted[i];
    for(var axis=0u;axis<3u;axis++){if open_face(pos,axis){var left=pos;left[axis]-=1;
        out[axis]-=mobility(pos,axis)*(old_potential(pos)-old_potential(left))/p.physical.x;
    }}predicted[i]=out;
}
@compute @workgroup_size(256)
fn pressure_guess(@builtin(global_invocation_id) gid:vec3<u32>) {
    let i=flat_id(gid);if i<p.dims.w {work[i]=vec4<f32>(0.0);}
}
@compute @workgroup_size(256)
fn pressure_commit(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat_id(gid);if i<p.dims.w && mask[i]!=0u {work[i].x+=old_potential(decode(i,p.dims.xyz));}
}
@compute @workgroup_size(256)
fn pressure_refresh(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>) {
    if group_id(group)>=count_groups() {return;}
    let i=flat_id(gid);var sum=vec4<f32>(0.0);
    if i<p.dims.w && mask[i]!=0u {
        let pos=decode(i,p.dims.xyz);let b=rhs(pos);let r=b-apply(pos,0u);let d=diagonal(pos);
        work[i].y=r;work[i].z=r/d;
        let physical=-p.physical.x*p.physical.x*divergence(pos,true);
        let true_residual=physical-laplacian(pos,0u);
        sum=vec4<f32>(r*r/d,true_residual*true_residual,physical*physical,0.0);
    }
    reduce[l]=sum;let total=sum_group(l);if l==0u {partials[group_id(group)]=total;}
}
@compute @workgroup_size(256)
fn initial_reduce(@builtin(local_invocation_index) l:u32) {
    var sum=vec4<f32>(0.0);for(var i=l;i<count_groups();i+=256u) {sum+=partials[i];}
    reduce[l]=sum;let total=sum_group(l);
    if l==0u {scalars[0]=total;scalars[1]=vec4<f32>(0.0,0.0,select(0.0,1.0,total.y<=p.solve.x*total.z+p.solve.y||total.z==0.0),0.0);}
}
@compute @workgroup_size(256)
fn pressure_apply(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>) {
    if group_id(group)>=count_groups() {return;}
    let i=flat_id(gid);var value=0.0;
    if i<p.dims.w && mask[i]!=0u && scalars[1].z<0.5 {let a=apply(decode(i,p.dims.xyz),1u);work[i].w=a;value=work[i].z*a;}
    reduce[l]=vec4<f32>(value,0.0,0.0,0.0);let total=sum_group(l);if l==0u {partials[group_id(group)]=total;}
}
@compute @workgroup_size(256)
fn alpha_reduce(@builtin(local_invocation_index) l:u32) {
    var sum=vec4<f32>(0.0);for(var i=l;i<count_groups();i+=256u) {sum+=partials[i];}reduce[l]=sum;let total=sum_group(l);
    if l==0u {var alpha=0.0;if scalars[1].z<0.5 {alpha=scalars[0].x/max(total.x,1e-35);}scalars[1].x=alpha;}
}
@compute @workgroup_size(256)
fn update(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>) {
    if group_id(group)>=count_groups() {return;}
    let i=flat_id(gid);var sum=vec4<f32>(0.0);
    if i<p.dims.w && mask[i]!=0u && scalars[1].z<0.5 {
        let a=scalars[1].x;let v=work[i];let r=v.y-a*v.w;work[i]=vec4<f32>(v.x+a*v.z,r,v.z,v.w);
        let diagonal=diagonal(decode(i,p.dims.xyz));sum=vec4<f32>(r*r/diagonal,r*r,0.0,0.0);
    }
    reduce[l]=sum;let total=sum_group(l);if l==0u {partials[group_id(group)]=total;}
}
@compute @workgroup_size(256)
fn beta_reduce(@builtin(local_invocation_index) l:u32) {
    var sum=vec4<f32>(0.0);for(var i=l;i<count_groups();i+=256u) {sum+=partials[i];}reduce[l]=sum;let total=sum_group(l);
    if l==0u && scalars[1].z<0.5 {
        scalars[1].y=total.x/max(scalars[0].x,1e-35);scalars[0].x=total.x;scalars[0].y=total.y;
        scalars[1].z=select(0.0,1.0,total.y<=(p.solve.x*scalars[0].z+p.solve.y)/p.solve.z);
    }
}
@compute @workgroup_size(256)
fn search(@builtin(global_invocation_id) gid:vec3<u32>) {
    let i=flat_id(gid);if i>=p.dims.w || mask[i]==0u || scalars[1].z>0.5 {return;}
    let v=work[i];work[i].z=v.y/diagonal(decode(i,p.dims.xyz))+scalars[1].y*v.z;
}
@compute @workgroup_size(256)
fn correct(@builtin(global_invocation_id) gid:vec3<u32>) {
    let i=flat_id(gid);if i>=p.faces.w {return;}let pos=decode(i,p.faces.xyz);var out=vec4<f32>(0.0);
    for(var axis=0u;axis<3u;axis++) {if open_face(pos,axis) {var left=pos;left[axis]-=1;out[axis]=face(pos,axis,true)-mobility(pos,axis)*(potential(pos,0u)-potential(left,0u))/p.physical.x;}}
    velocity[i]=out;
}
@compute @workgroup_size(256)
fn phase_faces(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=flat_id(gid);if i>=p.faces.w{return;}let pos=decode(i,p.faces.xyz);var v=velocity[i];
    for(var axis=0u;axis<3u;axis++){if !open_face(pos,axis){v[axis]=0.0;}}velocity[i]=v;
}
@compute @workgroup_size(256)
fn fields(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>) {
    if group_id(group)>=count_groups() {return;}
    let i=flat_id(gid);var stats=vec4<f32>(0.0);var other=vec4<f32>(0.0);var max_face=0.0;var melt=0.0;
    if i<p.dims.w {
        var out=Result(vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0));
        if p.surface.x>0.5 {out.reserved=vec4<f32>(0.0,output_fields[i].reserved.yzw);}
        if mask[i]!=0u {
            let pos=decode(i,p.dims.xyz);var v=vec3<f32>(0.0);var face_energy=0.0;
            for(var axis=0u;axis<3u;axis++) {var q=pos;q[axis]+=1;let a=face(pos,axis,false);let b=face(q,axis,false);v[axis]=0.5*(a+b);face_energy+=0.5*(a*a+b*b);max_face=max(max_face,max(abs(face(pos,axis,false)),abs(face(q,axis,false))));}
            let old_reserved=output_fields[i].reserved;
            melt=select(1.0,old_reserved.y,p.flags.w!=0u);
            if p.surface.x>0.5{melt*=old_reserved.z;}
            let speed=length(v);let div=divergence(pos,false);let pressure=work[i].x*p.physical.w/p.physical.y;let force=em_force(pos);
            out=Result(vec4<f32>(v,speed),vec4<f32>(pressure,div,0.0,0.0),vec4<f32>(p.physical.w*p.acceleration.xyz+force,0.0),vec4<f32>(1.0,select(1.0,old_reserved.y,p.flags.w!=0u),old_reserved.zw));
            let bad=any(v!=v)||any(force!=force)||any(abs(force)>vec3<f32>(1e30))||speed>1e25||!(abs(pressure)<1e30)||!(abs(div)<1e30);
            stats=vec4<f32>(speed,abs(div),div*div,0.5*select(p.physical.w,density(pos),p.surface.x>0.5)*face_energy*p.physical.x*p.physical.x*p.physical.x);
            other=vec4<f32>(abs(pressure),div,length(force),select(0.0,1.0,bad));
        }
        output_fields[i]=out;
    }
    reduce[l]=stats;reduce_b[l]=other;reduce_c[l]=vec4<f32>(max_face,melt,0.0,0.0);workgroupBarrier();
    for(var s=128u;s>0u;s/=2u) {
        if l<s {reduce[l]=vec4<f32>(max(reduce[l].xy,reduce[l+s].xy),reduce[l].zw+reduce[l+s].zw);reduce_b[l]=vec4<f32>(max(reduce_b[l].x,reduce_b[l+s].x),reduce_b[l].y+reduce_b[l+s].y,max(reduce_b[l].z,reduce_b[l+s].z),reduce_b[l].w+reduce_b[l+s].w);reduce_c[l]=vec4<f32>(max(reduce_c[l].x,reduce_c[l+s].x),reduce_c[l].y+reduce_c[l+s].y,0.0,0.0);}workgroupBarrier();
    }
    if l==0u {partials[group_id(group)]=reduce[0];partials[count_groups()+group_id(group)]=reduce_b[0];partials[2u*count_groups()+group_id(group)]=reduce_c[0];}
}
@compute @workgroup_size(256)
fn diagnostics(@builtin(local_invocation_index) l:u32) {
    var a=vec4<f32>(0.0);var b=vec4<f32>(0.0);var c=vec4<f32>(0.0);
    for(var i=l;i<count_groups();i+=256u) {let face_stats=partials[2u*count_groups()+i];c=vec4<f32>(max(c.x,face_stats.x),c.y+face_stats.y,0.0,0.0);let x=partials[i];let y=partials[count_groups()+i];a=vec4<f32>(max(a.xy,x.xy),a.zw+x.zw);b=vec4<f32>(max(b.x,y.x),b.y+y.y,max(b.z,y.z),b.w+y.w);}
    reduce[l]=a;reduce_b[l]=b;reduce_c[l]=c;workgroupBarrier();
    for(var s=128u;s>0u;s/=2u) {if l<s {reduce[l]=vec4<f32>(max(reduce[l].xy,reduce[l+s].xy),reduce[l].zw+reduce[l+s].zw);reduce_b[l]=vec4<f32>(max(reduce_b[l].x,reduce_b[l+s].x),reduce_b[l].y+reduce_b[l+s].y,max(reduce_b[l].z,reduce_b[l+s].z),reduce_b[l].w+reduce_b[l+s].w);reduce_c[l]=vec4<f32>(max(reduce_c[l].x,reduce_c[l+s].x),reduce_c[l].y+reduce_c[l+s].y,0.0,0.0);}workgroupBarrier();}
    if l==0u {scalars[2]=reduce[0];scalars[3]=reduce_b[0];scalars[4]=reduce_c[0];}
}
// In the energy bind group binding 3 contains the last accepted face velocity.
@compute @workgroup_size(256)
fn energy_collect(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
    if group_id(group)>=count_groups(){return;}let i=flat_id(gid);var sum=vec4<f32>(0.0);var capillary_work=0.0;
    if i<p.dims.w {
        let pos=decode(i,p.dims.xyz);let volume=p.physical.x*p.physical.x*p.physical.x;
        for(var axis=0u;axis<3u;axis++){
            let rho=select(p.physical.w,face_density(pos,axis),p.surface.x>0.5);
            let now=face(pos,axis,false);let old=predicted[index(pos,p.faces.xyz)][axis];let mean=0.5*(now+old);
            var left=pos;left[axis]-=1;let f=0.5*(em_force(pos)[axis]+em_force(left)[axis]);
            capillary_work+=p.physical.y*capillary_force(pos,axis)*mean*volume;
            sum+=vec4<f32>(0.5*rho*now*now,0.5*rho*(now-old)*(now+old),p.physical.y*f*mean,p.physical.y*rho*p.acceleration[axis]*mean)*volume;
        }
    }
    reduce_b[l]=vec4<f32>(capillary_work,0.0,0.0,0.0);reduce[l]=sum;let total=sum_group(l);
    for(var s=128u;s>0u;s/=2u){if l<s{reduce_b[l]+=reduce_b[l+s];}workgroupBarrier();}
    if l==0u {partials[group_id(group)]=total;partials[count_groups()+group_id(group)]=reduce_b[0];}
}
@compute @workgroup_size(256)
fn energy_reduce(@builtin(local_invocation_index) l:u32){
 var sum=vec4<f32>(0.0);var cap=vec4<f32>(0.0);for(var i=l;i<count_groups();i+=256u){sum+=partials[i];cap+=partials[count_groups()+i];}
 reduce_b[l]=cap;reduce[l]=sum;let total=sum_group(l);for(var s=128u;s>0u;s/=2u){if l<s{reduce_b[l]+=reduce_b[l+s];}workgroupBarrier();}
 if l==0u{scalars[0]=total;scalars[1]=reduce_b[0];}
}

// Independent residual check, including gauge cells, with no changes to PCG vectors.
@compute @workgroup_size(256)
fn pressure_check(@builtin(global_invocation_id) gid:vec3<u32>,@builtin(local_invocation_index) l:u32,@builtin(workgroup_id) group:vec3<u32>){
 if group_id(group)>=count_groups(){return;}let i=flat_id(gid);var sum=vec4<f32>(0.0);
 if i<p.dims.w && mask[i]!=0u {let pos=decode(i,p.dims.xyz);let b=-p.physical.x*p.physical.x*divergence(pos,true);let r=b-laplacian(pos,0u);sum.y=r*r;}
 reduce[l]=sum;let total=sum_group(l);if l==0u{partials[group_id(group)]=total;}
}
@compute @workgroup_size(256)
fn check_reduce(@builtin(local_invocation_index) l:u32){
 var sum=vec4<f32>(0.0);for(var i=l;i<count_groups();i+=256u){sum+=partials[i];}reduce[l]=sum;let total=sum_group(l);
 if l==0u {scalars[0].y=total.y;scalars[1].z=select(0.0,1.0,total.y<=p.solve.x*scalars[0].z+p.solve.y);}
}
