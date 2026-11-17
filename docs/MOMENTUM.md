# Mass-consistent interface momentum transport

Moving-interface runs now transport MAC momentum with the mass flux implied by geometric VOF, then recover velocity from the updated face density. Fixed-region diagnostics retain the semi-Lagrangian RK2 scheme. Turning off fluid advection disables momentum transport; material occupancy still follows the prescribed/projected velocity.

## Discrete balance

Write the liquid fraction as α, ambient density as ρg and liquid density as ρl. For a directional sweep, the shared geometric helper returns liquid flux Fα and total swept volume C=u Δt/h. Both are signed fractions of a cell volume. The primal mass flux is

`Fρ = ρg C + (ρl − ρg) Fα`.

The VOF split update is `α′ = α − δFα + cc δC`, where `cc=(αⁿ>0.5)` is fixed across all three sweeps and δ denotes upper minus lower face flux. Consequently its exactly corresponding density balance is

`ρ′ = ρ − δFρ + ρc δC`, with `ρc = ρg + (ρl − ρg) cc`.

For velocity component i, its MAC dual volume consists of half of each adjacent primal cell. The existing arithmetic face density is their mean. Averaging the two adjacent primal fluxes and split source terms gives the dual mass balance for that same density. A dual boundary normal to j uses

`Fρ,dual(q) = [Fρ,j(q) + Fρ,j(q−eᵢ)]/2`.

Each shared dual flux carries a reconstructed donor velocity component ûᵢ. The directional momentum update is

`ρ′dual u′ᵢ = ρdual uᵢ − δ(Fρ,dual ûᵢ) + uᵢ,frozen Sc,dual`.

The advecting MAC field, compression velocity and compression flags remain fixed during all sweeps. Thus the split source sums to zero for a divergence-free full step. Neighboring dual volumes evaluate the identical boundary flux; no global momentum correction or normalization is applied. The shared `surface_geometry.wgsl` computes the same PLIC liquid flux for fraction and momentum transport.

After transport, pressure projects the new velocity using the updated density. This is an increment to the preceding force-balance pressure, so the two pressure contributions are retained together for display and subsequent warm starts. The common run records only after the complete coupled step succeeds.

## Velocity reconstruction

Within a homogeneous phase stencil, carried velocity uses a monotonized-central (MC) slope and a MUSCL–Hancock half-step trace. Let ΔL and ΔR be the adjacent carried-velocity differences, and let s=sign(Fρ,dual). Then

`Δu = minmod[(ΔL+ΔR)/2, 2ΔL, 2ΔR]`,

`û = u_donor + (s − C_donor) Δu/2`.

C_donor is the mean volume Courant number at the donor dual-volume centre, obtained by averaging its four adjacent primal MAC Courant numbers along the sweep direction. The trace is clipped to the range of the donor and its two neighbors. A closed static/frozen neighbor disables the slope. The frozen volume advector supplies the predictor; the same signed geometric mass flux chooses the donor and multiplies the trace.

At unequal liquid/ambient densities, all six primal samples adjoining the three carried MAC faces must be within 10⁻⁶ of the same pure phase. Otherwise the trace uses first-order upwind velocity. This avoids extrapolating primitive velocity across a density jump when arithmetic MAC density provides no independent dual-phase geometry. At equal densities the phase guard is unnecessary. Shared mass fluxes and the frozen compression source retain their existing balance in both cases. No additional buffer storage is required.

The [MUSCL–Hancock notes](https://www.ammar-hakim.org/hancock-muscl.html) describe the predictor/limiter approach. [Arrufat et al.](https://arxiv.org/html/1811.12327v2) also investigate higher-order bulk interpolation with upwind velocity near interfaces. This implementation uses MC reconstruction, not their QUICK interpolation or auxiliary dual-volume geometry.

The bounds apply to reconstructed face traces. They are not a TVD or energy-stability proof for the complete variable-density, pressure-projected, directionally split method. Limiting reduces order near extrema; the coupled timestep remains first-order despite the half-step trace.

## Storage and API

`SurfaceState::advance_coupled` follows a successful fluid force/pressure step. `FluidState::update_surface` then updates density/phase and performs the required projection. A pending projection prevents another fluid or momentum step. The common workbench/CLI run uses this path automatically. `SurfaceState::advance` remains useful for passive transport with a supplied velocity buffer.

Momentum alternates the fluid velocity and predictor buffers. The now-unused fluid field checkpoint stores the frozen advector; the separate accepted-velocity checkpoint remains available for energy accounting. No new full-volume allocation is needed on normal 3D grids. For very thin/degenerate grids where the MAC field is larger than the cell packet, checkpoint capacity is the larger size; memory preflights include it. A rejected interface sweep restores occupancy, velocity and thermal input bytes without accepting a pending projection.

## Validation

```sh
cargo test --offline nonuniform_high_density_momentum -- --nocapture
cargo test --offline coupled_transport_restores -- --nocapture
cargo test --offline nonuniform_periodic_metal_drop -- --ignored --nocapture
```

The long diagnostic writes `artifacts/periodic-momentum.csv`. It initializes a spherical drop on a 24³ periodic unit box, density ratio 7000:1, zero surface tension/gravity and negligible liquid viscosity. Velocity combines uniform translation with a smooth, discretely divergence-free staggered vortex. It advances 64 steps of 0.01 s. Momentum uses each unique periodic MAC face once, with the same arithmetic face density used by the pressure operator. This avoids changing the diagnostic by interpolating velocity back to cells.

At baseline commit 38c5d71, relative vector momentum drift is **2.391%**, while reduced volume drift is zero. Kinetic energy falls **4.293%**. At the first consistent-transport implementation (830b7f6), momentum drift is **1.77×10⁻⁶** relative, volume drift **8.11×10⁻⁸**, and kinetic energy falls **0.0705%**. Its regular 16³/16-step case has momentum drift **2.52×10⁻⁷**. With the current reconstruction, the 24³ check has momentum drift **1.56×10⁻⁶**, volume drift **8.11×10⁻⁸**, and kinetic-energy loss **0.0650%**. The regular 16³ check has momentum drift **3.06×10⁻⁷**. The current tests assert momentum conservation within 10⁻⁵ relative, volume conservation within 2×10⁻⁶ and no unforced kinetic-energy growth beyond 10⁻⁵ relative. Uniform velocity at high density contrast, static-drop refinement and byte-for-byte rejection/valid retry checks also exercise the coupled path.

### Exact translating shear

A periodic inviscid field `u=(U, A sin[2π(x−Ut)], 0)` isolates advection from capillarity and pressure forcing. The diagnostic uses U=±0.2 m/s, A=0.02 m/s and a quarter-domain translation, with volume Courant number at most 0.12. Both density scales 1 and 7000 kg/m³ give the same velocity errors. Unique MAC faces provide momentum and energy measurements.

| Cells along translation | Upwind relative velocity L2 | MC–Hancock relative velocity L2 |
|---|---:|---:|
| 16 | 23.84% | 3.86% |
| 32 | 12.70% | 1.14% |
| 64 | 6.56% | 0.330% |

At 64 cells the sine amplitude ratio improves from 0.93437 to 0.99982. The observed L2 order is about 1.8; the refinement diagnostic requires at least order 1.5 for the requested grid ratios. The regular square-profile case checks velocity bounds after every step, plus final total variation and kinetic energy in both directions. A short 7000:1 density-jump shear case checks momentum, material volume and absence of unforced kinetic-energy growth; it does not validate an instability growth rate.

    cargo test --offline momentum_accuracy_tests -- --nocapture
    cargo test --offline translating_shear_momentum_refinement -- --ignored --nocapture

The ignored diagnostic writes artifacts/momentum-shear.csv. MHD_MOMENTUM_GRIDS accepts an increasing grid list (default 16,32,64).

## Limits

Velocity remains first-order upwind near density jumps. Numerical dissipation and interface geometry still require resolution/timestep studies for sheared jets; the bulk reconstruction alone does not establish accurate interfacial shear or breakup. The two pressure projections add solve time. The transport correction starts from a zero pressure increment, avoiding an inappropriate warm start from the larger force-balance pressure; the latter is added back to the final physical pressure. On the 24-tick coupled hotend check this reduces complete step CPU time from 5.14 s to 4.18 s, with pressure time falling from 1.43 s to 0.53 s. These are single-run comparisons of the same conservative transport with different pressure initial guesses, not a speed comparison with the earlier physics. This scheme conserves the discrete momentum defined by arithmetic MAC density; it does not geometrically reconstruct an independent phase distribution inside each dual volume. [Variable-viscosity stress transfer](VISCOUS_STRESS.md) now has separate conservation/refinement checks. Sharp-interface viscous damping, CAD wetting and resolved breakup remain separate validation work. Volume conservation alone never established momentum correctness.
