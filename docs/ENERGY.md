# Coupling benchmarks and energy accounting

The common timeline owns complete `RunFrame`s. Each frame records the active field outputs and its energy ledger; visualization panels borrow fields from the selected frame. Effective timesteps/source wiring are resolved once in the run core. Retention and scrubbing cannot select different EM/fluid times.

## Energy integrals

**Energy accounting (J)** below the timeline and `--run-check` CSV output expose the following quantities:

1. Stored magnetic energy of the staggered induction operator and kinetic energy of MAC face velocities.
2. Accumulated Joule heat generated, prescribed EM source/boundary work, and work by gravity or configured acceleration.
3. Backward-Euler magnetic numerical dissipation, the fluid work-balance loss, coupling work mismatch, and the independent EM closure error.

Let `A` contain total oriented edge vector potential, `C` be the dimensionless edge-to-face curl, `νr` the face inverse relative permeability, and `h` the spacing. The operator magnetic energy is `h/(2 μ₀) (CA)ᵀ νr (CA)`. Its boundary-face weighting is the operator's weighting; it is distinct from the physical-volume cell-quadrature estimate in the EM inspector.

The induction solve obeys the discrete step identity

`Δ magnetic energy + Joule heat + mechanical work + backward-Euler loss = external EM work`.

Joule heat is `Δt h³ Σ_edges σedge |−ΔA/Δt + (u×B)edge|²`. Mechanical work is `−Δt h³ Σ_edges Jedge·(u×B)edge`. Conductivity-weighted projection makes this agree with cell-current work against the velocity used by the EM solve. Backward-Euler loss is the same magnetic quadratic form applied to `ΔA`. External work is evaluated from the prescribed-source curl/adjoint forcing and reaction at the prescribed outer-boundary values. It is not obtained by setting the energy residual to zero. The small closure residual is reported separately. All physical field integrals/reductions execute on the GPU; only scalar totals are read back.

For fluid motion, work is summed over unique physical MAC faces using midpoint velocity `(uold+unew)/2` and the applied Lorentz/body force. Kinetic-energy change uses squared face velocities. **Fluid loss** is work minus kinetic-energy change: it includes viscosity, advection/projection dissipation and splitting effects. It is an inferred balance term, not an independently measured viscous heating source, and is not silently deposited as heat.

**Coupling work mismatch** compares fluid Lorentz work against the EM mechanical-transfer term. Lagged velocity/B and the partitioned step can make these differ. It is exposed rather than hidden in external work; refine timestep and resolution to investigate it. Accounting is not a claim of exact total-energy conservation of the coupled method. The implementation tests the algebraic relationship between the independently evaluated transfers and the overall ledger.

The air-region gauge term penalizes node divergence of reaction A only at strictly interior nodes whose incident edges have zero conductivity. Its gradient is a pure gauge supported on nonconducting edges, so it changes neither `curl A` nor conductor currents. It suppresses air null modes that otherwise degrade PCG conditioning. Strict true-residual checks still apply. Single precision and weak conductive mass can require smaller timesteps for a requested residual tolerance; a larger iteration cap cannot always overcome roundoff.

## Analytic rotating sphere

A conducting sphere of radius `R` rotates with angular speed `Ω` perpendicular to a uniform field `B`, at low magnetic Reynolds number. Its steady braking torque magnitude is

`|τ| = 2π σ Ω B² R⁵ / 15`, and `P_Joule = |τ| Ω`.

This is the controlled-spin relation in section 4.6.2 of the [NASA/General Electric containerless-processing report (1974)](https://ntrs.nasa.gov/api/citations/19740023829/downloads/19740023829.pdf). For `Ω` along X and B along Z, the analytic internal current is `J = σ Ω B (-z, 0, x)/2`; its surface-normal component vanishes and its Joule loss equals the torque work.

The GPU test uses a 3 mm sphere, σ=10⁶ S/m, Ω=10 rad/s and B=0.05 T, with air padding and a prescribed rigid-rotation velocity packet. This verifies motional induction/force transfer with known kinematics; it is not a free-decay fluid solution. Timestep scales with h² for single-precision conditioning and each run advances to 100 µs.

| Cells per radius | Measured power | Analytic power | Relative error |
|---:|---:|---:|---:|
| 4 | 2.485871×10⁻⁸ W | 2.544690×10⁻⁸ W | 2.31% |
| 8 | 2.517119×10⁻⁸ W | 2.544690×10⁻⁸ W | 1.08% |

Tests independently integrate torque from the reported Lorentz force, compare torque work with Joule power, and bound EM energy closure. Geometry remains a voxel staircase. A separate sinusoidal-field test checks positive external work, Joule generation, backward-Euler loss, cell/edge heating agreement, and relative energy closure below 10⁻⁴.

```sh
cargo test rotating_sphere_matches -- --nocapture
cargo test driven_induction_accounts -- --nocapture
cargo test coupled_em_and_velocity_time_refine_together -- --nocapture
./run.sh --run-check assets/hotend-cad.mhd.json 10
```

All checks are headless. They supplement the independent EM/fluid tests and the published TEAM6 case; no duplicate CPU physics solver is introduced.
