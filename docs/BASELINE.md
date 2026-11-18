# Simulator baseline before hardware experiments

The normal application is one integrated simulation: **Setup** imports geometry,
assigns shared materials, initial temperatures, currents/magnets and domain bounds;
**Simulation → Simulate** advances enabled physics on one clock. EM force drives
fluid motion, velocity supplies motional EMF, and liquid occupancy updates
conductivity. Optional enthalpy/thermal evolution controls phase mobility. Recorded
fields share the timeline, section views and probes. The headless command uses the
same run engine.

## Reproduce the shipped controls

The baseline through commit `e57b5f6` includes the integrated nozzle examples,
weak-field EM arithmetic fix, held-source cache and accepted-field export. These
commands are headless; use a fresh output directory for each exported run:

```sh
cargo build --offline -j2
./target/debug/mhd-workbench --run-check assets/nozzle-coil.mhd.json 400 \
  --output artifacts/baseline-coil --field-every 50
./target/debug/mhd-workbench --run-check assets/nozzle-pulse.mhd.json 1199 \
  --output artifacts/baseline-pulse --field-every 20
```

1. **Computed coil:** `nozzle-coil.mhd.json` uses five CAD current loops, each
   peaking at 100 A, with no prescribed fluid force. The coupled and stationary
   controls complete 400 ticks to 200 µs. This establishes continuation through
   switch-off, not droplet release. [Inputs and numerical controls](NOZZLE_COIL.md).
2. **Prescribed-force droplet:** `nozzle-pulse.mhd.json` reproduces a detached
   numerical droplet at 2 ms through the same application engine. Its body force
   is deliberately specified externally; it is not evidence that a particular
   coil will jet solder. [Results and refinement controls](JET_EXPERIMENT.md).
3. **Field evidence:** output contains diagnostics, performance rows and accepted
   field directories with metadata and SI-unit packets. Metadata is written
   last. Failed steps do not produce accepted frames. These exports are analysis
   data, not full restart checkpoints. [Schema and plotting](RUNS.md#headless-field-output).
4. **Verification at `e57b5f6`:** 163 regular headless tests pass; strict Clippy
   passes. The native export/no-export controls produce identical diagnostic
   CSVs. Owner hotend CAD/project files were not modified.

## What the results can support

1. The fluid/interface, pressure, viscous, capillary, thermal and induction tests
   provide numerical checks of their respective models. They do not constitute
   experimental validation of a molten-solder hotend.
2. The prescribed fine-grid experiment remains sensitive to geometry placement:
   shifting a 25 µm grid by half a cell changes primary droplet core volume by
   4.2–4.7%. Its matched unshifted half-timestep control changes volume by under
   0.2%, and late speed by about 1%. These are case-specific comparisons.
3. The computed-coil example has only six cells across its aperture. Halving its
   timestep changes final speed by 3% and kinetic energy by 2.4% at 75 µs. It is
   not yet time-converged. Resolve the actual nozzle/skin layer and repeat
   spacing, timestep, domain and source-segmentation checks for hardware geometry.
4. Material presets contain starting/reference values, often at room
   temperature. Benchmark liquid properties are explicitly uncalibrated. Use
   measured or sourced operating-temperature properties for the actual solder;
   properties are not automatically temperature-dependent curves.
5. Static CAD walls are voxelized; the contact model is neutral. Wetting,
   hysteresis, oxide films, feedstock replenishment and a drive circuit are not
   represented. Ideal prescribed currents omit electrical/thermal limitations of
   the winding and return path. Ferromagnetic-source force is not a full Maxwell
   stress calculation. [Material assumptions](MATERIALS.md),
   [interface limits](SURFACES.md), [EM model](ELECTROMAGNETICS.md).

## Stronger-drive limit

An exploratory variant raised each loop to **500 A**, held for 1.5 ms after its
25 µs rise, and extended the box to z=−3…4 mm at the same 50 µm spacing and
0.5 µs timestep (72×72×140 cells). Through `e57b5f6`, it accepts steps to
**60.5 µs**, then the 61 µs EM step exhausts **4,000 iterations** with true relative
residual **6.77×10⁻⁴**, above the unchanged **10⁻⁴** target. Positive edge
conductivities span approximately **0.417…2×10⁶ S/m**.

Recomputing the applied source instead of caching it reproduces the accepted
prefix diagnostics exactly. Disabling increment prediction still needs roughly
3,450 iterations at 30 µs. Neither optimization explains away the conditioning
problem. A failed step is rejected, rather than treated as a completed trajectory.
The partial run does not establish jetting or its absence. A separate
Rust/WGSL gradient-space preconditioner is being tested; it is not part of this
pinned baseline. See [conductivity contrast](ELECTROMAGNETICS.md#conductivity-contrast-and-iteration-limits).

## First hardware comparisons

1. Record the actual current waveform, winding/return geometry and temperature.
   Use these as simulation inputs rather than interpreting the illustrative
   100 A setting as a hardware recommendation.
2. Compare field or force/pool response before fitting droplet breakup. Then
   compare high-speed images of meniscus motion, detachment time, droplet
   diameter/volume and velocity, with measurement uncertainty.
3. Keep geometry, project, solver revision and output together. A small algebraic
   residual establishes equation convergence, not a resolved interface or
   experimentally accurate jetting threshold. No coil-driven droplet prediction
   has yet been validated against hardware.
