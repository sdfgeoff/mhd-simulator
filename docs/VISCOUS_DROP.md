# Viscous capillary-drop diagnostic

This extends the [3D inviscid drop check](DROP_OSCILLATION.md) to viscosity and measures shape decay. It is a diagnostic of coupled surface, momentum and stress errors; current results do **not** establish converged physical damping.

## Reference

For a small l=2 shape mode of an isolated spherical drop, write the modal eigenvalue as s=β+iω, so amplitude decays as exp(−βt). Lamb's weak-viscosity limit is βL=5ν/R². The more general linear characteristic equation includes internal vorticity:

    s² − 2 βL s [1 − Γ(x)] + ωR² = 0
    x² = s R²/ν
    Q(x) = j₃(x)/j₂(x)
    Γ(x) = (3/5) [2Q(x)/x] / [1 − 2Q(x)/x]
    ωR² = 8σ/(ρR³)

These are [Lohöfer (2020), equations 12–15](https://link.springer.com/article/10.1007/s10765-020-2608-z), specialized to l=2. The benchmark computes the scalar root using complex Bessel ratios and Newton iteration. Tests check the ratio against closed half-integer functions, the root against the unrearranged characteristic equation, and the weak-viscosity limit.

The reference assumes vacuum outside the drop. The numerical ambient is inviscid with density 1 kg/m³, compared with liquid density 7000 kg/m³. Small ambient inertia and finite box boundaries remain differences from this reference.

## Setup and measurement

The unit cube contains the same volume-preserving ellipsoid as the inviscid check: R=0.2 m, deformation ε=0.03, zero initial velocity, gravity zero and σ=1 N/m. Liquid kinematic viscosity is 2×10⁻⁴ m²/s, giving dynamic viscosity 1.4 Pa·s. These deliberately large dimensional values define a numerical test.

For the nominal radius, the full linear reference gives β=0.0223543 s⁻¹ and ω=0.375351 rad/s. Lamb gives βL=0.025 s⁻¹, about 11.8% above the full decay rate; using it alone would obscure numerical damping error. The simulation uses its initialized volume-equivalent radius for the reference.

Shape is the central second-moment quadrupole. The saved diagnostic fits

    q(t) = c + exp(−βt) [A cos(ωt) + B sin(ωt)]

with free amplitude, phase and offset. Gauss–Newton fitting is checked on a known decaying signal with nonzero phase/offset. This dominant-mode fit does not model all initial viscous transients, nonlinear shape corrections or higher harmonics.

## Observed limitations

The following 1.5-period runs use analytic initial fractions and the stabilized stress integrator. Windows start at zero, one-quarter or one-half of the inviscid reference period.

| Grid | Fit start | Fitted β, s⁻¹ | Decay error against full reference | Normalized fit RMS |
|---|---:|---:|---:|---:|
| 48³ | 0 | 0.025171 | +12.6% | 0.00818 |
| 48³ | T/4 | 0.022399 | +0.2% | 0.00247 |
| 48³ | T/2 | 0.021880 | −2.1% | 0.000737 |
| 64³ | 0 | 0.024577 | +9.9% | 0.00137 |
| 64³ | T/4 | 0.024788 | +10.9% | 0.00132 |
| 64³ | T/2 | 0.024635 | +10.2% | 0.000702 |

The finer-grid fits have smaller residuals but retain about 10% excess decay. The coarser-grid agreement depends on the fit window and does not prove accuracy. Zero-crossing frequency errors against the viscous reference decrease from about 1.25% to 0.67%; damping error does not decrease monotonically. Timestep, amplitude and further spatial refinement are still needed. No inviscid numerical loss is subtracted to manufacture a physical decay rate.

The 48³ stabilized and Euler-subcycled traces differ by at most 0.0341% of the initial quadrupole over the full run. Their measured zero-crossing frequencies differ by 0.0011%, and late-window fitted decay rates by about 0.1%. This supports the integration change, while leaving the shared spatial/shape errors unresolved.

## Reproduce

    MHD_DROP_LIQUID_NU=0.0002 MHD_DROP_GRIDS=48,64 cargo test --offline rayleigh_lamb_drop_oscillation -- --ignored --nocapture
    cargo test --offline viscous_drop_saved_diagnostics -- --ignored --nocapture
    cargo test --offline viscous_reference -- --nocapture
    cargo test --offline fitted_decay -- --nocapture

The first command writes artifacts/drop-oscillation-viscous-N.csv. The second reads those files and reports all three fitting windows without rerunning the GPU. CSVs include an inviscid linear trace as a baseline; it is not the viscous solution. New files also include absolute material volume so the analyzer can recover the initialized reference radius. Older diagnostic files use nominal R=0.2 m.

MHD_DROP_DT_FACTOR changes the capillary timestep fraction; MHD_DROP_DEFORMATION changes amplitude. The test-only MHD_DROP_EULER_VISCOSITY=1 selects the original GPU subcycling for comparisons. It does not change the production project schema. Analyze complete long traces before replacing them with short performance probes.

See [viscous stresses](VISCOUS_STRESS.md) for the operator, stage storage and timestep restrictions, and [performance measurements](PERFORMANCE.md) for timing comparisons.
