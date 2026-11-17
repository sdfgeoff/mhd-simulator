// Positive periodic indexing without signed remainder on a negative operand.
// WGSL specifies signed remainder, but that lowering has not been portable
// across the Vulkan toolchains used here. Keep all division/remainder unsigned,
// including the magnitude of i32::MIN, and handle exact negative periods.
fn wrap_periodic(value:i32,size:u32)->i32 {
    if value>=0 {return i32(u32(value)%size);}
    let magnitude=u32(-(value+1))+1u;
    let remainder=magnitude%size;
    return i32(select(size-remainder,0u,remainder==0u));
}
