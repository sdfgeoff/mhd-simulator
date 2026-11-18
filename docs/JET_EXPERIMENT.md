# Exploratory pulsed-nozzle experiment

## Run this case in the application

The prescribed-force nozzle case is available as **`assets/nozzle-pulse.mhd.json`**.
It uses the normal project loader, `RunState`, combined result frames, timeline
and section viewer. The headless regression shares its GPU pulse implementation.

1. Open it with `./run.sh assets/nozzle-pulse.mhd.json`, or use **Load file**.
   Setup contains the nozzle wall and liquid pool with assigned material references.
2. Switch to **Simulation**. The saved common timestep is **1.6680567 µs**.
   **Prescribed nozzle experiment** exposes peak force, duration and the cylindrical
   window bounds. The default is 7×10⁷ N/m³, a 0.5 ms sin² pulse and zero gravity, with thermal evolution off.
3. Set **1199 steps** to reach **2 ms**. Set **Record every** to **20** and increase
   the retained frame window to **64** if you want the whole trajectory. The default ten-frame
   window only retains the latest frames. Click **Simulate**; it can be stopped
   between steps and continued using the same button.
4. Under **View results → Section view**, select **Liquid occupancy** to see the
   moving liquid in both viewports. Velocity, pressure and animated trails use the
   same selected timeline frame. The imported liquid CAD mesh represents its initial
   shape; use Liquid occupancy when examining the evolving interface.

The pulse is prescribed external work, **not a computed coil force**. Its work is
included as external input in the run energy ledger. EM sources and this benchmark
pulse are mutually exclusive, preventing accidental interpretation as a coil-driven
result. Disable the pulse before assigning an actual coil. The liquid properties
are explicitly labelled uncalibrated benchmark values. This project demonstrates
application integration; it does not establish a hardware jetting threshold.

Regenerate the geometry and saved setup using pure Rust:

```sh
cargo run --offline --example make_nozzle
# Short headless check through the same application run engine:
cargo run --offline -- --run-check assets/nozzle-pulse.mhd.json 8
# Full release regression, including saved interface snapshots:
cargo test --offline --lib loadable_nozzle_release -- --ignored --nocapture
```

## Integrated release check

The full loadable-project run completes **1199 accepted steps to 2 ms** through
`RunState`, with real `RunFrame` snapshots. Maximum sampled liquid-volume drift
is **1.31×10⁻⁷**. The recorded liquid fraction matches the live interface field.
Compared with the earlier direct-harness production-pressure control, the first
saved release times are unchanged: **1.334 ms** at α>0.5 and **1.401 ms** at α>0.01.
At 2 ms the primary core measures **40.630–43.830 nL**, with late centroid speed
**0.3590–0.3656 m/s**. Core volume differs by at most 0.0022% and centroid speed
by at most 0.021% between the two execution paths. The glTF round trip, shared
pulse parameter conversion and pressure-preconditioner selection prevent a claim
of bit-identical trajectories. The small four-cell satellite remains unresolved.
All checks are headless; this does not claim an interactive desktop test.

## Numerical experiment

Both the loadable project and the headless regression use production CAD
initialization, liquid/air VOF, momentum transport, viscosity and pressure.
The historical studies below used the headless harness. The application project
now exposes the same prescribed pulse through the normal simulation workflow.

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

The first runs below predate neutral wall geometry and CAD phase projection.

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
`MHD_JET_Z_MAX_MM` changes the upper bound (1.5 mm by default).
`MHD_JET_PRESSURE_MG=0/1` overrides the pressure preconditioner for a diagnostic;
leaving it unset keeps production automatic selection.
Use a distinct output directory for every control: snapshots are not cleaned on
rerun, and differing step counts can otherwise leave stale files.

`grid.json` stores dimensions, origin, spacing, timestep and pulse parameters.
`alpha-NNNNN.bin` is little-endian f32, X-fastest. `flow-NNNNN.bin` stores four
f32 per cell: `(u_x,u_y,u_z,p)`. Snapshots occur every 20 steps and at completion;
`diagnostics.csv` uses the same samples. `MHD_JET_RECORD_EVERY` changes the
snapshot interval (default 20) without changing the physical timestep. Coarser
recording also coarsens the sampled boundary/contact checks. No desktop window opens.

Plot saved snapshots with `python scripts/plot_jet.py OUTPUT_DIRECTORY` (optional
NumPy/Matplotlib postprocessing; no additional simulation implementation).

`MHD_JET_INITIALIZATION=voxel` is a test-only initialization control using
cell-centre liquid occupancy alongside cell-centre walls. The default `cad`
uses the application's fractional integration and projection into the voxel
wall domain. `raw` retains the unconditioned CAD-volume integral for comparison.
Their initial volumes differ; compare them as different initial geometry
approximations, not as a mass-preserving conversion between geometries.

With neutral wall geometry, the matched 50.0417 µs unforced control changes from
**6.85 nJ** with raw fractions to **1.51×10⁻¹⁴ J** with CAD phase projection.
Initial liquid volume changes **0.83878 → 0.85200 mm³** (+1.58%). The projected
case completes 1 ms with maximum sampled speed **2.73 mm/s**, peak kinetic energy
**4.07×10⁻¹⁴ J**, no boundary contact and negligible expelled volume. Residual
motion and discrete wall/meniscus errors remain. These controls support the
initialization change; they do not validate a physical wetting model or breakup.

[Neutral-wall investigation](NEUTRAL_WALLS.md) and [projection definition](CAD_INITIALIZATION.md).

## First coarse release experiment

With projected initialization, peak force **7×10⁷ N/m³**, the same 0.5 ms pulse,
50 µm cells and timestep factor 0.1, the 2 ms run produces a separated main bolus
and a small fragment. The aperture is six cells across. This is a **coarse-grid
release event**, not validation of a real solder jetting threshold.

1. First saved separation of the large bolus occurs at **1.334 ms** for the
   `α>0.5` core and **1.401 ms** for `α>0.01`. The threshold dependence exposes
   an unresolved neck; saved frames are 33.36 µs apart.
2. At 2 ms, primary core volume is **40.63–43.83 nL** across those thresholds,
   corresponding to equivalent sphere diameter **0.427–0.437 mm**. A late
   centroid fit gives **0.359–0.366 m/s**. These ranges capture threshold
   sensitivity, not total uncertainty or a convergence interval.
3. The small fragment has only four cells in its `α>0.5` core, about two cells
   across in equivalent diameter. Its physical existence/size is unresolved.
   No significant liquid reaches the outer boundary in the saved frames.
4. The 10⁸ N/m³ run reaches the closed upper boundary at about **1.051 ms** and
   stops. It needs a larger air box before interpreting its later behavior.

```sh
MHD_JET_FORCE_N_M3=70000000 MHD_JET_END_S=0.002 MHD_JET_DT_FACTOR=0.1 \
  MHD_JET_PRESSURE_MG=1 MHD_JET_OUTPUT=artifacts/jet-force70 \
  cargo test --offline --lib pulsed_nozzle_experiment -- --ignored --nocapture
```

The pressure multigrid control at the lower 5×10⁷ N/m³ pulse reproduces the
2 ms retraction trajectory: maximum saved fraction L1 difference is
**1.36×10⁻⁷** relative to liquid volume, with unchanged convergence criteria.
A short sequential 120-step screening pair takes **19.33 → 12.78 s**, including
initialization/output; one pair is not a general performance ratio. The full
Jacobi run overlapped other GPU validation, so its wall time is not used as a
paired speedup measurement. Production's automatic size threshold is unchanged.

## Release refinement controls

The exact half-timestep and twice-finer spatial controls both complete 2 ms and
retain a released main bolus. For the `α>0.01` primary core:

| Cell size | Actual timestep | Core volume at 2 ms | Equivalent diameter | Late centroid speed |
| --- | ---: | ---: | ---: | ---: |
| 50 µm | 1.66806 µs | 43.830 nL | 0.43745 mm | 0.36567 m/s |
| 50 µm | 0.83403 µs | 43.832 nL | 0.43745 mm | 0.36512 m/s |
| 25 µm | 0.58997 µs | 39.298 nL | 0.42182 mm | 0.36066 m/s |

Across both 0.01/0.5 cutoffs, halving the timestep changes core volume by less
than 0.06% and fitted speed by less than 0.2%. First saved release frames are
unchanged. The spatial control changes volume by **−10.34%**, diameter by
**−3.57%**, and speed by **−1.37%** at the 0.01 cutoff. Two grids do not establish
a convergence order or a complete error bound.

The nozzle's initialized liquid cross-section is **0.08000 mm²** at 50 µm,
versus **0.07000 mm²** at 25 µm. The polygonal CAD opening is 0.0706354 mm²:
the coarse voxel opening is **13.26% too large**, while the finer one is
**0.90% too small**. This is an important geometric contribution to the spatial
change, alongside transport, curvature and unresolved-neck errors.

At 25 µm the two core cutoffs give diameter **0.415–0.422 mm** and speed about
**0.361 m/s**. First saved large-core separation is 1.357/1.416 ms; frames are
59.0 µs apart. There is no separate trailing component at 2 ms at either cutoff,
unlike the coarse result. Transient tiny fragments and the physical satellite
count remain unresolved. Maximum sampled relative volume drift is below
2.7×10⁻⁷ across the completed controls, with no significant boundary contact.

```sh
MHD_JET_CELL_MM=0.025 MHD_JET_FORCE_N_M3=70000000 MHD_JET_END_S=0.002 \
  MHD_JET_DT_FACTOR=0.1 MHD_JET_PRESSURE_MG=1 MHD_JET_RECORD_EVERY=100 \
  MHD_JET_OUTPUT=artifacts/jet-force70-fine \
  cargo test --offline --lib pulsed_nozzle_experiment -- --ignored --nocapture
```

This supports qualitative release in the idealized model. Quantitative nozzle
thresholds and real-alloy droplet/satellite predictions still require further
spatial checks, material data and contact physics.

## Grid-origin control

`MHD_JET_GRID_SHIFT_X_MM` shifts both X domain bounds relative to the fixed CAD
and prescribed force. It defaults to zero and is limited to one cell in either
direction. Domain extent and nominal cell size are preserved; the output's
`origin_m` records the actual grid. This probes voxel alignment sensitivity,
including the wall mask and initial liquid projection. The outer boundary moves
with the box, so the boundary-contact guard remains necessary.

At 25 µm spacing, an X shift of 12.5 µm changes the initialized opening area
from **0.07000 to 0.06750 mm²**, versus the polygonal CAD's 0.0706354 mm².
Thus the unshifted grid's small area error is placement-dependent. Initial
liquid volume changes from 0.856125 to 0.857625 mm³. The shifted run stops at **1.39233 ms**, step 2360, when a velocity spike exceeds
the conservative CFL limit at the original 0.5899705 µs timestep. It is an
incomplete control, not a completed 2 ms release result. The last saved frame
at 1.35693 ms has a separated 37.68 nL core at `alpha>0.01`; the 0.5 cutoff
also has several unresolved small components. Volume drift remains below
1.95e-7 and no liquid reaches an outer boundary. That original-step control remains incomplete; the matched smaller-step runs below complete the late-time comparison.

```sh
MHD_JET_CELL_MM=0.025 MHD_JET_GRID_SHIFT_X_MM=0.0125 \
  MHD_JET_FORCE_N_M3=70000000 MHD_JET_END_S=0.002 \
  MHD_JET_DT_FACTOR=0.1 MHD_JET_PRESSURE_MG=1 MHD_JET_RECORD_EVERY=100 \
  MHD_JET_OUTPUT=artifacts/jet-force70-fine-shifted \
  cargo test --offline --lib pulsed_nozzle_experiment -- --ignored --nocapture
```


### Matched half-timestep controls

Both origins now complete **6780 steps to 2 ms** at exactly **0.2949852507 µs**,
half the original timestep. Saved frames are 58.997 µs apart. These historical
controls use the original point-reference pressure solver and pulse kernel from
commit `aea6201`, with multigrid forced on; the frozen test executable is retained
in the local artifacts. They isolate placement at the same timestep, rather than
mixing a timestep change into the origin comparison.

| VOF core | Unshifted volume at 2 ms | X shifted 12.5 µm | Change | Unshifted late speed | Shifted late speed |
|---|---:|---:|---:|---:|---:|
| α>0.01 | 39.274 nL | 37.609 nL | −4.24% | 0.36541 m/s | 0.35818 m/s |
| α>0.5 | 37.491 nL | 35.714 nL | −4.74% | 0.36435 m/s | 0.35630 m/s |

First saved large release occurs at **1.47493 / 1.41593 ms** for the unshifted
0.01 / 0.5 cores, and **1.35693 ms** at both thresholds for the shifted case.
Maximum sampled volume drift is **3.90×10⁻⁷ / 4.54×10⁻⁷** respectively, with zero
occupied outer-boundary cells in every saved frame. Tiny fragments remain
unresolved. Halving the unshifted timestep changes core volume by at most 0.20%
and late speed by about 0.9–1.3%, but delays the first saved release by one sample.

The 4–5% placement dependence remains a quantitative accuracy limit. It is
consistent with the changed voxel aperture and warrants further spatial/contact
checks before using droplet size or jetting thresholds as hardware predictions.
The completed trajectories are numerical regression controls, distinct from the
computed-force coil example. Their overlapping runtimes are not performance data.

The exact half-step control uses `MHD_JET_DT_FACTOR=0.04999136166618539` and
`MHD_JET_RECORD_EVERY=200`, with X shifts 0 and 0.0125 mm. Pin the stated commit
when reproducing the original solver/pulse implementation. In the application,
`nozzle-pulse.mhd.json` exposes the same geometry/material/pulse setup: set 25 µm
cells, 0.2949852507 µs common steps and 6780 steps; the current production solver
and glTF round trip can produce small differences from these historical fields.
