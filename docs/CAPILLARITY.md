# Dynamic capillary validation

A static Laplace-pressure jump is necessary but does not establish that a moving interface has the right dynamics. The standing-wave benchmark couples projected fluid velocity, geometric VOF transport and reconstructed surface tension over 1.5 oscillation periods.

## Setup and reference

The box is 1 m long and 1 m high, with four cells in the extrusion direction. The initial interface is `η = 0.5 + 0.005 cos(2πx)` metres; its cell fractions are integrated over each cell. Initial velocity and gravity are zero. Surface tension is 1 N/m, both densities are 1 kg/m³ and liquid dynamic viscosity is 10⁻⁶ Pa·s, approximating the inviscid limit. These dimensions define a numerical benchmark, not a hotend geometry. Outer faces are impermeable. The wave has zero slope at the side walls, consistent with the neutral geometric wall extension.

Linear potential flow with rigid upper/lower boundaries gives

`ω² = σ k³ / [ρ_l coth(k h_l) + ρ_g coth(k h_g)]`.

Here `k = 2π m⁻¹` and both depths are 0.5 m, giving **ω = 11.1158783 rad/s**. The lower velocity potential at the interface is `η̇ coth(kh_l)/k`, and the upper one is its negative with the upper depth. The linear pressure jump is therefore `−[ρ_l coth(kh_l)+ρ_g coth(kh_g)] η̈/k`; equating this to the capillary jump `σ k² η` gives the expression above. The primary [Basilisk capillary-wave benchmark](https://basilisk.fr/src/test/capwave.c) supplies a related deep-water viscous-wave test; this implementation compares to the finite-depth, nearly inviscid relation derived here.

The measurements in the first two tables below precede [mass-consistent momentum transport](MOMENTUM.md), at commit 906a130. Updated measurements follow them.

The measured amplitude is the cosine Fourier coefficient of column-integrated liquid height. Frequency comes from same-direction zero crossings, reducing sensitivity to initial transients and amplitude damping. The timestep is 0.8 of the explicit capillary limit, with matched final physical time at each resolution.

| Grid | Frequency, rad/s | Relative frequency error |
| --- | ---: | ---: |
| 32×32×4 | 10.98256 | 1.20% |
| 64×64×4 | 11.01929 | 0.87% |

At 32×32×4, reducing the timestep by four gives frequency error 1.21%, close to the 1.20% value above; the remaining error is primarily spatial for this case.

The integrated volume remains constant to the reported f32 reduction precision. Further runs vary liquid density while keeping gas density at 1 kg/m³, surface tension at 1 N/m and liquid viscosity at 10⁻⁶ Pa·s:

| Liquid/gas density ratio | Frequency error, 32×32×4 | Frequency error, 64×64×4 |
| --- | ---: | ---: |
| 1000:1 | 0.584% | 0.145% |
| 7000:1 | 0.573% | 0.140% |

Those baseline cases preserve reduced volume and remain oscillatory over 1.5 periods. The regular half-period regression checks both 1:1 and 7000:1 ratios. They establish capillary frequency behavior at large density contrast, while viscous damping and droplet breakup need separate validation. Momentum transport now has its own periodic conservation test.

## With consistent MAC momentum transport

The current method uses the same PLIC mass fluxes to carry momentum and fraction, then projects with the updated density. At equal density, measured frequency is **10.9825924 / 11.0194531 rad/s**, retaining the **1.20% / 0.87%** refinement trend. At 7000:1, measured frequency is **0.1866898 / 0.1874863 rad/s** on the two grids, giving errors **0.633% / 0.209%**. Volume drift remains below **1.2×10⁻⁷** relative over 1.5 periods. This slight frequency-error increase accompanies the large improvement in nonuniform momentum conservation; both errors must be tracked when refining the method.

The extra density projection increases solve cost. At the momentum milestone, starting it from a zero pressure increment instead of the larger force-balance pressure cut the two-grid high-density benchmark from **153.8 s to 124.4 s** without changing equations or tolerances. With full [symmetric viscous stress transfer](VISCOUS_STRESS.md), the current benchmark takes **136.1 s**. The earlier interface-only coupling took 75.6 s, so the complete conservative method is still slower. These are single-run diagnostic timings and vary with GPU load. Combining transport/force stages into one projection remains a separate optimization requiring revalidation.

## Failure found and correction

Using the local Youngs volume gradient for reconstruction produced grid-crossing slope errors in this nearly aligned interface. The resulting small-scale curvature noise generated growing spurious currents and wave amplitude. Reducing the timestep by four did not remove the failure, so the problem could not be resolved by a timestep safety factor alone.

Reconstruction now uses centred height gradients when a complete monotone single-crossing stencil with consistent liquid orientation exists. It falls back to Youngs normals when height reconstruction is unavailable. Curvature retains its separate normal-divergence fallback; using the reconstruction normal indiscriminately for that fallback degraded the static-drop refinement check. The primary [Basilisk contact-angle implementation](https://basilisk.fr/src/contact.h) also describes height-derived interface normals with a fallback; the routines here are independently implemented.

Height columns reject multiple crossings rather than relying only on opposite pure values at their endpoints. At the simulation box, transverse height columns reflect and normal components have odd reflection, providing a neutral **90° geometric contact-angle extension**. This does not supply microscopic wetting, contact-line dynamics or a prescribed contact angle on imported CAD walls.

The regular suite includes a shorter half-period wave at equal density and 7000:1: its amplitude must reverse within 25% of the initial amplitude and its peak must stay below 1.1 times the initial amplitude. Separate geometric checks verify shallow-plane reconstruction through cell-row crossings in both liquid orientations and rejection of ambiguous height columns. Existing static-drop pressure/curvature refinement and transport-conservation checks still pass.

## Run it

```sh
cargo test surface::capillary_tests -- --nocapture
cargo test standing_capillary_wave -- --ignored --nocapture
# Optional refinement and investigation controls:
MHD_CAPILLARY_GRIDS=32 MHD_CAPILLARY_DT_FACTOR=0.2 cargo test standing_capillary_wave -- --ignored --nocapture
MHD_CAPILLARY_DENSITY_RATIO=1000 cargo test standing_capillary_wave -- --ignored --nocapture
```

The long benchmark writes `artifacts/capillary-wave-N.csv`. `MHD_CAPILLARY_DUMP=1` additionally writes raw fraction/curvature and fluid snapshots every 100 steps for headless analysis. Density-ratio controls support further investigation; the frequency table above uses ratio one.
