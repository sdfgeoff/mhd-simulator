# Periodic liquid-cylinder capillary diagnostic

This is **test-only validation tooling**, using the existing full-3D GPU fluid
and moving-interface solver. It measures linear cylinder growth and stable
oscillation before attempting breakup. It adds no application mode, physics
setting or second CPU field solver.

## Reference and initial conditions

For a small axisymmetric disturbance of an inviscid cylinder, let `R₀` be its
mean initial radius, `k` the axial wavenumber and `x=kR₀`. Matching cylindrical
velocity potentials proportional to `I₀(kr)` inside and `K₀(kr)` outside, then
applying the kinematic and Laplace-pressure conditions, gives

`s² = (σ/R₀³) x(1−x²) / [ρ_l I₀(x)/I₁(x) + ρ_g K₀(x)/K₁(x)]`.

The zero-ambient-density limit gives the classical liquid-jet relation. For
equal densities, the Bessel Wronskian reduces it to the isotropic ideal-fluid
result in [the published equal-density analysis, equation 3.2](https://www.cambridge.org/core/journals/journal-of-fluid-mechanics/article/rayleighplateau-instability-of-anisotropic-interfaces-part-1-an-analytical-and-numerical-study-of-fluid-interfaces/3655725B371DD3A0CA26CB4DC7BCB3D6).
[Basilisk's cylinder benchmark](https://basilisk.fr/src/test/plateau.c) separately
illustrates the distinction between initial linear growth and nonlinear
pinch-off scaling; that benchmark is axisymmetric, while this calculation uses
a three-dimensional Cartesian grid.

Initial velocity is **zero**. The linear amplitude is therefore `cosh(s t)` for
`x<1`, or `cos(ωt)` with `ω=√(−s²)` for `x>1`. An exponential growing mode would
also require its corresponding nonzero initial velocity.

The synthetic case uses `ρ_l/ρ_g=7000:1`, `σ=1 N/m`, zero viscosity/gravity and a
nominal volume-equivalent radius `R_v=0.1 m`. The initial radius is
`r(z)=R₀[1+ε cos(kz)]`, where `ε=0.01` and `R₀=R_v/√(1+ε²/2)`. This preserves the
analytic volume without renormalizing transported fractions. The box is eight
nominal radii wide, with impermeable transverse faces and one periodic axial
wavelength. The reference assumes unbounded ambient fluid; the finite square
outer boundary remains an approximation.

## Geometry and measurements

Circle/rectangle intersections are integrated analytically in each transverse
section, with 16 midpoint samples through each axial cell. Scalar Bessel series
and integrals are checked against independently calculated high-precision
values, the Wronskian and both density limits. These are analytic reference and
geometry calculations, not a CPU time-stepping solver.

A GPU reduction measures volume, signed axial Fourier modes and transverse
centroid. It reads the current volume-fraction field and writes private small
reduction storage; both ping-pong field bindings and byte-for-byte preservation
of accepted buffers are tested. Dense field readback is unnecessary each tick.
The measured signal is `C=2 Σα cos(kz)/Σα`, normalized by its initial value.
Keeping its sign is essential for the stable wave's reversal.

CSV files include actual radius/wavelength, timestep, deformation, measured
kinetic energy and the linear-mode kinetic reference. The latter is

`K_lin = [πσL/(2R₀)] (1−x²) (R₀ε)² [F(t)²−1]`,

where `F` is the corresponding cosh/cos response. This is a small-amplitude mode
comparison. Agreement does not establish total energy conservation or accurate
local velocities.

## Initial measured results

All cases use the existing physical pressure criterion, relative and absolute
tolerances `10⁻⁶`, and at most 4,000 iterations. The timestep is 0.4 of the
capillary bound. Radius-cell counts denote `R_v/h`; the actual radius and
wavenumber are used in the reference. Growth rates fit a fixed-amplitude cosh
response over `0.15≤s t≤1.5`, with no fitted phase shift:

| Cells per radius | Grid | Fitted growth-rate error | Final mode / initial |
| --- | --- | ---: | ---: |
| 8 | 64×64×72 | +1.610% | 2.39386 |
| 10 | 80×80×90 | +0.187% | 2.35888 |
| 12 | 96×96×108 | +0.910% | 2.37954 |

The reference final ratio is 2.35241 at `kR₀≈0.6981`. The trend is nonmonotonic;
these data do not establish a convergence order. The largest grid uses the
automatic pressure multigrid path, while the smaller ones use Jacobi-CG. This
matrix does not isolate preconditioner choice.

Halving the 8-cell-radius timestep changes its fitted error to **+1.465%**. The
unperturbed cylinder develops peak kinetic energy **0.0713 µJ** and a maximum
absolute first mode of **5.4×10⁻⁶** over the same growth-time window. Relative
volume drift stays below **1.6×10⁻⁷** in the completed matrix.

The short-wave control has `kR₀≈1.3962`. It oscillates rather than growing, but
amplitude error remains:

| Cells per radius | Fitted frequency error | Final mode at `ωt=3` |
| --- | ---: | ---: |
| 8 | +0.398% | −1.04854 |
| 10 | −0.171% | −0.94061 |

The reference final mode is −0.98999. Near-reference frequency alone does not
validate amplitude evolution. Perturbed full-timestep cases have peak kinetic energies differing from the linear-mode
reference by about 1–3.2%; maximum local speeds do not refine
monotonically. Neither local-flow accuracy, nozzle pinch-off, satellite sizes
nor jetting thresholds has been established.

## Follow-up controls

At the 12-cell radius, a Jacobi-CG control repeats the first 380 steps of the
multigrid run with the same f32 timestep and unchanged pressure criteria.
Maximum difference in normalized mode is **2.50×10⁻⁶**; final kinetic energy
changes **−0.00808%**. This is much smaller than the observed grid reversal over
that prefix (`s t≈0.501`). It does not isolate all late-time effects.

An initial-only curvature reduction compares mixed-cell curvature against the
analytic surface of revolution:

`κ(z)=1/[r√(1+r_z²)]−r_zz/(1+r_z²)^(3/2)`.

Weights are `α(1−α)`. A weighted regression against `cos(kz)` fits the mean and
harmonic together; a plain Fourier sum would leak the large mean curvature into
the small harmonic because mixed-cell weights vary with axial position.

| Cells per radius | Initial curvature harmonic error | Weighted RMS curvature error (m⁻¹) |
| --- | ---: | ---: |
| 8 | +2.127% | 0.08341 |
| 10 | +0.874% | 0.05149 |
| 12 | +0.432% | 0.03581 |
| 16 | +0.131% | 0.01958 |

Initial curvature improves monotonically in this placement. Together with the
pressure control, that points to examining evolving geometry/transport rather
than attributing the dynamic reversal to initial curvature or multigrid alone.
It does not prove which mechanism causes it. A scalar weighted-fit control and
an independent GPU/readback moment comparison verify this reduction; accepted
field, normal, statistics and checkpoint buffers are unchanged. The readback
comparison is test-only aggregation, not a CPU fluid solver.

## Reproduce

```sh
cargo test --offline --lib surface::cylinder_tests -- --nocapture
MHD_CYLINDER_RADIUS_CELLS=8,10,12 MHD_CYLINDER_DURATION=1.5 \
  cargo test --offline --lib rayleigh_plateau_cylinder_diagnostic -- --ignored --nocapture
MHD_CYLINDER_RADIUS_CELLS=8,10 MHD_CYLINDER_KR=1.4 MHD_CYLINDER_DURATION=3 \
  cargo test --offline --lib rayleigh_plateau_cylinder_diagnostic -- --ignored --nocapture
```

The initial-only diagnostic runs with:

```sh
cargo test --offline --lib cylinder_initial_curvature_diagnostic -- --ignored --nocapture
```

It writes `artifacts/cylinder-initial-curvature.csv` without time stepping.

Set `MHD_CYLINDER_DT_FACTOR=0.2` for the half-timestep control or
`MHD_CYLINDER_DEFORMATION=0` for an unperturbed cylinder. Optional
`MHD_CYLINDER_PRESSURE_MG=0/1` selects a test-only pressure control; leaving it
unset retains automatic selection. Scalar fits report both the rate and the
remaining RMS mode error. Successful completion enforces solver/fraction/volume
checks; it is not an assertion that the measured rate matches theory.

Output is `artifacts/cylinder-N-krX-eY.csv`, with `N` the cells per radius.
Archive files before changing timestep or pressure settings, since those runs
share filenames. No desktop window is opened. Six regular scalar/geometry/GPU
measurement checks accompany the ignored dynamic diagnostic; the last full
production suite passed 135 tests before this test-only addition.
