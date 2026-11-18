# Reproducible geometry and project inputs

These are **research inputs, not manufacturing drawings or qualified jetters**.
The first three retain zero uniform acceleration; the flat-tip cases have
−9.81 m/s² acceleration. See the gravity correction in the
[study README](../README.md). Their manifests record original and packaged hashes;
only geometry/buffer filenames changed, with identical float32 CAD payloads and
simulation properties.

1. `v6-wide-200A`: stainless V6 approximation, 0.4 mm bore, 0.8 × 0.4 mm external
   tip contacts, opposing 20 mm diameter × 10 mm poles. The coarse 200 A / 250 µs
   single-pulse result detached one approximately 0.595 mm component. Its cropped
   two-pulse screen does not qualify repetition.
2. `v6-compact-250A`: the same contact geometry with one 10 × 5 mm SmCo disc per
   pole; nominal span 14.2 mm excluding mounts/heater. The coarse 250 A / 250 µs
   result detached one approximately 0.596 mm component. Refinement remains open.
3. `single-coil-250A`: custom 6 mm ID chamber, 0.5 mm radial stainless wall,
   0.5 mm aperture and 0.15 mm exit land, one ten-turn coil. The normal unified
   solver completed the 250 A / 50 kHz trial without resolved ejection at 200 µm.
4. `v6-flat-compact-150A`: two external flats 0.8 mm apart over the lower 0.6 mm,
   preserving the 0.4 mm bore with 0.2 mm nominal minimum wall. Compact poles;
   first 150 A / 250 µs gravity-on trial completes 3.6 ms without detachment.
   Threads, fillets, clamps and tolerances omitted.

5. `v6-flat-shortbar`: machined tip and 4 mm shoulder clearance flats, with two
   25 × 5 × 5 mm SmCo bars stacked per pole. Nominal projection 0.2 mm below exit,
   excluding mounts/tolerances. The first 150 A case separates a component moving
   upward at 3.6 ms; this is not a usable jet. The 175 A control gives a coarse
   leading component around 0.595 mm moving down at 0.355 m/s. Higher-threshold
   returning-tail fragments, refinement and repetition remain open.

## Inspect and reproduce

Open a `case.mhd.json` in the main application to inspect CAD/materials. The first
two cases require the **research conduction client** for current-driven physics;
their contacts are not a GUI electrode feature. Pressing the normal GUI Simulate
button does not reproduce their applied-terminal-current experiment.

From the research worktree, with a new output directory each time:

```sh
cargo build --offline -j2 --example conduction_flow
target/debug/examples/conduction_flow \
  hardware/designs/v6-wide-200A/case.mhd.json 200 250 1200 \
  artifacts/hardware-research/NEW_WIDE_REPRO 0 .25 3 0 150
```

For the compact case, replace the input with
`hardware/designs/v6-compact-250A/case.mhd.json` and peak current with `250`.
Both commands use fixed 3 µs steps through 3.6 ms. Thermal evolution, feeding,
measured wetting/oxide skins, current self-field feedback and switching are absent.

For the new flat-tip trial, use
`hardware/designs/v6-flat-compact-150A/case.mhd.json` with `150` peak amperes;
the other arguments remain `250 1200 OUTPUT 0 .25 3 0 150`. This case explicitly
uses gravity and full-height nonperiodic bounds. It is an exploratory run.

For the near-flush short-bar case, use
`hardware/designs/v6-flat-shortbar/case.mhd.json` with `150` or `175` peak amperes
and `250 1200 OUTPUT 0 .25 3 0 150`. The peak is a research-client argument;
the inspection project does not impose it through the normal GUI.

The single-coil case uses the normal combined engine:

```sh
target/debug/mhd-workbench --run-check \
  hardware/designs/single-coil-250A/case.mhd.json 3500 \
  --output artifacts/hardware-research/NEW_COIL_REPRO --field-every 200 \
  --tail-after-us 300 --tail-dt-us 3
```

Measure actual accepted fields with:

```sh
python scripts/hardware_flow_report.py artifacts/hardware-research/NEW_WIDE_REPRO --plot
```

These commands reproduce the recorded screens, including their limitations.
They do not substitute for the separate fine-grid and full-domain gravity checks.

6. `short-m6-stub4`: revised short-body concept with 6 mm M6×1 engagement,
   2 mm lower bore, raised pool inside heater and 1.45 mm nominal magnet projection.
   Static electrical and separate thermal screens are promising; no qualified
   drop or repetition result. See its README and the short-nozzle screening records.
