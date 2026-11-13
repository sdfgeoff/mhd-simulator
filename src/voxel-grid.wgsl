// Shared by both views. Lines are evaluated at the destination framebuffer's
// fragment resolution, in world/grid coordinates, never stored in a fill image.
fn voxel_grid_coverage(world:vec3<f32>,minimum:vec3<f32>,cell_mm:f32)->f32{
    let grid=(world-minimum)/max(cell_mm,0.000001);
    let dx=dpdx(grid);let dy=dpdy(grid);
    let gradient=sqrt(dx*dx+dy*dy);
    let fraction=fract(grid);let distance=min(fraction,vec3<f32>(1.0)-fraction);
    var ink=0.0;
    for(var axis=0u;axis<3u;axis++){
        // Ignore a grid coordinate constant across the plane (including a
        // plane exactly on a voxel face), rather than colouring the whole cap.
        if gradient[axis]>0.000001{
            let pixels=distance[axis]/gradient[axis];
            let line=1.0-smoothstep(0.15,0.70,pixels);
            let fade=1.0-smoothstep(0.25,0.50,gradient[axis]);
            ink=max(ink,line*fade);
        }
    }
    return ink;
}
fn apply_voxel_grid(color:vec4<f32>,ink:f32,enabled:f32)->vec4<f32>{
    return vec4<f32>(mix(color.rgb,vec3<f32>(0.8,0.9,1.0)*color.a,ink*0.18*enabled),color.a);
}
