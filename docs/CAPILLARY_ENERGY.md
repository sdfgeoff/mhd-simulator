# Capillary-energy diagnostics

These are **test-only measurements**, separate from the application UI. They investigate numerical motion in an initially static drop and the uncertainty in surface-energy estimates. They do not change the simulation's force, transport or pressure equations. The application continues to report its existing smoothed-gradient surface-energy estimate.

## What is measured

The optional `MHD_DROP_ENERGY=1` diagnostic writes `artifacts/drop-energy-N.csv` alongside the usual drop trajectory. It records instantaneous kinetic energy, liquid/air contributions, two area estimates, their estimated `K + σA` changes, quadrupole, volume and maximum cell-centre speed.

1. **Gradient area** is the existing GPU estimate of `∫|∇α| dV`, using the smoothed volume-fraction gradient.
2. **PLIC patch area** sums plane/cube intersection areas over mixed cells. The kernel evaluates `|n| ∂V/∂d` analytically from the factored half-space volume formula used for reconstruction. This follows the mixed-cell patch convention of [Basilisk's interface-area estimator](https://basilisk.fr/src/fractions.h). Independent PLIC patches need not join continuously across cell faces; their sum is not the complete boundary area of a union of reconstructed cells. Pure-cell interfaces exactly on cell faces are another limitation.
3. **Phase kinetic energies** weight the same squared MAC-face velocities by `ρα`: liquid uses `ρ_l α`, air uses `ρ_g(1−α)`. They sum to the existing mixture kinetic energy within floating-point reduction error. This is an energy partition, not two independent phase velocities.

The area kernel agrees with an independent CPU cube-edge/polygon calculation in 3,529 scalar geometry cases, with maximum scaled error 5.1×10⁻⁶. The scalar reference uses f64 cube-edge intersections independently of the GPU CDF formula. A constant-translation check verifies the phase energies against analytic mass/velocity values.

The readback temporarily reuses existing normals, reduction storage and density parameters. Tests verify byte-for-byte restoration of accepted surface buffers and uniforms. A 23-tick moving-drop trajectory is byte-identical with the diagnostic disabled or enabled, and also matches the preceding solver executable. No full-volume workspace is added.

## Reproduce

All commands run headlessly:

```sh
cargo test --offline --lib surface::energy_tests -- --nocapture
cargo test --offline --lib reconstructed_area_refinement -- --ignored --nocapture
cargo test --offline --lib rigid_translation_area_control -- --ignored --nocapture
MHD_DROP_GRIDS=64 MHD_DROP_DEFORMATION=0 MHD_DROP_PERIODS=0.5 MHD_DROP_ENERGY=1 \
  cargo test --offline --lib rayleigh_lamb_drop_oscillation -- --ignored --nocapture
```

Set `MHD_DROP_DEFORMATION=0.03` for the deformed-drop control. Compare `MHD_DROP_DT_FACTOR=0.4` and `0.2`, or set `MHD_DROP_OFFSET_CELLS=0.5` to shift the initial sphere by half a cell in each axis. `MHD_AREA_GRIDS` selects grids for the two area-only diagnostics. Files with the same grid number are overwritten, so archive runs before changing parameters.

Zero deformation is a static diagnostic: it reports absolute quadrupole and does not fit an oscillation frequency or divide by a zero initial mode. The existing frequency and mode-growth bounds for positive deformation remain intact. Static runs still enforce the normal volume, fraction and solver checks; completing one is not by itself a validation of stationarity.

The audit adds GPU readbacks, reconstruction/restoration kernels and file output outside the timed simulation steps. Use separate runs without the energy flag for performance comparisons.

## Initial geometry and rigid translation

The synthetic benchmark uses `R=0.2 m`, `ρ_l=7000 kg/m³`, `ρ_g=1 kg/m³`, `σ=1 N/m`, zero viscosity and zero gravity. The prolate control has volume-preserving deformation 0.03. Its analytic initial surface-energy excess is **0.17444 mJ**, only **0.0347%** of the spherical surface energy.

Signed initial area errors are:

| Grid | Gradient, sphere | PLIC, sphere | Gradient, ellipsoid | PLIC, ellipsoid |
| --- | ---: | ---: | ---: | ---: |
| 32³ | −0.936% | −0.133% | −0.946% | +0.146% |
| 48³ | −0.311% | −0.214% | −0.314% | −0.0647% |
| 64³ | −0.101% | −0.0133% | −0.0825% | −0.0331% |
| 96³ | +0.0821% | +0.0258% | +0.0682% | −0.0457% |

Neither estimate shows monotonic fine-grid error in every case. Their initial errors can be comparable to, or larger than, the physical deformation energy being investigated.

A separate control prescribes uniform translation, with no capillary-force update. After one second, it compares the advected fractions with fractions independently initialized from the translated analytic geometry using the same cell quadrature:

| Grid | Gradient-area difference / analytic area | PLIC-area difference / analytic area |
| --- | ---: | ---: |
| 32³ | +0.00368% | +0.410% |
| 64³ | +0.00164% | +0.113% |

This control combines transport and area-reconstruction error. It demonstrates why an increase in the PLIC-based energy proxy alone cannot establish physical energy production. The gradient proxy has a different bias; it is not an exact conservation test either.

## Initially static drops

Over half a Rayleigh–Lamb period, initially spherical inviscid drops develop numerical motion:

| Grid | Peak kinetic energy | Maximum speed |
| --- | ---: | ---: |
| 48³ | 22.93 µJ | 8.66 mm/s |
| 64³ | 2.783 µJ | 5.47 mm/s |
| 80³ | 2.570 µJ | 5.18 mm/s |
| 96³ | 0.986 µJ | 3.99 mm/s |

At the energy peaks on 64³–96³, more than **99.7%** of this kinetic energy is liquid-weighted. Halving the 64³ timestep changes peak static kinetic energy by **0.61%**; the corresponding deformed-drop peak changes by **0.0072%**. A half-cell shift at 64³ increases peak static kinetic energy by **49%**.

These controls point mainly to spatial error and placement sensitivity over this time window. They do not yet isolate curvature, normal reconstruction and transport contributions, or demonstrate long-time equilibrium. The 96³ case uses the automatic multigrid pressure path; the smaller cases use Jacobi with the same physical acceptance criterion. A separate forced-Jacobi run at 96³ changes peak kinetic energy by only **0.16%**, supporting the spatial interpretation rather than attributing the grid trend to preconditioning.

The deformed 64³ energy proxies disagree strongly: after half a period, the gradient-based change is about **−0.0756 mJ**, while the PLIC-based change is **+5.82 mJ**. Given the geometry/translation controls, neither number is presented as a verified physical energy balance. Further work needs to distinguish interface-reconstruction error from force/transport inconsistency before changing the numerical model.
