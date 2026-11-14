# Fluid velocity and pressure

The GPU MAC solver supports fixed-region validation, temperature-driven mobility, and an optional **moving liquid/air interface** with low-density air and surface tension. The moving model, workflow and limits are described in [SURFACES.md](SURFACES.md); the equations below describe the fixed-region baseline. [Thermal evolution](THERMAL.md) adds latent heat and melting/freezing.

## Fixed-region workflow

1. In **Setup**, import geometry, select solid objects and assign a material with positive density and liquid viscosity. The solder presets start with an illustrative **0.002 Pa·s**, not calibrated operating-temperature data. The shared material editor affects all assigned objects.
2. In **Simulation**, enable **Fluid motion**, select the material defining the fixed liquid region, choose the common timestep and click **Simulate**. Electrical sources are optional. When present, EM and fluid steps exchange updated Lorentz force and motional `u × B` on the same clock; see [coupling equations and workflow](RUNS.md).
3. Choose **Fluid velocity**, **Fluid pressure (gauge)** or **Velocity divergence** in **Section view**. Pressure/divergence use signed colours and SI units. Both viewports sample the retained 3D output directly. Arrows/trails always use velocity, including when pressure supplies the background colour.
4. The common timeline retains a configurable frame count, default ten. Scrubbing changes all active fields, diagnostics and trails together. Animation has a separate clock and does not advance the fluid. **Hide below** defaults to 10⁻⁷ m/s for vector cues; it does not change solver data. Trail speed is a direction cue, not physical particle tracking.
5. **Fluid driving and numerical settings** exposes acceleration, initial velocity, periodic axes, advection and pressure tolerances. Periodicity is for matching periodic geometry. EM still requires air padding and nonperiodic outer boundaries, so use the periodic channel cases as fluid-only diagnostics. Material/domain/source changes invalidate dependent results.

Multiple disconnected regions of the selected material are supported with independent pressure gauges. With thermal evolution enabled, the one liquid material is inferred from assigned phase/viscosity data, with phase-dependent pressure connectivity and mushy resistance. Without it, the manual fixed-region diagnostic is retained. The low-level frozen-force CLI remains available for isolated verification, independently of the normal coupled workbench run.

## Discretization

Velocity components live on their respective MAC faces; pressure potential is cell-centred. A face is open only when both adjacent cells belong to the liquid region. Closed normal faces have zero velocity. Tangential viscous ghost values use odd reflection at voxel walls to impose no-slip at the wall location. Geometry remains a staircase; fractional boundary treatment is not implemented yet.

Density ρ and dynamic viscosity μ are uniform within the selected fluid material, with ν=μ/ρ. The first-order split step is:

1. Midpoint backtrace and trilinear semi-Lagrangian interpolation advect the velocity: `uᵃ = SL_RK2(uⁿ)`.
2. Explicit viscosity and forcing give `u* = uᵃ + Δt (ν Lₕ uᵃ + a + f_EM/ρ)`.
3. Solve `A φ = −h² Dₕ u*`, where `A = −h² Dₕ Gₕ` is the positive pressure graph Laplacian.
4. Project `uⁿ⁺¹ = u* − Gₕ φ`; report gauge pressure `p = ρ φ / Δt`.

Divergence and gradient use the same face connectivity. One pressure reference per connected fluid component removes the Neumann nullspace. Connectivity/gauge selection is a one-time CPU geometry preparation after a material-ID readback; physical field updates and the pressure solve run on the GPU. The matrix-free pressure solver uses Jacobi-preconditioned conjugate gradients with GPU reductions. It applies the last accepted pressure gradient to the predictor, solves a zero-based pressure correction, then accumulates pressure. This avoids losing small corrections when the existing pressure is large. It recomputes a true residual before acceptance, including the physical divergence equation at gauge cells. Inner CG stopping uses a stricter residual bound by the fluid cell count so the omitted gauge equation can also satisfy the final norm. The full physical residual is also checked every 64 iterations and at the iteration cap, without changing the CG vectors. This prevents that conservative internal bound from forcing false failures near f32 precision on large grids. Recursive convergence still triggers a true-residual refresh/restart when needed.

Acceptance combines relative pressure-correction residual and absolute RMS divergence tolerances (`b` is the RHS after applying the warm pressure gradient):

`||b − Aφ||² ≤ tolerance_relative² ||b||² + N_fluid (h² tolerance_divergence)²`.

The absolute term prevents chasing f32 roundoff in an almost divergence-free predictor. A relative residual above the relative setting can therefore be acceptable when the absolute divergence criterion controls. Diagnostics show the resulting actual maximum/RMS divergence, not just the algebraic residual. Iteration counts are upper bounds in eight-iteration GPU batches.

The explicit viscous bound is `Δt ≤ 0.95 h²/(6ν)`. A conservative face-velocity CFL check limits backtraces. Failed pressure solves, nonfinite fields and invalid input revisions do not advance time; accepted velocity/output buffers are restored after a failed step. Display velocity is reconstructed at cell centres; energy uses averages of **squared face velocities**, rather than squaring the cell-centre average. Pressure is relative to a zero gauge per connected region, not absolute thermodynamic pressure.

Reference equations and channel-test definitions are available in the primary [Basilisk incompressible solver documentation](https://basilisk.fr/src/navier-stokes/centered.h) and [Poiseuille test](https://basilisk.fr/src/test/poiseuille.c). This workbench implements the GPU scheme above; it does not embed Basilisk or a duplicate CPU fluid solver.

## Verify and export

```sh
cargo test fluid::tests -- --nocapture
cargo test fluid_analysis_runs -- --nocapture
cargo test fluid_pressure_and_velocity_trails -- --nocapture
# Ten fluid steps with gravity:
./run.sh --fluid-check assets/hotend-cad.mhd.json 10
# Compute five EM steps, freeze their force, then run ten fluid steps:
./run.sh --fluid-check assets/hotend-cad.mhd.json 10 5
```

All checks are headless. CLI output includes speed, gauge pressure, divergence, net flux and solve timing; `artifacts/fluid-probe.csv` records the fluid cell nearest the liquid-region centroid, with its position printed in the console. New projects store the `fluid` configuration and material viscosity alongside the electromagnetic setup; computed frames remain session-only.

1. Periodic uniform velocity remains uniform; closed-box gravity produces hydrostatic pressure with negligible velocity.
2. Plane Poiseuille flow uses periodic streamwise/spanwise boundaries, no-slip transverse walls and a constant acceleration. Its exact steady profile is `u(y)=g y(H−y)/(2ν)`. At 8 and 16 cells across the channel, measured relative L2 velocity errors are **2.130% and 0.526%** respectively, with zero reported divergence. This checks grid convergence of the wall/viscous discretization.
3. Periodic sinusoidal shear matches the exact discrete viscous decay factor and its face-energy decay. Excessive velocity CFL is rejected.
4. Frozen uniform EM force gives the expected acceleration, and later changes to its source buffer leave the frozen force unchanged.
5. Pressure failure rolls back accepted buffers/time; disconnected fluid components receive pressure gauges. The application test covers EM → fluid forcing, retained-frame scrubbing, material invalidation and native viewport sampling.

The bundled hotend is a coarse example, not a validated jetting design. Its material values, geometry resolution, timestep and force history all need convergence/operating-condition checks before quantitative interpretation. The optional [moving-interface stage](SURFACES.md) now supplies conservative material transport and balanced capillarity. Oscillating drops and nozzle breakup remain future validation.

## Interpreting convergence failures

The error now reports the **true physical pressure residual**, requested relative tolerance and absolute RMS divergence floor. The acceptance criterion combines the relative and absolute terms above; the RMS floor is not an independent hard maximum on every accepted step. On a large, density-contrasting grid, pressure needs more iterations and numerical precision matters. A relative residual printed by an older build could already be below the requested tolerance while its conservative internal gauge bound had not triggered the final physical check. That acceptance-path bug is fixed without relaxing the final physical equation.

The CLI prints EM and pressure iteration counts/residuals separately. Changing pressure tolerances cannot repair an EM failure, and vice versa. Moving weakly conducting interface cells can make EM harder even when the identical stationary conductor geometry converges. Test iteration limits and common timestep independently, preserving the waveform's physical time when comparing runs.
