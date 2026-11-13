# Fixed-geometry fluid velocity and pressure

This stage computes 3D incompressible velocity and gauge pressure inside a **fixed voxel liquid region**. It supports no-slip walls, viscosity, velocity advection, gravity/acceleration, periodic box boundaries, and a frozen electromagnetic force snapshot. It does not yet move the liquid–air interface, drain the pool, create droplets, model feedstock motion or evolve temperature. In particular, the pool's initial air-facing surface is treated as a wall in this validation stage; it is not the final open-pool boundary condition.

## Use the workbench

1. Load an assembly/project and assign the liquid bodies a material. In **Material library**, set positive density and **Liquid viscosity** (Pa·s). The solder presets start with an illustrative **0.002 Pa·s**, not calibrated operating-temperature data. Use **Fill missing preset properties** if an existing project has no viscosity value. Check density and the other material defaults for the intended temperature.
2. Under **03 / Simulate**, select **Fluids**. Choose the liquid material, timestep and step count, then **Simulate**. Cells occupied by that material form the fluid region; every other cell is outside it. Multiple disconnected regions of the selected material are supported, with independent pressure gauges.
3. Choose **Fluid velocity**, **Fluid pressure (gauge)** or **Velocity divergence** in the main **Section view** selector. Pressure/divergence use signed colours and SI units. Both viewports sample the retained 3D output directly. Arrows/trails always use velocity, including when pressure supplies the background colour. Dark trails remain visible over a near-zero/white pressure background.
4. The fluid timeline retains a configurable frame count, default ten. Scrubbing changes fields, diagnostics and trails. Animation has a separate clock and does not advance the fluid. **Hide below** defaults to 10⁻⁷ m/s for fluid vector cues to suppress numerical rest-state noise; it does not change solver data. Trail speed is a visual direction cue, not physical particle tracking.
5. For one-way EM forcing, first simulate electromagnetics and select a recorded EM frame. Switch to **Fluids**, enable **Apply frozen EM force frame**, and simulate. The solver copies that frame's Lorentz force density on the GPU and divides by fluid density to obtain acceleration. Later EM updates cannot mutate the frozen copy. Selecting another EM force frame or changing fluid settings restarts fluid history; repeating Simulate with unchanged inputs continues the latest fluid state.
6. **Fluid driving and numerical settings** exposes acceleration, initial velocity, periodic axes, advection and pressure tolerances. Periodicity connects opposite domain faces and is intended for matching periodic geometry. Default CAD boundaries are closed/no-slip. Material or domain changes invalidate dependent results. Source edits invalidate fluid results that depend on EM forcing.

The frozen-force experiment holds the chosen Lorentz force constant throughout the fluid run. It does not replay the current waveform, update conductivity as liquid moves, or include motional induction. Its recorded EM time and the elapsed fluid time are separate quantities.

## Discretization

Velocity components live on their respective MAC faces; pressure potential is cell-centred. A face is open only when both adjacent cells belong to the liquid region. Closed normal faces have zero velocity. Tangential viscous ghost values use odd reflection at voxel walls to impose no-slip at the wall location. Geometry remains a staircase; fractional boundary treatment is not implemented yet.

Density ρ and dynamic viscosity μ are uniform within the selected fluid material, with ν=μ/ρ. The first-order split step is:

1. Midpoint backtrace and trilinear semi-Lagrangian interpolation advect the velocity: `uᵃ = SL_RK2(uⁿ)`.
2. Explicit viscosity and forcing give `u* = uᵃ + Δt (ν Lₕ uᵃ + a + f_EM/ρ)`.
3. Solve `A φ = −h² Dₕ u*`, where `A = −h² Dₕ Gₕ` is the positive pressure graph Laplacian.
4. Project `uⁿ⁺¹ = u* − Gₕ φ`; report gauge pressure `p = ρ φ / Δt`.

Divergence and gradient use the same face connectivity. One pressure reference per connected fluid component removes the Neumann nullspace. Connectivity/gauge selection is a one-time CPU geometry preparation after a material-ID readback; physical field updates and the pressure solve run on the GPU. The matrix-free pressure solver uses Jacobi-preconditioned conjugate gradients with GPU reductions. It starts from the last accepted pressure and recomputes a true residual before acceptance, including the physical divergence equation at gauge cells.

Acceptance combines relative pressure residual and absolute RMS divergence tolerances:

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

The bundled hotend is a coarse example, not a validated jetting design. Its material values, geometry resolution, timestep and force history all need convergence/operating-condition checks before quantitative interpretation. The next physics stage is a conservative moving liquid–air interface and balanced surface tension, validated on static/oscillating drops and breakup, followed by time-dependent coupling to electromagnetics.
