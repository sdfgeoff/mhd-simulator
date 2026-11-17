# Neutral interface geometry at voxel CAD walls

The interface geometry uses a **90° neutral-contact approximation** where a
mirror stencil can be constructed at a fixed voxel wall. This affects normal,
curvature and PLIC reconstruction in the normal moving-interface solver. It
adds no material contact-angle property, wetting hysteresis or oxide model.
Fixed-wall flow still uses impermeable staircase faces, not subcell apertures.

## Boundary construction

For a flat wall with normal coordinate x, an even extension
`α(−x,y,z)=α(x,y,z)` gives `∂α/∂x=0` at the wall. The interface normal is then
tangent to the wall, corresponding to 90° contact. The normal component of its
normal vector is odd across that wall; the curvature-divergence fallback uses
this reflection instead of reading an unrelated normal inside solid material.

Geometry samples inside a CAD wall search along coordinate axes toward the
fluid reference cell. The first wall/fluid crossing defines a mirror location.
The mirrored segment must remain non-solid, preventing a jump through a second
wall into another channel. Periodic directions use the shortest wrapped offset.
The search is bounded to six cells. Samples without a valid mirror fall back to
the stored fraction; corners and under-resolved geometry therefore remain
approximations. Axis-order choices can matter at ambiguous corners.

The reference cell matters: a one-cell-thick wall can separate liquid from air,
and sampling whichever side is nearest would mix these unrelated regions.
Geometry ghost values are never stored as liquid occupancy. Face fluxes still
close at the wall, and transported volume/momentum/enthalpy retain their existing
conservation equations. There is no new global field allocation.

For prescribed angles other than 90°, three-dimensional height corrections
must account for the projected interface normal; see the primary
[Basilisk contact-angle notes](https://basilisk.fr/src/contact.h). This code uses
its own neutral mirror construction, not that implementation of arbitrary
contact angles.

## Verification

1. A planar neutral interface against a voxel wall previously gave maximum
   curvature **17.39 m⁻¹** where the analytic value is zero. The new check gives
   zero for all three wall axes, both liquid sides, and wall thicknesses of
   one, two and six cells. Eight coupled fluid/transport steps retain zero
   velocity within the check and unchanged fractions in the one-cell case.
2. A separate periodic-seam test places liquid and air on opposite sides of a
   one-cell wall; the liquid side remains planar with zero curvature.
3. A hemisphere touching a flat wall at its equator has analytic curvature
   `2/R`. Weighted RMS curvature error falls **2.99% → 1.38%** on 32³/48³ grids.
   Its geometry agrees with the mirrored full sphere to relative RMS
   **1.01×10⁻⁷ / 1.69×10⁻⁷**. This isolates the wall extension from the remaining
   sphere-curvature discretization error.
4. The full headless suite passes **147 tests**, with 17 longer diagnostics
   ignored, and strict Clippy passes. The [nozzle experiment](JET_EXPERIMENT.md)
   remains a separate test tool.

```sh
cargo test --offline --lib surface::wall_tests -- --nocapture
```

## Nozzle controls and remaining initialization issue

In the larger-box unforced nozzle control, the same 60 steps and timestep reach
50.0417 µs. Kinetic energy falls **27.45 nJ → 6.85 nJ** with the neutral geometry
extension, and maximum speed falls **0.927 → 0.557 m/s**. Initial volume,
pressure criteria, viscosity and surface tension are unchanged. This improves
startup, but substantial nonphysical motion remains possible.

The liquid CAD volume and binary solid wall do not represent identical subcell
geometry. In a filled reservoir cross-section, 48 of 448 voxel-open cell centres
have partial liquid occupancy. Solid intrusion into those cells is not carried
as a separate cut-cell volume, so the flow model interprets the missing fraction
as air. A diagnostic using the same voxel initialization as the walls reduces
50 µs kinetic energy to **1.37×10⁻¹⁴ J**, but changes initial volume by **+1.58%**.
That binary control is not a calibrated hotend initial condition. The application
now addresses the inconsistency through [CAD phase projection](CAD_INITIALIZATION.md):
liquid occupancy and enthalpy are divided by available non-solid fraction before
mapping into the voxel wall domain. Isolated free surfaces retain their fractional
geometry. This changes the approximated initial volume; it does not add cut-cell
flow or preserve source CAD volume across the conversion.

These are short controls. They do not validate long-duration nozzle equilibrium,
detachment, satellite sizes or a real solder/material contact angle.

## Cost check

One warmed V6 pair takes **24.86 s → 25.06 s** for three coupled ticks, with the
same **1715.4 MiB** logical working allocation. Interface/heat GPU time increases
**222.6 → 276.6 ms**; this is an accuracy change with compute overhead, not a
speedup. All compute passes are recorded. Physical output is intentionally
allowed to change with the boundary model; this is not a byte-identical solver
optimization. One pair does not establish a general performance ratio.

The first new-shader run takes 59.69 s before the warmed 25.06 s run, so shader
compilation/cache startup also matters. Owner CAD/project hashes remain unchanged.
