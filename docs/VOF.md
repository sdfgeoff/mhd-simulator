# Geometric liquid-volume transport

The interface is an occupied-volume fraction α, independent of the material's melt fraction. This implementation uses a piecewise-linear plane in mixed cells, geometric face-slab fluxes, and three split directional sweeps with the Weymouth–Yue compression correction. It replaces donor-cell volume transport; specific enthalpy still uses donor values consistent with the geometric volume flux.

## Plane integration

Inside a unit cube centred at zero, liquid occupies `n·x ≤ d`. The outward normal is the negative Youngs gradient, normalized by its L1 norm. The plane intercept is found by 24 bisection iterations to match the supplied cell fraction.

The volume is the distribution function of a sum of three independent weighted uniform coordinates. Reflecting to positive weights and then the lower half of the cube gives an inclusion–exclusion sum of truncated cubes. With sorted nonnegative weights `a ≤ b ≤ c` and lower-half coordinate t, the first difference is evaluated as `a(3t² − 3ta + a²)` rather than subtracting nearly equal cubes. Pure one-/two-dimensional cuts and the central constant-density interval have explicit branches. This avoids cancellation for nearly axis-aligned planes. Components below 10⁻⁸ of the largest normal component are dropped before integration, below the useful precision of the geometry calculation.

A face displacement is `C = |u_face| Δt/h`. The outgoing slab has width C and centre `sign(u_face)(1−C)/2`. Its occupied fraction follows by scaling that component of n by C and shifting d by the slab centre. Multiplying by signed displacement gives the liquid-volume flux. Each neighboring cell evaluates the identical face expression, so internal fluxes cancel without atomics or separate cell-based limiters.

The geometric routines are an independent WGSL implementation. GPU checks cover analytic planar cuts, complementary volumes, inverse reconstruction and exact slab partitions, including small/negative normal components. The underlying reconstruction and swept-volume formulation are described by the primary [Basilisk geometry](https://basilisk.fr/src/geometry.h) and [VOF](https://basilisk.fr/src/vof.h) references; the code here is not copied from Basilisk.

## Split conservation and heat

Each directional update adds `F_left − F_right + cc (C_right − C_left)`, with `cc = (αⁿ > 0.5)` held fixed for the entire timestep. This is the Weymouth–Yue correction: directional divergence contributions cancel when the projected velocity is divergence free. Sweep order rotates each timestep to reduce persistent directional bias. The material-volume diagnostic separately accumulates the correction caused by finite velocity divergence (`interface_divergence_volume_m3` in the CLI CSV), alongside the measured total transport drift. No mass redistribution or global normalization is applied.

For mixture enthalpy H, the same liquid-volume flux is multiplied by upwind specific enthalpy `H/α`. Its compression coefficient is frozen at `cc Hⁿ/αⁿ` through all sweeps. This preserves constant specific enthalpy under interface transport, up to f32 arithmetic, and cancels the directional source for exactly incompressible velocity. Empty cells and fixed CAD walls do not supply liquid flux. Thermal reconstruction recovers temperature and melt fraction after transport.

The conservative speed bound is `3 max|u_face| Δt/h ≤ 0.45`. The GPU independently rejects nonfinite or excessive actual face displacements. Pure cells, mixed cells without a usable normal, and fractions within 10⁻⁶ of a pure phase use donor fluxes; sub-10⁻¹² tails are flushed and remain visible in the balance error. There is no hidden clipping of a computed cell fraction into [0,1]. Bounds checks allow only f32 roundoff excursions.

## Storage and validation

Reconstruction reuses the normal buffer during advection; curvature normals are refreshed afterward. The two occupancy buffers carry the intermediate fraction and fixed compression metadata. Thermal transport alternates its existing two field buffers and reuses its existing rollback checkpoint. No additional grid-sized thermal scratch buffer is allocated. A rejected sweep restores the original occupancy and thermal input bytes, leaves accepted diagnostics intact, and does not advance sweep ordering.

1. Translating a planar slab for 40 steps preserves one mixed layer at each interface, with integrated fraction error approximately 10⁻⁷ and unchanged reduced volume.
2. A diagonally translated sphere gives relative L1 fraction errors 1.99% and 0.98% at 24³ and 48³. Relative volume drift stays below 2×10⁻⁷; the fine-grid occupancy extrema are within f32 roundoff of [0,1].
3. A closed-box circulation transports volume and uniform specific enthalpy conservatively. A deliberately invalid face speed verifies rejection and byte-for-byte rollback of both occupancy and compensated thermal state.
4. The existing pressure-balance, curvature refinement, thermal/phase, common-clock and offscreen rendering checks remain part of the regular headless suite.

```sh
cargo test surface::tests -- --nocapture
cargo test moving_liquid -- --nocapture
./run.sh --run-check assets/hotend-free-surface.mhd.json 24
```

These tests establish geometric transport behavior, not nozzle pinch-off or jetting thresholds. Initial CAD occupancy and walls are still voxel based. Momentum transport is not yet a geometrically consistent mass/momentum scheme, and contact angle, wetting and oxide films remain absent.
