# Pole-carrier assembly for review

1. Open `assembly.inspection.mhd.json` to inspect the short M6 nozzle, heated
   contact fingers, magnets and carrier hardware together. `carrier-only.gltf`
   shows the fixtures separately. The six binary STL files are individual
   carrier/keeper meshes in **millimetres**; `manifest.json` maps filenames to
   parts. They are review/remodelling inputs, not a shop release.
2. The new hardware comprises two aluminium carrier bodies, four keeper strips
   and eight nominal M2×4 screw envelopes. Inward stops act near the magnet ends,
   outside the nozzle/contact region. The nominal carriers carry magnet attraction
   through their upper mounting tabs, independently of the nozzle. Thread forms,
   tool reliefs, preload and a machine-specific carriage interface are absent.
3. The carrier envelope is approximately 37×36.6×23.9 mm. The lowest floor is
   **2.7 mm below the outlet**; magnet-only projection is 2.2 mm. Assembly,
   thermal-motion and target-surface allowances must be added. The central floor
   opening is 2.36 mm wide; this is a geometric passage, not a validated trajectory.
4. Mesh distance checks give **0.5 mm minimum gap to copper and to the heater**,
   and 2.33 mm to the nozzle. The carrier clears a Ø6.4 mm central stem envelope
   and an abstract Ø28 mm upper envelope from z=16 to 40 mm. Actual heatsink and
   carriage geometry is not supplied or qualified. All six STL round trips are
   closed manifolds and preserve their source volumes.
5. Both poles are 0.2 mm lower than the previous fixture. A native q4 field check
   with unchanged accepted static J gives +0.36% outlet-region force. This is
   essentially neutral for the current comparison. It does not establish drop
   size/rate, moving-current response or a physical operating waveform.
6. The fixed-source attraction screen is about 9.4 N per pole group; 20 N per
   group is a chosen sizing allowance. A conservative-section cantilever estimate
   gives about 0.03 mm elastic gap closure under that load. Actual clamp joints,
   local stresses, temperatures and thermal movement still need qualification.
7. Keepers allow nominal 0.2 mm magnet lift; no spring preload or magnet/carrier
   thermal contact conductance is specified. The cold-carriage interface must
   preserve the floating-head drive circuit; its electrical isolation stack is
   not included. These remain mechanical/thermal design tasks.
8. **All added fixture bodies are excluded from physics.** Earlier head thermal
   results do not qualify the carrier, magnet temperatures, eddy losses or new
   heat paths. The main simulator remains frozen; this file is for inspection.

Reproduce the CAD from the research worktree:

```sh
TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with manifold3d --with matplotlib \
  python scripts/make_pole_carrier.py artifacts/hardware-research/cases/NEW_CARRIER \
  --magnet-drop-mm .2
TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with manifold3d --with matplotlib \
  python scripts/check_pole_carrier.py --case NEW_CARRIER --output NEW_REPORT.json
```

See `hardware/m6-carrier-review-screening.json`,
`hardware/m6-carrier-field-screening.json` and the prototype measurement plan.
Screw head dimensions use the manufacturer's BN 610 nominal M2 envelope; exact
fastener grade, thread tolerances and hardware remain to be selected.
