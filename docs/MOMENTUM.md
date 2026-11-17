# Mass-consistent interface momentum transport

Moving-interface runs now transport MAC momentum with the mass flux implied by geometric VOF, then recover velocity from the updated face density. Fixed-region diagnostics retain the semi-Lagrangian RK2 scheme. Turning off fluid advection disables momentum transport; material occupancy still follows the prescribed/projected velocity.

## Discrete balance

Write the liquid fraction as α, ambient density as ρg and liquid density as ρl. For a directional sweep, the shared geometric helper returns liquid flux Fα and total swept volume C=u Δt/h. Both are signed fractions of a cell volume. The primal mass flux is

`Fρ = ρg C + (ρl − ρg) Fα`.

The VOF split update is `α′ = α − δFα + cc δC`, where `cc=(αⁿ>0.5)` is fixed across all three sweeps and δ denotes upper minus lower face flux. Consequently its exactly corresponding density balance is

`ρ′ = ρ − δFρ + ρc δC`, with `ρc = ρg + (ρl − ρg) cc`.

For velocity component i, its MAC dual volume consists of half of each adjacent primal cell. The existing arithmetic face density is their mean. Averaging the two adjacent primal fluxes and split source terms gives the dual mass balance for that same density. A dual boundary normal to j uses

`Fρ,dual(q) = [Fρ,j(q) + Fρ,j(q−eᵢ)]/2`.

Each shared dual flux carries an upwind face velocity component. The directional momentum update is

`ρ′dual u′ᵢ = ρdual uᵢ − δ(Fρ,dual uᵢ,upwind) + uᵢ,frozen Sc,dual`.

The advecting MAC field, compression velocity and compression flags remain fixed during all sweeps. Thus the split source sums to zero for a divergence-free full step. Neighboring dual volumes evaluate the identical boundary flux; no global momentum correction or normalization is applied. The shared `surface_geometry.wgsl` computes the same PLIC liquid flux for fraction and momentum transport.

After transport, pressure projects the new velocity using the updated density. This is an increment to the preceding force-balance pressure, so the two pressure contributions are retained together for display and subsequent warm starts. The common run records only after the complete coupled step succeeds.

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

At baseline commit 38c5d71, relative vector momentum drift is **2.391%**, while reduced volume drift is zero. Kinetic energy falls **4.293%**. With consistent transport, momentum drift is **1.77×10⁻⁶** relative, volume drift **8.11×10⁻⁸**, and kinetic energy falls **0.0705%**. The regular 16³/16-step case has momentum drift **2.52×10⁻⁷**. Both now assert momentum conservation within 10⁻⁵ relative, volume conservation within 2×10⁻⁶ and no unforced kinetic-energy growth beyond 10⁻⁵ relative. Uniform velocity at high density contrast, static-drop refinement and byte-for-byte rejection/valid retry checks also exercise the coupled path.

## Limits

Transported velocity is first-order upwind. It dissipates kinetic energy and needs resolution/timestep studies for strongly sheared jets; a higher-order bounded reconstruction is a useful next step. The two pressure projections add solve time. The transport correction starts from a zero pressure increment, avoiding an inappropriate warm start from the larger force-balance pressure; the latter is added back to the final physical pressure. On the 24-tick coupled hotend check this reduces complete step CPU time from 5.14 s to 4.18 s, with pressure time falling from 1.43 s to 0.53 s. These are single-run comparisons of the same conservative transport with different pressure initial guesses, not a speed comparison with the earlier physics. This scheme conserves the discrete momentum defined by arithmetic MAC density; it does not geometrically reconstruct an independent phase distribution inside each dual volume. [Variable-viscosity stress transfer](VISCOUS_STRESS.md) now has separate conservation/refinement checks. Sharp-interface viscous damping, CAD wetting and resolved breakup remain separate validation work. Volume conservation alone never established momentum correctness.

[Arrufat et al., *A mass-momentum consistent, Volume-of-Fluid method for incompressible flow on staggered grids*](https://arxiv.org/abs/1811.12327) describes the physical consistency requirement and staggered-grid complications. This arithmetic dual-volume construction follows this solver's existing face-density definition; it is not a claim to reproduce that paper's auxiliary geometric reconstruction.
