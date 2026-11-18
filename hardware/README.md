# Solder jetter hardware study

Research branch: `research/hardware-design-20261118`. The frozen application
baseline is `simulator-baseline-2026-11-18`. This study has **not demonstrated a
manufacturable solder jetter or qualified repetitive droplet production**.

## Requirements and evidence

1. [requirements.json](requirements.json) records the requested materials,
   droplet/bore sizes, fabrication preferences, drive limits and stop condition.
2. [sources.json](sources.json) records primary references, numerical values,
   and the limitations on transferring those values to this apparatus.
3. [screening-stage1.json](screening-stage1.json) records cold EM screening,
   including the refined post-pulse convergence failure. A radial force moment
   is a diagnostic; it is neither a pressure solution nor a jetting criterion.

## Reproduce the calculations

Use the study worktree. All runtime output stays under ignored `artifacts/`.
Headless checks use the same `RunState` as the GUI; there is no separate fluid
or electromagnetic solver for the study.

1. Generate a loadable CAD/project pair, for example:

   ```sh
   python scripts/make_hardware.py --name foil-open-z6-freewheel36-h015 \
     --family foil --chamber 6 --orifice .6 --land .15 \
     --coil-length 6 --layers 4 --coil-z 6 --cell .15 \
     --current 200 --voltage 48 --drive-voltage 36 --fluid --freewheel
   ```

   `--drive-voltage 36 --voltage 48` describes an ideal 36 V pulse waveform
   supplied by a 48 V system. It needs a regulated pulse bus or a separately
   checked PWM implementation. It is not a validated driver schematic.
   `--freewheel` models resistive decay through a shorted bridge, excluding
   switch/diode dynamics. The generated assumptions file contains the estimates.

2. Run the combined solver and export accepted frames:

   ```sh
   target/debug/mhd-workbench --run-check PROJECT.mhd.json 400 \
     --output artifacts/hardware-research/NEW_RUN --field-every 80
   python scripts/hardware_flow_report.py artifacts/hardware-research/NEW_RUN --plot
   ```

   The report measures all liquid fraction below the nominal exit at z=0.
   Detached components use alpha >= 0.01, 0.25, 0.5 and 0.75 with six-neighbour connectivity; small
   components and apparent breakup require grid/time refinement. It never
   substitutes a synthetic droplet for the combined run.

3. For EM-only screening, generate without `--fluid`, then:

   ```sh
   target/debug/examples/hardware_probe PROJECT.mhd.json 80 \
     artifacts/hardware-research/NEW_EM_RUN 3 8
   ```

   The final arguments select the liquid diagnostic interval in millimetres.
   Coil reaction linkage is obtained from GPU metal currents by reciprocity.
   Its energy-balance test and observer-invariance test are in
   `src/em/coil_response_tests.rs`. The observer supports one coil with unit
   permeability and no additional field sources.

4. Independently estimate the winding's low-frequency air inductance:

   ```sh
   OPENBLAS_NUM_THREADS=1 python scripts/winding_inductance.py PROJECT.mhd.json \
     --output artifacts/hardware-research/inductance.json
   ```

   This is a lumped thin-wire energy integral, not a second field solver. The
   generated helix includes its return lead. Its regularized round-wire model
   and circular-loop asymptote check are explicit in the output. Skin,
   proximity, finite cross-section and physical lead routing remain uncertain.

## Interpretation limits

1. Cold EM runs freeze the liquid geometry. Coupled runs enable its interface,
   velocity, pressure, surface tension and EM feedback. One-pulse experiments
   currently hold thermal evolution and gravity off, at prescribed initial
   temperatures. They do not model continuous wire replenishment or oxidation.
2. The feed-neck fixture has a stationary 0.8 mm wire and a 1.2 mm bore above
   the chamber. It is a separate hypothesis from the broad, open pool. Its cold
   wire does not melt or advance in these thermal-disabled runs.
3. A 0.15 or 0.20 mm cell spacing gives only a few cells across the aperture.
   These runs rank hypotheses. Ejection, droplet size and pinch-off require
   refinement; foil thickness and annular feed clearance need particular care.
4. The V6 heater is a simplified, solid aluminium block. The Volcano screening
   instead exposes the long nozzle to the coil and assumes a different heater
   arrangement. It does not represent an unmodified Volcano assembly.
5. Conductivity, viscosity, wetting and contact-angle uncertainty remain
   material. Copper heat estimates exclude AC effects and cooling qualification.
   A successful isolated pulse would not establish 200–500 Hz operation.

## Experimental current-driven layouts

The research examples also test direct current through the metal/liquid
assembly in a transverse permanent field. These are **headless research clients**,
not a GUI electrode feature. They reuse the normal `RunState` fluid/interface
engine through `new_with_impressed_force`. Loading their CAD/material project
in the GUI does not configure their current pulse.

1. `scripts/make_conduction_hardware.py` generates a 2 mm ID, 2.5 mm OD,
   6 mm long stainless capsule with a 0.5 mm downward side orifice, a 1.2 mm
   feed neck, copper end contacts and two 10 mm magnet cubes. The default wire
   is withdrawn above the free surface. The initial immersed-wire experiment
   used `--wire-tip 1.65` and deliberately froze that wire; do not treat it as
   a replenishment/melting calculation.
2. `scripts/make_v6_conduction.py` instead uses external copper jaws on the
   V6 hex flats. It omits the normal conductive heater block. Heater and mount
   design must avoid additional current shunts.
3. `examples/conduction/solver.wgsl` solves finite-volume current continuity,
   `div J = 0`, with `J = sigma (-grad phi + u cross B0)`, harmonic face
   conductivity, current-controlled terminals and optional specific contact
   resistance in ohm m². It recomputes conductivity from accepted liquid
   fractions and includes motional voltage. The static magnet field comes
   from the existing applied-field evaluator and actual magnet CAD.
4. This is an imposed-field, inductionless approximation. Applied-current
   self-field, magnetic reaction/diffusion and magnet heating are omitted.
   Thermal evolution, oxidation, contact chemistry and wire replenishment
   remain outside these pulse trials. The research report keeps the lagged
   electrical work and actual fluid mechanical work separate.

For a cold current-sharing calculation:

```sh
cargo run --offline -j2 --example conduction_probe -- PROJECT.mhd.json 100 \
  artifacts/hardware-research/NEW_COLD_RUN 1e-9
```

For a moving-interface trial:

```sh
cargo run --offline -j2 --example conduction_flow -- PROJECT.mhd.json \
  130 350 900 artifacts/hardware-research/NEW_FLOW_RUN 0 .25 3 0 50
```

The final arguments are contact resistivity, reverse-current fraction,
timestep in microseconds, repetition frequency in hertz (0 = one pulse), and
field-export interval. The positive trapezoid includes 20 µs rise/fall ramps;
the reverse trapezoid starts 25 µs later and lasts one third of the positive
duration. A zero peak current runs the passive control. Supply switching,
lead/inductor impedance and current-controller implementation are separate
from the computed head-terminal voltage.

The three numerical GPU checks in `examples/conduction/tests.rs` cover
uniform/parallel conduction, contact resistance and motional-voltage power
balance, and accepted-potential reuse under changing phase/velocity. There is
no duplicate CPU field solver. The potential predictors must pass the same
true residual criterion as a fresh solve.

Pressure acceleration now also attempts the existing symmetric multigrid
preconditioner after a preceding difficult solve, rather than relying only on
cell count. The fine pressure equation and physical acceptance criterion are
unchanged. A manufactured small-domain check reduced 3,456 iterations to 320
with normalized velocity difference 1.6e-6. Runtime profiling is retained in
the trial directories; concurrent desktop/GPU use limits timing comparisons.

## Source-traceable material variations and diagrams

1. `hardware/solder-properties.json` records the measured Sn60Pb40 liquid
   resistivity fit and both target alloys' Arrhenius viscosity fits, including
   units and PDF locations. `scripts/apply_solder_properties.py` creates a
   distinct trial project and a property provenance record. SnCu surface
   tension/density remain bracketed near-tin estimates.
2. `scripts/draw_hardware_layouts.py` renders sections and cutaways from the
   actual generated glTF meshes. It needs optional offline CAD/plotting tools
   (`manifold3d`, `matplotlib`); these are not runtime dependencies of the Rust
   simulator. PNGs are published as board attachments.
3. Component reports inspect multiple liquid-fraction connectivity thresholds.
   A coarse alpha-half detachment alone does not establish a converged droplet
   diameter or repeatable jetting. Flight clearance, nozzle voxel area, timestep
   and grid changes must be checked before selecting a design.

## Stage 2 checkpoint

[screening-stage2.json](screening-stage2.json) retains completed and failed trials,
local evidence paths and the limitations on coarse detachment. At this checkpoint,
the 130 A coarse cross-channel run detached a roughly 0.63 mm equivalent-volume
component, but finer runs have not confirmed that result. The 110 A, 300 µs run
at 75 µm remained attached at 3 ms. Neither is a qualified droplet design.

The V6 initializer initially rejected a small numerical excess at the common
steel/liquid conical surface. Matching the shared triangles through profile
traversal fixed initialization without changing dimensions or tolerances.
Fraction errors now report maximum total-fraction and liquid/open-ratio excess
to distinguish small raster discrepancies from substantial CAD intersections.

Initial exit-plane liquid area explains part of the grid sensitivity: the
0.5 mm cross-channel CAD aperture has area 0.19635 mm²; its initialized voxel
proxy is 0.160, 0.180 and 0.200 mm² at 100, 75 and 50 µm spacing. Thus the
coarse and finer cases do not yet present the same effective opening.
