# Heated contact fingers: thermal research fixture

1. This is the short stainless M6 concept with the same 0.4 mm outlet, 0.7 mm
   tip boss, 2 mm lower bore and raised pool, now using 118° drill-point geometry.
   Pole gap is 3.4 mm, side clearance 0.3 mm, heater clearance 1 mm and nominal
   magnet projection 2 mm. Thread helix/root, tool radii and tolerances remain
   unresolved. The original 0.1 mm side gaps were too small for a first assembly.
2. Folded copper fingers connect the tip contacts to two 6×10×0.4 mm copper
   plates. Each plate lies below the heater across 0.1 mm electrical mica
   insulation. The heater supplies heat to the fingers as well as the nozzle.
   Both plates must remain electrically insulated from the block and clamp.
   Actual insulated clamping hardware is not yet designed or qualified.
3. The two straight 50 mm, 1 mm² copper extensions are a thermal test fixture
   terminating at an imposed cold boundary. They are not final cable routing or
   a 120 mm-wide head requirement. Real wiring should be paired to control
   inductance, with insulation and joints qualified for the local temperatures.
4. A native 100 µm steady model with a 250 °C fixed heater and 25 °C outer ends
   gives approximately 237 °C minimum liquid temperature with effective mica
   k=0.1 W/(m K). This represents the sheet and extra interface resistance,
   rather than assuming the ideal room-temperature sheet k=0.528 W/(m K).
   The same narrow-anchor fixture without thermal coupling through the mica
   gives only 179 °C. These are thermal screening results, not operating limits.
5. No wire melting/advection, exterior gas/radiation losses, pulse-average Joule
   heat, phase-dependent conductivity or real microscopic contact heating is
   included. The fixed heater is an ideal power reservoir. Added hot copper
   also changes heat transfer to the magnets; the earlier bare-body gas-gap
   conductance comparison does not qualify this full assembly.
6. Open `hot-contact.mhd.json` for geometry/material inspection. The GUI does not
   reproduce these terminal-current/thermal boundary experiments with Simulate.
   The dedicated research clients and recorded command arguments are required.
   This is not a shop drawing, qualified jetter or a demonstrated droplet rate.

Generate the fixture with `scripts/make_hot_contact_case.py OUTPUT
--anchor-width-mm 10`. It uses the drilled source from
`scripts/make_stub_m6_cases.py`; see the associated screening records.

The native steady-conduction command, from the research worktree, is:

```sh
TMPDIR="$PWD/artifacts/build-temp" cargo build --offline -j2 --example thermal_conductance
target/debug/examples/thermal_conductance \
  hardware/designs/hot-contact-m6/hot-contact.mhd.json 4 NEW_REPORT.json \
  15 2e-6 anchors .1
```

`anchors` fixes the aluminium heater reservoir and the outermost copper planes.
The final argument is the effective insulation conductivity. The native unit
checks include a metal/mica/metal series rod with a one-cell insulation layer.

The matched 100 µm static-current check preserves outlet-region force within
0.1% after adding the folded fingers and supply extensions. Full fixture resistance
is about 7.1 mΩ with fixed hot copper properties: roughly 0.66 V resistive drop
at a 93 A comparison peak, and 2.8 W at an assumed 19.8 A RMS. These include both
50 mm copper extensions and must not be added again to a driver cable allowance.
Force changes by 0.014% between the accepted charge-tolerance controls. Neither
the charge nor thermal fixture is a fine-grid jetting qualification.
