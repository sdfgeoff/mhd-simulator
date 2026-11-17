# Exploratory pulsed-nozzle experiment

This **test-only experiment** runs the production CAD initialization, liquid/air
VOF, momentum transport, viscosity and pressure solver. It prescribes a force
rather than solving electromagnetics. It is separate from the application UI;
no additional production physics or user controls are introduced.

## Geometry and forcing

An open-backed reservoir has radius 0.6 mm. A fixed annular wall reduces this
to a 0.15 mm radius aperture through a 0.15 mm thick nozzle. Initially, liquid
occupies the reservoir from z=−0.75 mm to z=0, and the aperture to z=0.15 mm.
The back surface is exposed to air: no sealed pressure chamber or liquid inlet
is imposed. The 96-sided revolved CAD meshes are closed and consistently
oriented, with a scalar volume check. All outer simulation faces are closed;
the air domain is finite, extending ±0.9 mm transversely and −2.0 to +1.5 mm
axially by default. The first study below used the smaller −1.2 mm lower bound. This is a synthetic nozzle, not a model of the owner's hotend.

The liquid uses constant density 7000 kg/m³, viscosity 0.002 Pa·s and surface
tension 0.5 N/m; air uses 1.2 kg/m³ and 1.8×10⁻⁵ Pa·s. These are illustrative
inputs, not calibrated alloy/oxide data. Temperature is held outside this
isothermal experiment.

The axial force density is

`f_z(x,t)=α(x,t) f_peak W(x) sin²(πt/T_p)` for `0<t<T_p`, zero afterwards.

The spatial window is confined to the rear pool: a smooth radial cutoff between
0.45 and 0.55 mm, rising between z=−0.85 and −0.75 mm, falling between z=−0.25
and −0.15 mm. It does not directly force the emerging jet. Midpoint time samples
are used. The GPU shader writes the normal force-packet allocation, with all
magnetic/current/heating entries zero. A numerical GPU check verifies the window,
liquid fraction weighting, both ping-pong bindings and unchanged input fractions.
This does not validate a real coil's force distribution.

## Measurements and numerical limits

CSV rows report total liquid volume, drift, kinetic energy, maximum speed,
pressure iterations/residual, volume beyond the nozzle exit and leading occupied
cell centre (`α>0.5`). Later rows include maximum cell-centre speeds separately
for liquid-dominant (`α>0.99`), air-dominant (`α<0.01`) and mixed cells.

Sparse snapshots save liquid fraction and cell-centre velocity/pressure. The
connectivity diagnostic compares six-neighbour components at `α>0.01` and
`α>0.5`. The reported nonlargest core volume sums α only in the latter thresholded
cores, excluding the largest component (assumed to remain the reservoir). This
is **not the full volume of a detached droplet**. Different component counts at
the two thresholds expose unresolved bridges; matching counts alone do not prove
resolved pinch-off. No fragments are silently discarded by size.

Pressure uses relative tolerance 10⁻⁵, absolute divergence tolerance 10⁻³ s⁻¹
and at most 10,000 iterations. Conservative advection and capillary limits remain
enforced. The requested timestep is a fixed fraction of the capillary bound,
rounded down to reach the requested end time in an integer number of steps.
A CFL failure stops the experiment; it is not bypassed or accepted as ejection.
Saved snapshots also report maximum outer-boundary fraction. Values above 0.01
stop the experiment with a request to enlarge the box, after writing the sample.
This sampled guard can miss contact between snapshots; it is not an open-air
boundary condition.
The solver's residual is a criterion for the discrete pressure equation, not a
claim that the nozzle geometry/contact physics is accurate.

## Startup observations

A first 25 µm-grid attempt with capillary timestep factor 0.3 stops at the
conservative CFL check after its early 35.4 µs snapshot. The force pulse is still
weak at that point. A zero-force control at factor 0.1 completes 50 µs, developing
maximum speed 1.80 m/s and about 0.72 nL beyond the outlet. This motion cannot be
attributed to the prescribed pulse. The initially flat menisci touch voxel walls;
the code has no prescribed CAD contact angle, wetting hysteresis or oxide model.
The initial state has not been established as a discrete capillary equilibrium.

Initial sampled liquid volume is 0.8487 mm³ at 25 µm and 0.8388 mm³ at 50 µm,
compared with 0.8582 mm³ for the polygonal liquid mesh. That discrepancy includes
staircase-wall exclusion and must be considered in resolution comparisons.
A finer grid or smaller timestep alone does not supply missing wetting physics.

A 50 µm-grid pulse run at factor 0.05 completes 1 ms (1,199 steps). It moves
**38.95 nL beyond the exit**, with a leading occupied cell centre at z=0.675 mm
and sampled volume drift below **1.33×10⁻⁷**. Every saved snapshot has one
connected core at thresholds 0.01, 0.5 and 0.99: **no detached drop is observed**.
The rear meniscus also deforms substantially. This coarse aperture is only six
cells across and is a demonstration, not a resolved nozzle study. The matching
zero-force run also completes 1 ms and retracts into the nozzle, ending with its
leading occupied centre at z=0.075 mm and negligible volume beyond the exit.
This establishes a different response to the prescribed pulse in this setup;
it does not establish a correct physical jetting threshold. **The unforced rear
surface reaches the closed lower boundary by the saved 0.550 ms snapshot.**
Its subsequent trajectory is boundary-influenced, so the 1 ms comparison cannot
isolate an unbounded-air response. The pulse case does not touch the outer
boundary in its saved snapshots. A larger-domain control is in progress.

These runs are exploratory. Expelled volume is not detached-drop volume, and a
neck disappearing below one cell is not a validated physical detachment event.
The [cylinder benchmark](CYLINDER_INSTABILITY.md) and [drop oscillation](DROP_OSCILLATION.md)
provide complementary validation away from these CAD contacts.

## Reproduce

```sh
cargo test --offline --lib jet_ -- --nocapture
MHD_JET_CELL_MM=0.05 MHD_JET_END_S=0.001 MHD_JET_DT_FACTOR=0.05 \
  MHD_JET_OUTPUT=artifacts/jet-coarse-pulse \
  cargo test --offline --lib pulsed_nozzle_experiment -- --ignored --nocapture
```

The first smaller-box results above predate the boundary guard.
Defaults use 50 µm cells, 1 ms duration and timestep factor 0.05. Default peak force is 5×10⁷ N/m³ and duration 0.5 ms. Set
`MHD_JET_FORCE_N_M3=0` for the unforced control, `MHD_JET_PULSE_S` for pulse
duration, and `MHD_JET_CELL_MM`/`MHD_JET_DT_FACTOR` for spatial/time controls.
`MHD_JET_Z_MIN_MM` changes the lower bound (−2.0 mm by default).
Use a distinct output directory for every control: snapshots are not cleaned on
rerun, and differing step counts can otherwise leave stale files.

`grid.json` stores dimensions, origin, spacing, timestep and pulse parameters.
`alpha-NNNNN.bin` is little-endian f32, X-fastest. `flow-NNNNN.bin` stores four
f32 per cell: `(u_x,u_y,u_z,p)`. Snapshots occur every 20 steps and at completion;
`diagnostics.csv` uses the same samples. No desktop window opens.

Plot saved snapshots with `python scripts/plot_jet.py OUTPUT_DIRECTORY` (optional
NumPy/Matplotlib postprocessing; no additional simulation implementation).

`MHD_JET_INITIALIZATION=voxel` is a test-only initialization control that uses
cell-centre liquid occupancy alongside cell-centre walls. The default `cad`
retains fractional CAD integration. Their initial volumes differ; compare them
as different initial geometry approximations, not as an exact mass-preserving
initialization change. [Neutral-wall investigation and measurements](NEUTRAL_WALLS.md).
