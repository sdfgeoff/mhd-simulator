# Viscous momentum transfer between liquid and ambient

Moving-interface runs evaluate the Newtonian stress tensor

τ = μ [∇u + (∇u)ᵀ], with acceleration (∇·τ)/ρ.

This replaces the previous liquid-fraction-weighted local Laplacian. With varying density or viscosity, that approximation neither represents the complete stress divergence nor conserves viscous momentum. Fixed-region constant-property diagnostics retain their usual ν∇²u form; incompressibility makes it equivalent to the constant-μ stress divergence.

## Materials and discretization

Liquid and ambient density/viscosity come from their material entities. Cell viscosity is the arithmetic phase mixture of liquid and ambient values. Only its constitutive fraction is clamped to [0,1] to avoid negative coefficients from tiny transport roundoff; material volume is never renormalized. Normal stresses live at cell centres and shear stresses at MAC edges. Edge viscosity averages surrounding active fluid cells. Both adjoining momentum volumes evaluate the identical stress, so internal forces cancel.

Closed normal faces have zero velocity. Tangential ghosts reflect oddly about the wall to impose no-slip halfway between a fluid sample and its ghost. Fixed CAD and fully frozen cells are excluded from fluid coefficient averages. This remains a staircase wall model, without subcell geometry, slip or wetting physics.

The Air / void preset has **1.8×10⁻⁵ Pa·s**, representative near standard/room temperature; [NASA's standard-air properties](https://www1.grc.nasa.gov/beginners-guide-to-aeronautics/air-properties-definitions/) provide the reference scale. Properties remain constant rather than following temperature curves. An older saved project with ambient viscosity unspecified retains the **inviscid-ambient approximation**, shown in its material editor. Enter a known value to enable viscous ambient flow. Explicit zero viscosity is also supported for inviscid diagnostic cases; unknown liquid viscosity still requires a value.

The tensor equation is also documented in the primary [Basilisk viscosity solver](https://basilisk.fr/src/viscosity.h). The MAC stress/substep implementation here is independent Rust/WGSL code.

## Stability and common time

Density, occupancy and phase mobility are held fixed during the viscosity stage. A conservative material-based bound first checks whether one viscosity step is guaranteed safe. Otherwise the GPU measures the actual maximum row-sum bound R on ρ⁻¹∇·τ. For component i at a face,

Rᵢ = 4 [μnormal,left + μnormal,right + Σⱼ≠ᵢ (μedge,lower + μedge,upper)] / (ρface h²).

For uniform properties this is 24ν/h². Symmetry and dissipation give real nonpositive eigenvalues; forward Euler's interval is Δt R ≤ 2. We use **Δt_sub R ≤ 1.8**. The common step takes ceil(Δt R/1.8) viscosity substeps, then applies forcing/drag once and projects pressure. Interface, thermal, EM and output clocks keep the original common Δt.

**Max viscosity substeps**, default 4096, bounds work per common tick. Exceeding it rejects the step and restores accepted velocity/output rather than using an unstable timestep. Reduce the common Δt or raise a lower configured cap. The accepted frame and headless log report the substep count.

The one-step proof uses Rglobal = 24 μmax/(ρfloor h²), rounded upward for f32 arithmetic. The density floor includes the full accepted fraction roundoff interval, rather than assuming fractions are exactly in [0,1]. Interface and fluid ingestion share that tolerance; the existing GPU phase pass rejects nonfinite or out-of-range fractions before using the proof. Density itself is not clipped, preserving mass/momentum consistency. If the density floor cannot be proved positive, or the global bound is too large, the measured GPU path remains in use.

Substeps reuse the velocity/predictor buffers. Only the small reduction allocation grows to cover MAC faces; no new full-volume working buffer is added. Guaranteed one-step cases skip the GPU bound pass and scalar readback; other cases retain them. Very stiff cases can still be expensive; implicit viscosity would be a separate solver improvement.

## Validation

1. A smooth variable-viscosity manufactured solution with nonzero normal and shear stresses gives relative operator L2 error **4.44% → 1.13%** on 16²×4 / 32²×4 grids.
2. Variable-μ wall tests cover the box boundary, an internal voxel block and a step. Discrete symmetry defects are below **6×10⁻⁷**; viscous power and kinetic-energy change are negative.
3. A 7000:1 periodic drop/vortex operator test freezes occupancy and disables advection to isolate viscosity/pressure. On 24³ cells with synthetic liquid μ=50 Pa·s and inviscid ambient, 64 common steps use 42 viscosity substeps each. Relative momentum drift falls **1.45% → 4.82×10⁻⁷**. Energy decreases throughout; this is an operator test, not a freely moving drop-damping calibration. A shorter regular test also includes nonzero ambient viscosity.
4. An all-ambient periodic shear reads density/viscosity from the ambient material and matches its discrete sinusoidal decay within **1.4×10⁻⁷** relative L2, using four substeps per common tick.
5. A deliberately insufficient substep cap restores velocity/output bytes and time exactly; raising the cap permits a valid retry.

## Run checks

    cargo test --offline viscosity_tests -- --nocapture
    cargo test --offline viscous_high_density_stress -- --nocapture
    cargo test --offline variable_density_viscous_momentum_balance -- --ignored --nocapture

The long operator test writes artifacts/viscous-momentum.csv. Its deliberately large dimensional viscosity exposes conservation errors; it is not a solder property recommendation.

## Coupled checks and cost

The coupled hotend completes 24 ticks to 60 µs. The saved V6 project completes two 10 µs ticks; both examples need one viscosity substep per tick. V6 working storage remains **1,715.4 MiB** at the reported precision. In single-run measurements, the 24-tick hotend CPU step time increases from 4.18 s to 4.51 s, and the high-density two-grid wave diagnostic from 124.4 s to 136.1 s. The stress stencil and stability-bound synchronization added work; these measurements precede the one-step synchronization optimization. Three alternating comparisons of the optimized path preserve diagnostic CSV bytes exactly and reduce a fluid-only case from 6.41 s to 6.00 s median; the full hotend changes from 4.44 s to 4.36 s, within noticeable timing variation. See [performance details](PERFORMANCE.md).

## Limits

Arithmetic mixture/edge coefficients smear viscosity across the resolved interface. Sharp-interface viscous damping and breakup need resolution studies; these checks do not establish nozzle pinch-off or satellite sizes. Momentum reconstruction remains first-order upwind, and the coupled stages are temporally split. Viscous loss is included in the inferred fluid energy balance but is not deposited as thermal heat. Temperature-dependent rheology, oxide skins and non-Newtonian effects remain absent.
